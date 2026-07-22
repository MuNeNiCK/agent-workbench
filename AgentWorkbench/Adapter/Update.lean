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

private def point (path : System.FilePath) : IO StoragePoint := do
  let bytes ← IO.FS.readBinFile path
  let db ← _root_.SQLite.openWith path { mode := .readonly }
  let statement ← db.prepare "SELECT schema_version FROM metadata WHERE singleton = 1"
  unless ← statement.step do throw <| IO.userError "storage metadata is missing"
  return {
    schemaVersion := ← parseVersion (← statement.columnText 0)
    digest := ← DurableFilesystem.digest bytes }

private def inspectUnlocked (path : System.FilePath) : IO Inspection := do
  let observed ← point path
  if observed.schemaVersion = SQLite.schemaVersion then
    return .current observed
  if observed.schemaVersion = 0 then
    return .updateRequired { source := observed, targetVersion := SQLite.schemaVersion }
  return .unsupported observed

def inspect (path : System.FilePath) : IO Inspection :=
  SQLite.withWriterLock path (inspectUnlocked path)

private def stagedPath (path : System.FilePath) (digest : String) : System.FilePath :=
  path.parent.getD "." /
    s!".{path.fileName.getD "ledger"}.{digest.replace ":" "-"}.update"

private def applyUnlocked (path backupRoot : System.FilePath) (plan : Plan) : IO Receipt := do
  let observed ← point path
  unless observed = plan.source do
    throw <| IO.userError "update source changed after inspection"
  unless plan.source.schemaVersion = 0 && plan.targetVersion = SQLite.schemaVersion do
    throw <| IO.userError "unsupported update transition"
  let sourceBytes ← IO.FS.readBinFile path
  let backup ← DurableFilesystem.stage backupRoot sourceBytes
  let staged := stagedPath path plan.source.digest
  IO.FS.writeBinFile staged sourceBytes
  try
    let db ← _root_.SQLite.openWith staged { mode := .readWrite, threading := some .fullmutex }
    db.transaction (mode := .immediate) do
      let statement ← db.prepare
        "UPDATE metadata SET schema_version = ? WHERE singleton = 1 AND schema_version = '0'"
      statement.bindText 1 (toString plan.targetVersion)
      statement.exec
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

private def restoreUnlocked (path backupRoot : System.FilePath)
    (receipt : Receipt) : IO RestoreReceipt := do
  let observed ← point path
  unless observed = receipt.target do
    throw <| IO.userError "restore target changed after inspection"
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
