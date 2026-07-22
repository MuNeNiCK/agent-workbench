import AgentWorkbench.Adapter.Codec
import AgentWorkbench.Adapter.DurableFilesystem
import SQLite

namespace AgentWorkbench.Adapter.SQLite

open AgentWorkbench.Domain
open AgentWorkbench.Kernel
open SQLite.Blob

def schemaVersion : Nat := 1

private def writerPath (path : System.FilePath) : System.FilePath :=
  System.FilePath.mk s!"{path}.writer.sqlite3"

def withWriterLock (path : System.FilePath) (action : IO α) : IO α := do
  let coordinator ← _root_.SQLite.openWith (writerPath path)
    { mode := .readWriteCreate, threading := some .fullmutex } (busyTimeoutMs := 5000)
  coordinator.exec "
    PRAGMA journal_mode=DELETE;
    PRAGMA synchronous=FULL;
    CREATE TABLE IF NOT EXISTS writer_lock (singleton INTEGER PRIMARY KEY CHECK (singleton = 1));
    INSERT OR IGNORE INTO writer_lock VALUES (1);"
  coordinator.transaction action (mode := .immediate)

inductive OpenError
  | uninitialized
  | unsupportedSchema (found supported : Nat)
  | corrupt (reason : String)
deriving DecidableEq, Repr

inductive MutationError
  | openError (error : OpenError)
  | operationConflict
  | artifactInvalid (digest : String)
  | staleRevision
  | rejected (error : DomainError)
deriving DecidableEq, Repr

structure MutationOutcome where
  receipt : Policy.Update.Receipt
  store : Projection.Store
  exactRetry : Bool
deriving Repr

private def parseNat (field value : String) : Except OpenError Nat :=
  match value.toNat? with
  | some n => .ok n
  | none => .error (.corrupt s!"{field} is not a natural number")

private def decode [FromBinary α] (what : String) (bytes : ByteArray) : Except OpenError α :=
  match fromBinary bytes with
  | .ok value => .ok value
  | .error reason => .error (.corrupt s!"cannot decode {what}: {reason}")

private def throwOpen {α : Type} : OpenError → IO α := fun error =>
  throw <| IO.userError s!"{repr error}"

private def createSchema (db : _root_.SQLite) : IO Unit := do
  db.exec "
    CREATE TABLE metadata (
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
      schema_version TEXT NOT NULL,
      ledger_id TEXT NOT NULL,
      head_revision TEXT NOT NULL,
      history_digest TEXT NOT NULL
    );
    CREATE TABLE events (
      revision INTEGER PRIMARY KEY CHECK (revision > 0),
      payload BLOB NOT NULL
    );
    CREATE TABLE projection (
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
      revision TEXT NOT NULL,
      history_digest TEXT NOT NULL,
      state_digest TEXT NOT NULL,
      payload BLOB NOT NULL
    );
    CREATE TABLE operations (
      operation_id TEXT PRIMARY KEY,
      payload_digest TEXT NOT NULL,
      result_digest TEXT NOT NULL,
      receipt BLOB NOT NULL
    );
    CREATE TABLE artifacts (
      digest TEXT PRIMARY KEY,
      size TEXT NOT NULL
    );
    CREATE TABLE projection_repairs (
      observed_digest TEXT NOT NULL,
      head_revision TEXT NOT NULL,
      history_digest TEXT NOT NULL,
      adopted_digest TEXT NOT NULL,
      PRIMARY KEY (observed_digest, head_revision, history_digest)
    );"

private def writeInitialRows (db : _root_.SQLite) : IO Unit := do
  let store := Projection.initialStore
  let metadata ← db.prepare
    "INSERT INTO metadata VALUES (1, ?, ?, ?, ?)"
  metadata.bindText 1 (toString schemaVersion)
  metadata.bindText 2 store.ledger.id.value
  metadata.bindText 3 (toString store.ledger.storedHead.value)
  metadata.bindText 4 store.ledger.storedHistoryDigest.value
  metadata.exec
  let active ← match store.active with
    | some projection => pure projection
    | none => throw <| IO.userError "initial projection is missing"
  let projection ← db.prepare
    "INSERT INTO projection VALUES (1, ?, ?, ?, ?)"
  projection.bindText 1 (toString active.reference.revision.value)
  projection.bindText 2 active.reference.historyDigest.value
  projection.bindText 3 active.reference.stateDigest.value
  projection.bindBlob 4 (toBinary active)
  projection.exec

def initializeStore (path : System.FilePath) : IO Unit := do
  if ← path.pathExists then
    throw <| IO.userError "ledger already exists"
  let db ← _root_.SQLite.openWith path
    { mode := .readWriteCreate, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.exec "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;"
  db.transaction (mode := .immediate) do
    createSchema db
    writeInitialRows db

private def readMetadata (db : _root_.SQLite) : IO (Except OpenError (LedgerId × Revision × Digest)) := do
  let statement ← db.prepare
    "SELECT schema_version, ledger_id, head_revision, history_digest FROM metadata WHERE singleton = 1"
  unless ← statement.step do return .error .uninitialized
  let versionText ← statement.columnText 0
  let ledgerId ← statement.columnText 1
  let revisionText ← statement.columnText 2
  let digest ← statement.columnText 3
  let version ← match parseNat "schema version" versionText with
    | .ok value => pure value
    | .error error => return .error error
  if version != schemaVersion then return .error (.unsupportedSchema version schemaVersion)
  let revision ← match parseNat "head revision" revisionText with
    | .ok value => pure value
    | .error error => return .error error
  return .ok (⟨ledgerId⟩, ⟨revision⟩, ⟨digest⟩)

private def readEvents (db : _root_.SQLite) : IO (Except OpenError (List Replay.Event)) := do
  let statement ← db.prepare "SELECT revision, payload FROM events ORDER BY revision"
  let mut events := #[]
  let mut expected := 1
  while ← statement.step do
    let revision := (← statement.columnInt64 0).toInt.toNat
    if revision != expected then
      return .error (.corrupt s!"event revision gap at {expected}")
    let event ← match decode "event" (← statement.columnBlob 1) with
      | .ok value => pure value
      | .error error => return .error error
    events := events.push event
    expected := expected + 1
  return .ok events.toList

private def readProjection (db : _root_.SQLite) :
    IO (Except OpenError Projection.ProjectionObservation) := do
  let statement ← db.prepare
    "SELECT revision, history_digest, state_digest, payload FROM projection WHERE singleton = 1"
  unless ← statement.step do return .error (.corrupt "projection is missing")
  let revisionText ← statement.columnText 0
  let historyDigest ← statement.columnText 1
  let stateDigest ← statement.columnText 2
  let projection ← match decode (α := Projection.ProjectionObservation)
      "projection" (← statement.columnBlob 3) with
    | .ok value => pure value
    | .error error => return .error error
  let revision ← match parseNat "projection revision" revisionText with
    | .ok value => pure value
    | .error error => return .error error
  unless projection.reference.revision.value = revision &&
      projection.reference.historyDigest.value = historyDigest &&
      projection.reference.stateDigest.value = stateDigest do
    return .error (.corrupt "projection columns disagree with payload")
  return .ok projection

private def loadFrom (db : _root_.SQLite) : IO (Except OpenError Projection.Store) := do
  let metadata ← readMetadata db
  let (ledgerId, headRevision, historyDigest) ← match metadata with
    | .ok value => pure value
    | .error error => return .error error
  let events ← match ← readEvents db with
    | .ok value => pure value
    | .error error => return .error error
  let projection ← match ← readProjection db with
    | .ok value => pure value
    | .error error => return .error error
  let store : Projection.Store := {
    ledger := { id := ledgerId, events, storedHead := headRevision, storedHistoryDigest := historyDigest }
    active := some projection
    staged := []
    receipts := []
    nextStage := ⟨1⟩ }
  match Projection.inspect store with
  | .fresh _ _ => return .ok store
  | inspection => return .error (.corrupt s!"integrity inspection failed: {inspection.describe}")

def inspect (path : System.FilePath) : IO (Except OpenError Projection.Store) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  loadFrom db

private def readArtifactRefs (db : _root_.SQLite) :
    IO (Except OpenError (List DurableFilesystem.ArtifactRef)) := do
  let statement ← db.prepare "SELECT digest, size FROM artifacts ORDER BY digest"
  let mut references := []
  while ← statement.step do
    let digest ← statement.columnText 0
    let size ← match parseNat "artifact size" (← statement.columnText 1) with
      | .ok value => pure value
      | .error error => return .error error
    references := { digest, size } :: references
  return .ok references.reverse

def inspectWithArtifacts (path artifactRoot : System.FilePath) :
    IO (Except OpenError Projection.Store) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  let store ← match ← loadFrom db with
    | .ok value => pure value
    | .error error => return .error error
  let references ← match ← readArtifactRefs db with
    | .ok value => pure value
    | .error error => return .error error
  let reconciliation ← DurableFilesystem.reconcile artifactRoot references
  unless reconciliation.missing.isEmpty && reconciliation.mismatched.isEmpty do
    return .error (.corrupt s!"artifact reconciliation failed: {repr reconciliation}")
  return .ok store

structure ProjectionRepairPlan where
  head : Domain.Projection.LedgerPoint
  observedDigest : String
deriving DecidableEq, Repr

structure ProjectionRepairReceipt where
  plan : ProjectionRepairPlan
  adoptedDigest : String
deriving DecidableEq, Repr

inductive Diagnosis
  | healthy (store : Projection.Store)
  | projectionRepairRequired (plan : ProjectionRepairPlan)
deriving Repr

private def readLedger (db : _root_.SQLite) : IO (Except OpenError Replay.VerifiedLedger) := do
  let metadata ← readMetadata db
  let (ledgerId, headRevision, historyDigest) ← match metadata with
    | .ok value => pure value
    | .error error => return .error error
  let events ← match ← readEvents db with
    | .ok value => pure value
    | .error error => return .error error
  let image : Replay.LedgerImage := {
    id := ledgerId, events, storedHead := headRevision, storedHistoryDigest := historyDigest }
  match Replay.verifyLedger image with
  | .ok ledger => return .ok ledger
  | .error fault => return .error (.corrupt s!"ledger integrity failed: {repr fault}")

private def readProjectionPayload? (db : _root_.SQLite) : IO (Option ByteArray) := do
  let statement ← db.prepare "SELECT payload FROM projection WHERE singleton = 1"
  unless ← statement.step do return none
  return some (← statement.columnBlob 0)

private def diagnoseFrom (db : _root_.SQLite) : IO (Except OpenError Diagnosis) := do
  let ledger ← match ← readLedger db with
    | .ok value => pure value
    | .error error => return .error error
  let payload? ← readProjectionPayload? db
  let observedDigest ← match payload? with
    | none => pure "missing"
    | some payload => DurableFilesystem.digest payload
  let plan : ProjectionRepairPlan := { head := ledger.point, observedDigest }
  let projection? : Option Projection.ProjectionObservation := match payload? with
    | none => none
    | some payload =>
        match fromBinary payload with
        | .ok projection => some projection
        | .error _ => none
  let store : Projection.Store := {
    ledger := ledger.image
    active := projection?
    staged := []
    receipts := []
    nextStage := ⟨1⟩ }
  match Projection.inspect store with
  | .fresh _ _ => return .ok (.healthy store)
  | .ledgerCorrupt fault => return .error (.corrupt s!"ledger integrity failed: {repr fault}")
  | _ => return .ok (.projectionRepairRequired plan)

def diagnose (path : System.FilePath) : IO (Except OpenError Diagnosis) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  diagnoseFrom db

def repairProjection (path : System.FilePath) (plan : ProjectionRepairPlan) :
    IO (Except OpenError ProjectionRepairReceipt) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readWrite, threading := some .fullmutex } (busyTimeoutMs := 5000)
  withWriterLock path <| db.transaction (mode := .immediate) do
    let diagnosis ← match ← diagnoseFrom db with
      | .ok value => pure value
      | .error error => return .error error
    let current ← match diagnosis with
      | .projectionRepairRequired value => pure value
      | .healthy _ => return .error (.corrupt "projection repair is no longer required")
    unless current = plan do
      return .error (.corrupt "projection repair plan is stale")
    let ledger ← match ← readLedger db with
      | .ok value => pure value
      | .error error => return .error error
    let projection := Application.Service.projectionFor ledger.image ledger.head.state
    let statement ← db.prepare "
      INSERT INTO projection (singleton, revision, history_digest, state_digest, payload)
      VALUES (1, ?, ?, ?, ?)
      ON CONFLICT(singleton) DO UPDATE SET
        revision=excluded.revision,
        history_digest=excluded.history_digest,
        state_digest=excluded.state_digest,
        payload=excluded.payload"
    statement.bindText 1 (toString projection.reference.revision.value)
    statement.bindText 2 projection.reference.historyDigest.value
    statement.bindText 3 projection.reference.stateDigest.value
    statement.bindBlob 4 (toBinary projection)
    statement.exec
    let adoptedDigest ← DurableFilesystem.digest (toBinary projection)
    let receiptStatement ← db.prepare "
      INSERT INTO projection_repairs
        (observed_digest, head_revision, history_digest, adopted_digest)
      VALUES (?, ?, ?, ?)"
    receiptStatement.bindText 1 plan.observedDigest
    receiptStatement.bindText 2 (toString plan.head.revision.value)
    receiptStatement.bindText 3 plan.head.historyDigest.value
    receiptStatement.bindText 4 adoptedDigest
    receiptStatement.exec
    return .ok { plan, adoptedDigest }

private def lookupReceipt (db : _root_.SQLite) (operation : OperationId) :
    IO (Option (String × Policy.Update.Receipt)) := do
  let statement ← db.prepare
    "SELECT payload_digest, receipt FROM operations WHERE operation_id = ?"
  statement.bindText 1 operation.value
  unless ← statement.step do return none
  let payload ← statement.columnText 0
  match fromBinary (← statement.columnBlob 1) with
  | .ok receipt => return some (payload, receipt)
  | .error reason => throw <| IO.userError s!"cannot decode operation receipt: {reason}"

private def appendEvents (db : _root_.SQLite) (start : Nat)
    (events : List Replay.Event) : IO Unit := do
  let statement ← db.prepare "INSERT INTO events (revision, payload) VALUES (?, ?)"
  for (event, offset) in events.zipIdx do
    statement.bindInt64 1 (Int.ofNat (start + offset + 1)).toInt64
    statement.bindBlob 2 (toBinary event)
    statement.exec
    statement.reset
    statement.clearBindings

private def writeHead (db : _root_.SQLite) (store : Projection.Store) : IO Unit := do
  let metadata ← db.prepare
    "UPDATE metadata SET head_revision = ?, history_digest = ? WHERE singleton = 1"
  metadata.bindText 1 (toString store.ledger.storedHead.value)
  metadata.bindText 2 store.ledger.storedHistoryDigest.value
  metadata.exec
  let active ← match store.active with
    | some projection => pure projection
    | none => throw <| IO.userError "accepted transaction produced no projection"
  let projection ← db.prepare "
    UPDATE projection SET revision = ?, history_digest = ?, state_digest = ?, payload = ?
    WHERE singleton = 1"
  projection.bindText 1 (toString active.reference.revision.value)
  projection.bindText 2 active.reference.historyDigest.value
  projection.bindText 3 active.reference.stateDigest.value
  projection.bindBlob 4 (toBinary active)
  projection.exec

private def writeReceipt (db : _root_.SQLite) (receipt : Policy.Update.Receipt) : IO Unit := do
  let statement ← db.prepare "
    INSERT INTO operations (operation_id, payload_digest, result_digest, receipt)
    VALUES (?, ?, ?, ?)"
  statement.bindText 1 receipt.operation.value
  statement.bindText 2 receipt.payloadDigest
  statement.bindText 3 receipt.resultDigest
  statement.bindBlob 4 (toBinary receipt)
  statement.exec

private def writeArtifactRefs (db : _root_.SQLite)
    (references : List DurableFilesystem.ArtifactRef) : IO Unit := do
  let statement ← db.prepare
    "INSERT OR IGNORE INTO artifacts (digest, size) VALUES (?, ?)"
  for reference in references do
    statement.bindText 1 reference.digest
    statement.bindText 2 (toString reference.size)
    statement.exec
    statement.reset
    statement.clearBindings

private def eventArtifactDigest? : Replay.Event → Option String
  | .validationPassed _ _ digest => some digest
  | .evidenceRecorded evidence => some evidence.artifactDigest
  | .externalOperationRecorded attempt => some attempt.artifactDigest
  | _ => none

private def requiredFileArtifacts (events : List Replay.Event) : List String :=
  (events.filterMap eventArtifactDigest?).filter (·.startsWith "sha3-256:") |>.eraseDups

def mutate (path : System.FilePath) (operation : OperationId) (payloadDigest : String)
    (expectedRevision : Revision) (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef := [])
    (artifactRoot : Option System.FilePath := none) :
    IO (Except MutationError MutationOutcome) := do
  if !(← path.pathExists) then return .error (.openError .uninitialized)
  if !artifacts.isEmpty then
    let root ← match artifactRoot with
      | some value => pure value
      | none => return .error (.artifactInvalid "artifact root is required")
    for reference in artifacts do
      match ← DurableFilesystem.verify root reference with
      | .valid => pure ()
      | _ => return .error (.artifactInvalid reference.digest)
  let db ← _root_.SQLite.openWith path
    { mode := .readWrite, threading := some .fullmutex } (busyTimeoutMs := 5000)
  withWriterLock path <| db.transaction (mode := .immediate) do
    match ← lookupReceipt db operation with
    | some (storedPayload, receipt) =>
        if storedPayload = payloadDigest then
          match ← loadFrom db with
          | .ok store => return .ok { receipt, store, exactRetry := true }
          | .error error => return .error (.openError error)
        else
          return .error .operationConflict
    | none =>
        let store ← match ← loadFrom db with
          | .ok value => pure value
          | .error error => return .error (.openError error)
        if store.ledger.storedHead != expectedRevision then
          return .error .staleRevision
        let transaction ← match Application.Service.execute command store with
          | .ok value => pure value
          | .error error => return .error (.rejected error)
        let requiredArtifacts := requiredFileArtifacts transaction.accepted.events
        unless requiredArtifacts.all (fun digest => artifacts.any (·.digest = digest)) &&
            artifacts.all (fun reference => requiredArtifacts.contains reference.digest) do
          return .error (.artifactInvalid "event/artifact binding mismatch")
        appendEvents db store.ledger.events.length transaction.accepted.events
        writeHead db transaction.result
        writeArtifactRefs db artifacts
        let receipt : Policy.Update.Receipt := {
          operation
          payloadDigest
          resultDigest := Replay.stateDigest transaction.accepted.result.state |>.value }
        writeReceipt db receipt
        return .ok { receipt, store := transaction.result, exactRetry := false }

end AgentWorkbench.Adapter.SQLite
