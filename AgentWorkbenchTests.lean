import AgentWorkbenchTest

def main : IO Unit := do
  AgentWorkbenchTest.Lifecycle.run
  AgentWorkbenchTest.Completion.run
  AgentWorkbenchTest.Plan.run
  AgentWorkbenchTest.Review.run
  AgentWorkbenchTest.Operation.run
  AgentWorkbenchTest.ManagedRecovery.run
  AgentWorkbenchTest.Concurrency.run
  AgentWorkbenchTest.Atomicity.run
  AgentWorkbenchTest.Schema.run
  AgentWorkbenchTest.Markdown.run
  AgentWorkbenchTest.DesignArchive.run
  AgentWorkbenchTest.Guidance.run
  AgentWorkbenchTest.PublicRoute.run
  AgentWorkbenchTest.PublicDesignWorkRoute.run
  AgentWorkbenchTest.BuildBoundary.run
  AgentWorkbenchTest.Migration.run
  AgentWorkbenchTest.BinaryProtocol.run
  AgentWorkbenchTest.DesignClaim.run
  IO.println "agent-workbench-tests: pass"
