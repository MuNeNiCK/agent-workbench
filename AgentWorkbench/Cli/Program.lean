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

def renderDecision :=
  Application.Service.renderDecision

def renderBootstrap :=
  Application.Service.renderBootstrap

def run : IO Unit := do
  match renderBootstrap executeBootstrap with
  | .ok output => IO.println output
  | .error error => throw <| IO.userError error

end AgentWorkbench.Cli.Program
