import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Plan

open AgentWorkbench AgentWorkbenchTest

private def containsText (text fragment : String) : Bool :=
  (text.splitOn fragment).length > 1

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
  expect (!containsText (Lean.toJson step).compress "taskVerificationContracts")
    "an empty Task-local Plan contract changed the immutable legacy Plan encoding"
  expect (!containsText (Lean.toJson taskEntry).compress "taskVerificationContracts")
    "an empty Task-local contract changed the immutable legacy Task encoding"
  expect (!containsText (Lean.toJson evidenceEntry).compress "taskVerificationId")
    "an absent Task-local binding changed the immutable legacy evidence encoding"
  let legacyProfile : CommandProfileRecord := {
    purpose := "legacy profile", command := { executable := "true" } }
  expect (!containsText (Lean.toJson legacyProfile).compress "taskVerificationIds")
    "an absent Task-local binding changed the immutable legacy profile encoding"
  let legacyCommand : CommandExecutionRecord := {
    profileEntryId := "legacy-profile", criterionId := some criterion.id
    command := { executable := "true" }, exitCode := 0
    stdoutDigest := "blake3:stdout", stderrDigest := "blake3:stderr"
    successful := true, producerAgentRun := work.responsibleAgentRun }
  expect (!containsText (Lean.toJson legacyCommand).compress "taskVerificationId")
    "an absent Task-local binding changed the immutable legacy command encoding"
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
  let unreachableStep : PlanStep := {
    step with outputScopes := ["file:unrelated-output"] }
  let unreachablePlan : ImplementationPlan := {
    plan with steps := [unreachableStep] }
  expectError (validateState { baseState with implementationPlans := [unreachablePlan] })
    "Plan accepted a Criterion whose target has no Task output route"

  -- An implementation-required Statement may intentionally have no Design Criterion. Its Plan
  -- must then state the concrete, Task-local command/artifact verification contract instead of
  -- inventing a Criterion or allowing an unverifiable Task.
  let localDesign : DesignRevision := { design with
    id := "design-task-local"
    acceptanceCriteria := []
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { noSelectionReason := some "no logical Claim is selected" }
      acceptanceCriteria := {
        noSelectionReason := some "verification is local to the implementation Task" }
      implementationRequired := true }] }
  let commandTarget := "file:task-local-command.txt"
  let artifactTarget := "file:task-local-artifact.txt"
  let localStep : PlanStep := {
    id := "task-local-step", description := "implement and verify the local behavior"
    outputScopes := [commandTarget, artifactTarget]
    taskVerificationContracts := [
      { id := "task-local-command", kind := .command, target := commandTarget },
      { id := "task-local-artifact", kind := .artifact, target := artifactTarget }] }
  let localPlan : ImplementationPlan := { plan with
    id := "plan-task-local", designRevision := localDesign.id
    steps := [localStep]
    sourceUnitDispositions := [{ unitId := planUnit.id, stepId := some localStep.id }]
    statementDispositions := [{
      statementId := statement.id, statementText := statement.text
      deltaKind := .added, stepIds := [localStep.id] }] }
  let localWork : Work := { work with designRevision := some localDesign.id }
  let localTask : LedgerEntry := {
    id := "task-local-open", order := 1, scope := localWork.scope
    workId := some localWork.id, designRevision := some localDesign.id
    payload := .task {
      planId := some localPlan.id, planStepId := some localStep.id
      lineageId := some s!"{localWork.id}:{localStep.id}"
      outputScopes := localStep.outputScopes
      verificationCriterionIds := []
      taskVerificationContracts := localStep.taskVerificationContracts
      materializedAtOrder := 0, description := localStep.description
      required := true, closed := false } }
  let localCommand : CommandSpec := {
    executable := "true", workingDirectory := some "." }
  let localProfile : LedgerEntry := {
    id := "profile-task-local", order := 2, scope := localWork.scope
    workId := some localWork.id, designRevision := some localDesign.id
    payload := .commandProfile {
      purpose := "verify the Task-local command contract"
      taskEntryId := some localTask.id, inputTargets := some []
      outputScope := some commandTarget, criterionIds := some []
      taskVerificationIds := some ["task-local-command"]
      target := some commandTarget, command := localCommand } }
  let commandSnapshot := "blake3:task-local-command"
  let localCommandEvidence : LedgerEntry := {
    id := "evidence-task-local-command", order := 3, scope := localWork.scope
    workId := some localWork.id, designRevision := some localDesign.id
    payload := .commandExecution {
      profileEntryId := localProfile.id, taskEntryId := some localTask.id
      outputScope := some commandTarget, criterionId := none
      taskVerificationId := some "task-local-command"
      inputSnapshots := some [], environmentSnapshots := some []
      target := some commandTarget, snapshot := some commandSnapshot
      command := localCommand, exitCode := 0
      stdoutDigest := "blake3:stdout", stderrDigest := "blake3:stderr"
      successful := true, producerAgentRun := localWork.responsibleAgentRun } }
  let artifactSnapshot := "blake3:task-local-artifact"
  let localArtifactEvidence : LedgerEntry := {
    id := "evidence-task-local-artifact", order := 4, scope := localWork.scope
    workId := some localWork.id, designRevision := some localDesign.id
    payload := .artifactObservation {
      taskEntryId := some localTask.id, outputScope := some artifactTarget
      criterionId := none, taskVerificationId := some "task-local-artifact"
      target := artifactTarget, snapshot := artifactSnapshot
      operation := "inspect Task-local artifact", result := "verified"
      successful := true, producerAgentRun := localWork.responsibleAgentRun } }
  let localState : ProjectState := {
    revision := 4, acceptedDesignId := some localDesign.id
    focusedWorkId := some localWork.id, designRevisions := [localDesign]
    works := [localWork], implementationPlans := [localPlan]
    ledgerEntries := [localTask, localProfile, localCommandEvidence, localArtifactEvidence] }
  fromExcept (validateState localState)
  let localClosed ← fromExcept <| closeTask localState [
    TargetObservation.mk commandTarget commandSnapshot,
    TargetObservation.mk artifactTarget artifactSnapshot] {
      entryId := "task-local-closed", taskEntryId := localTask.id }
  fromExcept (validateState localClosed)
  let closedLocalTask ← match localClosed.entry? "task-local-closed" with
    | some { payload := .task value, .. } => pure value
    | _ => throw (IO.userError "Task-local verification did not produce a closed Task")
  expect (closedLocalTask.verificationEvidenceEntryIds ==
      [localCommandEvidence.id, localArtifactEvidence.id])
    "Task-local command and artifact contracts did not bind one exact evidence entry each"
  let noVerificationPlan : ImplementationPlan := { localPlan with
    id := "plan-task-local-missing", steps := [{ localStep with
      verificationCriterionIds := [], taskVerificationContracts := [] }] }
  expectError (validateState { localState with
    implementationPlans := [noVerificationPlan], ledgerEntries := [] })
    "Plan accepted an implementation Task with no Criterion or Task-local verification contract"
  let wrongKindEvidence : LedgerEntry := { localArtifactEvidence with
    id := "evidence-task-local-wrong-kind"
    payload := .artifactObservation {
      taskEntryId := some localTask.id, outputScope := some commandTarget
      criterionId := none, taskVerificationId := some "task-local-command"
      target := commandTarget, snapshot := artifactSnapshot
      operation := "misclassify command verification as an artifact"
      result := "invalid", successful := true
      producerAgentRun := localWork.responsibleAgentRun } }
  expectError (validateState { localState with
    ledgerEntries := [localTask, localProfile, wrongKindEvidence] })
    "command evidence satisfied an artifact-only Task-local verification contract"
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
  let mixedFreshnessReplaced ← fromExcept <| materializePlan staleState samePlan.id
    [TargetObservation.mk criterion.target "blake3:old-evidence-b"] []
  for taskId in ["task-plan-same-a", "task-plan-same-b"] do
    let entry ← match mixedFreshnessReplaced.entry? taskId with
      | some value => pure value
      | none => throw (IO.userError s!"mixed-freshness replacement omitted Task {taskId}")
    expect (match entry.payload with | .task value => !value.closed | _ => false)
      s!"Plan replacement preserved dependent Task {taskId} after its dependency became stale"

  -- A Task can become stale after materialization when another legitimate Task changes a
  -- declared input. Recovery stays within Task state: no artificial replacement Plan is needed.
  expect (operationApplicable dependencyState
      [TargetObservation.mk criterion.target "blake3:old-evidence-b"] [] .taskReopenStale)
    "stale closed Task did not expose the semantic recovery operation"
  let reopened ← fromExcept <| reopenStaleTasks dependencyState
    [TargetObservation.mk criterion.target "blake3:old-evidence-b"]
  for lineage in [s!"{work.id}:a", s!"{work.id}:b"] do
    let current ← match reopened.ledgerEntries.find? fun entry =>
        !entryIsSuperseded reopened entry && match entry.payload with
        | .task task => task.lineageId == some lineage
        | _ => false with
      | some value => pure value
      | none => throw (IO.userError s!"stale recovery omitted Task lineage {lineage}")
    expect (match current.payload with
      | .task task => !task.closed && task.verificationEvidenceEntryIds.isEmpty &&
          task.verificationTaskEntryId.isNone
      | _ => false)
      s!"stale recovery retained closed state or inherited evidence for {lineage}"
  expect (reopened.currentPlanFor? work.id |>.any (·.id == oldPlan.id))
    "stale Task recovery replaced Plan authority"
  let openDependentState : ProjectState := { dependencyState with
    ledgerEntries := dependencyState.ledgerEntries.filter fun entry =>
      entry.id != "old-evidence-b" && entry.id != "old-task-b" }
  fromExcept (validateState openDependentState)
  let reopenedWithOpenDependent ← fromExcept <| reopenStaleTasks openDependentState []
  let retainedOpenDependent ← match reopenedWithOpenDependent.entry? "old-task-b-open" with
    | some value => pure value
    | none => throw (IO.userError "open dependent fixture lost its current Task")
  expect (!entryIsSuperseded reopenedWithOpenDependent retainedOpenDependent)
    "stale recovery replaced a transitive dependent that was already open"

  let reorderedPlan : ImplementationPlan := { oldPlan with
    id := "plan-reordered", predecessorPlanId := some oldPlan.id
    status := .candidate, contentDigest := "blake3:reordered"
    sourceUnitDispositions := oldPlan.sourceUnitDispositions.reverse
    statementDispositions := oldPlan.statementDispositions.map fun disposition =>
      { disposition with stepIds := disposition.stepIds.reverse } }
  let reorderedState : ProjectState := { dependencyState with
    implementationPlans := [oldPlan, reorderedPlan] }
  let reordered ← fromExcept <| materializePlan reorderedState reorderedPlan.id
    [TargetObservation.mk criterion.target "blake3:old-evidence-a"] []
  let reorderedTaskA ← match reordered.entry? "task-plan-reordered-a" with
    | some value => pure value
    | none => throw (IO.userError "reordered Plan omitted retained Task A")
  expect (match reorderedTaskA.payload with
    | .task value => value.closed && value.verificationEvidenceEntryIds == ["old-evidence-a"]
    | _ => false)
    "Plan replacement reopened a retained Task after obligation ordering changed only"

  let obligationBase := dependencyPlanState
  let obligationOld ← match obligationBase.plan? "plan-old" with
    | some value => pure value
    | none => throw (IO.userError "obligation fixture omitted its current Plan")
  let stepA ← match uniqueBy? obligationOld.steps (·.id) "a" with
    | some value => pure value
    | none => throw (IO.userError "obligation fixture omitted step A")
  let secondStatement : Statement := {
    id := "statement-2", text := "the second obligation is implemented" }
  let secondSource : DesignSourceUnit := { sourceUnit with
    id := "unit-2"
    path := "requirement/2"
    text := secondStatement.text
    digest := "blake3:unit-2" }
  let primaryCoverage ← match design.statementCoverage.head? with
    | some value => pure value
    | none => throw (IO.userError "obligation fixture omitted primary Statement coverage")
  let expandedDesign : DesignRevision := { design with
    sourceUnits := [sourceUnit, secondSource]
    sourceUnitDispositions := [
      { unitId := sourceUnit.id, role := .requirement },
      { unitId := secondSource.id, role := .requirement }]
    statements := [statement, secondStatement]
    statementCoverage := [primaryCoverage, {
      statementId := secondStatement.id, sourceUnitIds := [secondSource.id]
      leanClaims := { noSelectionReason := some "no logical Claim is selected" }
      acceptanceCriteria := { noSelectionReason := some "verified through the shared implementation output" }
      implementationRequired := true }] }
  let oldWithTwoObligations : ImplementationPlan := { obligationOld with
    statementDispositions := [
      { statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := ["a"] },
      { statementId := secondStatement.id, statementText := secondStatement.text
        deltaKind := .added, stepIds := ["b"] }] }
  let reassignedCandidate : ImplementationPlan := { oldWithTwoObligations with
    id := "plan-reassigned", predecessorPlanId := some oldWithTwoObligations.id
    status := .candidate, contentDigest := "blake3:reassigned"
    sourceUnitDispositions := obligationOld.sourceUnitDispositions.map fun disposition =>
      { disposition with stepId := some stepA.id }
    statementDispositions := [
      { statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := [stepA.id] },
      { statementId := secondStatement.id, statementText := secondStatement.text
        deltaKind := .added, stepIds := [stepA.id] }]
    steps := [stepA] }
  let obligationState : ProjectState := { obligationBase with
    designRevisions := [expandedDesign]
    implementationPlans := [oldWithTwoObligations, reassignedCandidate] }
  fromExcept (validateState obligationState)
  let obligationReplaced ← fromExcept <| materializePlan obligationState
    reassignedCandidate.id
    [TargetObservation.mk criterion.target "blake3:old-evidence-a"] []
  let reassignedTask ← match obligationReplaced.entry? "task-plan-reassigned-a" with
    | some value => pure value
    | none => throw (IO.userError "reassigned Plan omitted retained Task A")
  expect (match reassignedTask.payload with
    | .task value => !value.closed && value.verificationEvidenceEntryIds.isEmpty
    | _ => false)
    "Plan replacement preserved closing evidence after assigning a new obligation to retained step A"

end AgentWorkbenchTest.Plan
