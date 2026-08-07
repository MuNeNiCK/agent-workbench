namespace AgentWorkbench.PathPolicy

/-- Checks every path component below an already selected project root without treating symlinks in
the external path used to reach the project root itself as project content. -/
def containsSymlinkBelow
    (root relative : System.FilePath) : IO Bool := do
  let mut current := root
  for component in relative.normalize.components do
    if component == "." then continue
    current := current / component
    if (← current.symlinkMetadata).type == .symlink then return true
  pure false

end AgentWorkbench.PathPolicy
