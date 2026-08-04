import AgentWorkbenchTest.ProofBuild

def main : IO Unit := do
  AgentWorkbenchTest.ProofBuild.run
  IO.println "agent-workbench-proof-tests: pass"
