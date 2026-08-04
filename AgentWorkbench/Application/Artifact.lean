import AgentWorkbench.Application.Ledger
import AgentWorkbench.Adapter.Snapshot

namespace AgentWorkbench

structure ArtifactObserveRequest where
  entryId : String
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
  let snapshot ← Snapshot.target projectRoot criterion.target
  match appendCurrentEntry state request.entryId (.artifactObservation {
      criterionId := criterion.id, target := criterion.target, snapshot
      operation := request.operation, result := request.result
      successful := request.successful, producerAgentRun := work.responsibleAgentRun }) with
  | .ok next => pure next
  | .error message => throw (IO.userError message)

end AgentWorkbench
