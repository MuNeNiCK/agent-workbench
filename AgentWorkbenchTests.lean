import AgentWorkbenchTest

def main : IO Unit := do
  AgentWorkbenchTest.Decision.run
  AgentWorkbenchTest.Adapter.run
  IO.println "agent-workbench-tests: pass"
