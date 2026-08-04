import AgentWorkbench.Domain.Design
import AgentWorkbench.Adapter.ContentDigest

namespace AgentWorkbench.Process

structure Result where
  exitCode : Nat
  stdout : String
  stderr : String
  stdoutDigest : String
  stderrDigest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def workingDirectory
    (projectRoot : System.FilePath) (configured : Option String) : Option System.FilePath :=
  match configured with
  | none => some projectRoot
  | some path =>
      let value : System.FilePath := path
      some (if value.isAbsolute then value else projectRoot / value)

def execute (projectRoot : System.FilePath) (command : CommandSpec) : IO Result := do
  let output ← IO.Process.output {
    cmd := command.executable
    args := command.arguments
    cwd := workingDirectory projectRoot command.workingDirectory
    env := command.environment.map (fun (key, value) => (key, some value)) }
  pure {
    exitCode := output.exitCode.toNat
    stdout := output.stdout
    stderr := output.stderr
    stdoutDigest := ContentDigest.string output.stdout
    stderrDigest := ContentDigest.string output.stderr }

end AgentWorkbench.Process
