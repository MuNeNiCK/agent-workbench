import AgentWorkbench.Adapter.ContentDigest

namespace AgentWorkbench.Snapshot

private def ignoredTreeDirectory (path : System.FilePath) : Bool :=
  match path.fileName with
  | some name => [".git", ".lake", "dist", "site"].contains name
  | none => false

private def frame (value : String) : String :=
  s!"{value.toUTF8.size}:{value}"

private def tree (configured : System.FilePath) : IO String := do
  let root ← IO.FS.realPath configured
  if !(← root.isDir) then
    throw (IO.userError s!"tree snapshot target is not a directory: {configured}")
  let prefixLength := root.toString.length + 1
  let paths ← root.walkDir (fun path => pure (!ignoredTreeDirectory path))
  let mut records : List (String × String) := []
  for path in paths do
    if !ignoredTreeDirectory path then
      let relative := (path.toString.drop prefixLength).toString
      let metadata ← path.symlinkMetadata
      match metadata.type with
      | .dir => records := records ++ [(relative, "directory")]
      | .file => records := records ++ [(relative, ← ContentDigest.file path)]
      | .symlink => throw (IO.userError s!"tree snapshot target contains a symlink: {path}")
      | .other => throw (IO.userError s!"tree snapshot target contains a special file: {path}")
  let sorted := records.mergeSort (fun left right => left.1 < right.1)
  let material := sorted.foldl (fun value record =>
    value ++ frame record.1 ++ frame record.2) ""
  pure (ContentDigest.string material)

def target (projectRoot : System.FilePath) (identity : String) : IO String := do
  if identity.startsWith "file:" then
    let configured : System.FilePath := (identity.drop 5).toString
    let path := if configured.isAbsolute then configured else projectRoot / configured
    if ← path.pathExists then
      ContentDigest.file path
    else
      pure "missing"
  else if identity.startsWith "tree:" then
    let configured : System.FilePath := (identity.drop 5).toString
    let path := if configured.isAbsolute then configured else projectRoot / configured
    if ← path.pathExists then tree path else pure "missing"
  else
    throw (IO.userError s!"unsupported snapshot target: {identity}")

def requiredTarget (projectRoot : System.FilePath) (identity : String) : IO String := do
  let snapshot ← target projectRoot identity
  if snapshot == "missing" then
    throw (IO.userError s!"required snapshot target does not exist: {identity}")
  pure snapshot

end AgentWorkbench.Snapshot
