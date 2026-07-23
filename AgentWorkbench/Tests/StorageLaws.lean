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
    execSql ledger "UPDATE projection SET payload = x'00' WHERE singleton = 1"
    let repairPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"journal repair fixture failed: {repr other}"
    execSql ledger sql
    let before ← IO.FS.readBinFile ledger
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"journal corruption {name} was accepted: {repr other}"
    match ← Adapter.SQLite.diagnose ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"diagnosis accepted journal corruption {name}: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted journal corruption {name}: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == before)
      s!"journal-corrupt repair attempt changed bytes: {name}"

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
  let repairReceipt ← match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .ok receipt => pure receipt
  | .error error => throw <| IO.userError s!"explicit projection repair failed: {repr error}"
  let _ ← load projectionLedger
  match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .ok retry => expect (retry == repairReceipt) "projection repair retry changed receipt"
  | .error error => throw <| IO.userError s!"exact projection repair retry failed: {repr error}"

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
  let artifactRoot := Adapter.SQLite.artifactRoot ledger
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
    work := ⟨1⟩, key := "GATE-006", revision := ⟨1⟩
    commandProfile := "storage-laws"
    invocation := ".lake/build/bin/storage-laws --fault-matrix"
    repository := "main", snapshot := "commit:test"
    artifactDigest := "proof:test", current := true }
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
    let artifactRoot := Adapter.SQLite.artifactRoot ledger
    Adapter.SQLite.initializeStore ledger
    let initialized ← bootstrap ledger
    let reference ← Adapter.DurableFilesystem.stage artifactRoot s!"artifact-{name}".toUTF8
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"artifact-{name}"⟩
      artifactDigest := reference.digest
      state := .prepared }
    let recorded ← match ← Adapter.SQLite.mutate ledger ⟨s!"record-{name}"⟩
        initialized.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .ok outcome => pure outcome
    | .error error => throw <| IO.userError s!"artifact fixture failed: {repr error}"
    corruptProjection ledger
    let repairPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"artifact repair fixture failed: {repr other}"
    execSql ledger sql
    let corruptBytes ← IO.FS.readBinFile ledger
    match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"artifact table corruption {name} was accepted: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted artifact table corruption {name}: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"unrelated-after-{name}"⟩
        recorded.store.ledger.storedHead Application.Service.bootstrapCommand with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"mutation accepted artifact corruption {name}: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"record-{name}"⟩
        recorded.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"exact retry accepted artifact corruption {name}: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == corruptBytes)
      s!"rejected artifact corruption mutation changed ledger bytes: {name}"

  let raceLedger := root / "artifact-race.sqlite3"
  let raceRoot := Adapter.SQLite.artifactRoot raceLedger
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

def testFilesystemArtifactFaults (root : System.FilePath) : IO Unit := do
  for name in ["deleted", "truncated", "replaced"] do
    let ledger := root / s!"artifact-file-{name}.sqlite3"
    let artifactRoot := Adapter.SQLite.artifactRoot ledger
    Adapter.SQLite.initializeStore ledger
    let initialized ← bootstrap ledger
    let reference ← Adapter.DurableFilesystem.stage artifactRoot
      s!"committed artifact {name}".toUTF8
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"artifact-file-{name}"⟩
      artifactDigest := reference.digest
      state := .prepared }
    let recorded ← match ← Adapter.SQLite.mutate ledger ⟨s!"record-file-{name}"⟩
        initialized.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
      | .ok outcome => pure outcome
      | .error error => throw <| IO.userError s!"file artifact fixture failed: {repr error}"
    corruptProjection ledger
    let repairPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"file artifact repair fixture failed: {repr other}"
    let object := Adapter.DurableFilesystem.objectPath artifactRoot reference
    IO.FS.removeFile object
    if name = "truncated" then IO.FS.writeBinFile object ByteArray.empty
    else if name = "replaced" then IO.FS.writeBinFile object "replacement".toUTF8
    let before ← IO.FS.readBinFile ledger
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"normal inspect accepted {name} artifact: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted {name} file artifact: {repr other}"
    let unrelated : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"unrelated-{name}"⟩, artifactDigest := "proof:unrelated", state := .prepared }
    match ← Adapter.SQLite.mutate ledger ⟨s!"unrelated-{name}"⟩
        recorded.store.ledger.storedHead
        (.recordExternalOperation recorded.store.ledger.storedHead unrelated) with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"unrelated mutation accepted {name} artifact: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"record-file-{name}"⟩
        recorded.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"exact retry accepted {name} artifact: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == before)
      s!"artifact file fault changed ledger bytes: {name}"

  let orphanLedger := root / "artifact-orphan.sqlite3"
  Adapter.SQLite.initializeStore orphanLedger
  let _ ← bootstrap orphanLedger
  let _ ← Adapter.DurableFilesystem.stage (Adapter.SQLite.artifactRoot orphanLedger)
    "unreferenced durable object".toUTF8
  let _ ← load orphanLedger

def makeLegacyV1 (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.transaction (mode := .immediate) do
    db.exec "
      DROP TABLE update_provenance;
      ALTER TABLE events DROP COLUMN operation_id;
      ALTER TABLE operations DROP COLUMN request_payload;
      ALTER TABLE operations DROP COLUMN start_revision;
      ALTER TABLE operations DROP COLUMN end_revision;
      ALTER TABLE operations DROP COLUMN history_digest;
      ALTER TABLE artifacts DROP COLUMN payload;
      ALTER TABLE projection_repairs RENAME TO current_projection_repairs;
      CREATE TABLE projection_repairs (
        observed_digest TEXT NOT NULL,
        head_revision TEXT NOT NULL,
        history_digest TEXT NOT NULL,
        adopted_digest TEXT NOT NULL,
        PRIMARY KEY (observed_digest, head_revision, history_digest)
      );
      INSERT INTO projection_repairs
        (observed_digest, head_revision, history_digest, adopted_digest)
      SELECT observed_digest, head_revision, history_digest, adopted_digest
      FROM current_projection_repairs;
      DROP TABLE current_projection_repairs;
      UPDATE metadata SET schema_version = '1' WHERE singleton = 1;"

def makePredecessorV1 (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.transaction (mode := .immediate) do
    db.exec "
      DROP TABLE update_provenance;
      ALTER TABLE projection_repairs RENAME TO current_projection_repairs;
      CREATE TABLE projection_repairs (
        observed_digest TEXT NOT NULL,
        head_revision TEXT NOT NULL,
        history_digest TEXT NOT NULL,
        adopted_digest TEXT NOT NULL,
        PRIMARY KEY (observed_digest, head_revision, history_digest)
      );
      INSERT INTO projection_repairs
        (observed_digest, head_revision, history_digest, adopted_digest)
      SELECT observed_digest, head_revision, history_digest, adopted_digest
      FROM current_projection_repairs;
      DROP TABLE current_projection_repairs;
      UPDATE metadata SET schema_version = '1' WHERE singleton = 1;"

def makePredecessorV2 (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.transaction (mode := .immediate) do
    db.exec "
      ALTER TABLE projection_repairs RENAME TO current_projection_repairs;
      CREATE TABLE projection_repairs (
        observed_digest TEXT NOT NULL,
        head_revision TEXT NOT NULL,
        history_digest TEXT NOT NULL,
        adopted_digest TEXT NOT NULL,
        PRIMARY KEY (observed_digest, head_revision, history_digest)
      );
      INSERT INTO projection_repairs
        (observed_digest, head_revision, history_digest, adopted_digest)
      SELECT observed_digest, head_revision, history_digest, adopted_digest
      FROM current_projection_repairs;
      DROP TABLE current_projection_repairs;
      UPDATE metadata SET schema_version = '2' WHERE singleton = 1;"

def removeCoordinator (ledger : System.FilePath) : IO System.FilePath := do
  let coordinator := System.FilePath.mk s!"{ledger}.writer.sqlite3"
  if ← coordinator.pathExists then IO.FS.removeFile coordinator
  return coordinator

def testUpdateInspectionReadOnly (root : System.FilePath) : IO Unit := do
  let check (ledger : System.FilePath) : IO Unit := do
    let coordinator ← removeCoordinator ledger
    let beforeBytes ← IO.FS.readBinFile ledger
    let beforeEntries := (← root.readDir).size
    let _ ← Adapter.Update.inspect ledger
    expect (!(← coordinator.pathExists)) "update inspection created a writer coordinator"
    expect ((← IO.FS.readBinFile ledger) == beforeBytes)
      "update inspection changed ledger bytes"
    expect ((← root.readDir).size == beforeEntries)
      "update inspection changed the directory entry set"

  let current := root / "inspect-current.sqlite3"
  Adapter.SQLite.initializeStore current
  check current
  let legacy := root / "inspect-legacy.sqlite3"
  Adapter.SQLite.initializeStore legacy
  makeLegacyV1 legacy
  check legacy
  let unsupported := root / "inspect-unsupported.sqlite3"
  Adapter.SQLite.initializeStore unsupported
  let _ ← bootstrap unsupported
  makeLegacyV1 unsupported
  check unsupported

def testSchemaFingerprintsAndPredecessorMigration (root : System.FilePath) : IO Unit := do
  let predecessor := root / "predecessor-v1.sqlite3"
  let backups := root / "predecessor-v1-backups"
  let artifactRoot := Adapter.SQLite.artifactRoot predecessor
  Adapter.SQLite.initializeStore predecessor
  let initialized ← bootstrap predecessor
  let reference ← Adapter.DurableFilesystem.stage artifactRoot "predecessor artifact".toUTF8
  let attempt : Domain.ExternalOperation.Attempt := {
    operation := ⟨"predecessor-artifact"⟩, artifactDigest := reference.digest, state := .prepared }
  let recorded ← match ← Adapter.SQLite.mutate predecessor ⟨"predecessor-record"⟩
      initialized.store.ledger.storedHead
      (.recordExternalOperation initialized.store.ledger.storedHead attempt)
      [reference] (some artifactRoot) with
    | .ok outcome => pure outcome
    | .error error => throw <| IO.userError s!"predecessor fixture failed: {repr error}"
  makePredecessorV1 predecessor
  let sourceBytes ← IO.FS.readBinFile predecessor
  let plan ← match ← Adapter.Update.inspect predecessor with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"real predecessor v1 was not migratable: {repr other}"
  let receipt ← Adapter.Update.apply predecessor backups plan
  expect ((← load predecessor) == recorded.store)
    "predecessor migration changed replayed authoritative state"
  let restored ← Adapter.Update.restore predecessor backups receipt
  expect (restored.restored == receipt.source) "predecessor restore returned the wrong source"
  expect ((← IO.FS.readBinFile predecessor) == sourceBytes)
    "predecessor restore was not byte-exact"

  let predecessorV2 := root / "predecessor-v2.sqlite3"
  let predecessorV2Backups := root / "predecessor-v2-backups"
  Adapter.SQLite.initializeStore predecessorV2
  let v2Store ← bootstrap predecessorV2
  corruptProjection predecessorV2
  let v2RepairPlan ← match ← Adapter.SQLite.diagnose predecessorV2 with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"predecessor v2 repair fixture failed: {repr other}"
  let v2RepairReceipt ← match ← Adapter.SQLite.repairProjection predecessorV2 v2RepairPlan with
    | .ok receipt => pure receipt
    | .error error => throw <| IO.userError s!"predecessor v2 repair fixture failed: {repr error}"
  makePredecessorV2 predecessorV2
  let v2Plan ← match ← Adapter.Update.inspect predecessorV2 with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"real predecessor v2 was not migratable: {repr other}"
  let _ ← Adapter.Update.apply predecessorV2 predecessorV2Backups v2Plan
  expect ((← load predecessorV2) == v2Store.store)
    "predecessor v2 migration changed authoritative state"
  match ← Adapter.SQLite.repairProjection predecessorV2 v2RepairPlan with
  | .ok retry =>
      expect (retry == v2RepairReceipt)
        "predecessor v2 repair receipt changed across v3 migration"
  | .error error =>
      throw <| IO.userError s!"migrated predecessor v2 repair retry failed: {repr error}"

  let malformedCases := [
    ("extra", "ALTER TABLE events ADD COLUMN unexpected TEXT"),
    ("missing", "DROP TABLE update_provenance"),
    ("forged", "DROP TABLE update_provenance; ALTER TABLE artifacts DROP COLUMN payload"),
    ("constraints", "
      ALTER TABLE events RENAME TO old_events;
      CREATE TABLE events (revision INTEGER, payload BLOB, operation_id TEXT);
      INSERT INTO events SELECT revision, payload, operation_id FROM old_events;
      DROP TABLE old_events;"),
    ("trigger", "CREATE TRIGGER unexpected_trigger BEFORE INSERT ON events
      BEGIN SELECT RAISE(ABORT, 'blocked'); END"),
    ("view", "CREATE VIEW unexpected_view AS SELECT * FROM events"),
    ("index", "CREATE INDEX unexpected_index ON events(operation_id)")]
  for (name, sql) in malformedCases do
    let ledger := root / s!"malformed-v2-{name}.sqlite3"
    Adapter.SQLite.initializeStore ledger
    let healthy ← load ledger
    let repairPlan : Adapter.SQLite.ProjectionRepairPlan := {
      head := {
        ledger := healthy.ledger.id
        revision := healthy.ledger.storedHead
        historyDigest := healthy.ledger.storedHistoryDigest }
      observedDigest := "malformed-schema" }
    execSql ledger sql
    let before ← IO.FS.readBinFile ledger
    match ← Adapter.Update.inspect ledger with
    | .unsupported point =>
        expect (point.schemaVersion == Adapter.SQLite.schemaVersion)
          s!"{name} current identity drifted"
    | other => throw <| IO.userError s!"malformed current schema {name} was accepted: {repr other}"
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"normal inspect accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.inspectWithArtifacts ledger (Adapter.SQLite.artifactRoot ledger) with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"artifact inspect accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.diagnose ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"diagnosis accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"malformed-{name}"⟩ ⟨0⟩
        Application.Service.bootstrapCommand with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"mutation accepted malformed schema {name}: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == before)
      s!"malformed current inspection mutated storage: {name}"

  let predecessorCorruptions := [
    ("constraints", "
      ALTER TABLE events RENAME TO old_events;
      CREATE TABLE events (revision INTEGER, payload BLOB, operation_id TEXT);
      INSERT INTO events SELECT revision, payload, operation_id FROM old_events;
      DROP TABLE old_events;"),
    ("trigger", "CREATE TRIGGER unexpected_trigger BEFORE INSERT ON events
      BEGIN SELECT RAISE(ABORT, 'blocked'); END"),
    ("view", "CREATE VIEW unexpected_view AS SELECT * FROM events"),
    ("index", "CREATE INDEX unexpected_index ON events(operation_id)")]
  for (name, sql) in predecessorCorruptions do
    let malformedPredecessor := root / s!"malformed-predecessor-{name}.sqlite3"
    Adapter.SQLite.initializeStore malformedPredecessor
    makePredecessorV1 malformedPredecessor
    execSql malformedPredecessor sql
    let predecessorBefore ← IO.FS.readBinFile malformedPredecessor
    match ← Adapter.Update.inspect malformedPredecessor with
    | .unsupported point =>
        expect (point.schemaVersion == 1) s!"malformed predecessor identity drifted: {name}"
    | other => throw <| IO.userError s!"malformed predecessor was accepted ({name}): {repr other}"
    expect ((← IO.FS.readBinFile malformedPredecessor) == predecessorBefore)
      s!"malformed predecessor inspection mutated storage: {name}"

def testExplicitUpdateAndRestore (root : System.FilePath) : IO Unit := do
  let ledger := root / "update.sqlite3"
  let backups := root / "backups"
  Adapter.SQLite.initializeStore ledger
  makeLegacyV1 ledger
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
  let forgedLedger := root / "forged-unrelated.sqlite3"
  let unrelatedBackups := root / "forged-unrelated-backups"
  Adapter.SQLite.initializeStore forgedLedger
  makeLegacyV1 forgedLedger
  execSql forgedLedger "INSERT INTO projection_repairs VALUES ('marker','0','marker','marker')"
  let unrelatedPlan ← match ← Adapter.Update.inspect forgedLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"unrelated update fixture failed: {repr other}"
  let unrelatedReceipt ← Adapter.Update.apply forgedLedger unrelatedBackups unrelatedPlan
  let forged : Adapter.Update.Receipt := {
    unrelatedReceipt with target := receipt.target }
  expectFailure (Adapter.Update.restore ledger unrelatedBackups forged)
    "coherently forged restore receipt was accepted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "forged restore receipt changed live storage"
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
  makeLegacyV1 corruptLedger
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

  let unsupportedLedger := root / "legacy-v1-nonempty.sqlite3"
  Adapter.SQLite.initializeStore unsupportedLedger
  let _ ← bootstrap unsupportedLedger
  makeLegacyV1 unsupportedLedger
  let unsupportedBefore ← IO.FS.readBinFile unsupportedLedger
  match ← Adapter.Update.inspect unsupportedLedger with
  | .unsupported point => expect (point.schemaVersion == 1) "legacy schema identity drifted"
  | other => throw <| IO.userError s!"nonempty legacy v1 was not explicit unsupported: {repr other}"
  expect ((← IO.FS.readBinFile unsupportedLedger) == unsupportedBefore)
    "unsupported legacy inspection mutated storage"

def postRenameSyncFailureChild (root : System.FilePath) : IO Unit := do
  let ledger := root / "post-rename-uncertain.sqlite3"
  let backups := root / "post-rename-uncertain-backups"
  Adapter.SQLite.initializeStore ledger
  makeLegacyV1 ledger
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

def updateCrashChild (ledger backups : System.FilePath) : IO Unit := do
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"update crash fixture failed: {repr other}"
  let _ ← Adapter.Update.applyWithHook ledger backups plan (IO.Process.forceExit 86)
  throw <| IO.userError "update replacement crash failpoint returned"

def restoreCrashChild (ledger backups : System.FilePath) (sourceDigest backupDigest : String)
    (backupSize : Nat) (targetDigest : String) : IO Unit := do
  let receipt : Adapter.Update.Receipt := {
    source := { schemaVersion := 1, digest := sourceDigest }
    backup := { digest := backupDigest, size := backupSize }
    target := { schemaVersion := Adapter.SQLite.schemaVersion, digest := targetDigest }
    targetDurability := .uncertain }
  let _ ← Adapter.Update.restoreWithHook ledger backups receipt (IO.Process.forceExit 86)
  throw <| IO.userError "restore replacement crash failpoint returned"

def testReplacementCrashReconciliation (root : System.FilePath) : IO Unit := do
  let ledger := root / "replacement-crash.sqlite3"
  let backups := root / "replacement-crash-backups"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  makePredecessorV1 ledger
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"replacement crash plan failed: {repr other}"
  let applyCrash ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--update-replacement-crash-child", ledger.toString, backups.toString] }
  expect (applyCrash.exitCode == 86) "update replacement crash did not reach failpoint"
  let receipt ← Adapter.Update.apply ledger backups plan
  expect (receipt.targetDurability == .uncertain)
    "post-crash update retry did not recover the durable uncertain receipt"
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "reconciled update target drifted"
  | other => throw <| IO.userError s!"reconciled update is not current: {repr other}"

  let restoreCrash ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--restore-replacement-crash-child", ledger.toString, backups.toString,
      receipt.source.digest, receipt.backup.digest, toString receipt.backup.size,
      receipt.target.digest] }
  expect (restoreCrash.exitCode == 86) "restore replacement crash did not reach failpoint"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source && restored.durability == .uncertain)
    "post-crash restore retry did not recover the durable uncertain receipt"
  expect ((← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile ledger)) =
      receipt.source.digest) "reconciled restore bytes drifted"

partial def awaitWriterBoundary (reached : IO.Ref Bool) (remaining : Nat := 5000) : IO Unit := do
  if ← reached.get then return
  if remaining = 0 then
    throw <| IO.userError "replacement-race writer did not reach the lock boundary"
  IO.sleep 1
  awaitWriterBoundary reached (remaining - 1)

def replacementAttempt (operation : String) : Domain.ExternalOperation.Attempt := {
  operation := ⟨operation⟩
  artifactDigest := s!"proof:{operation}"
  state := .prepared }

def assertMutationRaceOutcome (ledger : System.FilePath)
    (task : Task (Except IO.Error
      (Except Adapter.SQLite.MutationError Adapter.SQLite.MutationOutcome))) : IO Unit := do
  match ← IO.ofExcept task.get with
  | .ok outcome =>
      expect ((← load ledger) == outcome.store)
        "acknowledged replacement-race mutation was absent from live path"
  | .error _ => pure ()

def assertRepairRaceOutcome (ledger : System.FilePath)
    (task : Task (Except IO.Error
      (Except Adapter.SQLite.OpenError Adapter.SQLite.ProjectionRepairReceipt))) : IO Unit := do
  match ← IO.ofExcept task.get with
  | .ok _ =>
      match ← Adapter.SQLite.inspect ledger with
      | .ok _ => pure ()
      | other => throw <| IO.userError s!"acknowledged repair was absent from live path: {repr other}"
  | .error _ => pure ()

def testReplacementWriterRaces (root : System.FilePath) : IO Unit := do
  let updateMutationLedger := root / "update-mutation-race.sqlite3"
  let updateMutationBackups := root / "update-mutation-race-backups"
  Adapter.SQLite.initializeStore updateMutationLedger
  let updateMutationStore ← bootstrap updateMutationLedger
  makePredecessorV2 updateMutationLedger
  let updateMutationPlan ← match ← Adapter.Update.inspect updateMutationLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"update/mutation race plan failed: {repr other}"
  let updateMutationReached ← IO.mkRef false
  let updateMutationPending ← IO.mkRef (none : Option
    (Task (Except IO.Error (Except Adapter.SQLite.MutationError Adapter.SQLite.MutationOutcome))))
  let updateMutationName := "writer-after-update"
  let _ ← Adapter.Update.applyWithLockHook updateMutationLedger updateMutationBackups
    updateMutationPlan (do
      let task ← IO.asTask <| Adapter.SQLite.mutateWithLockHook updateMutationLedger
        ⟨updateMutationName⟩ updateMutationStore.store.ledger.storedHead
        (.recordExternalOperation updateMutationStore.store.ledger.storedHead
          (replacementAttempt updateMutationName)) [] none (updateMutationReached.set true)
      updateMutationPending.set (some task)
      awaitWriterBoundary updateMutationReached) (pure ())
  let updateMutationTask ← match ← updateMutationPending.get with
    | some task => pure task
    | none => throw <| IO.userError "update/mutation race task was not started"
  assertMutationRaceOutcome updateMutationLedger updateMutationTask

  let restoreMutationLedger := root / "restore-mutation-race.sqlite3"
  let restoreMutationBackups := root / "restore-mutation-race-backups"
  Adapter.SQLite.initializeStore restoreMutationLedger
  let restoreMutationStore ← bootstrap restoreMutationLedger
  makePredecessorV2 restoreMutationLedger
  let restoreMutationPlan ← match ← Adapter.Update.inspect restoreMutationLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"restore/mutation race plan failed: {repr other}"
  let restoreMutationReceipt ← Adapter.Update.apply restoreMutationLedger
    restoreMutationBackups restoreMutationPlan
  let restoreMutationReached ← IO.mkRef false
  let restoreMutationPending ← IO.mkRef (none : Option
    (Task (Except IO.Error (Except Adapter.SQLite.MutationError Adapter.SQLite.MutationOutcome))))
  let restoreMutationName := "writer-after-restore"
  let _ ← Adapter.Update.restoreWithLockHook restoreMutationLedger restoreMutationBackups
    restoreMutationReceipt (do
      let task ← IO.asTask <| Adapter.SQLite.mutateWithLockHook restoreMutationLedger
        ⟨restoreMutationName⟩ restoreMutationStore.store.ledger.storedHead
        (.recordExternalOperation restoreMutationStore.store.ledger.storedHead
          (replacementAttempt restoreMutationName)) [] none (restoreMutationReached.set true)
      restoreMutationPending.set (some task)
      awaitWriterBoundary restoreMutationReached) (pure ())
  let restoreMutationTask ← match ← restoreMutationPending.get with
    | some task => pure task
    | none => throw <| IO.userError "restore/mutation race task was not started"
  assertMutationRaceOutcome restoreMutationLedger restoreMutationTask

  let updateRepairLedger := root / "update-repair-race.sqlite3"
  let updateRepairBackups := root / "update-repair-race-backups"
  Adapter.SQLite.initializeStore updateRepairLedger
  let _ ← bootstrap updateRepairLedger
  corruptProjection updateRepairLedger
  let updateRepairPlan ← match ← Adapter.SQLite.diagnose updateRepairLedger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"update/repair fixture failed: {repr other}"
  match ← Adapter.SQLite.repairProjection updateRepairLedger updateRepairPlan with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"update/repair fixture repair failed: {repr error}"
  makePredecessorV2 updateRepairLedger
  let updatePlan ← match ← Adapter.Update.inspect updateRepairLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"update/repair race plan failed: {repr other}"
  let updateRepairReached ← IO.mkRef false
  let updateRepairPending ← IO.mkRef (none : Option
    (Task (Except IO.Error
      (Except Adapter.SQLite.OpenError Adapter.SQLite.ProjectionRepairReceipt))))
  let _ ← Adapter.Update.applyWithLockHook updateRepairLedger updateRepairBackups updatePlan (do
    let task ← IO.asTask <| Adapter.SQLite.repairProjectionWithLockHook updateRepairLedger
      updateRepairPlan (updateRepairReached.set true) (pure ())
    updateRepairPending.set (some task)
    awaitWriterBoundary updateRepairReached) (pure ())
  let updateRepairTask ← match ← updateRepairPending.get with
    | some task => pure task
    | none => throw <| IO.userError "update/repair race task was not started"
  assertRepairRaceOutcome updateRepairLedger updateRepairTask

  let restoreRepairLedger := root / "restore-repair-race.sqlite3"
  let restoreRepairBackups := root / "restore-repair-race-backups"
  Adapter.SQLite.initializeStore restoreRepairLedger
  let _ ← bootstrap restoreRepairLedger
  corruptProjection restoreRepairLedger
  let restoreRepairPlan ← match ← Adapter.SQLite.diagnose restoreRepairLedger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"restore/repair fixture failed: {repr other}"
  match ← Adapter.SQLite.repairProjection restoreRepairLedger restoreRepairPlan with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"restore/repair fixture repair failed: {repr error}"
  makePredecessorV2 restoreRepairLedger
  let restoreRepairUpdatePlan ← match ← Adapter.Update.inspect restoreRepairLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"restore/repair update plan failed: {repr other}"
  let restoreRepairReceipt ← Adapter.Update.apply restoreRepairLedger restoreRepairBackups
    restoreRepairUpdatePlan
  let restoreRepairReached ← IO.mkRef false
  let restoreRepairPending ← IO.mkRef (none : Option
    (Task (Except IO.Error
      (Except Adapter.SQLite.OpenError Adapter.SQLite.ProjectionRepairReceipt))))
  let _ ← Adapter.Update.restoreWithLockHook restoreRepairLedger restoreRepairBackups
    restoreRepairReceipt (do
      let task ← IO.asTask <| Adapter.SQLite.repairProjectionWithLockHook restoreRepairLedger
        restoreRepairPlan (restoreRepairReached.set true) (pure ())
      restoreRepairPending.set (some task)
      awaitWriterBoundary restoreRepairReached) (pure ())
  let restoreRepairTask ← match ← restoreRepairPending.get with
    | some task => pure task
    | none => throw <| IO.userError "restore/repair race task was not started"
  assertRepairRaceOutcome restoreRepairLedger restoreRepairTask

def repairCrashChild (ledger : System.FilePath) (ledgerId : String) (revision : Nat)
    (historyDigest observedDigest : String) : IO Unit := do
  let plan : Adapter.SQLite.ProjectionRepairPlan := {
    head := {
      ledger := ⟨ledgerId⟩
      revision := ⟨revision⟩
      historyDigest := ⟨historyDigest⟩ }
    observedDigest }
  let _ ← Adapter.SQLite.repairProjectionWithHook ledger plan (IO.Process.forceExit 86)
  throw <| IO.userError "projection repair crash failpoint returned"

def testProjectionRepairCrashRetry (root : System.FilePath) : IO Unit := do
  let ledger := root / "projection-repair-crash.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  corruptProjection ledger
  let plan ← match ← Adapter.SQLite.diagnose ledger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"repair crash plan unavailable: {repr other}"
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--repair-crash-child", ledger.toString, plan.head.ledger.value,
      toString plan.head.revision.value, plan.head.historyDigest.value, plan.observedDigest] }
  expect (result.exitCode == 86) "projection repair crash did not reach failpoint"
  let recovered ← match ← Adapter.SQLite.repairProjection ledger plan with
    | .ok receipt => pure receipt
    | .error error => throw <| IO.userError s!"projection repair retry failed: {repr error}"
  match ← Adapter.SQLite.repairProjection ledger plan with
  | .ok receipt => expect (receipt == recovered) "projection repair canonical receipt drifted"
  | .error error => throw <| IO.userError s!"second projection repair retry failed: {repr error}"
  let changed := { plan with observedDigest := plan.observedDigest ++ "-changed" }
  match ← Adapter.SQLite.repairProjection ledger changed with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"changed projection repair plan was accepted: {repr other}"
  let changedLedger := { plan with head := { plan.head with ledger := ⟨"different-ledger"⟩ } }
  match ← Adapter.SQLite.repairProjection ledger changedLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"changed-ledger projection repair was accepted: {repr other}"
  let unseen := { plan with head := { plan.head with revision := plan.head.revision.next } }
  match ← Adapter.SQLite.repairProjection ledger unseen with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"unseen stale projection repair was accepted: {repr other}"

  let ledgerB := root / "projection-repair-other-ledger.sqlite3"
  Adapter.SQLite.initializeStore ledgerB
  let _ ← bootstrap ledgerB
  corruptProjection ledgerB
  let planB ← match ← Adapter.SQLite.diagnose ledgerB with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"other-ledger repair plan unavailable: {repr other}"
  execSql ledgerB s!"
    ATTACH DATABASE '{ledger}' AS source;
    INSERT INTO projection_repairs SELECT * FROM source.projection_repairs;
    DETACH DATABASE source;"
  let receiptB ← match ← Adapter.SQLite.repairProjection ledgerB planB with
    | .ok receipt => pure receipt
    | .error error => throw <| IO.userError s!"other-ledger repair rejected: {repr error}"
  expect (receiptB.plan.head.ledger == planB.head.ledger &&
      receiptB.plan.head.ledger != recovered.plan.head.ledger)
    "repair receipt crossed ledger identities"
  match ← Adapter.SQLite.repairProjection ledgerB planB with
  | .ok retry => expect (retry == receiptB) "other-ledger exact retry drifted"
  | .error error => throw <| IO.userError s!"other-ledger exact retry failed: {repr error}"

def main (args : List String) : IO Unit :=
  match args with
  | ["--crash-child", ledger] => crashChild ledger
  | ["--post-rename-sync-failure-child", root] => postRenameSyncFailureChild root
  | ["--update-replacement-crash-child", ledger, backups] =>
      updateCrashChild ledger backups
  | ["--restore-replacement-crash-child", ledger, backups, sourceDigest,
      backupDigest, backupSize, targetDigest] =>
      match backupSize.toNat? with
      | some size => restoreCrashChild ledger backups sourceDigest backupDigest size targetDigest
      | none => throw <| IO.userError "invalid restore crash backup size"
  | ["--repair-crash-child", ledger, ledgerId, revision, historyDigest, observedDigest] =>
      match revision.toNat? with
      | some value => repairCrashChild ledger ledgerId value historyDigest observedDigest
      | none => throw <| IO.userError "invalid repair crash revision"
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
    testFilesystemArtifactFaults root
    testUpdateInspectionReadOnly root
    testSchemaFingerprintsAndPredecessorMigration root
    testExplicitUpdateAndRestore root
    testPostRenameSyncFailure root
    testReplacementCrashReconciliation root
    testReplacementWriterRaces root
    testProjectionRepairCrashRetry root
    IO.println "storage laws: pass"
  | _ => throw <| IO.userError "unsupported storage-laws arguments"

end AgentWorkbench.Tests.StorageLaws

def main (args : List String) : IO Unit :=
  AgentWorkbench.Tests.StorageLaws.main args
