import AgentWorkbench.Application.Common

namespace AgentWorkbench

structure WorkStartRequest where
  id : String
  outcome : String
  scope : String
  responsibleAgentRun : String
  delegatedReviewDecisions : List DispositionDecision := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def WorkStartRequest.work (designRevision : String) (request : WorkStartRequest) : Work :=
  { id := request.id, outcome := request.outcome, scope := request.scope
    designRevision, status := .focused
    responsibleAgentRun := request.responsibleAgentRun
    delegatedReviewDecisions := request.delegatedReviewDecisions }

def startWork (state : ProjectState) (work : Work) : Except String ProjectState := do
  if (state.work? work.id).isSome then throw s!"work id {work.id} already exists"
  let accepted ← match state.acceptedDesignId with
    | some id => pure id
    | none => throw "no accepted design"
  if state.focusedWorkId.isSome then throw "another Work is already focused"
  if work.status != .focused || work.designRevision != accepted then
    throw "new Work must be focused and bound to the accepted design"
  validated { state with
    revision := state.revision + 1
    focusedWorkId := some work.id
    works := state.works ++ [work] }

def startWorkRequest
    (state : ProjectState) (request : WorkStartRequest) : Except String ProjectState := do
  let designId ← match state.acceptedDesignId with
    | some value => pure value
    | none => throw "no accepted design"
  startWork state (request.work designId)

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
  if work.status != .suspended && work.status != .blocked then
    throw s!"work {workId} is not resumable"
  if some work.designRevision != state.acceptedDesignId then
    throw s!"work {workId} must explicitly adopt the accepted design before resume"
  let works := state.works.map (fun candidate =>
    if candidate.id == workId then { candidate with status := .focused }
    else candidate)
  validated { state with
    revision := state.revision + 1, focusedWorkId := some workId, works := works }

def adoptDesignForWork
    (state : ProjectState) (workId entryId impact run : String) : Except String ProjectState := do
  if state.focusedWorkId == some workId then throw "suspend Work before adopting a successor design"
  if impact.isEmpty then throw "design adoption requires an impact disposition"
  let work ← match state.work? workId with
    | some value => pure value
    | none => throw s!"work {workId} does not exist"
  if work.responsibleAgentRun != run then throw "only the responsible work agent may adopt design"
  let successorId ← match state.acceptedDesignId with
    | some value => pure value
    | none => throw "no accepted design"
  let successor ← match state.design? successorId with
    | some value => pure value
    | none => throw "accepted design selector is invalid"
  if !state.designDescendsFrom work.designRevision successor.id then
    throw "accepted design does not descend from the Work design"
  if (state.entry? entryId).isSome then throw s!"entry id {entryId} already exists"
  let updatedWorks := state.works.map (fun candidate =>
    if candidate.id == workId then { candidate with designRevision := successorId }
    else candidate)
  let adoption : LedgerEntry :=
    { id := entryId
      order := nextEntryOrder state
      scope := work.scope
      workId := some work.id
      designRevision := some successorId
      payload := .workDesignAdoption
        { predecessor := work.designRevision, successor := successorId
          impactDisposition := impact, adoptedByRun := run } }
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
      designRevision := some work.designRevision
      payload := .workHandoff {
        predecessorRun := work.responsibleAgentRun
        successorRun
        reason } }
  validated { state with
    revision := state.revision + 1
    works
    ledgerEntries := state.ledgerEntries ++ [handoff] }

end AgentWorkbench
