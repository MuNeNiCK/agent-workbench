import AgentWorkbench.Adapter.Update

open AgentWorkbench
open AgentWorkbench.Domain

namespace AgentWorkbench.Tests.StorageLaws

def expect (condition : Bool) (message : String) : IO Unit := do
  unless condition do throw <| IO.userError message

def expectFailure (action : IO α) (message : String) : IO Unit := do
  let failed ← try
    let _ ← action
    pure false
  catch _ => pure true
  expect failed message

def execSql (ledger : System.FilePath) (sql : String) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec sql

def load (ledger : System.FilePath) : IO Kernel.Projection.Store := do
  match ← Adapter.SQLite.inspect ledger with
  | .ok store => pure store
  | .error error => throw <| IO.userError s!"store inspection failed: {repr error}"

def mutate (ledger : System.FilePath) (operation : String)
    (revision : Nat) (command : Kernel.Decide.Command) : IO Adapter.SQLite.MutationOutcome := do
  match ← Adapter.SQLite.mutate ledger ⟨operation⟩ ⟨revision⟩ command with
  | .ok outcome => pure outcome
  | .error error => throw <| IO.userError s!"storage mutation failed: {repr error}"

def bootstrap (ledger : System.FilePath) : IO Adapter.SQLite.MutationOutcome :=
  mutate ledger "bootstrap" 0 Application.Service.bootstrapCommand

def testRecoveryAndRetry (root : System.FilePath) : IO Unit := do
  let ledger := root / "recovery.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  expect (initialized.store.ledger.storedHead == ⟨1⟩)
    "bootstrap did not durably advance the ledger"
  let reconstructed ← load ledger
  expect (reconstructed == initialized.store)
    "fresh-process reconstruction did not recover the exact committed store"
  let unrelated : Domain.ExternalOperation.Attempt := {
    operation := ⟨"unrelated-after-bootstrap"⟩
    artifactDigest := "proof:unrelated"
    state := .prepared }
  match ← Adapter.SQLite.mutate ledger ⟨"unrelated"⟩ ⟨1⟩
      (.recordExternalOperation ⟨1⟩ unrelated) with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"unrelated transaction failed: {repr error}"
  let retry ← Adapter.SQLite.mutate ledger ⟨"bootstrap"⟩ ⟨99⟩
    Application.Service.bootstrapCommand
  match retry with
  | .ok outcome =>
      expect outcome.exactRetry "committed retry was not identified as exact"
      expect (outcome.receipt == initialized.receipt)
        "committed retry did not return the canonical receipt"
      expect (outcome.store.ledger.storedHead == ⟨2⟩)
        "exact retry advanced the ledger"
  | .error error => throw <| IO.userError s!"exact retry rejected: {repr error}"
  let conflicting : Adapter.DurableFilesystem.ArtifactRef := {
    digest := "sha3-256:conflict", size := 1 }
  match ← Adapter.SQLite.mutate ledger ⟨"bootstrap"⟩ ⟨1⟩
      Application.Service.bootstrapCommand [conflicting] (some root) with
  | .error .operationConflict => pure ()
  | other => throw <| IO.userError s!"payload conflict was not rejected: {repr other}"
  match ← Adapter.SQLite.mutate ledger ⟨"unseen-stale"⟩ ⟨0⟩
      Application.Service.bootstrapCommand with
  | .error .staleRevision => pure ()
  | other => throw <| IO.userError s!"unseen stale mutation was not rejected: {repr other}"
  expect ((← load ledger).ledger.storedHead == ⟨2⟩)
    "rejected mutation changed authoritative state"

def testOperationJournalCorruption (root : System.FilePath) : IO Unit := do
  let cases := [
    ("deleted", "DELETE FROM operations WHERE operation_id='bootstrap'"),
    ("request", "UPDATE operations SET request_payload=x'00' WHERE operation_id='bootstrap'"),
    ("result", "UPDATE operations SET result_digest='substituted' WHERE operation_id='bootstrap'"),
    ("receipt", "UPDATE operations SET receipt=x'00' WHERE operation_id='bootstrap'"),
    ("event-binding", "UPDATE events SET operation_id='substituted' WHERE revision=1")]
  for (name, sql) in cases do
    let ledger := root / s!"journal-{name}.sqlite3"
    Adapter.SQLite.initializeStore ledger
    let _ ← bootstrap ledger
    execSql ledger sql
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"journal corruption {name} was accepted: {repr other}"

def testCrashRollback (root : System.FilePath) : IO Unit := do
  let ledger := root / "crash-rollback.sqlite3"
  Adapter.SQLite.initializeStore ledger
  expectFailure
    (Adapter.SQLite.mutateWithHook ledger ⟨"crash"⟩ ⟨0⟩
      Application.Service.bootstrapCommand [] none (pure ()) (pure ())
      (throw <| IO.userError "injected crash after journal write"))
    "injected transaction crash was reported as success"
  expect ((← load ledger).ledger.storedHead == ⟨0⟩)
    "transaction crash left a partial journal or event commit"

def crashChild (ledger : System.FilePath) : IO Unit := do
  let _ ← Adapter.SQLite.mutateWithHook ledger ⟨"process-crash"⟩ ⟨0⟩
    Application.Service.bootstrapCommand [] none (pure ()) (pure ()) (IO.Process.forceExit 86)
  throw <| IO.userError "crash failpoint returned"

def testProcessCrashRecovery (root : System.FilePath) : IO Unit := do
  let ledger := root / "process-crash.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--crash-child", ledger.toString] }
  expect (result.exitCode == 86) "storage crash subprocess did not reach the failpoint"
  expect ((← load ledger).ledger.storedHead == ⟨0⟩)
    "subprocess crash left a partial authoritative commit"

def testSingleWriter (root : System.FilePath) : IO Unit := do
  let ledger := root / "single-writer.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let left ← IO.asTask <| Adapter.SQLite.mutate ledger ⟨"writer-left"⟩
    ⟨0⟩ Application.Service.bootstrapCommand
  let right ← IO.asTask <| Adapter.SQLite.mutate ledger ⟨"writer-right"⟩
    ⟨0⟩ Application.Service.bootstrapCommand
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

def testConcurrentInspection (root : System.FilePath) : IO Unit := do
  let ledger := root / "concurrent-inspection.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  for index in [0:24] do
    let before ← load ledger
    let revision := before.ledger.storedHead
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"inspect-race-{index}"⟩
      artifactDigest := s!"proof:{index}"
      state := .prepared }
    let inspection ← IO.asTask (Adapter.SQLite.inspect ledger)
    match ← Adapter.SQLite.mutate ledger ⟨s!"inspect-writer-{index}"⟩ revision
        (.recordExternalOperation revision attempt) with
    | .ok _ => pure ()
    | .error error => throw <| IO.userError s!"concurrent writer failed: {repr error}"
    match ← IO.ofExcept inspection.get with
    | .ok _ => pure ()
    | .error error => throw <| IO.userError s!"coherent inspection reported corruption: {repr error}"

def testRelativeReplacement (root : System.FilePath) : IO Unit := do
  let run (staged current : System.FilePath) (payload : String) : IO Unit := do
    if ← staged.pathExists then IO.FS.removeFile staged
    if ← current.pathExists then IO.FS.removeFile current
    IO.FS.writeBinFile staged payload.toUTF8
    let _ ← Adapter.DurableFilesystem.replace staged current
    expect ((← IO.FS.readBinFile current) == payload.toUTF8)
      s!"replacement outcome disagreed with adopted bytes for {current}"
    IO.FS.removeFile current
  run ".storage-laws-bare.stage" ".storage-laws-bare.current" "bare"
  run "./.storage-laws-dot.stage" "./.storage-laws-dot.current" "dot"
  run (root / "absolute.stage") (root / "absolute.current") "absolute"

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
  match ← Adapter.SQLite.mutate ledger ⟨"artifact-event-missing"⟩
      initialized.store.ledger.storedHead
      (.recordExternalOperation initialized.store.ledger.storedHead operation) with
  | .error (.artifactInvalid _) => pure ()
  | other => throw <| IO.userError s!"missing staged artifact was accepted: {repr other}"
  expect ((← load ledger).ledger.storedHead == initialized.store.ledger.storedHead)
    "rejected artifact reference advanced the ledger"
  let recorded ← Adapter.SQLite.mutate ledger ⟨"artifact-event"⟩
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
  let obligated ← mutate evidenceLedger "obligation" 1
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
  let evidenced ← mutate evidenceLedger "evidence" 2
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

def testArtifactBindingsAndRace (root : System.FilePath) : IO Unit := do
  let corruptions := [
    ("deleted", "DELETE FROM artifacts"),
    ("altered", "UPDATE artifacts SET digest='sha3-256:substituted'"),
    ("size", "UPDATE artifacts SET size='999'"),
    ("payload", "UPDATE artifacts SET payload=x'00'"),
    ("extra", "INSERT INTO artifacts (digest,size,payload) VALUES ('sha3-256:extra','1',x'00')")]
  for (name, sql) in corruptions do
    let ledger := root / s!"artifact-table-{name}.sqlite3"
    let artifactRoot := root / s!"artifact-table-{name}"
    Adapter.SQLite.initializeStore ledger
    let initialized ← bootstrap ledger
    let reference ← Adapter.DurableFilesystem.stage artifactRoot s!"artifact-{name}".toUTF8
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"artifact-{name}"⟩
      artifactDigest := reference.digest
      state := .prepared }
    match ← Adapter.SQLite.mutate ledger ⟨s!"record-{name}"⟩
        initialized.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .ok _ => pure ()
    | .error error => throw <| IO.userError s!"artifact fixture failed: {repr error}"
    execSql ledger sql
    match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"artifact table corruption {name} was accepted: {repr other}"

  let raceLedger := root / "artifact-race.sqlite3"
  let raceRoot := root / "artifact-race"
  Adapter.SQLite.initializeStore raceLedger
  let initialized ← bootstrap raceLedger
  let reference ← Adapter.DurableFilesystem.stage raceRoot "race artifact".toUTF8
  let attempt : Domain.ExternalOperation.Attempt := {
    operation := ⟨"artifact-race"⟩, artifactDigest := reference.digest, state := .prepared }
  expectFailure
    (Adapter.SQLite.mutateWithHook raceLedger ⟨"artifact-race"⟩
      initialized.store.ledger.storedHead
      (.recordExternalOperation initialized.store.ledger.storedHead attempt)
      [reference] (some raceRoot) (pure ())
      (IO.FS.removeFile (Adapter.DurableFilesystem.objectPath raceRoot reference)))
    "artifact deletion after verification committed"
  expect ((← load raceLedger).ledger.storedHead == initialized.store.ledger.storedHead)
    "artifact race advanced the authoritative ledger"

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
  expect (receipt.targetDurability == .confirmed)
    "update adoption durability was not confirmed"
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "published update point drifted"
  | other => throw <| IO.userError s!"update did not publish current storage: {repr other}"
  let targetBytes ← IO.FS.readBinFile ledger
  let arbitrary ← Adapter.DurableFilesystem.stage backups "not a sqlite database".toUTF8
  expectFailure (Adapter.Update.restore ledger backups { receipt with backup := arbitrary })
    "arbitrary restore bytes were adopted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "arbitrary restore bytes changed live storage"
  let unrelatedLedger := root / "unrelated.sqlite3"
  Adapter.SQLite.initializeStore unrelatedLedger
  let unrelated ← Adapter.DurableFilesystem.stage backups (← IO.FS.readBinFile unrelatedLedger)
  expectFailure (Adapter.Update.restore ledger backups { receipt with backup := unrelated })
    "unrelated valid storage was adopted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "unrelated restore changed live storage"
  expectFailure (Adapter.Update.restore ledger backups { receipt with target := receipt.source })
    "stale restore target was accepted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "stale restore changed live storage"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source && restored.durability == .confirmed)
    "restore did not recover the exact confirmed source image"
  match ← Adapter.Update.inspect ledger with
  | .updateRequired restoredPlan =>
      expect (restoredPlan.source == receipt.source) "restored update plan changed source identity"
  | other => throw <| IO.userError s!"restored old storage was not diagnosed: {repr other}"

  let corruptLedger := root / "corrupt-backup.sqlite3"
  let corruptBackups := root / "corrupt-backups"
  Adapter.SQLite.initializeStore corruptLedger
  let _ ← bootstrap corruptLedger
  setSchemaZero corruptLedger
  let corruptPlan ← match ← Adapter.Update.inspect corruptLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"corrupt backup fixture failed: {repr other}"
  let corruptReceipt ← Adapter.Update.apply corruptLedger corruptBackups corruptPlan
  let corruptTarget ← IO.FS.readBinFile corruptLedger
  let corruptBackupPath := Adapter.DurableFilesystem.objectPath corruptBackups corruptReceipt.backup
  IO.FS.removeFile corruptBackupPath
  IO.FS.writeBinFile corruptBackupPath "corrupted backup".toUTF8
  expectFailure (Adapter.Update.restore corruptLedger corruptBackups corruptReceipt)
    "corrupted backup was adopted"
  expect ((← IO.FS.readBinFile corruptLedger) == corruptTarget)
    "corrupted backup changed live storage"

def postRenameSyncFailureChild (root : System.FilePath) : IO Unit := do
  let ledger := root / "post-rename-uncertain.sqlite3"
  let backups := root / "post-rename-uncertain-backups"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  setSchemaZero ledger
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"uncertain update fixture failed: {repr other}"
  let receipt ← Adapter.Update.apply ledger backups plan
  expect (receipt.targetDurability == .uncertain)
    "post-rename sync failure was not reported as adopted-but-uncertain"
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "uncertain update was not adopted"
  | other => throw <| IO.userError s!"uncertain update became a rejection: {repr other}"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source && restored.durability == .uncertain)
    "uncertain restore did not report its adopted source"

def testPostRenameSyncFailure (root : System.FilePath) : IO Unit := do
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--post-rename-sync-failure-child", root.toString]
    env := #[("AW_TEST_FAIL_REPLACEMENT_PARENT_FSYNC", some "1")] }
  unless result.exitCode = 0 do
    throw <| IO.userError s!"post-rename sync failure child failed: {result.stderr}"

def main (args : List String) : IO Unit :=
  match args with
  | ["--crash-child", ledger] => crashChild ledger
  | ["--post-rename-sync-failure-child", root] => postRenameSyncFailureChild root
  | [] => IO.FS.withTempDir fun root => do
    testRecoveryAndRetry root
    testOperationJournalCorruption root
    testCrashRollback root
    testProcessCrashRecovery root
    testSingleWriter root
    testConcurrentInspection root
    testRelativeReplacement root
    testReadOnlyFaultDetection root
    testArtifactsAndEvidence root
    testArtifactBindingsAndRace root
    testExplicitUpdateAndRestore root
    testPostRenameSyncFailure root
    IO.println "storage laws: pass"
  | _ => throw <| IO.userError "unsupported storage-laws arguments"

end AgentWorkbench.Tests.StorageLaws

def main (args : List String) : IO Unit :=
  AgentWorkbench.Tests.StorageLaws.main args
