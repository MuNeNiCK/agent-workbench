import AgentWorkbench.Application.Ledger
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure ArtifactObserveRequest where
  entryId : String
  taskEntryId : String
  criterionId : String
  operation : String
  result : String
  successful : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def observeArtifact
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : ArtifactObserveRequest) : IO ProjectState := do
  let (design, work) ← match currentBinding state with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  let criterion ← match design.criterion? request.criterionId with
    | some value => pure value
    | none => throw (IO.userError s!"no current criterion {request.criterionId}")
  let taskEntry ← match state.entry? request.taskEntryId with
    | some value => pure value
    | none => throw (IO.userError s!"no current Task {request.taskEntryId}")
  if entryIsSuperseded state taskEntry then
    throw (IO.userError s!"Task {request.taskEntryId} is not current")
  let task ← match taskEntry.payload with
    | .task value => pure value
    | _ => throw (IO.userError s!"entry {request.taskEntryId} is not a Task")
  if !task.verificationCriterionIds.contains criterion.id ||
      !task.outputScopes.contains criterion.target then
    throw (IO.userError "criterion does not verify the selected Task output")
  let snapshot ← Snapshot.target projectRoot criterion.target
  match appendCurrentEntry state request.entryId (.artifactObservation {
      taskEntryId := some taskEntry.id, outputScope := some criterion.target
      criterionId := criterion.id, target := criterion.target, snapshot
      operation := request.operation, result := request.result
      successful := request.successful, producerAgentRun := work.responsibleAgentRun }) with
  | .ok next => pure next
  | .error message => throw (IO.userError message)

end AgentWorkbench
