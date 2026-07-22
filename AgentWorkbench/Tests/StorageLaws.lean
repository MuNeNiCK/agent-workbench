import AgentWorkbench.Adapter.Update

open AgentWorkbench
open AgentWorkbench.Domain

namespace AgentWorkbench.Tests.StorageLaws

def expect (condition : Bool) (message : String) : IO Unit := do
  unless condition do throw <| IO.userError message

def load (ledger : System.FilePath) : IO Kernel.Projection.Store := do
  match ← Adapter.SQLite.inspect ledger with
  | .ok store => pure store
  | .error error => throw <| IO.userError s!"store inspection failed: {repr error}"

def mutate (ledger : System.FilePath) (operation payload : String)
    (revision : Nat) (command : Kernel.Decide.Command) : IO Adapter.SQLite.MutationOutcome := do
  match ← Adapter.SQLite.mutate ledger ⟨operation⟩ payload ⟨revision⟩ command with
  | .ok outcome => pure outcome
  | .error error => throw <| IO.userError s!"storage mutation failed: {repr error}"

def bootstrap (ledger : System.FilePath) : IO Adapter.SQLite.MutationOutcome :=
  mutate ledger "bootstrap" "payload:bootstrap" 0 Application.Service.bootstrapCommand

def testRecoveryAndRetry (root : System.FilePath) : IO Unit := do
  let ledger := root / "recovery.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  expect (initialized.store.ledger.storedHead == ⟨1⟩)
    "bootstrap did not durably advance the ledger"
  let reconstructed ← load ledger
  expect (reconstructed == initialized.store)
    "fresh-process reconstruction did not recover the exact committed store"
  let retry ← Adapter.SQLite.mutate ledger ⟨"bootstrap"⟩ "payload:bootstrap" ⟨99⟩
    Application.Service.bootstrapCommand
  match retry with
  | .ok outcome =>
      expect outcome.exactRetry "committed retry was not identified as exact"
      expect (outcome.receipt == initialized.receipt)
        "committed retry did not return the canonical receipt"
      expect (outcome.store.ledger.storedHead == ⟨1⟩)
        "exact retry advanced the ledger"
  | .error error => throw <| IO.userError s!"exact retry rejected: {repr error}"
  match ← Adapter.SQLite.mutate ledger ⟨"bootstrap"⟩ "payload:changed" ⟨1⟩
      Application.Service.bootstrapCommand with
  | .error .operationConflict => pure ()
  | other => throw <| IO.userError s!"payload conflict was not rejected: {repr other}"
  match ← Adapter.SQLite.mutate ledger ⟨"unseen-stale"⟩ "payload:new" ⟨0⟩
      Application.Service.bootstrapCommand with
  | .error .staleRevision => pure ()
  | other => throw <| IO.userError s!"unseen stale mutation was not rejected: {repr other}"
  expect ((← load ledger).ledger.storedHead == ⟨1⟩)
    "rejected mutation changed authoritative state"

def testSingleWriter (root : System.FilePath) : IO Unit := do
  let ledger := root / "single-writer.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let left ← IO.asTask <| Adapter.SQLite.mutate ledger ⟨"writer-left"⟩
    "payload:left" ⟨0⟩ Application.Service.bootstrapCommand
  let right ← IO.asTask <| Adapter.SQLite.mutate ledger ⟨"writer-right"⟩
    "payload:right" ⟨0⟩ Application.Service.bootstrapCommand
  let leftResult ← IO.ofExcept left.get
  let rightResult ← IO.ofExcept right.get
  let accepted := [leftResult, rightResult].countP fun result => result.isOk
  let stale := [leftResult, rightResult].countP fun
    | .error .staleRevision => true
    | _ => false
  expect (accepted == 1 && stale == 1)
    "single-writer coordination did not serialize competing revisions"
  expect ((← load ledger).ledger.storedHead == ⟨1⟩)
    "competing writers committed more than one authoritative transaction"

def corruptProjection (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec "UPDATE projection SET payload = x'00' WHERE singleton = 1"

def corruptHistory (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec "UPDATE metadata SET history_digest = 'corrupt' WHERE singleton = 1"

def testReadOnlyFaultDetection (root : System.FilePath) : IO Unit := do
  let projectionLedger := root / "projection-fault.sqlite3"
  Adapter.SQLite.initializeStore projectionLedger
  let _ ← bootstrap projectionLedger
  corruptProjection projectionLedger
  let before ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile projectionLedger)
  match ← Adapter.SQLite.inspect projectionLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"projection corruption was not detected: {repr other}"
  let after ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile projectionLedger)
  expect (before == after) "inspection wrote while diagnosing projection corruption"
  let repairPlan ← match ← Adapter.SQLite.diagnose projectionLedger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"projection repair plan was not exposed: {repr other}"
  match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"explicit projection repair failed: {repr error}"
  let _ ← load projectionLedger
  match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"stale projection repair replayed: {repr other}"

  let historyLedger := root / "history-fault.sqlite3"
  Adapter.SQLite.initializeStore historyLedger
  let _ ← bootstrap historyLedger
  corruptHistory historyLedger
  let historyBefore ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile historyLedger)
  match ← Adapter.SQLite.inspect historyLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"history corruption was not detected: {repr other}"
  let historyAfter ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile historyLedger)
  expect (historyBefore == historyAfter) "inspection silently repaired corrupt history"
  match ← Adapter.SQLite.diagnose historyLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"corrupt history exposed a repair action: {repr other}"

def testArtifactsAndEvidence (root : System.FilePath) : IO Unit := do
  let ledger := root / "artifacts.sqlite3"
  let artifactRoot := root / "artifacts"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let bytes := "exact evidence artifact".toUTF8
  let reference ← Adapter.DurableFilesystem.stage artifactRoot bytes
  let duplicate ← Adapter.DurableFilesystem.stage artifactRoot bytes
  expect (duplicate == reference) "exact artifact retry changed its identity"
  let operation : Domain.ExternalOperation.Attempt := {
    operation := ⟨"artifact-operation"⟩
    artifactDigest := reference.digest
    state := .prepared }
  match ← Adapter.SQLite.mutate ledger ⟨"artifact-event-missing"⟩ "payload:artifact"
      initialized.store.ledger.storedHead
      (.recordExternalOperation initialized.store.ledger.storedHead operation) with
  | .error (.artifactInvalid _) => pure ()
  | other => throw <| IO.userError s!"missing staged artifact was accepted: {repr other}"
  expect ((← load ledger).ledger.storedHead == initialized.store.ledger.storedHead)
    "rejected artifact reference advanced the ledger"
  let recorded ← Adapter.SQLite.mutate ledger ⟨"artifact-event"⟩ "payload:artifact"
    initialized.store.ledger.storedHead
    (.recordExternalOperation initialized.store.ledger.storedHead operation)
    [reference] (some artifactRoot)
  let artifactStore ← match recorded with
    | .ok outcome => pure outcome.store
    | .error error => throw <| IO.userError s!"artifact transaction rejected: {repr error}"
  match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
  | .ok recovered => expect (recovered == artifactStore) "artifact-backed recovery drifted"
  | .error error => throw <| IO.userError s!"valid artifact rejected: {repr error}"
  IO.FS.removeFile (Adapter.DurableFilesystem.objectPath artifactRoot reference)
  match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"missing committed artifact was not detected: {repr other}"

  let evidenceLedger := root / "evidence.sqlite3"
  Adapter.SQLite.initializeStore evidenceLedger
  let evidenceInitialized ← bootstrap evidenceLedger
  let obligation : Domain.Evidence.Obligation := {
    work := ⟨1⟩, key := "GATE-006", revision := ⟨1⟩, current := true }
  let obligated ← mutate evidenceLedger "obligation" "payload:obligation" 1
    (.recordObligation ⟨1⟩ obligation)
  let item : Domain.Evidence.Evidence := {
    id := ⟨1⟩
    work := ⟨1⟩
    obligation := "GATE-006"
    revision := obligated.store.ledger.storedHead
    commandProfile := "storage-laws"
    invocation := ".lake/build/bin/storage-laws --fault-matrix"
    exitCode := 0
    repository := "main"
    snapshot := "commit:test"
    artifactDigest := "proof:test"
    current := true }
  let evidenced ← mutate evidenceLedger "evidence" "payload:evidence" 2
    (.recordEvidence ⟨2⟩ item)
  let recovered ← load evidenceLedger
  expect (recovered == evidenced.store) "exact evidence did not survive reconstruction"
  let current ← match (Kernel.Projection.inspect recovered).currentState? with
    | some state => pure state
    | none => throw <| IO.userError "recovered evidence projection is not fresh"
  expect (current.evidence.any fun found =>
      found.invocation == item.invocation && found.repository == item.repository &&
      found.snapshot == item.snapshot && found.artifactDigest == item.artifactDigest)
    "typed exact evidence identities were not retained"
  expect (evidenceInitialized.store.ledger.storedHead == ⟨1⟩)
    "evidence fixture bootstrap drifted"

def setSchemaZero (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec "UPDATE metadata SET schema_version = '0' WHERE singleton = 1"

def testExplicitUpdateAndRestore (root : System.FilePath) : IO Unit := do
  let ledger := root / "update.sqlite3"
  let backups := root / "backups"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  setSchemaZero ledger
  let beforeBytes ← IO.FS.readBinFile ledger
  let beforeDigest ← Adapter.DurableFilesystem.digest beforeBytes
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"incompatible schema did not require update: {repr other}"
  let afterInspectDigest ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile ledger)
  expect (beforeDigest == afterInspectDigest) "update inspection mutated storage"
  let receipt ← Adapter.Update.apply ledger backups plan
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "published update point drifted"
  | other => throw <| IO.userError s!"update did not publish current storage: {repr other}"
  let restored ← Adapter.Update.restore ledger backups receipt.target receipt.backup
  expect (restored == receipt.source) "restore did not recover the exact source image"
  match ← Adapter.Update.inspect ledger with
  | .updateRequired restoredPlan =>
      expect (restoredPlan.source == receipt.source) "restored update plan changed source identity"
  | other => throw <| IO.userError s!"restored old storage was not diagnosed: {repr other}"

def main : IO Unit :=
  IO.FS.withTempDir fun root => do
    testRecoveryAndRetry root
    testSingleWriter root
    testReadOnlyFaultDetection root
    testArtifactsAndEvidence root
    testExplicitUpdateAndRestore root
    IO.println "storage laws: pass"

end AgentWorkbench.Tests.StorageLaws

def main : IO Unit :=
  AgentWorkbench.Tests.StorageLaws.main
