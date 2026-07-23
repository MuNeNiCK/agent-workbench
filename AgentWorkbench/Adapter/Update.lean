import AgentWorkbench.Adapter.SQLite

namespace AgentWorkbench.Adapter.Update

open AgentWorkbench.Adapter

structure StoragePoint where
  schemaVersion : Nat
  digest : String
deriving DecidableEq, Repr

structure Plan where
  source : StoragePoint
  targetVersion : Nat
deriving DecidableEq, Repr

inductive Inspection
  | current (point : StoragePoint)
  | updateRequired (plan : Plan)
  | unsupported (point : StoragePoint)
deriving DecidableEq, Repr

structure Receipt where
  source : StoragePoint
  backup : DurableFilesystem.ArtifactRef
  target : StoragePoint
  targetDurability : DurableFilesystem.ReplacementDurability
deriving DecidableEq, Repr

structure RestoreReceipt where
  restored : StoragePoint
  durability : DurableFilesystem.ReplacementDurability
deriving DecidableEq, Repr

private def parseVersion (value : String) : IO Nat :=
  match value.toNat? with
  | some version => pure version
  | none => throw <| IO.userError "storage schema version is invalid"

private def pointFrom (db : _root_.SQLite) (path : System.FilePath) : IO StoragePoint := do
  let statement ← db.prepare "SELECT schema_version FROM metadata WHERE singleton = 1"
  unless ← statement.step do throw <| IO.userError "storage metadata is missing"
  let version ← parseVersion (← statement.columnText 0)
  let bytes ← IO.FS.readBinFile path
  return {
    schemaVersion := version
    digest := ← DurableFilesystem.digest bytes }

private def point (path : System.FilePath) : IO StoragePoint := do
  let db ← _root_.SQLite.openWith path { mode := .readonly, threading := some .fullmutex }
  db.transaction (pointFrom db path)

def inspect (path : System.FilePath) : IO Inspection := do
  let db ← _root_.SQLite.openWith path { mode := .readonly, threading := some .fullmutex }
  db.transaction do
    let observed ← pointFrom db path
    if observed.schemaVersion = SQLite.schemaVersion then
      if (← SQLite.currentSchemaSupported db) then
        match ← SQLite.inspectFromAt db SQLite.schemaVersion with
        | .ok _ => return .current observed
        | .error _ => return .unsupported observed
      return .unsupported observed
    if observed.schemaVersion = SQLite.legacySchemaVersion then
      match ← SQLite.legacyV1Layout? db with
      | some _ => return .updateRequired { source := observed, targetVersion := SQLite.schemaVersion }
      | none => pure ()
    return .unsupported observed

private def stagedPath (path : System.FilePath) (digest : String) : System.FilePath :=
  path.parent.getD "." /
    s!".{path.fileName.getD "ledger"}.{digest.replace ":" "-"}.update"

private def operationJournalPath (backupRoot : System.FilePath) : System.FilePath :=
  backupRoot / ".update-operations.sqlite3"

private def durabilityText : DurableFilesystem.ReplacementDurability → String
  | .confirmed => "confirmed"
  | .uncertain => "uncertain"

private def parseDurability (value : String) : IO DurableFilesystem.ReplacementDurability :=
  match value with
  | "confirmed" => pure .confirmed
  | "uncertain" => pure .uncertain
  | _ => throw <| IO.userError "update operation durability is invalid"

private def operationJournal (backupRoot : System.FilePath) : IO _root_.SQLite := do
  IO.FS.createDirAll backupRoot
  let db ← _root_.SQLite.openWith (operationJournalPath backupRoot)
    { mode := .readWriteCreate, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.exec "
    PRAGMA journal_mode=DELETE;
    PRAGMA synchronous=FULL;
    CREATE TABLE IF NOT EXISTS replacement_operations (
      kind TEXT NOT NULL,
      source_schema TEXT NOT NULL,
      source_digest TEXT NOT NULL,
      target_schema TEXT NOT NULL,
      target_digest TEXT NOT NULL,
      backup_digest TEXT NOT NULL,
      backup_size TEXT NOT NULL,
      durability TEXT NOT NULL,
      PRIMARY KEY (kind, source_schema, source_digest, target_schema, target_digest,
                   backup_digest, backup_size)
    );"
  return db

private def recordReplacement (backupRoot : System.FilePath) (kind : String)
    (source target : StoragePoint) (backup : DurableFilesystem.ArtifactRef)
    (durability : DurableFilesystem.ReplacementDurability) : IO Unit := do
  let db ← operationJournal backupRoot
  db.transaction (mode := .immediate) do
    let statement ← db.prepare "
      INSERT INTO replacement_operations
        (kind, source_schema, source_digest, target_schema, target_digest,
         backup_digest, backup_size, durability)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(kind, source_schema, source_digest, target_schema, target_digest,
                  backup_digest, backup_size)
      DO UPDATE SET durability=excluded.durability"
    statement.bindText 1 kind
    statement.bindText 2 (toString source.schemaVersion)
    statement.bindText 3 source.digest
    statement.bindText 4 (toString target.schemaVersion)
    statement.bindText 5 target.digest
    statement.bindText 6 backup.digest
    statement.bindText 7 (toString backup.size)
    statement.bindText 8 (durabilityText durability)
    statement.exec

private def lookupApplyReplacement (backupRoot : System.FilePath) (plan : Plan) :
    IO (Option Receipt) := do
  let journal := operationJournalPath backupRoot
  if !(← journal.pathExists) then return none
  let db ← _root_.SQLite.openWith journal
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.transaction do
    let statement ← db.prepare "
      SELECT target_schema, target_digest, backup_digest, backup_size, durability
      FROM replacement_operations
      WHERE kind='apply' AND source_schema=? AND source_digest=? AND target_schema=?"
    statement.bindText 1 (toString plan.source.schemaVersion)
    statement.bindText 2 plan.source.digest
    statement.bindText 3 (toString plan.targetVersion)
    unless ← statement.step do return none
    let targetSchema ← parseVersion (← statement.columnText 0)
    let targetDigest ← statement.columnText 1
    let backupDigest ← statement.columnText 2
    let backupSize ← parseVersion (← statement.columnText 3)
    let durability ← parseDurability (← statement.columnText 4)
    return some {
      source := plan.source
      backup := { digest := backupDigest, size := backupSize }
      target := { schemaVersion := targetSchema, digest := targetDigest }
      targetDurability := durability }

private def lookupRestoreReplacement (backupRoot : System.FilePath) (receipt : Receipt) :
    IO (Option RestoreReceipt) := do
  let journal := operationJournalPath backupRoot
  if !(← journal.pathExists) then return none
  let db ← _root_.SQLite.openWith journal
    { mode := .readonly, threading := some .fullmutex } (busyTimeoutMs := 5000)
  db.transaction do
    let statement ← db.prepare "
      SELECT durability FROM replacement_operations
      WHERE kind='restore' AND source_schema=? AND source_digest=?
        AND target_schema=? AND target_digest=? AND backup_digest=? AND backup_size=?"
    statement.bindText 1 (toString receipt.target.schemaVersion)
    statement.bindText 2 receipt.target.digest
    statement.bindText 3 (toString receipt.source.schemaVersion)
    statement.bindText 4 receipt.source.digest
    statement.bindText 5 receipt.backup.digest
    statement.bindText 6 (toString receipt.backup.size)
    unless ← statement.step do return none
    return some { restored := receipt.source, durability := ← parseDurability (← statement.columnText 0) }

private def provenanceMatches (path : System.FilePath) (receipt : Receipt) : IO Bool := do
  try
    let db ← _root_.SQLite.openWith path { mode := .readonly, threading := some .fullmutex }
    db.transaction do
      let statement ← db.prepare "
        SELECT source_schema, source_digest, backup_digest, backup_size
        FROM update_provenance WHERE singleton = 1"
      unless ← statement.step do return false
      return (← statement.columnText 0) = toString receipt.source.schemaVersion &&
        (← statement.columnText 1) = receipt.source.digest &&
        (← statement.columnText 2) = receipt.backup.digest &&
        (← statement.columnText 3) = toString receipt.backup.size
  catch _ => return false

private def applyUnlocked (path backupRoot : System.FilePath) (plan : Plan)
    (afterReplacement : IO Unit) : IO Receipt := do
  let observed ← point path
  unless observed = plan.source do
    match ← lookupApplyReplacement backupRoot plan with
    | some receipt =>
        unless observed = receipt.target && (← provenanceMatches path receipt) do
          throw <| IO.userError "update source changed after inspection"
        match ← SQLite.inspect path with
        | .ok _ => pure ()
        | .error error => throw <| IO.userError s!"adopted update is invalid: {repr error}"
        match ← DurableFilesystem.verify backupRoot receipt.backup with
        | .valid => return receipt
        | _ => throw <| IO.userError "adopted update backup is unavailable"
    | none => throw <| IO.userError "update source changed after inspection"
  unless plan.source.schemaVersion = SQLite.legacySchemaVersion &&
      plan.targetVersion = SQLite.schemaVersion do
    throw <| IO.userError "unsupported update transition"
  let sourceBytes ← IO.FS.readBinFile path
  let backup ← DurableFilesystem.stage backupRoot sourceBytes
  let staged := stagedPath path plan.source.digest
  IO.FS.writeBinFile staged sourceBytes
  try
    let db ← _root_.SQLite.openWith staged { mode := .readWrite, threading := some .fullmutex }
    db.transaction (mode := .immediate) do
      let layout ← match ← SQLite.legacyV1Layout? db with
        | some value => pure value
        | none => throw <| IO.userError "legacy v1 storage is not safely migratable"
      match layout with
      | .originalEmpty => db.exec "
          DROP TABLE events;
          CREATE TABLE events (
            revision INTEGER PRIMARY KEY CHECK (revision > 0),
            payload BLOB NOT NULL,
            operation_id TEXT NOT NULL
          );
          DROP TABLE operations;
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
          DROP TABLE artifacts;
          CREATE TABLE artifacts (
            digest TEXT PRIMARY KEY,
            size TEXT NOT NULL,
            payload BLOB NOT NULL
          );"
      | .predecessor => pure ()
      db.exec "CREATE TABLE update_provenance (
          singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
          source_schema TEXT NOT NULL,
          source_digest TEXT NOT NULL,
          backup_digest TEXT NOT NULL,
          backup_size TEXT NOT NULL
        );"
      let provenance ← db.prepare "INSERT INTO update_provenance VALUES (1, ?, ?, ?, ?)"
      provenance.bindText 1 (toString plan.source.schemaVersion)
      provenance.bindText 2 plan.source.digest
      provenance.bindText 3 backup.digest
      provenance.bindText 4 (toString backup.size)
      provenance.exec
      let version ← db.prepare
        "UPDATE metadata SET schema_version = ? WHERE singleton = 1 AND schema_version = ?"
      version.bindText 1 (toString plan.targetVersion)
      version.bindText 2 (toString plan.source.schemaVersion)
      version.exec
    let stagedDb ← _root_.SQLite.openWith staged
      { mode := .readonly, threading := some .fullmutex }
    stagedDb.transaction do
      unless ← SQLite.currentSchemaSupported stagedDb do
        throw <| IO.userError "staged update schema fingerprint is invalid"
      match ← SQLite.inspectFromAt stagedDb SQLite.schemaVersion with
      | .error error => throw <| IO.userError s!"staged update failed integrity: {repr error}"
      | .ok _ => pure ()
    let target ← point staged
    unless target.schemaVersion = plan.targetVersion do
      throw <| IO.userError "staged update has the wrong schema version"
    recordReplacement backupRoot "apply" plan.source target backup .uncertain
    let targetDurability ← DurableFilesystem.replace staged path
    afterReplacement
    recordReplacement backupRoot "apply" plan.source target backup targetDurability
    return { source := plan.source, backup, target, targetDurability }
  catch error =>
    if ← staged.pathExists then IO.FS.removeFile staged
    throw error

def applyWithHook (path backupRoot : System.FilePath) (plan : Plan)
    (afterReplacement : IO Unit) : IO Receipt :=
  SQLite.withWriterLock path (applyUnlocked path backupRoot plan afterReplacement)

def apply (path backupRoot : System.FilePath) (plan : Plan) : IO Receipt :=
  applyWithHook path backupRoot plan (pure ())

private def restoreUnlocked (path backupRoot : System.FilePath)
    (receipt : Receipt) (afterReplacement : IO Unit) : IO RestoreReceipt := do
  let observed ← point path
  unless observed = receipt.target do
    if observed = receipt.source then
      match ← lookupRestoreReplacement backupRoot receipt with
      | some recovered =>
          if receipt.source.schemaVersion = SQLite.legacySchemaVersion then
            let db ← _root_.SQLite.openWith path
              { mode := .readonly, threading := some .fullmutex }
            unless (← db.transaction (SQLite.legacyV1Layout? db)).isSome do
              throw <| IO.userError "reconciled restore source is invalid"
          else
            match ← SQLite.inspectAtSchema path receipt.source.schemaVersion with
            | .ok _ => pure ()
            | .error error => throw <| IO.userError s!"reconciled restore is invalid: {repr error}"
          return recovered
      | none => pure ()
    throw <| IO.userError "restore target changed after inspection"
  unless ← provenanceMatches path receipt do
    throw <| IO.userError "restore receipt is not bound to the adopted update"
  match ← DurableFilesystem.verify backupRoot receipt.backup with
  | .valid => pure ()
  | other => throw <| IO.userError s!"backup is not restorable: {repr other}"
  let bytes ← IO.FS.readBinFile (DurableFilesystem.objectPath backupRoot receipt.backup)
  let staged := stagedPath path receipt.backup.digest
  IO.FS.writeBinFile staged bytes
  try
    let stagedPoint ← point staged
    unless stagedPoint = receipt.source do
      throw <| IO.userError "staged backup is not the update receipt source"
    if receipt.source.schemaVersion = SQLite.legacySchemaVersion then
      let db ← _root_.SQLite.openWith staged
        { mode := .readonly, threading := some .fullmutex }
      unless (← db.transaction (SQLite.legacyV1Layout? db)).isSome do
        throw <| IO.userError "staged legacy backup failed integrity"
    else
      let db ← _root_.SQLite.openWith staged
        { mode := .readonly, threading := some .fullmutex }
      match ← db.transaction (SQLite.inspectFromAt db receipt.source.schemaVersion) with
      | .ok _ => pure ()
      | .error error => throw <| IO.userError s!"staged backup failed integrity: {repr error}"
    recordReplacement backupRoot "restore" receipt.target receipt.source receipt.backup .uncertain
    let durability ← DurableFilesystem.replace staged path
    afterReplacement
    recordReplacement backupRoot "restore" receipt.target receipt.source receipt.backup durability
    return { restored := receipt.source, durability }
  catch error =>
    if ← staged.pathExists then IO.FS.removeFile staged
    throw error

def restoreWithHook (path backupRoot : System.FilePath) (receipt : Receipt)
    (afterReplacement : IO Unit) : IO RestoreReceipt :=
  SQLite.withWriterLock path (restoreUnlocked path backupRoot receipt afterReplacement)

def restore (path backupRoot : System.FilePath) (receipt : Receipt) : IO RestoreReceipt :=
  restoreWithHook path backupRoot receipt (pure ())

end AgentWorkbench.Adapter.Update
