import AgentWorkbench.Adapter.DesignMarkdown
import AgentWorkbench.Adapter.PathPolicy

namespace AgentWorkbench.DesignSource

structure Captured where
  target : String
  mediaKind : String := "markdown"
  digest : String
  content : ByteArray
  units : List DesignSourceUnit

structure Inspection where
  target : String
  digest : String
  units : List DesignSourceUnit
  deriving Lean.ToJson

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def pathWithin (root path : System.FilePath) : Bool :=
  root.normalize.components.isPrefixOf path.normalize.components

private def configuredPath (projectRoot : System.FilePath) (target : String) : IO System.FilePath := do
  unless target.startsWith "file:" do
    fail s!"Design source target must use file: {target}"
  let relative : System.FilePath := (target.drop 5).toString
  if relative.isAbsolute || relative.components.any (· == "..") then
    fail s!"Design source escapes the project: {target}"
  pure (projectRoot / relative)

private def validateLocation
    (projectRoot configured canonical : System.FilePath) (target : String) : IO Unit := do
  let designRoot ← IO.FS.realPath (projectRoot / ".agent-workbench" / "design")
  let productRoot ← IO.FS.realPath (designRoot / "product")
  let implementationRoot ← IO.FS.realPath (designRoot / "implementation")
  unless pathWithin designRoot canonical &&
      (pathWithin productRoot canonical || pathWithin implementationRoot canonical) do
    fail s!"Design source is outside product/ or implementation/: {target}"
  let metadata ← configured.symlinkMetadata
  unless metadata.type == .file do
    fail s!"Design source is not a regular file: {target}"
  if configured.fileName == some "README.md" || configured.extension != some "md" then
    fail s!"Design source must be a non-README Markdown file: {target}"

def capture (projectRoot : System.FilePath) (target : String) : IO Captured := do
  let configured ← configuredPath projectRoot target
  let relative : System.FilePath := (target.drop 5).toString
  unless ← configured.pathExists do fail s!"Design source does not exist: {target}"
  let canonical ← IO.FS.realPath configured
  if ← PathPolicy.containsSymlinkBelow projectRoot relative then
    fail s!"Design source path contains a symlink: {target}"
  validateLocation projectRoot configured canonical target
  let content ← IO.FS.readBinFile canonical
  let source ← match String.fromUTF8? content with
    | some value => pure value
    | none => fail s!"Design Markdown is not valid UTF-8: {target}"
  let units ← match DesignMarkdown.inspect target source with
    | .ok value => pure value
    | .error message => fail message
  pure { target, mediaKind := "markdown", digest := ContentDigest.bytes content, content, units }

def captureAll (projectRoot : System.FilePath) (targets : List String) : IO (List Captured) := do
  if targets.isEmpty then fail "Design proposal requires at least one source"
  if !(targets.all fun target => targets.count target == 1) then
    fail "Design proposal contains duplicate source targets"
  let mut captured := []
  for target in targets do captured := captured ++ [← capture projectRoot target]
  pure captured

def inspectAll (projectRoot : System.FilePath) (targets : List String) : IO (List Inspection) := do
  let captured ← captureAll projectRoot targets
  pure (captured.map fun source =>
    { target := source.target, digest := source.digest, units := source.units }
  )

end AgentWorkbench.DesignSource
