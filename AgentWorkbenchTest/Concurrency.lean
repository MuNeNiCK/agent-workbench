import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store

namespace AgentWorkbenchTest.Concurrency

open AgentWorkbench AgentWorkbenchTest

private def succeeded : Except IO.Error AgentWorkbench.MutationResult → Bool
  | .ok _ => true
  | .error _ => false

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let workbenchRoot := root / ".agent-workbench"
    let database := workbenchRoot / "state.db"
    IO.FS.createDirAll workbenchRoot
    let _ ← Store.open database
    let first : WorkStartRequest :=
      { id := "work-concurrent-a", outcome := "first concurrent mutation"
        scope := "project", responsibleAgentRun := "agent-a" }
    let second : WorkStartRequest :=
      { id := "work-concurrent-b", outcome := "second concurrent mutation"
        scope := "project", responsibleAgentRun := "agent-b" }
    let firstTask ← IO.asTask
      (Store.executeMutation root database (.workStart first)) Task.Priority.dedicated
    let secondTask ← IO.asTask
      (Store.executeMutation root database (.workStart second)) Task.Priority.dedicated
    let firstResult := firstTask.get
    let secondResult := secondTask.get
    expect (succeeded firstResult != succeeded secondResult)
      "concurrent mutations did not serialize into exactly one accepted transition"
    let store ← Store.openReadOnly database
    let state ← Store.loadState store
    expect (state.revision == 1)
      "concurrent mutation rejection changed the authoritative revision"
    expect (state.works.length == 1 && state.focusedWorkId == state.works.head?.map (·.id))
      "concurrent mutations created multiple Work authorities or an invalid focus selector"

end AgentWorkbenchTest.Concurrency
