import AgentWorkbench.Application.Service

open AgentWorkbench

def main : IO Unit := do
  let state := Application.Service.initialState
  match Application.Service.queryValidity state, Application.Service.resolve state with
  | .pass, some action => IO.println s!"agent-workbench verified core: {repr action}"
  | .blocked reason, _ => throw <| IO.userError reason
  | _, none => throw <| IO.userError "resolver produced no allowed action"
