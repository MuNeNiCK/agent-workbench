import AgentWorkbench.Application.Common
import AgentWorkbench.Decision.Projection
import AgentWorkbench.Decision.PlanCoverage

namespace AgentWorkbench

structure WorkStartRequest where
  id : String
  outcome : String
  scope : String
  responsibleAgentRun : String
  causalFindingEntryId : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkWithdrawRequest where
  workId : String
  entryId : String
  correctionEntryId : String
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkAdoptDesignRequest where
  workId : String
  entryId : String
  agentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkResumeRequest where
  workId : String
  entryId : String
  satisfaction : String
  basisEntryIds : List String
  agentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkRemediationBindingRequest where
  workId : String
  entryId : String
  findingEntryId : String
  agentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkAdoptionImpact where
  workId : String
  predecessorDesignId : Option String
  successorDesignId : String
  statementDeltas : List ExpectedStatementDelta
  materializedPlanId : Option String
  requiresPlanReplacement : Bool
  affectedTaskEntryIds : List String
  staleEvidenceEntryIds : List String
  requiredClaimIds : List String
  deriving Repr, DecidableEq, Lean.ToJson

def workAdoptionImpact (state : ProjectState) (workId : String) : Except String WorkAdoptionImpact := do
  let work ← match state.work? workId with
    | some value => pure value
    | none => throw s!"work {workId} does not exist"
  let successorId ← match state.acceptedDesignId with
    | some value => pure value
    | none => throw "no accepted design"
  let successor ← match state.design? successorId with
    | some value => pure value
    | none => throw "accepted design selector is invalid"
  if work.designRevision == some successor.id then
    throw "Work already uses the accepted Design"
  match work.designRevision with
  | some predecessor =>
      if predecessor != successor.id && !state.designDescendsFrom predecessor successor.id then
        throw "accepted design does not descend from the Work design"
  | none =>
      if successor.parent.isSome then
        throw "an initially unbound Work can adopt only an initial Design"
  let adoptionBaseline := { work with baselineDesignRevision := work.designRevision }
  let deltas ← expectedStatementDeltas state adoptionBaseline successor
  let plan := state.materializedPlanFor? work.id
  let affectedTasks := state.ledgerEntries.filterMap fun entry =>
    if entry.workId != some work.id || entryIsSuperseded state entry then none else
    match entry.payload with
    | .task task => if task.planId == plan.map (·.id) && !task.retired then some entry.id else none
    | _ => none
  let staleEvidence := state.ledgerEntries.filterMap fun entry =>
    if entry.workId != some work.id || entry.designRevision != work.designRevision then none else
    match entry.payload with
    | .commandExecution _ | .artifactObservation _ | .leanProofReceipt _ => some entry.id
    | _ => none
  pure {
    workId := work.id, predecessorDesignId := work.designRevision
    successorDesignId := successor.id, statementDeltas := deltas
    materializedPlanId := plan.map (·.id)
    requiresPlanReplacement := plan.any (·.designRevision != successor.id)
    affectedTaskEntryIds := affectedTasks, staleEvidenceEntryIds := staleEvidence
    requiredClaimIds := successor.leanClaims.map (·.id) }

def WorkStartRequest.work (designRevision : Option String) (request : WorkStartRequest) : Work :=
  { id := request.id, outcome := request.outcome, scope := request.scope
    baselineDesignRevision := designRevision, designRevision, status := .active
    responsibleAgentRun := request.responsibleAgentRun }

def startWork (state : ProjectState) (work : Work) : Except String ProjectState := do
  if (state.work? work.id).isSome then throw s!"work id {work.id} already exists"
  if state.focusedWorkId.isSome then throw "another Work is already focused"
  if work.status != .active || work.designRevision != state.acceptedDesignId ||
      work.baselineDesignRevision != state.acceptedDesignId then
    throw "new Work must be active and use the current accepted Design as its immutable baseline"
  validated { state with
    revision := state.revision + 1
    focusedWorkId := some work.id
    works := (state.works ++ [work]).mergeSort (fun left right => left.id < right.id) }

private def postcompletionFindingOrigin
    (state : ProjectState) (findingEntryId : String) : Except String (LedgerEntry × Work) := do
  let finding ← match state.entry? findingEntryId with
    | some value => pure value
    | none => throw s!"no postcompletion Finding {findingEntryId}"
  match finding.payload with
  | .finding _ => pure ()
  | _ => throw s!"entry {findingEntryId} is not a Finding"
  let originWorkId ← match finding.workId with
    | some value => pure value
    | none => throw "postcompletion Finding has no origin Work"
  let origin ← match state.work? originWorkId with
    | some value => pure value
    | none => throw s!"postcompletion Finding origin Work {originWorkId} does not exist"
  if origin.status != .completed then
    throw "remediation requires an already-completed origin Work"
  let completionPrecedes := state.ledgerEntries.any fun entry =>
    entry.order < finding.order && entry.workId == some origin.id &&
      match entry.payload with | .workCompletion _ => true | _ => false
  if !completionPrecedes then throw "Finding is not postcompletion incident evidence"
  let accepted := (state.findingDisposition? finding.id origin.id).any fun entry =>
    match entry.payload with
    | .reviewDisposition disposition =>
        disposition.decision == .accepted && disposition.impact == .implementationDefect
    | _ => false
  if !accepted then throw "remediation requires an accepted implementation-defect Finding"
  pure (finding, origin)

private def remediationBindingExists (state : ProjectState) (workId : String) : Bool :=
  state.ledgerEntries.any fun entry =>
    entry.workId == some workId && match entry.payload with
    | .workRemediation _ => true
    | _ => false

private def remediationEntry
    (state : ProjectState) (work : Work) (entryId findingEntryId : String)
    (origin : Work) : LedgerEntry :=
  { id := entryId, order := nextEntryOrder state, scope := work.scope
    workId := some work.id, designRevision := work.designRevision
    payload := .workRemediation {
      originWorkId := origin.id, findingEntryId, boundByRun := work.responsibleAgentRun } }

def startWorkRequest
    (state : ProjectState) (request : WorkStartRequest) : Except String ProjectState := do
  let work := request.work state.acceptedDesignId
  match request.causalFindingEntryId with
  | none => startWork state work
  | some findingEntryId =>
      let (_, origin) ← postcompletionFindingOrigin state findingEntryId
      if origin.id == work.id || origin.scope != work.scope then
        throw "remediation Work must be distinct from and share scope with its origin Work"
      let started ← startWork state work
      let entryId := s!"remediation-{work.id}-{started.revision}"
      let entry := remediationEntry state work entryId findingEntryId origin
      validated { started with ledgerEntries := state.ledgerEntries ++ [entry] }

def bindRemediationWork
    (state : ProjectState) (request : WorkRemediationBindingRequest) : Except String ProjectState := do
  let work ← match state.work? request.workId with
    | some value => pure value
    | none => throw s!"work {request.workId} does not exist"
  if work.status != .active || state.focusedWorkId != some work.id then
    throw "only the focused active remediation Work can be bound"
  if request.agentRun != work.responsibleAgentRun then
    throw "only the responsible remediation agent may bind its causal Finding"
  if remediationBindingExists state work.id then
    throw "remediation Work already has a causal Finding binding"
  if (state.entry? request.entryId).isSome then throw s!"entry id {request.entryId} already exists"
  let (_, origin) ← postcompletionFindingOrigin state request.findingEntryId
  if origin.id == work.id || origin.scope != work.scope then
    throw "remediation Work must be distinct from and share scope with its origin Work"
  let entry := remediationEntry state work request.entryId request.findingEntryId origin
  validated { state with revision := state.revision + 1, ledgerEntries := state.ledgerEntries ++ [entry] }

def suspendWork
    (state : ProjectState) (workId condition : String) : Except String ProjectState := do
  if state.focusedWorkId != some workId then throw s!"work {workId} is not focused"
  if condition.isEmpty then throw "suspension requires an explicit resume condition"
  let works := state.works.map (fun work =>
    if work.id == workId then
      { work with status := .suspended, resumeCondition := some condition }
    else work)
  validated { state with
    revision := state.revision + 1, focusedWorkId := none, works := works }

def focusWork (state : ProjectState) (workId : String) : Except String ProjectState := do
  if state.focusedWorkId.isSome then throw "another Work is already focused"
  let work ← match state.work? workId with
    | some value => pure value
    | none => throw s!"work {workId} does not exist"
  if work.status != .active then throw s!"work {workId} is not active"
  if work.designRevision != state.acceptedDesignId then
    throw s!"work {workId} must explicitly adopt the accepted design before resume"
  validated { state with
    revision := state.revision + 1, focusedWorkId := some workId }

def resumeWork (state : ProjectState) (request : WorkResumeRequest) : Except String ProjectState := do
  if state.focusedWorkId.isSome then throw "another Work is already focused"
  let work ← match state.work? request.workId with
    | some value => pure value
    | none => throw s!"work {request.workId} does not exist"
  if work.status != .suspended then throw s!"work {request.workId} is not suspended"
  if work.designRevision != state.acceptedDesignId then
    throw s!"work {request.workId} must explicitly adopt the accepted design before resume"
  if request.agentRun != work.responsibleAgentRun then
    throw "only the responsible work agent may satisfy the resume condition"
  let condition ← match work.resumeCondition with
    | some value => pure value
    | none => throw "suspended Work has no recorded resume condition"
  if request.satisfaction.isEmpty || request.basisEntryIds.isEmpty then
    throw "Work resume requires a satisfaction statement and at least one immutable basis entry"
  if request.basisEntryIds.eraseDups.length != request.basisEntryIds.length then
    throw "Work resume basis entries must be unique"
  for basisId in request.basisEntryIds do
    let basis ← match state.entry? basisId with
      | some value => pure value
      | none => throw s!"resume basis entry {basisId} does not exist"
    if entryIsSuperseded state basis || basis.workId != some work.id ||
        basis.designRevision != work.designRevision then
      throw s!"resume basis entry {basisId} is not current for the suspended Work"
  if (state.entry? request.entryId).isSome then
    throw s!"entry id {request.entryId} already exists"
  let works := state.works.map fun candidate =>
    if candidate.id == request.workId then
      { candidate with status := .active, resumeCondition := none }
    else candidate
  let resumeEntry : LedgerEntry := {
    id := request.entryId, order := nextEntryOrder state, scope := work.scope
    workId := some work.id, designRevision := work.designRevision
    payload := .workResume {
      condition, satisfaction := request.satisfaction
      basisEntryIds := request.basisEntryIds, resumedByRun := request.agentRun } }
  validated { state with
    revision := state.revision + 1, focusedWorkId := some request.workId, works
    ledgerEntries := state.ledgerEntries ++ [resumeEntry] }

def adoptDesignForWork
    (state : ProjectState) (request : WorkAdoptDesignRequest) : Except String ProjectState := do
  if state.focusedWorkId == some request.workId then throw "suspend Work before adopting a successor design"
  let impact ← workAdoptionImpact state request.workId
  let work ← match state.work? request.workId with
    | some value => pure value
    | none => throw s!"work {request.workId} does not exist"
  if work.responsibleAgentRun != request.agentRun then
    throw "only the responsible work agent may adopt design"
  if (state.entry? request.entryId).isSome then throw s!"entry id {request.entryId} already exists"
  let updatedWorks := state.works.map (fun candidate =>
    if candidate.id == request.workId then { candidate with designRevision := some impact.successorDesignId }
    else candidate)
  let adoption : LedgerEntry :=
    { id := request.entryId
      order := nextEntryOrder state
      scope := work.scope
      workId := some work.id
      designRevision := some impact.successorDesignId
      payload := .workDesignAdoption
        { predecessor := work.designRevision.getD "", successor := impact.successorDesignId
          impactDisposition := (Lean.toJson impact).compress, adoptedByRun := request.agentRun } }
  validated { state with
    revision := state.revision + 1
    works := updatedWorks
    ledgerEntries := state.ledgerEntries ++ [adoption] }

def handoffWork
    (state : ProjectState) (workId entryId successorRun reason : String) : Except String ProjectState := do
  if state.focusedWorkId != some workId then
    throw s!"work {workId} is not the focused Work"
  if successorRun.isEmpty || reason.isEmpty then
    throw "Work handoff requires a successor run and reason"
  let work ← match state.work? workId with
    | some value => pure value
    | none => throw s!"work {workId} does not exist"
  if work.status == .completed then throw "completed Work cannot be handed off"
  if work.responsibleAgentRun == successorRun then throw "Work handoff successor is already responsible"
  if (state.entry? entryId).isSome then throw s!"entry id {entryId} already exists"
  let works := state.works.map (fun candidate =>
    if candidate.id == workId then { candidate with responsibleAgentRun := successorRun }
    else candidate)
  let handoff : LedgerEntry :=
    { id := entryId
      order := nextEntryOrder state
      scope := work.scope
      workId := some work.id
      designRevision := work.designRevision
      payload := .workHandoff {
        predecessorRun := work.responsibleAgentRun
        successorRun
        reason } }
  validated { state with
    revision := state.revision + 1
    works
    ledgerEntries := state.ledgerEntries ++ [handoff] }

def withdrawWork
    (state : ProjectState) (request : WorkWithdrawRequest) : Except String ProjectState := do
  let work ← match state.work? request.workId with
    | some value => pure value
    | none => throw s!"work {request.workId} does not exist"
  if work.status != .active && work.status != .suspended then
    throw "only active or suspended Work can be withdrawn"
  if request.reason.isEmpty then throw "Work withdrawal requires a reason"
  if (state.entry? request.entryId).isSome then throw s!"entry id {request.entryId} already exists"
  let correctionEntry ← match state.entry? request.correctionEntryId with
    | some value => pure value
    | none => throw s!"no User Correction {request.correctionEntryId}"
  if correctionEntry.workId != some work.id || correctionEntry.scope != work.scope ||
      entryIsSuperseded state correctionEntry then
    throw "Work withdrawal requires a current same-Work User Correction"
  let correction ← match correctionEntry.payload with
    | .userCorrection value => pure value
    | _ => throw s!"entry {request.correctionEntryId} is not a User Correction"
  if correction.resolvedByEntryId.isSome || correction.incorporatedIn.isSome then
    throw "Work withdrawal Correction is already resolved"
  let works := state.works.map fun candidate =>
    if candidate.id == work.id then { candidate with status := .withdrawn } else candidate
  let entry : LedgerEntry := {
    id := request.entryId, order := nextEntryOrder state, scope := work.scope
    workId := some work.id, designRevision := work.designRevision
    payload := .workWithdrawal {
      correctionEntryId := correctionEntry.id, reason := request.reason
      withdrawnByRun := work.responsibleAgentRun } }
  validated {
    state with
    revision := state.revision + 1
    focusedWorkId := if state.focusedWorkId == some work.id then none else state.focusedWorkId
    works
    ledgerEntries := state.ledgerEntries ++ [entry] }

end AgentWorkbench
