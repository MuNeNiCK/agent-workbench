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

abbrev Environment := Array (String × Option String)

def resolveEnvironment (names : Array String) : IO Environment := do
  let mut resolved := #[]
  for name in names do
    resolved := resolved.push (name, ← IO.getEnv name)
  pure resolved

def environmentIdentity (name : String) (value : Option String) : String × String :=
  (s!"env:{name}", ContentDigest.string (value.map ("present:" ++ ·) |>.getD "absent"))

def executeResolved
    (projectRoot : System.FilePath) (command : CommandSpec) (environment : Environment) : IO Result := do
  let output ← IO.Process.output {
    cmd := command.executable
    args := command.arguments
    cwd := workingDirectory projectRoot command.workingDirectory
    env := environment }
  pure {
    exitCode := output.exitCode.toNat
    stdout := output.stdout
    stderr := output.stderr
    stdoutDigest := ContentDigest.string output.stdout
    stderrDigest := ContentDigest.string output.stderr }

def execute (projectRoot : System.FilePath) (command : CommandSpec) : IO Result := do
  executeResolved projectRoot command (← resolveEnvironment command.environment)

def executeWithOverrides
    (projectRoot : System.FilePath) (command : CommandSpec)
    (overrides : Array (String × String)) : IO Result := do
  let inherited ← resolveEnvironment command.environment
  let environment := overrides.foldl (fun current override =>
    (current.filter (·.1 != override.1)).push (override.1, some override.2)) inherited
  executeResolved projectRoot command environment

end AgentWorkbench.Process
