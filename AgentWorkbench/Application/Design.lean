import AgentWorkbench.Application.Common
import AgentWorkbench.Adapter.Snapshot

namespace AgentWorkbench

structure DesignProposalRequest where
  producerAgentRun : String
  changeRationale : String
  changeBasisEntryIds : List String := []
  amendsCandidate : Option String := none
  sourceDocumentTargets : List String := []
  sourceUnitDispositions : List SourceUnitDisposition
  assumptions : List DesignAssumption := []
  statements : List Statement
  statementCoverage : List StatementCoverage
  removedStatements : List RemovedStatementTombstone := []
  acceptanceCriteria : List AcceptanceCriterion
  leanClaims : List LeanClaim := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignRejectRequest where
  designId : String
  entryId : String
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def nextDesignId (state : ProjectState) : String :=
  s!"design-{state.designRevisions.length + 1}"

def DesignProposalRequest.design
    (state : ProjectState) (workId : String) (sources : List DesignSource)
    (units : List DesignSourceUnit)
    (request : DesignProposalRequest) : DesignRevision :=
  { id := nextDesignId state, workId := some workId, parent := state.acceptedDesignId
    amendsCandidate := request.amendsCandidate
    producerAgentRun := request.producerAgentRun
    changeRationale := request.changeRationale
    changeBasisEntryIds := request.changeBasisEntryIds
    sourceArchiveAvailable := true
    sourceDocuments := sources, sourceUnits := units
    sourceUnitDispositions := request.sourceUnitDispositions
    assumptions := request.assumptions
    statements := request.statements, statementCoverage := request.statementCoverage
    removedStatements := request.removedStatements
    acceptanceCriteria := request.acceptanceCriteria, leanClaims := request.leanClaims }

def proposeDesign
    (state : ProjectState) (candidate : DesignRevision) : Except String ProjectState := do
  if (state.design? candidate.id).isSome then
    throw s!"design id {candidate.id} already exists"
  if candidate.status != .candidate then
    throw "a proposed design must have candidate status"
  let workId ← match state.focusedWorkId with
    | some value => pure value
    | none => throw "Design proposal requires a focused Work"
  if candidate.workId != some workId then
    throw "candidate Work binding must be the focused Work"
  if candidate.parent != state.acceptedDesignId then
    throw "candidate parent must be the current accepted design"
  if candidate.changeRationale.isEmpty then
    throw "Design proposal requires a change rationale"
  let designs ← match candidate.amendsCandidate with
    | none => pure state.designRevisions
    | some predecessorId =>
        let predecessor ← match state.design? predecessorId with
          | some value => pure value
          | none => throw s!"amended candidate {predecessorId} does not exist"
        if predecessor.status != .candidate || predecessor.parent != candidate.parent ||
            predecessor.workId != candidate.workId then
          throw "candidate amendment crosses its Work or accepted parent"
        if state.designRevisions.any (fun design =>
            design.amendsCandidate == some predecessorId && design.status == .candidate) then
          throw "candidate amendment would fork the current head"
        pure (state.designRevisions.map fun design =>
          if design.id == predecessorId then { design with status := .superseded } else design)
  let candidate := { candidate with createdAfterEntryOrder := nextEntryOrder state - 1 }
  validated { state with
    revision := state.revision + 1
    designRevisions := (designs ++ [candidate]).mergeSort (fun left right => left.id < right.id) }

def acceptDesign (state : ProjectState) (id : String) : Except String ProjectState := do
  let candidate ← match state.design? id with
    | some value => pure value
    | none => throw s!"design {id} does not exist"
  if candidate.status != .candidate then
    throw s!"design {id} is not a candidate"
  if candidate.parent != state.acceptedDesignId then
    throw "candidate does not succeed the current accepted design"
  if candidate.parent.isSome && state.focusedWorkId.isSome then
    throw "suspend focused Work before accepting a successor Design"
  if candidate.parent.isNone && state.focusedWorkId != candidate.workId then
    throw "initial Design acceptance must remain bound to its focused Work"
  if state.designRevisions.any (fun design =>
      design.amendsCandidate == some candidate.id && design.status == .candidate) then
    throw "only the current candidate amendment head can be accepted"
  let designs := state.designRevisions.map (fun design =>
    if design.id == id then { design with status := .accepted }
    else if some design.id == state.acceptedDesignId then { design with status := .superseded }
    else design)
  let works := state.works.map fun work =>
    if candidate.parent.isNone && candidate.workId == some work.id && work.designRevision.isNone then
      { work with designRevision := some id }
    else work
  validated { state with
    revision := state.revision + 1
    acceptedDesignId := some id
    designRevisions := designs
    works }

def rejectDesign
    (state : ProjectState) (request : DesignRejectRequest) : Except String ProjectState := do
  let focusedWork ← match state.currentWork? with
    | some value => pure value
    | none => throw "Design rejection requires a focused Work"
  let candidate ← match state.design? request.designId with
    | some value => pure value
    | none => throw s!"design {request.designId} does not exist"
  if candidate.status != .candidate then throw s!"design {request.designId} is not a candidate"
  if request.reason.isEmpty then throw "Design rejection requires a reason"
  let workId ← match candidate.workId with
    | some value => pure value
    | none => throw "Design candidate is not Work-bound"
  if workId != focusedWork.id then
    throw s!"Design candidate {candidate.id} is not bound to focused Work {focusedWork.id}"
  let work ← match state.work? workId with
    | some value => pure value
    | none => throw s!"no Work {workId}"
  if (state.entry? request.entryId).isSome then throw s!"entry id {request.entryId} already exists"
  let designs := state.designRevisions.map fun design =>
    if design.id == candidate.id then { design with status := .rejected } else design
  let entry : LedgerEntry := {
    id := request.entryId, order := nextEntryOrder state, scope := work.scope
    workId := some work.id, designRevision := some candidate.id
    payload := .designRejection {
      designId := candidate.id, reason := request.reason
      rejectedByRun := work.responsibleAgentRun } }
  validated {
    state with
    revision := state.revision + 1
    designRevisions := designs
    ledgerEntries := state.ledgerEntries ++ [entry] }

end AgentWorkbench
