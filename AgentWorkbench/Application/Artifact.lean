import AgentWorkbench.Application.Ledger
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure ArtifactObserveRequest where
  entryId : String
  taskEntryId : String
  criterionId : Option String := none
  taskVerificationId : Option String := none
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
  let taskEntry ← match state.entry? request.taskEntryId with
    | some value => pure value
    | none => throw (IO.userError s!"no current Task {request.taskEntryId}")
  if entryIsSuperseded state taskEntry then
    throw (IO.userError s!"Task {request.taskEntryId} is not current")
  let task ← match taskEntry.payload with
    | .task value => pure value
    | _ => throw (IO.userError s!"entry {request.taskEntryId} is not a Task")
  unless request.criterionId.isSome != request.taskVerificationId.isSome do
    throw (IO.userError "artifact observation requires exactly one Criterion or Task verification")
  let (target, criterionId, taskVerificationId) ← match request.criterionId, request.taskVerificationId with
    | some criterionId, none =>
        let criterion ← match design.criterion? criterionId with
          | some value => pure value
          | none => throw (IO.userError s!"no current criterion {criterionId}")
        if !task.verificationCriterionIds.contains criterion.id ||
            !task.outputScopes.contains criterion.target then
          throw (IO.userError "criterion does not verify the selected Task output")
        pure (criterion.target, some criterion.id, none)
    | none, some verificationId =>
        let contract ← match task.taskVerificationContracts.find? (·.id == verificationId) with
          | some value => pure value
          | none => throw (IO.userError s!"no current Task verification {verificationId}")
        if contract.kind != .artifact || !task.outputScopes.contains contract.target then
          throw (IO.userError "Task artifact verification does not verify the selected Task output")
        pure (contract.target, none, some contract.id)
    | _, _ => throw (IO.userError "invalid artifact verification binding")
  let snapshot ← Snapshot.target projectRoot target
  match appendCurrentEntry state request.entryId (.artifactObservation {
      taskEntryId := some taskEntry.id, outputScope := some target
      criterionId, taskVerificationId, target, snapshot
      operation := request.operation, result := request.result
      successful := request.successful, producerAgentRun := work.responsibleAgentRun }) with
  | .ok next => pure next
  | .error message => throw (IO.userError message)

end AgentWorkbench
