import AgentWorkbenchTest

def main : IO Unit := do
  AgentWorkbenchTest.Decision.run
  AgentWorkbenchTest.Adapter.run
  AgentWorkbenchTest.Finding.run
  AgentWorkbenchTest.FindingStore.run
  IO.println "agent-workbench-tests: pass"
