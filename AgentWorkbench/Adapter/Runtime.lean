import AgentWorkbench.Adapter.Process
import AgentWorkbench.Domain.ProofToolchain

namespace AgentWorkbench.Runtime

def toolchain : String := ProofToolchain.identifier

structure Layout where
  elanExecutable : System.FilePath
  elanHome : System.FilePath

def layout (projectRoot : System.FilePath) : Layout :=
  let executable := if System.Platform.isWindows then "elan.exe" else "elan"
  { elanExecutable := projectRoot / ".agent-workbench" / "bin" / executable
    elanHome := projectRoot / ".agent-workbench" / "toolchains" }

def initializeProject (projectRoot : System.FilePath) : IO Unit := do
  let runtime := layout projectRoot
  if !(← runtime.elanExecutable.pathExists) then
    throw (IO.userError s!"bundled project-local Elan is missing: {runtime.elanExecutable}")
  IO.FS.createDirAll runtime.elanHome
  let designRoot := projectRoot / ".agent-workbench" / "design"
  IO.FS.createDirAll (designRoot / "product")
  IO.FS.createDirAll (designRoot / "implementation")
  IO.FS.createDirAll (designRoot / "plans")
  IO.FS.createDirAll (designRoot / "proofs")
  let available ← Process.executeWithOverrides projectRoot {
    executable := runtime.elanExecutable.toString
    arguments := #["run", toolchain, "lean", "--version"] }
    #[("ELAN_HOME", runtime.elanHome.toString)]
  if available.exitCode != 0 then
    let installed ← Process.executeWithOverrides projectRoot {
      executable := runtime.elanExecutable.toString
      arguments := #["toolchain", "install", toolchain] }
      #[("ELAN_HOME", runtime.elanHome.toString)]
    if installed.exitCode != 0 then
      throw (IO.userError s!"project-local Lean toolchain acquisition failed: {installed.stderr}")

end AgentWorkbench.Runtime
