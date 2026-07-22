import AgentWorkbench.Application.Service

namespace AgentWorkbench.Cli.Program

open AgentWorkbench

def executeBootstrap :=
  Application.Service.execute Application.Service.bootstrapCommand
    Application.Service.initialState

def run : IO Unit := do
  match executeBootstrap with
  | .error error => throw <| IO.userError s!"verified mutation rejected: {repr error}"
  | .ok transaction =>
      match Application.Service.queryValidity transaction.result.state,
          Application.Service.resolve transaction.result.state with
      | .pass, .action action => IO.println s!"agent-workbench verified core: {repr action}"
      | .blocked reason, _ => throw <| IO.userError reason
      | _, .blocked blocker => throw <| IO.userError s!"resolver blocked: {repr blocker}"

end AgentWorkbench.Cli.Program
