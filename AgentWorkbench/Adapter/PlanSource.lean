import AgentWorkbench.Adapter.DesignMarkdown
import AgentWorkbench.Adapter.PathPolicy

namespace AgentWorkbench.PlanSource

structure Captured where
  target : String
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

def capture
    (projectRoot : System.FilePath) (workId target : String) : IO Captured := do
  unless target.startsWith "file:" do fail s!"Plan source target must use file: {target}"
  let relative : System.FilePath := (target.drop 5).toString
  if relative.isAbsolute || relative.components.any (· == "..") then
    fail s!"Plan source escapes the project: {target}"
  let configured := projectRoot / relative
  unless ← configured.pathExists do fail s!"Plan source does not exist: {target}"
  let canonical ← IO.FS.realPath configured
  if ← PathPolicy.containsSymlinkBelow projectRoot relative then
    fail s!"Plan source path contains a symlink: {target}"
  let planRoot ← IO.FS.realPath
    (projectRoot / ".agent-workbench" / "design" / "plans" / workId)
  unless pathWithin planRoot canonical do
    fail s!"Plan source is outside plans/{workId}/: {target}"
  let metadata ← configured.symlinkMetadata
  unless metadata.type == .file do fail s!"Plan source is not a regular file: {target}"
  if configured.fileName == some "README.md" || configured.extension != some "md" then
    fail s!"Plan source must be a non-README Markdown file: {target}"
  let content ← IO.FS.readBinFile canonical
  let source ← match String.fromUTF8? content with
    | some value => pure value
    | none => fail s!"Plan Markdown is not valid UTF-8: {target}"
  let units ← match DesignMarkdown.inspect target source with
    | .ok value => pure value
    | .error message => fail message
  pure { target, digest := ContentDigest.bytes content, content, units }

def captureAll
    (projectRoot : System.FilePath) (workId : String) (targets : List String) :
    IO (List Captured) := do
  if targets.isEmpty then fail "Plan proposal requires at least one source"
  if !(targets.all fun target => targets.count target == 1) then
    fail "Plan proposal contains duplicate source targets"
  let mut captured := []
  for target in targets do captured := captured ++ [← capture projectRoot workId target]
  pure captured

def inspectAll
    (projectRoot : System.FilePath) (workId : String) (targets : List String) :
    IO (List Inspection) := do
  let captured ← captureAll projectRoot workId targets
  pure (captured.map fun source =>
    { target := source.target, digest := source.digest, units := source.units })

end AgentWorkbench.PlanSource
