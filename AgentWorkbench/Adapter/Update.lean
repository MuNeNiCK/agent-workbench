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
      return .current observed
    if observed.schemaVersion = SQLite.legacySchemaVersion &&
        (← SQLite.legacyV1EmptySupported db) then
      return .updateRequired { source := observed, targetVersion := SQLite.schemaVersion }
    return .unsupported observed

private def stagedPath (path : System.FilePath) (digest : String) : System.FilePath :=
  path.parent.getD "." /
    s!".{path.fileName.getD "ledger"}.{digest.replace ":" "-"}.update"

private def applyUnlocked (path backupRoot : System.FilePath) (plan : Plan) : IO Receipt := do
  let observed ← point path
  unless observed = plan.source do
    throw <| IO.userError "update source changed after inspection"
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
      unless ← SQLite.legacyV1EmptySupported db do
        throw <| IO.userError "legacy v1 storage is not safely migratable"
      db.exec "
        ALTER TABLE events ADD COLUMN operation_id TEXT NOT NULL DEFAULT '';
        ALTER TABLE operations ADD COLUMN request_payload BLOB NOT NULL DEFAULT x'';
        ALTER TABLE operations ADD COLUMN start_revision TEXT NOT NULL DEFAULT '0';
        ALTER TABLE operations ADD COLUMN end_revision TEXT NOT NULL DEFAULT '0';
        ALTER TABLE operations ADD COLUMN history_digest TEXT NOT NULL DEFAULT '';
        ALTER TABLE artifacts ADD COLUMN payload BLOB NOT NULL DEFAULT x'';
        CREATE TABLE update_provenance (
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
    match ← SQLite.inspect staged with
    | .error error => throw <| IO.userError s!"staged update failed integrity: {repr error}"
    | .ok _ => pure ()
    let target ← point staged
    unless target.schemaVersion = plan.targetVersion do
      throw <| IO.userError "staged update has the wrong schema version"
    let targetDurability ← DurableFilesystem.replace staged path
    return { source := plan.source, backup, target, targetDurability }
  catch error =>
    if ← staged.pathExists then IO.FS.removeFile staged
    throw error

def apply (path backupRoot : System.FilePath) (plan : Plan) : IO Receipt :=
  SQLite.withWriterLock path (applyUnlocked path backupRoot plan)

private def provenanceMatches (path : System.FilePath) (receipt : Receipt) : IO Bool := do
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

private def restoreUnlocked (path backupRoot : System.FilePath)
    (receipt : Receipt) : IO RestoreReceipt := do
  let observed ← point path
  unless observed = receipt.target do
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
      unless ← db.transaction (SQLite.legacyV1EmptySupported db) do
        throw <| IO.userError "staged legacy backup failed integrity"
    else
      match ← SQLite.inspectAtSchema staged receipt.source.schemaVersion with
      | .ok _ => pure ()
      | .error error =>
          throw <| IO.userError s!"staged backup failed integrity: {repr error}"
    let durability ← DurableFilesystem.replace staged path
    return { restored := receipt.source, durability }
  catch error =>
    if ← staged.pathExists then IO.FS.removeFile staged
    throw error

def restore (path backupRoot : System.FilePath) (receipt : Receipt) : IO RestoreReceipt :=
  SQLite.withWriterLock path (restoreUnlocked path backupRoot receipt)

end AgentWorkbench.Adapter.Update
