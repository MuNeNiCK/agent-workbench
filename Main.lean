import AgentWorkbench.Application.Service

open AgentWorkbench

def main : IO Unit := do
  let state := Application.Service.initialState
  match Application.Service.queryValidity state, Application.Service.resolve state with
  | .pass, .action action => IO.println s!"agent-workbench verified core: {repr action}"
  | .blocked reason, _ => throw <| IO.userError reason
  | _, .blocked blocker => throw <| IO.userError s!"resolver blocked: {repr blocker}"
