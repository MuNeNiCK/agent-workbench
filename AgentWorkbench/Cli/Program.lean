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
  | .error error => throw <| IO.userError s!"verified mutation rejected: {repr error}"
  | .ok transaction =>
      match (Application.Service.queryValidity transaction.result).value,
          (Application.Service.resolve transaction.result).value with
      | .pass, .action action =>
          match executeRequest (.action action) transaction.result with
          | .ok response => IO.println s!"agent-workbench verified core: {response.output}"
          | .error error => throw <| IO.userError s!"resolver action rejected: {error}"
      | .blocked reason, _ => throw <| IO.userError reason
      | _, .blocked blocker => throw <| IO.userError s!"resolver blocked: {repr blocker}"

end AgentWorkbench.Cli.Program
