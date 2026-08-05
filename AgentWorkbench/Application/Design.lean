import AgentWorkbench.Application.Common
import AgentWorkbench.Adapter.Snapshot

namespace AgentWorkbench

structure DesignProposalRequest where
  producerAgentRun : String
  sourceDocumentTargets : List String := []
  statements : List Statement
  acceptanceCriteria : List AcceptanceCriterion
  leanClaims : List LeanClaim := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def nextDesignId (state : ProjectState) : String :=
  s!"design-{state.designRevisions.length + 1}"

def DesignProposalRequest.design
    (state : ProjectState) (sources : List DesignSource)
    (request : DesignProposalRequest) : DesignRevision :=
  { id := nextDesignId state, producerAgentRun := request.producerAgentRun
    sourceDocuments := sources, statements := request.statements
    acceptanceCriteria := request.acceptanceCriteria, leanClaims := request.leanClaims }

def proposeDesign
    (state : ProjectState) (candidate : DesignRevision) : Except String ProjectState := do
  if (state.design? candidate.id).isSome then
    throw s!"design id {candidate.id} already exists"
  if candidate.status != .candidate then
    throw "a proposed design must have candidate status"
  if candidate.parent != state.acceptedDesignId then
    throw "candidate parent must be the current accepted design"
  let candidate := { candidate with createdAfterEntryOrder := nextEntryOrder state - 1 }
  validated { state with
    revision := state.revision + 1
    designRevisions := state.designRevisions ++ [candidate] }

def proposeDesignRequest
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : DesignProposalRequest) : IO (ProjectState × DesignRevision) := do
  let mut sources := []
  for target in request.sourceDocumentTargets do
    let snapshot ← Snapshot.requiredTarget projectRoot target
    sources := sources ++ [DesignSource.mk target snapshot]
  let candidate := { request.design state sources with parent := state.acceptedDesignId }
  match proposeDesign state candidate with
  | .ok next =>
      let stored ← match next.design? candidate.id with
        | some value => pure value
        | none => throw (IO.userError "stored generated Design disappeared")
      pure (next, stored)
  | .error message => throw (IO.userError message)

private def sourcesCurrent
    (projectRoot : System.FilePath) (design : DesignRevision) : IO Bool := do
  for source in design.sourceDocuments do
    try
      if (← Snapshot.target projectRoot source.target) != source.snapshot then return false
    catch _ => return false
  pure true

def acceptDesign (state : ProjectState) (id : String) : Except String ProjectState := do
  if state.focusedWorkId.isSome then
    throw "suspend focused Work before accepting a successor design"
  let candidate ← match state.design? id with
    | some value => pure value
    | none => throw s!"design {id} does not exist"
  if candidate.status != .candidate then
    throw s!"design {id} is not a candidate"
  if candidate.parent != state.acceptedDesignId then
    throw "candidate does not succeed the current accepted design"
  let designs := state.designRevisions.map (fun design =>
    if design.id == id then { design with status := .accepted }
    else if some design.id == state.acceptedDesignId then { design with status := .replaced }
    else design)
  validated { state with
    revision := state.revision + 1
    acceptedDesignId := some id
    designRevisions := designs }

def acceptDesignRequest
    (projectRoot : System.FilePath) (state : ProjectState) (id : String) : IO ProjectState := do
  let candidate ← match state.design? id with
    | some value => pure value
    | none => throw (IO.userError s!"design {id} does not exist")
  if !(← sourcesCurrent projectRoot candidate) then
    throw (IO.userError "candidate Design source content changed; propose a successor candidate")
  match acceptDesign state id with
  | .ok next => pure next
  | .error message => throw (IO.userError message)

end AgentWorkbench
