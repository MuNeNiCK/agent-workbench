import Lean.Data.Json
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Domain.Validation.OutputScope

namespace AgentWorkbench.ManagedOutput

inductive Kind where
  | file | tree
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Node where
  relativePath : String
  directory : Bool
  contentBytes : List Nat := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Baseline where
  identity : String
  kind : Kind
  existed : Bool
  nodes : List Node := []
  digest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def fail (message : String) : IO α := throw (IO.userError message)

private def configuredPath (projectRoot : System.FilePath) (identity : String) : IO (Kind × System.FilePath) := do
  match AgentWorkbench.Validation.validateManagedOutputScope identity with
  | .error message => fail message
  | .ok _ => pure ()
  let (kind, source) ← if identity.startsWith "file:" then
      pure (.file, (identity.drop 5).toString)
    else if identity.startsWith "tree:" then
      pure (.tree, (identity.drop 5).toString)
    else fail s!"managed output has unsupported identity: {identity}"
  let configured : System.FilePath := source
  if configured.isAbsolute || configured.components.any (· == "..") || source.isEmpty then
    fail s!"managed output must be a project-relative path: {identity}"
  let mut current := projectRoot
  for component in configured.components do
    current := current / component
    if ← current.pathExists then
      let metadata ← current.symlinkMetadata
      if metadata.type == .symlink then
        fail s!"managed output path traverses a symlink: {identity}"
  pure (kind, projectRoot / configured)

private def bytes (values : List Nat) : ByteArray :=
  ByteArray.mk (values.toArray.map UInt8.ofNat)

private def removeCurrent (path : System.FilePath) : IO Unit := do
  if ← path.pathExists then
    let metadata ← path.symlinkMetadata
    match metadata.type with
    | .file => IO.FS.removeFile path
    | .dir => IO.FS.removeDirAll path
    | .symlink | .other => fail s!"managed output became a symlink or special file: {path}"

def capture (projectRoot : System.FilePath) (identity : String) : IO Baseline := do
  let (kind, path) ← configuredPath projectRoot identity
  let existed ← path.pathExists
  let mut nodes := []
  if existed then
    let metadata ← path.symlinkMetadata
    match kind, metadata.type with
    | .file, .file =>
        nodes := [{
          relativePath := ""
          directory := false
          contentBytes := (← IO.FS.readBinFile path).data.toList.map (·.toNat)
        }]
    | .tree, .dir =>
        let root ← IO.FS.realPath path
        let prefixLength := root.toString.length + 1
        let entries ← root.walkDir
        for entry in entries do
          let metadata ← entry.symlinkMetadata
          let relative := (entry.toString.drop prefixLength).toString
          match metadata.type with
          | .dir => nodes := nodes ++ [{ relativePath := relative, directory := true }]
          | .file => nodes := nodes ++ [{
              relativePath := relative
              directory := false
              contentBytes := (← IO.FS.readBinFile entry).data.toList.map (·.toNat)
            }]
          | .symlink | .other => fail s!"managed output contains a symlink or special file: {entry}"
    | .file, _ => fail s!"managed file output is not a regular file: {path}"
    | .tree, _ => fail s!"managed tree output is not a directory: {path}"
  let ordered := nodes.mergeSort (fun left right => left.relativePath < right.relativePath)
  let material := Lean.toJson (kind, existed, ordered) |>.compress
  pure { identity, kind, existed, nodes := ordered, digest := ContentDigest.string material }

def restore (projectRoot : System.FilePath) (baseline : Baseline) : IO Unit := do
  let (kind, path) ← configuredPath projectRoot baseline.identity
  if kind != baseline.kind then fail "managed output kind differs from its durable baseline"
  let material := Lean.toJson (baseline.kind, baseline.existed, baseline.nodes) |>.compress
  if ContentDigest.string material != baseline.digest then
    fail "managed output baseline digest is invalid"
  for node in baseline.nodes do
    let relative : System.FilePath := node.relativePath
    if relative.isAbsolute || relative.components.any (· == "..") ||
        (baseline.kind == .tree && node.relativePath.isEmpty) then
      fail "managed output baseline contains an escaping or empty tree path"
  removeCurrent path
  if baseline.existed then
    match baseline.kind with
    | .file =>
        let node ← match baseline.nodes with
          | [value] => pure value
          | _ => fail "managed file baseline has an invalid node set"
        if node.directory || !node.relativePath.isEmpty then
          fail "managed file baseline node is invalid"
        if let some parent := path.parent then IO.FS.createDirAll parent
        IO.FS.writeBinFile path (bytes node.contentBytes)
    | .tree =>
        IO.FS.createDirAll path
        for node in baseline.nodes.filter (·.directory) do
          IO.FS.createDirAll (path / node.relativePath)
        for node in baseline.nodes.filter (!·.directory) do
          let output := path / node.relativePath
          if let some parent := output.parent then IO.FS.createDirAll parent
          IO.FS.writeBinFile output (bytes node.contentBytes)

end AgentWorkbench.ManagedOutput
