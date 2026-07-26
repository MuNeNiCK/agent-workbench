import AgentWorkbench.Adapter.Codec
import AgentWorkbench.Adapter.DurableFilesystem
import AgentWorkbench.Adapter.LegacyV5
import SQLite

namespace AgentWorkbench.Adapter.SQLite

open AgentWorkbench.Domain
open AgentWorkbench.Kernel
open SQLite.Blob

def schemaVersion : Nat := 6
def legacyV5SchemaVersion : Nat := 5
def predecessorSchemaVersion : Nat := 2

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

private structure CanonicalRequest where
  command : Decide.Command
  artifacts : List (String × Nat)
deriving DecidableEq, Repr

deriving instance ToBinary, FromBinary for CanonicalRequest

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
      payload BLOB NOT NULL,
      operation_id TEXT NOT NULL
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
      request_payload BLOB NOT NULL,
      payload_digest TEXT NOT NULL,
      result_digest TEXT NOT NULL,
      start_revision TEXT NOT NULL,
      end_revision TEXT NOT NULL,
      history_digest TEXT NOT NULL,
      receipt BLOB NOT NULL
    );
    CREATE TABLE artifacts (
      digest TEXT PRIMARY KEY,
      size TEXT NOT NULL,
      payload BLOB NOT NULL
    );
    CREATE TABLE projection_repairs (
      ledger_id TEXT NOT NULL,
      observed_digest TEXT NOT NULL,
      head_revision TEXT NOT NULL,
      history_digest TEXT NOT NULL,
      adopted_digest TEXT NOT NULL,
      PRIMARY KEY (ledger_id, observed_digest, head_revision, history_digest)
    );
    CREATE TABLE update_provenance (
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
      source_schema TEXT NOT NULL,
      source_digest TEXT NOT NULL,
      backup_digest TEXT NOT NULL,
      backup_size TEXT NOT NULL
    );"

private def writeInitialRows (db : _root_.SQLite) (ledgerIdentity : String) : IO Unit := do
  let initial := Projection.initialStore
  let store : Projection.Store := {
    initial with ledger := { initial.ledger with id := ⟨ledgerIdentity⟩ } }
  let metadata ← db.prepare
    "INSERT INTO metadata VALUES (1, ?, ?, ?, ?)"
  metadata.bindText 1 (toString schemaVersion)
  metadata.bindText 2 store.ledger.id.value
  metadata.bindText 3 (toString store.ledger.storedHead.value)
  metadata.bindText 4 store.ledger.storedHistoryDigest.value
  metadata.exec
  let verified ← match Replay.verifyLedger store.ledger with
    | .ok ledger => pure ledger
    | .error fault => throw <| IO.userError s!"initial ledger is invalid: {repr fault}"
  let active := Application.Service.projectionFor store.ledger verified.head.state
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
    writeInitialRows db path.toString
  withWriterLock path (pure ())

private def readMetadataAt (db : _root_.SQLite) (expectedSchema : Nat) :
    IO (Except OpenError (LedgerId × Revision × Digest)) := do
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
  if version != expectedSchema then return .error (.unsupportedSchema version expectedSchema)
  let revision ← match parseNat "head revision" revisionText with
    | .ok value => pure value
    | .error error => return .error error
  return .ok (⟨ledgerId⟩, ⟨revision⟩, ⟨digest⟩)

private def readMetadata (db : _root_.SQLite) : IO (Except OpenError (LedgerId × Revision × Digest)) :=
  readMetadataAt db schemaVersion

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

private def verifySQLiteIntegrity (db : _root_.SQLite) : IO (Except OpenError Unit) := do
  let statement ← db.prepare "PRAGMA quick_check"
  unless ← statement.step do return .error (.corrupt "SQLite integrity check returned no result")
  let result ← statement.columnText 0
  if result = "ok" then return .ok ()
  return .error (.corrupt s!"SQLite integrity check failed: {result}")

private def storedTransactionMatches (command : Decide.Command)
    (prior result : Replay.State) (events : List Replay.Event) : Bool :=
  match Decide.decide command prior with
  | .ok decided => decided.events == events && decided.result.state == result
  | .error _ =>
      command.expectedRevision == prior.revision &&
        match command, events with
        | .recordExternalOperation _ attempt,
            [.externalOperationRecorded recorded] =>
            attempt.target == .unresolved && recorded == attempt &&
              attempt.state == .prepared && attempt.wellFormed &&
              attempt.work.all (fun work =>
                prior.work.any fun unit =>
                  unit.id == work && unit.status == .open) &&
              !prior.externalOperations.any (·.operation == attempt.operation)
        | .advanceExternalOperation _ attempt,
            [.externalOperationAdvanced advanced] =>
            attempt.target == .unresolved && advanced == attempt &&
              prior.externalOperations.any (fun current =>
                current.operation == attempt.operation &&
                  Domain.ExternalOperation.transitionAllowed current attempt)
        | _, _ => false

private def validateOperationJournal (db : _root_.SQLite)
    (ledger : Replay.VerifiedLedger) : IO (Except OpenError Unit) := do
  let eventRows ← db.prepare "SELECT revision, operation_id FROM events ORDER BY revision"
  let mut eventOperations : Array String := #[]
  while ← eventRows.step do
    eventOperations := eventOperations.push (← eventRows.columnText 1)
  let statement ← db.prepare "
    SELECT operation_id, request_payload, payload_digest, result_digest, start_revision,
           end_revision, history_digest, receipt
    FROM operations ORDER BY CAST(start_revision AS INTEGER), operation_id"
  let mut expectedStart := 0
  while ← statement.step do
    let operationId ← statement.columnText 0
    let requestPayload ← statement.columnBlob 1
    let payloadDigest ← statement.columnText 2
    let resultDigest ← statement.columnText 3
    let startRevision ← match parseNat "operation start revision" (← statement.columnText 4) with
      | .ok value => pure value
      | .error error => return .error error
    let endRevision ← match parseNat "operation end revision" (← statement.columnText 5) with
      | .ok value => pure value
      | .error error => return .error error
    let historyDigest ← statement.columnText 6
    let receipt ← match decode (α := Policy.Update.Receipt)
        "operation receipt" (← statement.columnBlob 7) with
      | .ok value => pure value
      | .error error => return .error error
    unless startRevision = expectedStart && startRevision < endRevision &&
        endRevision ≤ ledger.image.events.length do
      return .error (.corrupt s!"operation journal range is not contiguous at {operationId}")
    unless receipt.operation.value = operationId && receipt.payloadDigest = payloadDigest &&
        receipt.resultDigest = resultDigest do
      return .error (.corrupt s!"operation journal columns disagree with receipt at {operationId}")
    unless (← DurableFilesystem.digest requestPayload) = payloadDigest do
      return .error (.corrupt s!"operation request digest is invalid at {operationId}")
    let request ← match decode (α := CanonicalRequest)
        "canonical operation request" requestPayload with
      | .ok value => pure value
      | .error error => return .error error
    for index in [startRevision:endRevision] do
      unless eventOperations[index]? = some operationId do
        return .error (.corrupt s!"event is not bound to operation {operationId}")
    let prefixEvents := ledger.image.events.take endRevision
    unless (Replay.eventDigest prefixEvents).value = historyDigest do
      return .error (.corrupt s!"operation history binding is invalid at {operationId}")
    let replayed ← match Replay.replayAt ledger ⟨endRevision⟩ with
      | .ok value => pure value
      | .error fault => return .error (.corrupt s!"operation result replay failed: {repr fault}")
    unless (Replay.stateDigest replayed.state).value = resultDigest do
      return .error (.corrupt s!"operation result binding is invalid at {operationId}")
    let prior ← match Replay.replayAt ledger ⟨startRevision⟩ with
      | .ok value => pure value
      | .error fault => return .error (.corrupt s!"operation source replay failed: {repr fault}")
    let storedEvents := ledger.image.events.drop startRevision |>.take (endRevision - startRevision)
    unless storedTransactionMatches request.command prior.state replayed.state storedEvents do
      return .error (.corrupt s!"operation request does not derive its event range at {operationId}")
    let requiredDigests := (storedEvents.filterMap fun
      | .validationPassed _ _ digest => some digest
      | .evidenceRecorded evidence => some evidence.artifactDigest
      | .externalOperationRecorded attempt => some attempt.artifactDigest
      | _ => none).filter (·.startsWith "sha3-256:") |>.eraseDups
    let requestDigests := request.artifacts.map (·.1)
    unless requiredDigests.all requestDigests.contains &&
        requestDigests.all requiredDigests.contains &&
        request.artifacts.eraseDups.length = request.artifacts.length do
      return .error (.corrupt s!"operation request artifact binding is invalid at {operationId}")
    expectedStart := endRevision
  unless expectedStart = ledger.image.events.length &&
      eventOperations.size = ledger.image.events.length do
    return .error (.corrupt "operation journal does not cover the complete event history")
  return .ok ()

private def loadFromAt (db : _root_.SQLite) (expectedSchema : Nat) :
    IO (Except OpenError Projection.Store) := do
  match ← verifySQLiteIntegrity db with
  | .ok () => pure ()
  | .error error => return .error error
  let metadata ← readMetadataAt db expectedSchema
  let (ledgerId, headRevision, historyDigest) ← match metadata with
    | .ok value => pure value
    | .error error => return .error error
  let events ← match ← readEvents db with
    | .ok value => pure value
    | .error error => return .error error
  let image : Replay.LedgerImage := {
    id := ledgerId, events, storedHead := headRevision, storedHistoryDigest := historyDigest }
  let ledger ← match Replay.verifyLedger image with
    | .ok value => pure value
    | .error fault => return .error (.corrupt s!"ledger integrity failed: {repr fault}")
  match ← validateOperationJournal db ledger with
  | .ok () => pure ()
  | .error error => return .error error
  let artifactRows ← db.prepare "SELECT digest, size, payload FROM artifacts ORDER BY digest"
  let mut artifactDigests := []
  while ← artifactRows.step do
    let digest ← artifactRows.columnText 0
    let size ← match parseNat "artifact size" (← artifactRows.columnText 1) with
      | .ok value => pure value
      | .error error => return .error error
    let payload ← artifactRows.columnBlob 2
    unless payload.size = size && (← DurableFilesystem.digest payload) = digest do
      return .error (.corrupt s!"artifact payload is not content-addressed at {digest}")
    artifactDigests := digest :: artifactDigests
  let requiredArtifacts := (events.filterMap fun
    | .validationPassed _ _ digest => some digest
    | .evidenceRecorded evidence => some evidence.artifactDigest
    | .externalOperationRecorded attempt => some attempt.artifactDigest
    | _ => none).filter (·.startsWith "sha3-256:") |>.eraseDups
  unless requiredArtifacts.all artifactDigests.contains &&
      artifactDigests.all requiredArtifacts.contains &&
      artifactDigests.eraseDups.length = artifactDigests.length do
    return .error (.corrupt "artifact table does not exactly match authoritative events")
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

private def loadFrom (db : _root_.SQLite) : IO (Except OpenError Projection.Store) :=
  loadFromAt db schemaVersion

def inspectFromAt (db : _root_.SQLite) (expectedSchema : Nat) :
    IO (Except OpenError Projection.Store) :=
  loadFromAt db expectedSchema

private def tableColumns (db : _root_.SQLite) (table : String) : IO (List String) := do
  let statement ← db.prepare s!"PRAGMA table_info({table})"
  let mut columns := []
  while ← statement.step do
    columns := (← statement.columnText 1) :: columns
  return columns.reverse

private def applicationTables (db : _root_.SQLite) : IO (List String) := do
  let statement ← db.prepare "
    SELECT name FROM sqlite_schema
    WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
    ORDER BY name"
  let mut tables := []
  while ← statement.step do
    tables := (← statement.columnText 0) :: tables
  return tables.reverse

private def applicationSchemaObjects (db : _root_.SQLite) : IO (List (String × String)) := do
  let statement ← db.prepare "
    SELECT type, name FROM sqlite_schema
    WHERE name NOT LIKE 'sqlite_%'
    ORDER BY type, name"
  let mut objects := []
  while ← statement.step do
    objects := ((← statement.columnText 0), (← statement.columnText 1)) :: objects
  return objects.reverse

private def currentSchemaObjects : List (String × String) := [
  ("table", "artifacts"),
  ("table", "events"),
  ("table", "metadata"),
  ("table", "operations"),
  ("table", "projection"),
  ("table", "projection_repairs"),
  ("table", "update_provenance")]

private def normalizeSchemaSql (value : String) : String :=
  String.ofList <| value.toList.filter fun character => !character.isWhitespace

private def tableDefinitionMatches (db : _root_.SQLite) (table expected : String) : IO Bool := do
  let statement ← db.prepare "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?"
  statement.bindText 1 table
  unless ← statement.step do return false
  return normalizeSchemaSql (← statement.columnText 0) = normalizeSchemaSql expected

private def metadataDefinition := "CREATE TABLE metadata (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  schema_version TEXT NOT NULL,
  ledger_id TEXT NOT NULL,
  head_revision TEXT NOT NULL,
  history_digest TEXT NOT NULL
)"

private def projectionDefinition := "CREATE TABLE projection (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  revision TEXT NOT NULL,
  history_digest TEXT NOT NULL,
  state_digest TEXT NOT NULL,
  payload BLOB NOT NULL
)"

private def predecessorProjectionRepairsDefinition := "CREATE TABLE projection_repairs (
  observed_digest TEXT NOT NULL,
  head_revision TEXT NOT NULL,
  history_digest TEXT NOT NULL,
  adopted_digest TEXT NOT NULL,
  PRIMARY KEY (observed_digest, head_revision, history_digest)
)"

private def projectionRepairsDefinition := "CREATE TABLE projection_repairs (
  ledger_id TEXT NOT NULL,
  observed_digest TEXT NOT NULL,
  head_revision TEXT NOT NULL,
  history_digest TEXT NOT NULL,
  adopted_digest TEXT NOT NULL,
  PRIMARY KEY (ledger_id, observed_digest, head_revision, history_digest)
)"

private def currentEventsDefinition := "CREATE TABLE events (
  revision INTEGER PRIMARY KEY CHECK (revision > 0),
  payload BLOB NOT NULL,
  operation_id TEXT NOT NULL
)"

private def currentOperationsDefinition := "CREATE TABLE operations (
  operation_id TEXT PRIMARY KEY,
  request_payload BLOB NOT NULL,
  payload_digest TEXT NOT NULL,
  result_digest TEXT NOT NULL,
  start_revision TEXT NOT NULL,
  end_revision TEXT NOT NULL,
  history_digest TEXT NOT NULL,
  receipt BLOB NOT NULL
)"

private def currentArtifactsDefinition := "CREATE TABLE artifacts (
  digest TEXT PRIMARY KEY,
  size TEXT NOT NULL,
  payload BLOB NOT NULL
)"

private def updateProvenanceDefinition := "CREATE TABLE update_provenance (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  source_schema TEXT NOT NULL,
  source_digest TEXT NOT NULL,
  backup_digest TEXT NOT NULL,
  backup_size TEXT NOT NULL
)"

private def definitionsMatch (db : _root_.SQLite)
    (definitions : List (String × String)) : IO Bool := do
  for (table, expected) in definitions do
    unless ← tableDefinitionMatches db table expected do return false
  return true

private def commonDefinitionsMatch (db : _root_.SQLite) : IO Bool :=
  definitionsMatch db [
    ("metadata", metadataDefinition),
    ("projection", projectionDefinition)]

private def commonSchemaMatches (db : _root_.SQLite) : IO Bool := do
  return (← tableColumns db "metadata") =
      ["singleton", "schema_version", "ledger_id", "head_revision", "history_digest"] &&
    (← tableColumns db "projection") =
      ["singleton", "revision", "history_digest", "state_digest", "payload"]

def currentSchemaSupported (db : _root_.SQLite) : IO Bool := do
  try
    return (← applicationTables db) =
        ["artifacts", "events", "metadata", "operations", "projection",
         "projection_repairs", "update_provenance"] &&
      (← applicationSchemaObjects db) = currentSchemaObjects &&
      (← commonSchemaMatches db) &&
      (← tableColumns db "events") = ["revision", "payload", "operation_id"] &&
      (← tableColumns db "operations") =
        ["operation_id", "request_payload", "payload_digest", "result_digest",
         "start_revision", "end_revision", "history_digest", "receipt"] &&
      (← tableColumns db "artifacts") = ["digest", "size", "payload"] &&
      (← tableColumns db "projection_repairs") =
        ["ledger_id", "observed_digest", "head_revision", "history_digest", "adopted_digest"] &&
      (← tableColumns db "update_provenance") =
        ["singleton", "source_schema", "source_digest", "backup_digest", "backup_size"] &&
      (← commonDefinitionsMatch db) &&
      (← definitionsMatch db [
        ("events", currentEventsDefinition),
        ("operations", currentOperationsDefinition),
        ("artifacts", currentArtifactsDefinition),
        ("projection_repairs", projectionRepairsDefinition),
        ("update_provenance", updateProvenanceDefinition)])
  catch _ => return false

def predecessorV2Supported (db : _root_.SQLite) : IO Bool := do
  try
    unless (← applicationSchemaObjects db) = currentSchemaObjects &&
        (← commonDefinitionsMatch db) &&
        (← tableColumns db "projection_repairs") =
          ["observed_digest", "head_revision", "history_digest", "adopted_digest"] &&
        (← definitionsMatch db [
          ("events", currentEventsDefinition),
          ("operations", currentOperationsDefinition),
          ("artifacts", currentArtifactsDefinition),
          ("projection_repairs", predecessorProjectionRepairsDefinition),
          ("update_provenance", updateProvenanceDefinition)]) do
      return false
    match ← loadFromAt db predecessorSchemaVersion with
    | .ok _ => return true
    | .error _ => return false
  catch _ => return false

private structure LegacyV5Operation where
  operation : OperationId
  request : CanonicalRequest
  startRevision : Nat
  endRevision : Nat

private structure LegacyV5Snapshot where
  ledger : LedgerId
  events : List Replay.Event
  eventOperations : List String
  state : Replay.State
  operations : List LegacyV5Operation

private def legacyV5EventArtifactDigest? : LegacyV5.Event → Option String
  | .validationPassed _ _ digest => some digest
  | .evidenceRecorded evidence => some evidence.artifactDigest
  | .externalOperationRecorded attempt => some attempt.artifactDigest
  | _ => none

private def convertLegacyV5Events (events : List LegacyV5.Event) :
    Except OpenError (List Replay.Event × Array Replay.VerifiedState) := do
  let initial ← match Replay.replay [] Replay.emptyState with
    | .ok state => .ok state
    | .error error => .error (.corrupt
        s!"current empty state is invalid during legacy conversion: {repr error}")
  let mut verified := initial
  let mut converted := []
  let mut states := #[initial]
  for event in events do
    let current := event.toCurrentAt verified.state
    verified ← match Replay.applyEvent current verified with
      | .ok state => .ok state
      | .error error => .error (.corrupt
          s!"legacy v5 event is not valid under its compatibility mapping: {repr error}")
    converted := converted ++ [current]
    states := states.push verified
  return (converted, states)

private def validateLegacyV5OperationJournal (db : _root_.SQLite)
    (legacyEvents : List LegacyV5.Event) (events : List Replay.Event)
    (eventOperations : Array String) (states : Array Replay.VerifiedState) :
    IO (Except OpenError (List LegacyV5Operation)) := do
  let statement ← db.prepare "
    SELECT operation_id, request_payload, payload_digest, result_digest,
           start_revision, end_revision, history_digest, receipt
    FROM operations ORDER BY CAST(start_revision AS INTEGER), operation_id"
  let mut expectedStart := 0
  let mut operations := []
  while ← statement.step do
    let operationId ← statement.columnText 0
    let requestPayload ← statement.columnBlob 1
    let payloadDigest ← statement.columnText 2
    let resultDigest ← statement.columnText 3
    let startRevision ← match
        parseNat "legacy v5 operation start revision" (← statement.columnText 4) with
      | .ok value => pure value
      | .error error => return .error error
    let endRevision ← match
        parseNat "legacy v5 operation end revision" (← statement.columnText 5) with
      | .ok value => pure value
      | .error error => return .error error
    let historyDigest ← statement.columnText 6
    let receipt ← match decode (α := Policy.Update.Receipt)
        "legacy v5 operation receipt" (← statement.columnBlob 7) with
      | .ok receipt => pure receipt
      | .error error => return .error error
    let legacyRequest ← match decode (α := LegacyV5.CanonicalRequest)
        "legacy v5 canonical operation request" requestPayload with
      | .ok request => pure request
      | .error error => return .error error
    unless startRevision = expectedStart && startRevision < endRevision &&
        endRevision ≤ events.length do
      return .error (.corrupt
        s!"legacy v5 operation range is not contiguous at {operationId}")
    unless receipt.operation.value = operationId &&
        receipt.payloadDigest = payloadDigest &&
        receipt.resultDigest = resultDigest &&
        (← DurableFilesystem.digest requestPayload) = payloadDigest do
      return .error (.corrupt
        s!"legacy v5 operation receipt binding is invalid at {operationId}")
    for index in [startRevision:endRevision] do
      unless eventOperations[index]? = some operationId do
        return .error (.corrupt
          s!"legacy v5 event is not bound to operation {operationId}")
    unless LegacyV5.eventDigest (legacyEvents.take endRevision) = historyDigest do
      return .error (.corrupt
        s!"legacy v5 operation history is invalid at {operationId}")
    let prior ← match states[startRevision]? with
      | some state => pure state
      | none => return .error (.corrupt
          s!"legacy v5 operation source state is missing at {operationId}")
    let result ← match states[endRevision]? with
      | some state => pure state
      | none => return .error (.corrupt
          s!"legacy v5 operation result state is missing at {operationId}")
    let reconstructed ← match
        LegacyV5.State.fromCurrent (legacyEvents.take endRevision) result.state with
      | .ok state => pure state
      | .error error => return .error (.corrupt error)
    unless LegacyV5.stateDigest reconstructed = resultDigest do
      return .error (.corrupt
        s!"legacy v5 operation result is not derived from history at {operationId}")
    let command := legacyRequest.command.toCurrentAt prior.state
    let storedEvents := events.drop startRevision |>.take (endRevision - startRevision)
    unless storedTransactionMatches command prior.state result.state storedEvents do
      return .error (.corrupt
        s!"legacy v5 operation request does not derive its event range at {operationId}")
    let requiredDigests := (legacyEvents.drop startRevision
      |>.take (endRevision - startRevision)
      |>.filterMap legacyV5EventArtifactDigest?)
      |>.filter (·.startsWith "sha3-256:") |>.eraseDups
    let requestDigests := legacyRequest.artifacts.map (·.1)
    unless requiredDigests.all requestDigests.contains &&
        requestDigests.all requiredDigests.contains &&
        legacyRequest.artifacts.eraseDups.length = legacyRequest.artifacts.length do
      return .error (.corrupt
        s!"legacy v5 operation request artifact binding is invalid at {operationId}")
    operations := operations ++ [{
      operation := ⟨operationId⟩
      request := { command, artifacts := legacyRequest.artifacts }
      startRevision
      endRevision }]
    expectedStart := endRevision
  unless expectedStart = events.length &&
      eventOperations.size = events.length do
    return .error (.corrupt
      "legacy v5 operation journal does not cover the complete event history")
  return .ok operations

private def readLegacyV5Snapshot (db : _root_.SQLite) :
    IO (Except OpenError LegacyV5Snapshot) := do
  let (ledger, head, historyDigest) ← match ←
      readMetadataAt db legacyV5SchemaVersion with
    | .ok metadata => pure metadata
    | .error error => return .error error
  let integrity ← verifySQLiteIntegrity db
  if let .error error := integrity then return .error error
  unless ← currentSchemaSupported db do
    return .error (.corrupt "legacy v5 schema fingerprint is not canonical")
  let eventRows ← db.prepare
    "SELECT revision, payload, operation_id FROM events ORDER BY revision"
  let mut events : Array LegacyV5.Event := #[]
  let mut eventOperations : Array String := #[]
  let mut expected := 1
  while ← eventRows.step do
    let revision := (← eventRows.columnInt64 0).toInt.toNat
    unless revision = expected do
      return .error (.corrupt s!"legacy v5 event revision gap at {expected}")
    let event ← match decode (α := LegacyV5.Event)
        "legacy v5 event" (← eventRows.columnBlob 1) with
      | .ok event => pure event
      | .error error => return .error error
    events := events.push event
    eventOperations := eventOperations.push (← eventRows.columnText 2)
    expected := expected + 1
  unless events.size = head.value &&
      LegacyV5.eventDigest events.toList = historyDigest.value do
    return .error (.corrupt "legacy v5 event history binding is invalid")
  let projectionRow ← db.prepare "
    SELECT revision, history_digest, state_digest, payload
    FROM projection WHERE singleton = 1"
  unless ← projectionRow.step do
    return .error (.corrupt "legacy v5 projection is missing")
  let projectionRevision ← match
      parseNat "legacy v5 projection revision" (← projectionRow.columnText 0) with
    | .ok revision => pure revision
    | .error error => return .error error
  let projectionHistory ← projectionRow.columnText 1
  let projectionStateDigest ← projectionRow.columnText 2
  let projection ← match decode (α := LegacyV5.ProjectionObservation)
      "legacy v5 projection" (← projectionRow.columnBlob 3) with
    | .ok projection => pure projection
    | .error error => return .error error
  let state ← match projection.payload with
    | .decoded state => pure state
    | .decodeFailed fault =>
        return .error (.corrupt s!"legacy v5 projection is undecodable: {repr fault}")
  unless projectionRevision = head.value &&
      projection.reference.revision == head &&
      projectionHistory = historyDigest.value &&
      projection.reference.historyDigest.value = historyDigest.value &&
      projectionStateDigest = LegacyV5.stateDigest state &&
      projection.reference.stateDigest.value = projectionStateDigest &&
      state.revision == head do
    return .error (.corrupt "legacy v5 projection binding is invalid")
  let (convertedEvents, states) ← match convertLegacyV5Events events.toList with
    | .ok converted => pure converted
    | .error error => return .error error
  let convertedState ← match states[events.size]? with
    | some state => pure state.state
    | none => return .error (.corrupt "legacy v5 converted head is missing")
  let reconstructed ← match LegacyV5.State.fromCurrent events.toList convertedState with
    | .ok reconstructed => pure reconstructed
    | .error error => return .error (.corrupt error)
  unless reconstructed = state do
    return .error (.corrupt
      "legacy v5 projection is not the state derived from authoritative events")
  let operations ← match ← validateLegacyV5OperationJournal db events.toList
      convertedEvents eventOperations states with
    | .ok operations => pure operations
    | .error error => return .error error
  let artifacts ← db.prepare "SELECT digest, size, payload FROM artifacts ORDER BY digest"
  let mut artifactDigests := []
  while ← artifacts.step do
    let digest ← artifacts.columnText 0
    let size ← match parseNat "legacy v5 artifact size" (← artifacts.columnText 1) with
      | .ok value => pure value
      | .error error => return .error error
    let payload ← artifacts.columnBlob 2
    unless payload.size = size && (← DurableFilesystem.digest payload) = digest do
      return .error (.corrupt
        s!"legacy v5 artifact payload is not content-addressed at {digest}")
    artifactDigests := digest :: artifactDigests
  let requiredArtifacts := (events.toList.filterMap legacyV5EventArtifactDigest?)
    |>.filter (·.startsWith "sha3-256:") |>.eraseDups
  unless requiredArtifacts.all artifactDigests.contains &&
      artifactDigests.all requiredArtifacts.contains &&
      artifactDigests.eraseDups.length = artifactDigests.length do
    return .error (.corrupt
      "legacy v5 artifact table does not exactly match authoritative events")
  return .ok {
    ledger
    events := convertedEvents
    eventOperations := eventOperations.toList
    state := convertedState
    operations }

def legacyV5Supported (db : _root_.SQLite) : IO Bool := do
  try
    return (← readLegacyV5Snapshot db).isOk
  catch _ => return false

def migrateLegacyV5 (db : _root_.SQLite) (_sourceDigest : String) : IO Unit := do
  let legacy ← match ← readLegacyV5Snapshot db with
    | .ok snapshot => pure snapshot
    | .error error => throwOpen error
  let events := legacy.events
  let historyDigest := Replay.eventDigest events
  let ledger : Replay.LedgerImage := {
    id := legacy.ledger
    events
    storedHead := legacy.state.revision
    storedHistoryDigest := historyDigest }
  let projection := Application.Service.projectionFor ledger legacy.state
  db.exec "
    DELETE FROM events;
    DELETE FROM operations;
    DELETE FROM projection;
    DELETE FROM projection_repairs;"
  for (event, index) in events.zipIdx do
    let eventStatement ← db.prepare
      "INSERT INTO events (revision, payload, operation_id) VALUES (?, ?, ?)"
    eventStatement.bindInt64 1 (Int64.ofInt (index + 1))
    eventStatement.bindBlob 2 (toBinary event)
    eventStatement.bindText 3 legacy.eventOperations[index]!
    eventStatement.exec
  let metadata ← db.prepare "
    UPDATE metadata
    SET schema_version=?, head_revision=?, history_digest=?
    WHERE singleton=1"
  metadata.bindText 1 (toString schemaVersion)
  metadata.bindText 2 (toString legacy.state.revision.value)
  metadata.bindText 3 historyDigest.value
  metadata.exec
  let projectionStatement ← db.prepare
    "INSERT INTO projection VALUES (1, ?, ?, ?, ?)"
  projectionStatement.bindText 1 (toString projection.reference.revision.value)
  projectionStatement.bindText 2 projection.reference.historyDigest.value
  projectionStatement.bindText 3 projection.reference.stateDigest.value
  projectionStatement.bindBlob 4 (toBinary projection)
  projectionStatement.exec
  let verified ← match Replay.verifyLedger ledger with
    | .ok verified => pure verified
    | .error error =>
        throw (IO.userError s!"converted legacy v5 ledger is invalid: {repr error}")
  for migrated in legacy.operations do
    let requestPayload := toBinary migrated.request
    let payloadDigest ← DurableFilesystem.digest requestPayload
    let result ← match Replay.replayAt verified ⟨migrated.endRevision⟩ with
      | .ok state => pure state
      | .error error =>
          throw (IO.userError s!"converted legacy v5 result is unavailable: {repr error}")
    let resultDigest := Replay.stateDigest result.state
    let operationHistory := Replay.eventDigest (events.take migrated.endRevision)
    let receipt : Policy.Update.Receipt := {
      operation := migrated.operation
      payloadDigest
      resultDigest := resultDigest.value }
    let operationStatement ← db.prepare "
      INSERT INTO operations
        (operation_id, request_payload, payload_digest, result_digest,
         start_revision, end_revision, history_digest, receipt)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    operationStatement.bindText 1 migrated.operation.value
    operationStatement.bindBlob 2 requestPayload
    operationStatement.bindText 3 payloadDigest
    operationStatement.bindText 4 resultDigest.value
    operationStatement.bindText 5 (toString migrated.startRevision)
    operationStatement.bindText 6 (toString migrated.endRevision)
    operationStatement.bindText 7 operationHistory.value
    operationStatement.bindBlob 8 (toBinary receipt)
    operationStatement.exec

private def readArtifactRefs (db : _root_.SQLite) :
    IO (Except OpenError (List DurableFilesystem.ArtifactRef)) := do
  let statement ← db.prepare "SELECT digest, size, payload FROM artifacts ORDER BY digest"
  let mut references := []
  while ← statement.step do
    let digest ← statement.columnText 0
    let size ← match parseNat "artifact size" (← statement.columnText 1) with
      | .ok value => pure value
      | .error error => return .error error
    let payload ← statement.columnBlob 2
    unless payload.size = size && (← DurableFilesystem.digest payload) = digest do
      return .error (.corrupt s!"artifact payload is not content-addressed at {digest}")
    references := { digest, size } :: references
  return .ok references.reverse

private def eventArtifactDigest? : Replay.Event → Option String
  | .validationPassed _ _ digest => some digest
  | .evidenceRecorded evidence => some evidence.artifactDigest
  | .externalOperationRecorded attempt => some attempt.artifactDigest
  | _ => none

private def requiredFileArtifacts (events : List Replay.Event) : List String :=
  (events.filterMap eventArtifactDigest?).filter (·.startsWith "sha3-256:") |>.eraseDups

def artifactRoot (path : System.FilePath) : System.FilePath :=
  path.parent.getD "." / s!"{path.fileName.getD "ledger"}.artifacts"

def artifactsSupportedAt (path : System.FilePath) (db : _root_.SQLite) : IO Bool := do
  let references ← match ← readArtifactRefs db with
    | .ok references => pure references
    | .error _ => return false
  let reconciliation ← DurableFilesystem.reconcile (artifactRoot path) references
  return reconciliation.missing.isEmpty && reconciliation.mismatched.isEmpty

private def loadAuthoritativeFrom (path : System.FilePath) (db : _root_.SQLite) :
    IO (Except OpenError Projection.Store) := do
  unless ← currentSchemaSupported db do
    return .error (.corrupt "storage schema fingerprint is not canonical")
  let store ← match ← loadFrom db with
    | .ok value => pure value
    | .error error => return .error error
  let references ← match ← readArtifactRefs db with
    | .ok value => pure value
    | .error error => return .error error
  let reconciliation ← DurableFilesystem.reconcile (artifactRoot path) references
  unless reconciliation.missing.isEmpty && reconciliation.mismatched.isEmpty do
    return .error (.corrupt s!"artifact reconciliation failed: {repr reconciliation}")
  return .ok store

def inspect (path : System.FilePath) : IO (Except OpenError Projection.Store) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.transaction (loadAuthoritativeFrom path db)

def inspectAtSchema (path : System.FilePath) (expectedSchema : Nat) :
    IO (Except OpenError Projection.Store) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  if expectedSchema = schemaVersion then
    db.transaction (loadAuthoritativeFrom path db)
  else
    db.transaction (loadFromAt db expectedSchema)

def inspectWithArtifacts (path artifactRoot : System.FilePath) :
    IO (Except OpenError Projection.Store) := do
  if !(← path.pathExists) then return .error .uninitialized
  unless artifactRoot = SQLite.artifactRoot path do
    return .error (.corrupt "artifact root does not match the store binding")
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.transaction do
    let store ← match ← loadAuthoritativeFrom path db with
      | .ok value => pure value
      | .error error => return .error error
    let references ← match ← readArtifactRefs db with
      | .ok value => pure value
      | .error error => return .error error
    let required := requiredFileArtifacts store.ledger.events
    unless required.all (fun digest => references.any (·.digest = digest)) &&
        references.all (fun reference => required.contains reference.digest) do
      return .error (.corrupt "artifact table does not exactly match authoritative events")
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

private structure StoredProjectionRow where
  revision : String
  historyDigest : String
  stateDigest : String
  payload : ByteArray

deriving instance ToBinary for StoredProjectionRow

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

private def validateProjectionTolerantFrom (path : System.FilePath) (db : _root_.SQLite) :
    IO (Except OpenError Replay.VerifiedLedger) := do
  unless ← currentSchemaSupported db do
    return .error (.corrupt "storage schema fingerprint is not canonical")
  match ← verifySQLiteIntegrity db with
  | .ok () => pure ()
  | .error error => return .error error
  let ledger ← match ← readLedger db with
    | .ok value => pure value
    | .error error => return .error error
  match ← validateOperationJournal db ledger with
  | .ok () => pure ()
  | .error error => return .error error
  let references ← match ← readArtifactRefs db with
    | .ok value => pure value
    | .error error => return .error error
  let required := requiredFileArtifacts ledger.image.events
  unless required.all (fun digest => references.any (·.digest = digest)) &&
      references.all (fun reference => required.contains reference.digest) do
    return .error (.corrupt "artifact table does not exactly match authoritative events")
  let reconciliation ← DurableFilesystem.reconcile (artifactRoot path) references
  unless reconciliation.missing.isEmpty && reconciliation.mismatched.isEmpty do
    return .error (.corrupt s!"artifact reconciliation failed: {repr reconciliation}")
  return .ok ledger

private def readStoredProjectionRow? (db : _root_.SQLite) : IO (Option StoredProjectionRow) := do
  let statement ← db.prepare "
    SELECT revision, history_digest, state_digest, payload
    FROM projection WHERE singleton = 1"
  unless ← statement.step do return none
  return some {
    revision := ← statement.columnText 0
    historyDigest := ← statement.columnText 1
    stateDigest := ← statement.columnText 2
    payload := ← statement.columnBlob 3 }

private def projectionRowMatches (row : StoredProjectionRow)
    (projection : Projection.ProjectionObservation) : Bool :=
  row.revision = toString projection.reference.revision.value &&
    row.historyDigest = projection.reference.historyDigest.value &&
    row.stateDigest = projection.reference.stateDigest.value &&
    row.payload = toBinary projection

private def observedProjectionDigest (row? : Option StoredProjectionRow) : IO String := do
  let some row := row? | return "missing"
  match fromBinary row.payload with
  | .ok (projection : Projection.ProjectionObservation) =>
      if row.revision = toString projection.reference.revision.value &&
          row.historyDigest = projection.reference.historyDigest.value &&
          row.stateDigest = projection.reference.stateDigest.value then
        DurableFilesystem.digest row.payload
      else
        return "row:" ++ (← DurableFilesystem.digest (toBinary row))
  | .error _ => DurableFilesystem.digest row.payload

private def diagnoseFrom (db : _root_.SQLite) : IO (Except OpenError Diagnosis) := do
  let ledger ← match ← readLedger db with
    | .ok value => pure value
    | .error error => return .error error
  let row? ← readStoredProjectionRow? db
  let observedDigest ← observedProjectionDigest row?
  let plan : ProjectionRepairPlan := { head := ledger.point, observedDigest }
  let canonical := Application.Service.projectionFor ledger.image ledger.head.state
  match row? with
  | some row =>
      if projectionRowMatches row canonical then
        return .ok (.healthy {
          ledger := ledger.image
          active := some canonical
          staged := []
          receipts := []
          nextStage := ⟨1⟩ })
      else
        return .ok (.projectionRepairRequired plan)
  | none => return .ok (.projectionRepairRequired plan)

def diagnose (path : System.FilePath) : IO (Except OpenError Diagnosis) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.transaction do
    match ← validateProjectionTolerantFrom path db with
    | .ok _ => pure ()
    | .error error => return .error error
    diagnoseFrom db

private def lookupProjectionRepair (db : _root_.SQLite) (plan : ProjectionRepairPlan) :
    IO (Option ProjectionRepairReceipt) := do
  let statement ← db.prepare "
    SELECT repair.adopted_digest FROM projection_repairs AS repair
    WHERE repair.ledger_id=? AND repair.observed_digest=?
      AND repair.head_revision=? AND repair.history_digest=?"
  statement.bindText 1 plan.head.ledger.value
  statement.bindText 2 plan.observedDigest
  statement.bindText 3 (toString plan.head.revision.value)
  statement.bindText 4 plan.head.historyDigest.value
  unless ← statement.step do return none
  return some { plan, adoptedDigest := ← statement.columnText 0 }

private def writeCanonicalProjection (db : _root_.SQLite)
    (projection : Projection.ProjectionObservation) : IO Unit := do
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

private def projectionAtPoint (ledger : Replay.VerifiedLedger)
    (point : Domain.Projection.LedgerPoint) : Except OpenError Projection.ProjectionObservation := do
  unless point.ledger = ledger.image.id do
    throw (.corrupt "projection repair receipt belongs to another ledger")
  let events := ledger.image.events.take point.revision.value
  let historyDigest := Replay.eventDigest events
  unless historyDigest = point.historyDigest do
    throw (.corrupt "projection repair receipt history is not authoritative")
  let state ← match Replay.replayAt ledger point.revision with
    | .ok value => pure value
    | .error fault => throw (.corrupt s!"projection repair receipt replay failed: {repr fault}")
  let image : Replay.LedgerImage := {
    id := ledger.image.id
    events
    storedHead := point.revision
    storedHistoryDigest := historyDigest }
  return Application.Service.projectionFor image state.state

def repairProjectionWithLockHook (path : System.FilePath) (plan : ProjectionRepairPlan)
    (beforeWriterLock afterCommit : IO Unit) : IO (Except OpenError ProjectionRepairReceipt) := do
  if !(← path.pathExists) then return .error .uninitialized
  beforeWriterLock
  withWriterLock path do
    let db ← _root_.SQLite.openWith path
      { mode := .readWrite, threading := some .fullmutex } (busyTimeoutMs := 5000)
    let outcome : Except OpenError ProjectionRepairReceipt ←
      db.transaction (mode := .immediate) do
        match ← validateProjectionTolerantFrom path db with
        | .ok _ => pure ()
        | .error error => return .error error
        let ledger ← match ← readLedger db with
          | .ok value => pure value
          | .error error => return .error error
        let projection := Application.Service.projectionFor ledger.image ledger.head.state
        let canonicalDigest ← DurableFilesystem.digest (toBinary projection)
        match ← lookupProjectionRepair db plan with
        | some receipt =>
            let historical ← match projectionAtPoint ledger plan.head with
              | .ok value => pure value
              | .error error => return .error error
            let historicalDigest ← DurableFilesystem.digest (toBinary historical)
            unless receipt.adoptedDigest = historicalDigest do
              return .error (.corrupt "projection repair receipt does not match canonical projection")
            if ledger.point = plan.head then
              let diagnosis ← match ← diagnoseFrom db with
                | .ok value => pure value
                | .error error => return .error error
              match diagnosis with
              | .healthy _ => pure ()
              | .projectionRepairRequired current =>
                  unless current = plan do
                    return .error (.corrupt "projection repair plan is stale")
                  writeCanonicalProjection db projection
            return .ok receipt
        | none => pure ()
        let diagnosis ← match ← diagnoseFrom db with
          | .ok value => pure value
          | .error error => return .error error
        let current ← match diagnosis with
          | .projectionRepairRequired value => pure value
          | .healthy _ => return .error (.corrupt "projection repair is no longer required")
        unless current = plan do
          return .error (.corrupt "projection repair plan is stale")
        writeCanonicalProjection db projection
        let receiptStatement ← db.prepare "
          INSERT INTO projection_repairs
            (ledger_id, observed_digest, head_revision, history_digest, adopted_digest)
          VALUES (?, ?, ?, ?, ?)"
        receiptStatement.bindText 1 plan.head.ledger.value
        receiptStatement.bindText 2 plan.observedDigest
        receiptStatement.bindText 3 (toString plan.head.revision.value)
        receiptStatement.bindText 4 plan.head.historyDigest.value
        receiptStatement.bindText 5 canonicalDigest
        receiptStatement.exec
        return .ok { plan, adoptedDigest := canonicalDigest }
    match outcome with
    | .ok _ => afterCommit
    | .error _ => pure ()
    return outcome

def repairProjectionWithHook (path : System.FilePath) (plan : ProjectionRepairPlan)
    (afterCommit : IO Unit) : IO (Except OpenError ProjectionRepairReceipt) :=
  repairProjectionWithLockHook path plan (pure ()) afterCommit

def repairProjection (path : System.FilePath) (plan : ProjectionRepairPlan) :
    IO (Except OpenError ProjectionRepairReceipt) :=
  repairProjectionWithHook path plan (pure ())

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

private def appendEvents (db : _root_.SQLite) (operation : OperationId) (start : Nat)
    (events : List Replay.Event) : IO Unit := do
  let statement ← db.prepare
    "INSERT INTO events (revision, payload, operation_id) VALUES (?, ?, ?)"
  for (event, offset) in events.zipIdx do
    statement.bindInt64 1 (Int.ofNat (start + offset + 1)).toInt64
    statement.bindBlob 2 (toBinary event)
    statement.bindText 3 operation.value
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

private def writeReceipt (db : _root_.SQLite) (receipt : Policy.Update.Receipt)
    (requestPayload : ByteArray)
    (startRevision endRevision : Nat) (historyDigest : String) : IO Unit := do
  let statement ← db.prepare "
    INSERT INTO operations
      (operation_id, request_payload, payload_digest, result_digest, start_revision,
       end_revision, history_digest, receipt)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
  statement.bindText 1 receipt.operation.value
  statement.bindBlob 2 requestPayload
  statement.bindText 3 receipt.payloadDigest
  statement.bindText 4 receipt.resultDigest
  statement.bindText 5 (toString startRevision)
  statement.bindText 6 (toString endRevision)
  statement.bindText 7 historyDigest
  statement.bindBlob 8 (toBinary receipt)
  statement.exec

private def writeArtifactRefs (db : _root_.SQLite) (root : System.FilePath)
    (references : List DurableFilesystem.ArtifactRef) : IO Unit := do
  for reference in references do
    let payload ← IO.FS.readBinFile (DurableFilesystem.objectPath root reference)
    unless payload.size = reference.size && (← DurableFilesystem.digest payload) = reference.digest do
      throw <| IO.userError s!"artifact changed after verification: {reference.digest}"
    let existing ← db.prepare "SELECT size, payload FROM artifacts WHERE digest = ?"
    existing.bindText 1 reference.digest
    if ← existing.step then
      let storedSize ← existing.columnText 0
      let storedPayload ← existing.columnBlob 1
      unless storedSize = toString reference.size && storedPayload = payload do
        throw <| IO.userError s!"stored artifact conflicts with content address: {reference.digest}"
    else
      let insert ← db.prepare
        "INSERT INTO artifacts (digest, size, payload) VALUES (?, ?, ?)"
      insert.bindText 1 reference.digest
      insert.bindText 2 (toString reference.size)
      insert.bindBlob 3 payload
      insert.exec

private def canonicalPayload (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef) : ByteArray :=
  toBinary ({
    command
    artifacts := artifacts.map fun reference => (reference.digest, reference.size)
  } : CanonicalRequest)

def mutateWithLockHook (path : System.FilePath) (operation : OperationId)
    (expectedRevision : Revision) (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef := [])
    (artifactRoot : Option System.FilePath := none)
    (beforeWriterLock : IO Unit := pure ())
    (beforeArtifactCommit : IO Unit := pure ())
    (afterArtifactVerification : IO Unit := pure ())
    (afterJournalWrite : IO Unit := pure ()) :
    IO (Except MutationError MutationOutcome) := do
  if !(← path.pathExists) then return .error (.openError .uninitialized)
  let requestPayload := canonicalPayload command artifacts
  let payloadDigest ← DurableFilesystem.digest requestPayload
  beforeWriterLock
  withWriterLock path do
    let db ← _root_.SQLite.openWith path
      { mode := .readWrite, threading := some .fullmutex } (busyTimeoutMs := 5000)
    db.transaction (mode := .immediate) do
      let store ← match ← loadAuthoritativeFrom path db with
        | .ok value => pure value
        | .error error => return .error (.openError error)
      match ← lookupReceipt db operation with
      | some (storedPayload, receipt) =>
          if storedPayload = payloadDigest then
            return .ok { receipt, store, exactRetry := true }
          else
            return .error .operationConflict
      | none =>
          if store.ledger.storedHead != expectedRevision then
            return .error .staleRevision
          let transaction ← match Application.Service.execute command store with
            | .ok value => pure value
            | .error error => return .error (.rejected error)
          let requiredArtifacts := requiredFileArtifacts transaction.accepted.events
          unless requiredArtifacts.all (fun digest => artifacts.any (·.digest = digest)) &&
              artifacts.all (fun reference => requiredArtifacts.contains reference.digest) &&
              artifacts.eraseDups.length = artifacts.length do
            return .error (.artifactInvalid "event/artifact binding mismatch")
          beforeArtifactCommit
          if !artifacts.isEmpty then
            let root ← match artifactRoot with
              | some value => pure value
              | none => return .error (.artifactInvalid "artifact root is required")
            unless root = SQLite.artifactRoot path do
              return .error (.artifactInvalid "artifact root does not match the store binding")
            for reference in artifacts do
              match ← DurableFilesystem.verify root reference with
              | .valid => pure ()
              | _ => return .error (.artifactInvalid reference.digest)
          afterArtifactVerification
          let startRevision := store.ledger.events.length
          let endRevision := transaction.result.ledger.events.length
          let historyDigest := transaction.result.ledger.storedHistoryDigest.value
          let receipt : Policy.Update.Receipt := {
            operation
            payloadDigest
            resultDigest := Replay.stateDigest transaction.accepted.result.state |>.value }
          writeReceipt db receipt requestPayload startRevision endRevision historyDigest
          appendEvents db operation startRevision transaction.accepted.events
          afterJournalWrite
          writeHead db transaction.result
          if !artifacts.isEmpty then
            writeArtifactRefs db artifactRoot.get! artifacts
          return .ok { receipt, store := transaction.result, exactRetry := false }

def mutateWithHook (path : System.FilePath) (operation : OperationId)
    (expectedRevision : Revision) (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef := [])
    (artifactRoot : Option System.FilePath := none)
    (beforeArtifactCommit : IO Unit := pure ())
    (afterArtifactVerification : IO Unit := pure ())
    (afterJournalWrite : IO Unit := pure ()) :
    IO (Except MutationError MutationOutcome) :=
  mutateWithLockHook path operation expectedRevision command artifacts artifactRoot
    (pure ()) beforeArtifactCommit afterArtifactVerification afterJournalWrite

def mutate (path : System.FilePath) (operation : OperationId)
    (expectedRevision : Revision) (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef := [])
    (artifactRoot : Option System.FilePath := none) :
    IO (Except MutationError MutationOutcome) :=
  mutateWithHook path operation expectedRevision command artifacts artifactRoot

end AgentWorkbench.Adapter.SQLite
