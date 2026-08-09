import AgentWorkbenchTest

private def runSuite : IO Unit := do
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
  AgentWorkbenchTest.Operation.verifyPositiveRouteReceipts
  AgentWorkbenchTest.BinaryProtocol.run
  AgentWorkbenchTest.DesignClaim.run
  IO.println "agent-workbench-tests: pass"

def main (arguments : List String) : IO Unit := do
  match arguments with
  | ["write-artifact", path, content] =>
      IO.FS.writeFile path content
  | ["write-artifact-fail", path, content] =>
      IO.FS.writeFile path content
      throw (IO.userError "intentional command failure")
  | [] => runSuite
  | _ => throw (IO.userError "unknown test-helper arguments")
