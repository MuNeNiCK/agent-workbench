import AgentWorkbench.Application.Service

namespace AgentWorkbench.Cli.Program

open AgentWorkbench

abbrev Request := Application.Service.Request
abbrev Response := Application.Service.Response

def executeRequest :=
  Application.Service.executeRequest

def executeBootstrap :=
  Application.Service.execute Application.Service.bootstrapCommand
    Application.Service.initialStore

def run : IO Unit := do
  match executeBootstrap with
  | .error _ => throw <| IO.userError "verified mutation rejected"
  | .ok transaction =>
      match (Application.Service.queryValidity transaction.result).value,
          (Application.Service.resolve transaction.result).value with
      | .pass, .action action =>
          match executeRequest (.action action) transaction.result with
          | .ok response => IO.println s!"agent-workbench verified core: {response.output}"
          | .error error => throw <| IO.userError s!"resolver action rejected: {error}"
      | .blocked _, _ => throw <| IO.userError "verified state blocked"
      | _, .blocked _ => throw <| IO.userError "resolver blocked"

end AgentWorkbench.Cli.Program
