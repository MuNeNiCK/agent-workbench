import AgentWorkbench.Adapter.Codec
import AgentWorkbench.Adapter.DurableFilesystem
import SQLite

namespace AgentWorkbench.Adapter.SQLite

open AgentWorkbench.Domain
open AgentWorkbench.Kernel
open SQLite.Blob

def schemaVersion : Nat := 2
def legacySchemaVersion : Nat := 1

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
      observed_digest TEXT NOT NULL,
      head_revision TEXT NOT NULL,
      history_digest TEXT NOT NULL,
      adopted_digest TEXT NOT NULL,
      PRIMARY KEY (observed_digest, head_revision, history_digest)
    );
    CREATE TABLE update_provenance (
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
      source_schema TEXT NOT NULL,
      source_digest TEXT NOT NULL,
      backup_digest TEXT NOT NULL,
      backup_size TEXT NOT NULL
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
    let decided ← match Decide.decide request.command prior.state with
      | .ok value => pure value
      | .error error =>
          return .error (.corrupt s!"stored operation request is not replayable: {repr error}")
    let storedEvents := ledger.image.events.drop startRevision |>.take (endRevision - startRevision)
    unless decided.events = storedEvents && decided.result.state = replayed.state do
      return .error (.corrupt s!"operation request does not derive its event range at {operationId}")
    let requiredDigests := (decided.events.filterMap fun
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

private def legacySchemaObjects : List (String × String) := [
  ("table", "artifacts"),
  ("table", "events"),
  ("table", "metadata"),
  ("table", "operations"),
  ("table", "projection"),
  ("table", "projection_repairs")]

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

private def projectionRepairsDefinition := "CREATE TABLE projection_repairs (
  observed_digest TEXT NOT NULL,
  head_revision TEXT NOT NULL,
  history_digest TEXT NOT NULL,
  adopted_digest TEXT NOT NULL,
  PRIMARY KEY (observed_digest, head_revision, history_digest)
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

private def originalEventsDefinition := "CREATE TABLE events (
  revision INTEGER PRIMARY KEY CHECK (revision > 0),
  payload BLOB NOT NULL
)"

private def originalOperationsDefinition := "CREATE TABLE operations (
  operation_id TEXT PRIMARY KEY,
  payload_digest TEXT NOT NULL,
  result_digest TEXT NOT NULL,
  receipt BLOB NOT NULL
)"

private def originalArtifactsDefinition := "CREATE TABLE artifacts (
  digest TEXT PRIMARY KEY,
  size TEXT NOT NULL
)"

private def definitionsMatch (db : _root_.SQLite)
    (definitions : List (String × String)) : IO Bool := do
  for (table, expected) in definitions do
    unless ← tableDefinitionMatches db table expected do return false
  return true

private def commonDefinitionsMatch (db : _root_.SQLite) : IO Bool :=
  definitionsMatch db [
    ("metadata", metadataDefinition),
    ("projection", projectionDefinition),
    ("projection_repairs", projectionRepairsDefinition)]

private def commonSchemaMatches (db : _root_.SQLite) : IO Bool := do
  return (← tableColumns db "metadata") =
      ["singleton", "schema_version", "ledger_id", "head_revision", "history_digest"] &&
    (← tableColumns db "projection") =
      ["singleton", "revision", "history_digest", "state_digest", "payload"] &&
    (← tableColumns db "projection_repairs") =
      ["observed_digest", "head_revision", "history_digest", "adopted_digest"]

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
      (← tableColumns db "update_provenance") =
        ["singleton", "source_schema", "source_digest", "backup_digest", "backup_size"] &&
      (← commonDefinitionsMatch db) &&
      (← definitionsMatch db [
        ("events", currentEventsDefinition),
        ("operations", currentOperationsDefinition),
        ("artifacts", currentArtifactsDefinition),
        ("update_provenance", updateProvenanceDefinition)])
  catch _ => return false

inductive LegacyV1Layout
  | originalEmpty
  | predecessor
deriving DecidableEq, Repr

private def originalV1EmptyValid (db : _root_.SQLite) : IO Bool := do
  try
    for table in ["events", "operations", "artifacts"] do
      let count ← db.prepare s!"SELECT count(*) FROM {table}"
      unless ← count.step do return false
      unless (← count.columnInt64 0).toInt = 0 do return false
    let metadata ← readMetadataAt db legacySchemaVersion
    let (ledgerId, headRevision, historyDigest) ← match metadata with
      | .ok value => pure value
      | .error _ => return false
    let events ← match ← readEvents db with
      | .ok value => pure value
      | .error _ => return false
    let projection ← match ← readProjection db with
      | .ok value => pure value
      | .error _ => return false
    let store : Projection.Store := {
      ledger := {
        id := ledgerId
        events := events
        storedHead := headRevision
        storedHistoryDigest := historyDigest }
      active := some projection
      staged := []
      receipts := []
      nextStage := ⟨1⟩ }
    match Projection.inspect store with
    | .fresh _ _ => return true
    | _ => return false
  catch _ => return false

def legacyV1Layout? (db : _root_.SQLite) : IO (Option LegacyV1Layout) := do
  try
    unless (← applicationTables db) =
        ["artifacts", "events", "metadata", "operations", "projection", "projection_repairs"] &&
        (← applicationSchemaObjects db) = legacySchemaObjects &&
        (← commonSchemaMatches db) && (← commonDefinitionsMatch db) do
      return none
    if (← tableColumns db "events") = ["revision", "payload"] &&
        (← tableColumns db "operations") =
          ["operation_id", "payload_digest", "result_digest", "receipt"] &&
        (← tableColumns db "artifacts") = ["digest", "size"] &&
        (← definitionsMatch db [
          ("events", originalEventsDefinition),
          ("operations", originalOperationsDefinition),
          ("artifacts", originalArtifactsDefinition)]) &&
        (← originalV1EmptyValid db) then
      return some .originalEmpty
    if (← tableColumns db "events") = ["revision", "payload", "operation_id"] &&
        (← tableColumns db "operations") =
          ["operation_id", "request_payload", "payload_digest", "result_digest",
           "start_revision", "end_revision", "history_digest", "receipt"] &&
        (← tableColumns db "artifacts") = ["digest", "size", "payload"] &&
        (← definitionsMatch db [
          ("events", currentEventsDefinition),
          ("operations", currentOperationsDefinition),
          ("artifacts", currentArtifactsDefinition)]) then
      match ← loadFromAt db legacySchemaVersion with
      | .ok _ => return some .predecessor
      | .error _ => return none
    return none
  catch _ => return none

def legacyV1EmptySupported (db : _root_.SQLite) : IO Bool := do
  return (← legacyV1Layout? db) = some .originalEmpty

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
  db.transaction do
    unless ← currentSchemaSupported db do
      return .error (.corrupt "storage schema fingerprint is not canonical")
    diagnoseFrom db

private def lookupProjectionRepair (db : _root_.SQLite) (plan : ProjectionRepairPlan) :
    IO (Option ProjectionRepairReceipt) := do
  let statement ← db.prepare "
    SELECT adopted_digest FROM projection_repairs
    WHERE observed_digest=? AND head_revision=? AND history_digest=?"
  statement.bindText 1 plan.observedDigest
  statement.bindText 2 (toString plan.head.revision.value)
  statement.bindText 3 plan.head.historyDigest.value
  unless ← statement.step do return none
  return some { plan, adoptedDigest := ← statement.columnText 0 }

def repairProjectionWithHook (path : System.FilePath) (plan : ProjectionRepairPlan)
    (afterCommit : IO Unit) : IO (Except OpenError ProjectionRepairReceipt) := do
  if !(← path.pathExists) then return .error .uninitialized
  let db ← _root_.SQLite.openWith path
    { mode := .readWrite, threading := some .fullmutex } (busyTimeoutMs := 5000)
  withWriterLock path do
    let outcome : Except OpenError ProjectionRepairReceipt ←
      db.transaction (mode := .immediate) do
      unless ← currentSchemaSupported db do
        return .error (.corrupt "storage schema fingerprint is not canonical")
      match ← lookupProjectionRepair db plan with
      | some receipt => return .ok receipt
      | none => pure ()
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
    match outcome with
    | .ok _ => afterCommit
    | .error _ => pure ()
    return outcome

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

def mutateWithHook (path : System.FilePath) (operation : OperationId)
    (expectedRevision : Revision) (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef := [])
    (artifactRoot : Option System.FilePath := none)
    (beforeArtifactCommit : IO Unit := pure ())
    (afterArtifactVerification : IO Unit := pure ())
    (afterJournalWrite : IO Unit := pure ()) :
    IO (Except MutationError MutationOutcome) := do
  if !(← path.pathExists) then return .error (.openError .uninitialized)
  let requestPayload := canonicalPayload command artifacts
  let payloadDigest ← DurableFilesystem.digest requestPayload
  let db ← _root_.SQLite.openWith path
    { mode := .readWrite, threading := some .fullmutex } (busyTimeoutMs := 5000)
  withWriterLock path <| db.transaction (mode := .immediate) do
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

def mutate (path : System.FilePath) (operation : OperationId)
    (expectedRevision : Revision) (command : Decide.Command)
    (artifacts : List DurableFilesystem.ArtifactRef := [])
    (artifactRoot : Option System.FilePath := none) :
    IO (Except MutationError MutationOutcome) :=
  mutateWithHook path operation expectedRevision command artifacts artifactRoot

end AgentWorkbench.Adapter.SQLite
