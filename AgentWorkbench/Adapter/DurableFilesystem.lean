import SQLite

namespace AgentWorkbench.Adapter.DurableFilesystem

structure ArtifactRef where
  digest : String
  size : Nat
deriving DecidableEq, Repr

inductive Verification
  | valid
  | missing
  | mismatch (observed : String)
deriving DecidableEq, Repr

@[extern "aw_stage_durable_file"]
private opaque stageDurableFile (temporary final : @& String)
    (bytes : @& ByteArray) : IO UInt32

@[extern "aw_create_durable_directory"]
private opaque createDurableDirectory (path : @& String) : IO Unit

@[extern "aw_replace_durable_file"]
private opaque replaceDurableFile (staged current : @& String) : IO UInt32

inductive ReplacementDurability
  | confirmed
  | uncertain
deriving DecidableEq, Repr

def digest (bytes : ByteArray) : IO String := do
  let db ← _root_.SQLite.open ":memory:"
  db.enableSha3
  let statement ← db.prepare "SELECT lower(hex(sha3(?, 256)))"
  statement.bindBlob 1 bytes
  unless ← statement.step do
    throw <| IO.userError "SQLite SHA3 returned no digest"
  return s!"sha3-256:{← statement.columnText 0}"

private def objectName (digest : String) : String :=
  digest.replace ":" "-"

def objectPath (root : System.FilePath) (reference : ArtifactRef) : System.FilePath :=
  root / objectName reference.digest

def verify (root : System.FilePath) (reference : ArtifactRef) : IO Verification := do
  let path := objectPath root reference
  if !(← path.pathExists) then return .missing
  let bytes ← IO.FS.readBinFile path
  let observed ← digest bytes
  if observed = reference.digest && bytes.size = reference.size then
    return .valid
  return .mismatch observed

def stage (root : System.FilePath) (bytes : ByteArray) : IO ArtifactRef := do
  createDurableDirectory root.toString
  let reference : ArtifactRef := { digest := ← digest bytes, size := bytes.size }
  let final := objectPath root reference
  let temporary := root / s!".{objectName reference.digest}.stage"
  let _ ← stageDurableFile temporary.toString final.toString bytes
  match ← verify root reference with
  | .valid => return reference
  | .missing => throw <| IO.userError "durable artifact adoption produced no object"
  | .mismatch observed =>
      throw <| IO.userError s!"durable artifact digest mismatch: {observed}"

def replace (staged current : System.FilePath) : IO ReplacementDurability := do
  if (← replaceDurableFile staged.toString current.toString) = 0 then
    return .confirmed
  return .uncertain

structure Reconciliation where
  missing : List ArtifactRef
  mismatched : List (ArtifactRef × String)
deriving Repr

def reconcile (root : System.FilePath) (references : List ArtifactRef) : IO Reconciliation := do
  let mut missing := []
  let mut mismatched := []
  for reference in references do
    match ← verify root reference with
    | .valid => pure ()
    | .missing => missing := reference :: missing
    | .mismatch observed => mismatched := (reference, observed) :: mismatched
  return { missing := missing.reverse, mismatched := mismatched.reverse }

end AgentWorkbench.Adapter.DurableFilesystem
