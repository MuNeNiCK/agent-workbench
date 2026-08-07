import AgentWorkbench.Application.Common
import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

structure PlanProposalRequest where
  predecessorPlanId : Option String := none
  producerAgentRun : String
  reason : String
  changeBasisEntryIds : List String := []
  sourceDocumentTargets : List String
  sourceUnitDispositions : List PlanSourceUnitDisposition
  statementDispositions : List PlanStatementDisposition
  steps : List PlanStep
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def nextPlanId (state : ProjectState) : String :=
  s!"plan-{state.implementationPlans.length + 1}"

def PlanProposalRequest.plan
    (state : ProjectState) (work : Work) (sources : List PlanSource)
    (units : List DesignSourceUnit) (request : PlanProposalRequest) : ImplementationPlan :=
  { id := nextPlanId state, workId := work.id
    designRevision := work.designRevision.getD ""
    predecessorPlanId := request.predecessorPlanId
    producerAgentRun := request.producerAgentRun, reason := request.reason
    changeBasisEntryIds := request.changeBasisEntryIds
    contentDigest := "", sourceDocuments := sources, sourceUnits := units
    sourceUnitDispositions := request.sourceUnitDispositions
    statementDispositions := request.statementDispositions, steps := request.steps }

def proposePlan
    (state : ProjectState) (candidate : ImplementationPlan) : Except String ProjectState := do
  let (_, work) ← currentBinding state
  if (state.plan? candidate.id).isSome then throw s!"Plan id {candidate.id} already exists"
  if candidate.status != .candidate || candidate.workId != work.id ||
      candidate.designRevision != work.designRevision.getD "" then
    throw "Plan candidate must bind the focused Work and its adopted Design"
  if candidate.reason.isEmpty then throw "Plan proposal requires a reason"
  let current := state.materializedPlanFor? work.id
  let openCandidates := state.implementationPlans.filter fun plan =>
    plan.workId == work.id && plan.status == .candidate &&
      !(state.implementationPlans.any fun successor =>
        successor.predecessorPlanId == some plan.id && successor.status == .candidate)
  let expectedPredecessor := openCandidates.head?.map (·.id) |>.orElse fun _ => current.map (·.id)
  if candidate.predecessorPlanId != expectedPredecessor then
    throw "Plan candidate does not replace the current candidate head or current Plan"
  let plans := state.implementationPlans.map fun plan =>
    if candidate.predecessorPlanId == some plan.id && plan.status == .candidate then
      { plan with status := .superseded }
    else plan
  validated { state with
    revision := state.revision + 1
    implementationPlans := (plans ++ [candidate]).mergeSort (fun left right => left.id < right.id) }

private def currentTaskEntries
    (state : ProjectState) (work : Work) (plan : Option ImplementationPlan) : List LedgerEntry :=
  state.ledgerEntries.filter fun entry =>
    entry.workId == some work.id &&
    !entryIsSuperseded state entry && match entry.payload with
    | .task task => !task.retired && task.planId == plan.map (·.id)
    | _ => false

private def taskLineage (workId stepId : String) : String :=
  s!"{workId}:{stepId}"

private def affectedStepIds
    (prior : Option ImplementationPlan) (candidate : ImplementationPlan) : List String :=
  let direct := candidate.steps.filterMap fun step =>
    match prior.bind fun plan => uniqueBy? plan.steps (·.id) step.id with
    | some old => if old == step then none else some step.id
    | none => some step.id
  let rec close (fuel : Nat) (affected : List String) : List String :=
    match fuel with
    | 0 => affected
    | remaining + 1 =>
        let expanded := candidate.steps.foldl (fun found step =>
          if found.contains step.id || !step.dependsOnStepIds.any found.contains then found
          else found ++ [step.id]) affected
        if expanded.length == affected.length then affected else close remaining expanded
  close candidate.steps.length direct

def materializePlan
    (state : ProjectState) (planId : String) (digests : List CurrentClaimDigest) :
    Except String ProjectState := do
  let (design, work) ← currentBinding state
  let candidate ← match state.plan? planId with
    | some value => pure value
    | none => throw s!"no Plan {planId}"
  if candidate.status != .candidate || candidate.workId != work.id ||
      candidate.designRevision != design.id then
    throw "only a current-bound Plan candidate can be materialized"
  if state.implementationPlans.any (fun plan =>
      plan.predecessorPlanId == some candidate.id && plan.status == .candidate) then
    throw "only the current Plan candidate head can be materialized"
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw "Plan materialization requires current Work and Design"
  if !design.leanClaims.all (claimHasReceipt projection digests) then
    throw "Plan materialization requires current receipts for every selected Design Claim"
  let requiredFindingIds := acceptedImplementationFindingIds state work.id design.id
  let coveredFindingIds := candidate.steps.flatMap (·.acceptedFindingEntryIds)
  if !requiredFindingIds.all coveredFindingIds.contains then
    throw "Plan materialization omits an accepted Implementation Review Finding"
  let priorPlan := state.materializedPlanFor? work.id
  let priorTasks := currentTaskEntries state work priorPlan
  let affected := affectedStepIds priorPlan candidate
  let plans := state.implementationPlans.map fun plan =>
    if plan.id == candidate.id then { plan with status := .current }
    else if priorPlan.map (·.id) == some plan.id then { plan with status := .superseded }
    else plan
  let firstOrder := nextEntryOrder state
  let mut entries := state.ledgerEntries
  let mut created := 0
  for step in candidate.steps do
    let lineage := taskLineage work.id step.id
    let priorTask := priorTasks.find? fun entry => match entry.payload with
      | .task task => task.lineageId == some lineage
      | _ => false
    let preservedClosed := priorTask.any fun entry =>
      match entry.payload, priorPlan with
      | .task task, some oldPlan =>
          task.closed && !affected.contains step.id && oldPlan.steps.any fun oldStep => oldStep == step
      | _, _ => false
    let entry : LedgerEntry := {
      id := s!"task-{candidate.id}-{step.id}"
      order := firstOrder + created, scope := work.scope
      workId := some work.id, designRevision := some design.id
      supersedes := priorTask.map (fun value => [value.id]) |>.getD []
      payload := .task {
        planId := some candidate.id, planStepId := some step.id, lineageId := some lineage
        dependencyLineageIds := step.dependsOnStepIds.map (taskLineage work.id)
        outputScopes := step.outputScopes
        verificationCriterionIds := step.verificationCriterionIds
        materializedAtOrder := firstOrder, description := step.description
        required := true, closed := preservedClosed } }
    entries := entries ++ [entry]
    created := created + 1
  for prior in priorTasks do
    let retained := candidate.steps.any fun step =>
      match prior.payload with
      | .task task => task.lineageId == some (taskLineage work.id step.id)
      | _ => false
    if !retained then
      let priorTask ← match prior.payload with
        | .task value => pure value
        | _ => throw "current Task projection contains a non-Task"
      entries := entries ++ [{
        id := s!"task-retired-{candidate.id}-{prior.id}"
        order := firstOrder + created, scope := work.scope
        workId := some work.id, designRevision := some design.id
        supersedes := [prior.id]
        payload := .task { priorTask with
          required := false, closed := true, retired := true
          materializedAtOrder := firstOrder } }]
      created := created + 1
  validated { state with
    revision := state.revision + 1
    implementationPlans := plans
    ledgerEntries := entries }

end AgentWorkbench
