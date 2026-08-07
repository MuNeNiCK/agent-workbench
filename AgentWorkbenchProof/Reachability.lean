import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Ledger
import AgentWorkbench.Application.Plan
import AgentWorkbench.Application.Review
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Mutation

namespace AgentWorkbenchProof

open AgentWorkbench

private def routeStatement : Statement :=
  { id := "statement-route", text := "the two-step artifact is produced" }

private def routeCriterion : AcceptanceCriterion :=
  { id := "criterion-route", statementId := some routeStatement.id
    statement := "the final artifact is observed"
    target := "file:artifact.txt", evidenceKind := "artifact" }

private def routeDesignUnit : DesignSourceUnit :=
  { id := "design-unit-route", target := "file:.agent-workbench/design/product/design.md"
    path := ".agent-workbench/design/product/design.md", kind := .paragraph
    text := routeStatement.text, digest := "blake3:design-unit" }

private def routeDesign : DesignRevision :=
  { id := "design-1", workId := some "work-route"
    producerAgentRun := "agent-route", changeRationale := "initial Design for the route witness"
    revisionContentDigest := "blake3:design-route", sourceArchiveAvailable := true
    sourceDocuments := [{ target := routeDesignUnit.target, snapshot := "blake3:design-source" }]
    sourceUnits := [routeDesignUnit]
    sourceUnitDispositions := [{ unitId := routeDesignUnit.id, role := .requirement }]
    statements := [routeStatement]
    statementCoverage := [{
      statementId := routeStatement.id, sourceUnitIds := [routeDesignUnit.id]
      leanClaims := { noSelectionReason := some "the route has no Design-time logical Claim" }
      acceptanceCriteria := { selectedIds := [routeCriterion.id] }
      implementationRequired := true }]
    acceptanceCriteria := [routeCriterion] }

private def routeStepA : PlanStep :=
  { id := "a", description := "produce the dependency"
    outputScopes := [routeCriterion.target]
    verificationCriterionIds := [routeCriterion.id] }

private def routeStepB : PlanStep :=
  { id := "b", description := "produce the dependent output"
    dependsOnStepIds := [routeStepA.id]
    outputScopes := [routeCriterion.target]
    verificationCriterionIds := [routeCriterion.id] }

private def routePlanUnitA : DesignSourceUnit :=
  { id := "plan-unit-route"
    target := "file:.agent-workbench/design/plans/work-route/plan.md"
    path := ".agent-workbench/design/plans/work-route/plan.md", kind := .paragraph
    text := "produce the dependency and then the dependent output"
    digest := "blake3:plan-unit" }

private def routePlanUnitB : DesignSourceUnit :=
  { id := "plan-unit-route-b", target := routePlanUnitA.target, path := routePlanUnitA.path
    kind := .paragraph, text := "produce the dependent output", digest := "blake3:plan-unit-b" }

private def routePlan : ImplementationPlan :=
  { id := "plan-1", workId := "work-route", designRevision := routeDesign.id
    producerAgentRun := "agent-route", reason := "implement the complete initial Design delta"
    contentDigest := "blake3:plan-route", sourceArchiveAvailable := true
    sourceDocuments := [{ target := routePlanUnitA.target, digest := "blake3:plan-source" }]
    sourceUnits := [routePlanUnitA, routePlanUnitB]
    sourceUnitDispositions := [
      { unitId := routePlanUnitA.id, stepId := some routeStepA.id },
      { unitId := routePlanUnitB.id, stepId := some routeStepB.id }]
    statementDispositions := [{
      statementId := routeStatement.id, statementText := routeStatement.text
      deltaKind := .added, stepIds := [routeStepA.id, routeStepB.id] }]
    steps := [routeStepA, routeStepB] }

private def routeEvidence (taskId : String) : EntryPayload :=
  .artifactObservation {
    taskEntryId := some taskId, outputScope := some routeCriterion.target
    criterionId := routeCriterion.id, target := routeCriterion.target
    snapshot := "blake3:artifact", operation := "observe route artifact"
    result := "artifact exists", successful := true, producerAgentRun := "agent-route" }

/-- A constructive route over the actual production transitions. It starts one Work before a
Design exists, accepts the initial Design into that Work, materializes a non-empty dependency Plan,
closes both Tasks with current evidence in dependency order, and completes the same Work. -/
def constructiveNormalPath : Except String ProjectState := do
  let started ← startWorkRequest ProjectState.empty {
    id := "work-route", outcome := "complete the constructive route"
    scope := "project", responsibleAgentRun := "agent-route" }
  let proposed ← proposeDesign started routeDesign
  let accepted ← acceptDesign proposed routeDesign.id
  let planned ← proposePlan accepted routePlan
  let materialized ← materializePlan planned routePlan.id []
  let evidenceA ← appendCurrentEntry materialized "evidence-a"
    (routeEvidence "task-plan-1-a")
  let closedA ← closeTask evidenceA
    [{ target := routeCriterion.target, snapshot := "blake3:artifact" }]
    { entryId := "task-closed-a", taskEntryId := "task-plan-1-a" }
  let evidenceB ← appendCurrentEntry closedA "evidence-b"
    (routeEvidence "task-plan-1-b")
  let closedB ← closeTask evidenceB
    [{ target := routeCriterion.target, snapshot := "blake3:artifact" }]
    { entryId := "task-closed-b", taskEntryId := "task-plan-1-b" }
  completeFocusedWork closedB
    [{ target := routeCriterion.target, snapshot := "blake3:artifact" }] []
    "blake3:constructive-completion-input"

private def constructiveFinalState : ProjectState :=
  match constructiveNormalPath with
  | .ok state => state
  | .error _ => ProjectState.empty

private def constructiveNormalPathSucceeded : Bool :=
  match constructiveNormalPath with
  | .ok _ => true
  | .error _ => false

theorem constructive_normal_path_succeeds :
    constructiveNormalPath = .ok constructiveFinalState := by
  have succeeds : constructiveNormalPathSucceeded = true := by native_decide
  cases result : constructiveNormalPath with
  | error message => simp [constructiveNormalPathSucceeded, result] at succeeds
  | ok state => simp [constructiveFinalState, result]

theorem constructive_normal_path_keeps_one_completed_work :
    constructiveFinalState.works.length = 1 ∧
    constructiveFinalState.works.head?.any (fun work =>
      work.id == "work-route" && work.outcome == "complete the constructive route" &&
        work.status == .completed) = true := by
  native_decide

theorem constructive_normal_path_closes_nonempty_plan :
    constructiveFinalState.implementationPlans.head?.any (fun plan =>
      plan.status == .current && plan.steps.length == 2) = true ∧
    (constructiveFinalState.ledgerEntries.filter fun entry =>
      !entryIsSuperseded constructiveFinalState entry && match entry.payload with
      | .task task => task.required && task.closed && !task.retired
      | _ => false).length = 2 := by
  native_decide

theorem constructive_normal_path_records_completion_authority :
    (constructiveFinalState.ledgerEntries.filter fun entry => match entry.payload with
      | .workCompletion completion =>
          completion.workId == "work-route" && completion.designRevision == "design-1" &&
            completion.planId == "plan-1" && !completion.inputDigest.isEmpty
      | _ => false).length = 1 := by
  native_decide

theorem constructive_normal_path_is_valid :
    ValidProjectState constructiveFinalState := by
  let validationSucceeded := match validateState constructiveFinalState with
    | .ok _ => true
    | .error _ => false
  have succeeds : validationSucceeded = true := by native_decide
  cases result : validateState constructiveFinalState with
  | error message => simp [validationSucceeded, result] at succeeds
  | ok value =>
      cases value
      exact validProjectState_of_validation constructiveFinalState result

private def successorDesign
    (id digest rationale : String) (parent amended : Option String := none) : DesignRevision :=
  { routeDesign with
    id := id
    parent := parent
    amendsCandidate := amended
    changeRationale := rationale
    revisionContentDigest := digest }

/-- Candidate amendment is a distinct production route: the old candidate is superseded, the
amendment head is accepted, and accepted-Design ancestry remains separate from drafting lineage. -/
def constructiveAmendmentPath : Except String ProjectState := do
  let started ← startWorkRequest ProjectState.empty {
    id := "work-route", outcome := "complete the constructive route"
    scope := "project", responsibleAgentRun := "agent-route" }
  let first ← proposeDesign started routeDesign
  let amendment := successorDesign "design-2" "blake3:design-amendment"
    "replace the candidate without changing accepted ancestry" none (some routeDesign.id)
  let amended ← proposeDesign first amendment
  acceptDesign amended amendment.id

private def constructiveAmendmentFinal : ProjectState :=
  match constructiveAmendmentPath with
  | .ok state => state
  | .error _ => ProjectState.empty

theorem constructive_amendment_accepts_only_the_head :
    constructiveAmendmentFinal.acceptedDesignId = some "design-2" ∧
    (constructiveAmendmentFinal.design? "design-1").any (·.status == .superseded) ∧
    (constructiveAmendmentFinal.design? "design-2").any fun design =>
      design.status == .accepted && design.parent.isNone &&
        design.amendsCandidate == some "design-1" := by
  native_decide

/-- A strict accepted successor is adopted without replacing the Work. Suspension, adoption,
resume, and handoff preserve the original Work identity and outcome. -/
def constructiveSuccessorLifecycle : Except String ProjectState := do
  let started ← startWorkRequest ProjectState.empty {
    id := "work-route", outcome := "complete the constructive route"
    scope := "project", responsibleAgentRun := "agent-route" }
  let proposed ← proposeDesign started routeDesign
  let accepted ← acceptDesign proposed routeDesign.id
  let successor := successorDesign "design-2" "blake3:design-successor"
    "adopt a strict accepted successor" (some routeDesign.id)
  let proposedSuccessor ← proposeDesign accepted successor
  let suspended ← suspendWork proposedSuccessor "work-route" "adopt the accepted successor"
  let acceptedSuccessor ← acceptDesign suspended successor.id
  let adopted ← adoptDesignForWork acceptedSuccessor {
    workId := "work-route", entryId := "adoption-route", agentRun := "agent-route" }
  let resumed ← resumeWork adopted "work-route"
  handoffWork resumed "work-route" "handoff-route" "agent-successor" "continue the same outcome"

private def constructiveSuccessorFinal : ProjectState :=
  match constructiveSuccessorLifecycle with
  | .ok state => state
  | .error _ => ProjectState.empty

theorem constructive_successor_keeps_one_work_identity :
    constructiveSuccessorFinal.works.length = 1 ∧
    constructiveSuccessorFinal.focusedWorkId = some "work-route" ∧
    (constructiveSuccessorFinal.work? "work-route").any fun current =>
      current.outcome == "complete the constructive route" && current.status == .active &&
        current.designRevision == some "design-2" &&
        current.responsibleAgentRun == "agent-successor" := by
  native_decide

private def designReviewEntry : LedgerEntry := {
  id := "review-route", order := 1, scope := "project"
  workId := some "work-route", designRevision := some routeDesign.id
  payload := .review {
    reviewId := "review-lineage-route", purpose := .design, context := .fresh
    targetSourceId := routeDesign.id, target := s!"design:{routeDesign.id}"
    targetSnapshot := "blake3:review-route"
    targetManifest := [{
      kind := "design", id := routeDesign.id, snapshot := routeDesign.revisionContentDigest
      producerAgentRuns := [routeDesign.producerAgentRun] }]
    producerAgentRuns := [routeDesign.producerAgentRun]
    reviewerAgentRun := "reviewer-route" } }

/-- Review remains advisory: its Finding changes no Design. Only the responsible Work agent's
accepted disposition can become the explicit basis of a later candidate amendment. -/
def constructiveDesignFindingPath : Except String ProjectState := do
  let started ← startWorkRequest ProjectState.empty {
    id := "work-route", outcome := "complete the constructive route"
    scope := "project", responsibleAgentRun := "agent-route" }
  let proposed ← proposeDesign started routeDesign
  let reviewed ← (PreparedMutation.reviewStart designReviewEntry).execute proposed
  let found ← recordFinding reviewed {
    entryId := "finding-route", reviewEntryId := designReviewEntry.id
    subject := { kind := .statement, id := routeStatement.id, exactQuote := routeStatement.text }
    summary := "the candidate needs one explicit correction" }
  let concluded ← concludeReview found {
    entryId := "review-conclusion-route", reviewEntryId := designReviewEntry.id
    clean := false, summary := "one candidate correction is required" }
  let disposed ← recordDisposition concluded {
    entryId := "disposition-route", findingEntryId := "finding-route"
    decision := .accepted, reason := "amend the fixed candidate" }
  let correction := { (successorDesign "design-2" "blake3:design-reviewed-amendment"
    "apply the accepted fixed-target Finding" none (some routeDesign.id)) with
      changeBasisEntryIds := ["finding-route"] }
  proposeDesign disposed correction

private def constructiveDesignFindingFinal : ProjectState :=
  match constructiveDesignFindingPath with
  | .ok state => state
  | .error _ => ProjectState.empty

theorem constructive_review_is_advisory_until_disposition_and_amendment :
    constructiveDesignFindingFinal.acceptedDesignId.isNone ∧
    (constructiveDesignFindingFinal.design? "design-2").any fun design =>
      design.status == .candidate && design.changeBasisEntryIds == ["finding-route"] ∧
    (constructiveDesignFindingFinal.entry? "disposition-route").any fun entry =>
      match entry.payload with
      | .reviewDisposition value =>
          value.findingEntryId == "finding-route" && value.decision == .accepted
      | _ => false := by
  native_decide

/-- Functional rejection has no hidden mutable state: a rejected production transition returns no
post-state, so the authoritative pre-state is the only state available to the Store transaction. -/
theorem rejected_prepared_transition_has_no_post_state
    (prepared : PreparedMutation) (state : ProjectState) (message : String)
    (rejected : prepared.transition state = .error message) :
    (match prepared.transition state with | .error _ => state | .ok next => next) = state := by
  simp [rejected]

end AgentWorkbenchProof
