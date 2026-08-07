import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Plan

open AgentWorkbench AgentWorkbenchTest

private def dependencyPlanState : ProjectState :=
  let stepA : PlanStep := {
    id := "a", description := "build A", outputScopes := [criterion.target]
    verificationCriterionIds := [criterion.id] }
  let stepB : PlanStep := {
    id := "b", description := "build B", dependsOnStepIds := ["a"]
    outputScopes := [criterion.target], verificationCriterionIds := [criterion.id] }
  let unitA : DesignSourceUnit := { planUnit with id := "plan-unit-a", text := "build A" }
  let unitB : DesignSourceUnit := { planUnit with id := "plan-unit-b", text := "build B" }
  let oldPlan : ImplementationPlan := {
    plan with
    id := "plan-old"
    contentDigest := "blake3:old"
    sourceUnits := [unitA, unitB]
    sourceUnitDispositions := [
      { unitId := unitA.id, stepId := some stepA.id },
      { unitId := unitB.id, stepId := some stepB.id }]
    statementDispositions := [{
      statementId := statement.id
      statementText := statement.text
      deltaKind := .added
      stepIds := [stepA.id, stepB.id] }]
    steps := [stepA, stepB] }
  let changedA := { stepA with description := "rebuild A with changed output logic" }
  let candidate : ImplementationPlan := {
    oldPlan with
    id := "plan-new"
    predecessorPlanId := some oldPlan.id
    status := .candidate
    contentDigest := "blake3:new"
    steps := [changedA, stepB] }
  let task (id : String) (order : Nat) (step : PlanStep) (closed : Bool)
      (sourceId evidenceId : String := "") : LedgerEntry := {
    id, order, scope := work.scope
    workId := some work.id, designRevision := some design.id
    supersedes := if closed then [sourceId] else []
    payload := .task {
      planId := some oldPlan.id, planStepId := some step.id
      lineageId := some s!"{work.id}:{step.id}"
      dependencyLineageIds := step.dependsOnStepIds.map (fun id => s!"{work.id}:{id}")
      outputScopes := step.outputScopes, verificationCriterionIds := step.verificationCriterionIds
      verificationEvidenceEntryIds := if closed then [evidenceId] else []
      verificationTaskEntryId := if closed then some sourceId else none
      materializedAtOrder := 0, description := step.description, required := true, closed } }
  let evidence (id : String) (order : Nat) (taskId : String) : LedgerEntry := {
    id, order, scope := work.scope, workId := some work.id, designRevision := some design.id
    payload := .artifactObservation {
      taskEntryId := some taskId, outputScope := some criterion.target
      criterionId := criterion.id, target := criterion.target, snapshot := s!"blake3:{id}"
      operation := "verify Plan Task", result := "verified", successful := true
      producerAgentRun := work.responsibleAgentRun } }
  let taskAOpen := task "old-task-a-open" 1 stepA false
  let taskBOpen := task "old-task-b-open" 2 stepB false
  let evidenceA := evidence "old-evidence-a" 3 taskAOpen.id
  let taskA := task "old-task-a" 4 stepA true taskAOpen.id evidenceA.id
  let evidenceB := evidence "old-evidence-b" 5 taskBOpen.id
  let taskB := task "old-task-b" 6 stepB true taskBOpen.id evidenceB.id
  { revision := 8, acceptedDesignId := some design.id, focusedWorkId := some work.id
    designRevisions := [design], works := [work]
    implementationPlans := [oldPlan, candidate]
    ledgerEntries := [taskAOpen, taskBOpen, evidenceA, taskA, evidenceB, taskB] }

def run : IO Unit := do
  let baseline : DesignRevision := { design with id := "design-baseline", status := .superseded }
  let successor : DesignRevision := { design with
    id := "design-successor"
    statements := [{ statement with assumptions := ["the service is online"] }] }
  let changedWork : Work := { work with
    baselineDesignRevision := some baseline.id, designRevision := some successor.id }
  let changedState : ProjectState := { baseState with
    acceptedDesignId := some successor.id, designRevisions := [baseline, successor]
    works := [changedWork], implementationPlans := [], ledgerEntries := [] }
  let changedDeltas ← fromExcept <| expectedStatementDeltas changedState changedWork successor
  expect (changedDeltas.length == 1 && changedDeltas.head?.any (·.kind == .modified))
    "Plan delta omitted a Statement assumption change with unchanged ID and text"
  let assumptionId := "assumption-service-online"
  let baselineAssumption : DesignAssumption := {
    id := assumptionId, text := "the service is online", sourceUnitIds := [sourceUnit.id] }
  let changedAssumption : DesignAssumption := {
    baselineAssumption with text := "the service is online and authenticated" }
  let assumptionBaseline : DesignRevision := { baseline with
    assumptions := [baselineAssumption]
    statements := [{ statement with assumptions := [assumptionId] }] }
  let assumptionSuccessor : DesignRevision := { successor with
    assumptions := [changedAssumption]
    statements := [{ statement with assumptions := [assumptionId] }] }
  let assumptionWork : Work := { changedWork with
    baselineDesignRevision := some assumptionBaseline.id
    designRevision := some assumptionSuccessor.id }
  let assumptionState : ProjectState := { changedState with
    acceptedDesignId := some assumptionSuccessor.id
    designRevisions := [assumptionBaseline, assumptionSuccessor]
    works := [assumptionWork] }
  let assumptionDeltas ← fromExcept <|
    expectedStatementDeltas assumptionState assumptionWork assumptionSuccessor
  expect (assumptionDeltas.length == 1 && assumptionDeltas.head?.any (·.kind == .modified))
    "Plan delta omitted changed authoritative assumption text with a stable ID"
  let coverageSuccessor : DesignRevision := { design with
    id := "design-coverage-successor"
    acceptanceCriteria := [{ criterion with statement := "the changed artifact is verified" }] }
  let coverageWork : Work := { work with
    baselineDesignRevision := some baseline.id, designRevision := some coverageSuccessor.id }
  let coverageState : ProjectState := { baseState with
    acceptedDesignId := some coverageSuccessor.id
    designRevisions := [baseline, coverageSuccessor], works := [coverageWork]
    implementationPlans := [], ledgerEntries := [] }
  let coverageDeltas ← fromExcept <|
    expectedStatementDeltas coverageState coverageWork coverageSuccessor
  expect (coverageDeltas.length == 1 && coverageDeltas.head?.any (·.kind == .modified))
    "Plan delta omitted a changed Criterion selected by an unchanged Statement"
  let extraStep : PlanStep := {
    id := "step-extra", description := "perform unrelated work"
    outputScopes := [criterion.target], verificationCriterionIds := [criterion.id] }
  let extraUnit : DesignSourceUnit := {
    planUnit with id := "plan-unit-extra", text := extraStep.description }
  let planWithExtraStep : ImplementationPlan := {
    plan with
    sourceUnits := [planUnit, extraUnit]
    sourceUnitDispositions := [
      { unitId := planUnit.id, stepId := some step.id },
      { unitId := extraUnit.id, stepId := some extraStep.id }]
    steps := [step, extraStep] }
  expectError (validateState { baseState with implementationPlans := [planWithExtraStep] })
    "Plan accepted a Markdown-grounded step with no Design delta or Finding obligation"
  expectError (closeTask baseState [] { entryId := "task-closed", taskEntryId := "task-open" })
    "Task closed without successful post-materialization evidence"
  let closed ← fromExcept <| closeTask evidencedState
    [TargetObservation.mk criterion.target "blake3:artifact"] {
    entryId := "task-closed", taskEntryId := "task-open" }
  let currentTasks := closed.ledgerEntries.filter fun entry =>
    !entryIsSuperseded closed entry && match entry.payload with
    | .task value => !value.retired
    | _ => false
  expect (currentTasks.length == 1 && currentTasks.head?.any fun entry =>
    match entry.payload with | .task value => value.closed | _ => false)
    "Task close did not preserve exactly one closed current lineage"
  fromExcept (validateState closed)
  let dependencyState := dependencyPlanState
  fromExcept (validateState dependencyState)
  let replaced ← fromExcept <| materializePlan dependencyState "plan-new" [] []
  for taskId in ["task-plan-new-a", "task-plan-new-b"] do
    let entry ← match replaced.entry? taskId with
      | some value => pure value
      | none => throw (IO.userError s!"replacement omitted Task {taskId}")
    expect (match entry.payload with | .task value => !value.closed | _ => false)
      s!"Plan replacement did not reopen affected transitive Task {taskId}"
  let oldPlan ← match dependencyState.plan? "plan-old" with
    | some value => pure value
    | none => throw (IO.userError "dependency fixture omitted its current Plan")
  let samePlan : ImplementationPlan := { oldPlan with
    id := "plan-same", predecessorPlanId := some oldPlan.id
    status := .candidate, contentDigest := "blake3:same" }
  let staleState : ProjectState := { dependencyState with
    implementationPlans := [oldPlan, samePlan] }
  let staleReplaced ← fromExcept <| materializePlan staleState samePlan.id [] []
  for taskId in ["task-plan-same-a", "task-plan-same-b"] do
    let entry ← match staleReplaced.entry? taskId with
      | some value => pure value
      | none => throw (IO.userError s!"stale replacement omitted Task {taskId}")
    expect (match entry.payload with | .task value => !value.closed | _ => false)
      s!"Plan replacement preserved closed Task {taskId} with stale evidence"

end AgentWorkbenchTest.Plan
