import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Completion

open AgentWorkbench AgentWorkbenchTest

private def observations : List TargetObservation :=
  [{ target := criterion.target, snapshot := "blake3:artifact" }]

private def currentClosedState : IO ProjectState :=
  fromExcept <| closeTask evidencedState observations {
    entryId := "task-closed", taskEntryId := "task-open" }

private def expectCompletionRejected (state : ProjectState) (message : String) : IO Unit := do
  let before := state
  expectError (completeFocusedWork state observations [] "blake3:completion-input") message
  expect (state == before) s!"{message}; rejected pure completion changed its input state"

private def siblingTaskEvidenceState : ProjectState :=
  let stepA : PlanStep := { step with id := "shared-a", description := "produce the first output" }
  let stepB : PlanStep := { step with id := "shared-b", description := "produce the second output" }
  let unitA : DesignSourceUnit := { planUnit with id := "shared-unit-a", text := stepA.description }
  let unitB : DesignSourceUnit := { planUnit with id := "shared-unit-b", text := stepB.description }
  let sharedPlan : ImplementationPlan := { plan with
    id := "plan-shared", contentDigest := "blake3:plan-shared"
    sourceUnits := [unitA, unitB]
    sourceUnitDispositions := [
      { unitId := unitA.id, stepId := some stepA.id },
      { unitId := unitB.id, stepId := some stepB.id }]
    statementDispositions := [{
      statementId := statement.id, statementText := statement.text, deltaKind := .added
      stepIds := [stepA.id, stepB.id] }]
    steps := [stepA, stepB] }
  let openTask (id : String) (order : Nat) (planStep : PlanStep) : LedgerEntry := {
    id, order, scope := work.scope, workId := some work.id, designRevision := some design.id
    payload := .task {
      planId := some sharedPlan.id, planStepId := some planStep.id
      lineageId := some s!"{work.id}:{planStep.id}"
      outputScopes := planStep.outputScopes
      verificationCriterionIds := planStep.verificationCriterionIds
      materializedAtOrder := 0, description := planStep.description
      required := true, closed := false } }
  let evidence (id : String) (order : Nat) (taskId snapshot : String) : LedgerEntry := {
    id, order, scope := work.scope, workId := some work.id, designRevision := some design.id
    payload := .artifactObservation {
      taskEntryId := some taskId, outputScope := some criterion.target
      criterionId := criterion.id, target := criterion.target, snapshot
      operation := "verify exact Task output", result := "verified", successful := true
      producerAgentRun := work.responsibleAgentRun
      assuranceBinding := some <| design.assuranceBindingForCriterion
        work.responsibleAgentRun criterion.id } }
  let closedTask (id : String) (order : Nat) (source : LedgerEntry)
      (proof : LedgerEntry) (planStep : PlanStep) : LedgerEntry := {
    id, order, scope := work.scope, workId := some work.id, designRevision := some design.id
    supersedes := [source.id]
    payload := .task {
      planId := some sharedPlan.id, planStepId := some planStep.id
      lineageId := some s!"{work.id}:{planStep.id}"
      outputScopes := planStep.outputScopes
      verificationCriterionIds := planStep.verificationCriterionIds
      verificationEvidenceEntryIds := [proof.id]
      verificationTaskEntryId := some source.id
      materializedAtOrder := 0, description := planStep.description
      required := true, closed := true } }
  let openA := openTask "shared-open-a" 1 stepA
  let proofA := evidence "shared-evidence-a" 2 openA.id "blake3:older-output"
  let closedA := closedTask "shared-closed-a" 3 openA proofA stepA
  let openB := openTask "shared-open-b" 4 stepB
  let proofB := evidence "shared-evidence-b" 5 openB.id "blake3:current-output"
  let closedB := closedTask "shared-closed-b" 6 openB proofB stepB
  { revision := 8, acceptedDesignId := some design.id, focusedWorkId := some work.id
    designRevisions := [design], works := [work], implementationPlans := [sharedPlan]
    ledgerEntries := [openA, proofA, closedA, openB, proofB, closedB] }

def run : IO Unit := do
  let noPlan : ProjectState := { baseState with
    implementationPlans := [], ledgerEntries := [] }
  fromExcept (validateState noPlan)
  expectCompletionRejected noPlan "completion accepted a Work without a current Plan"

  fromExcept (validateState baseState)
  expectCompletionRejected baseState "completion accepted an open required Task"

  let closed ← currentClosedState
  expectError (completeFocusedWork closed
    [{ target := criterion.target, snapshot := "blake3:changed-after-task-close" }] []
    "blake3:completion-input")
    "completion accepted a Task whose exact closing evidence became stale"
  let siblingState := siblingTaskEvidenceState
  fromExcept (validateState siblingState)
  expect (!completionReady siblingState
    [{ target := criterion.target, snapshot := "blake3:current-output" }] [])
    "current evidence from a sibling Task substituted for stale evidence bound to another Task"

  -- A Criterion observation from a Task removed by Plan replacement is historical only. Closing
  -- the replacement Task through a local contract must not promote that old observation back into
  -- Design-level completion authority.
  IO.FS.withTempDir fun root => do
    IO.FS.writeFile (root / "artifact.txt") "replacement output\n"
    let priorClosed ← currentClosedState
    let contractStep : PlanStep := { step with
      verificationCriterionIds := []
      taskVerificationContracts := [{
        id := "replacement-contract", kind := .artifact, target := criterion.target }] }
    let replacementPlan : ImplementationPlan := { plan with
      id := "plan-contract-replacement", predecessorPlanId := some plan.id
      status := .candidate, contentDigest := "blake3:contract-replacement"
      steps := [contractStep] }
    let candidateState : ProjectState := { priorClosed with
      implementationPlans := [plan, replacementPlan] }
    let materialized ← fromExcept <| materializePlan candidateState replacementPlan.id observations []
    let replacementTaskId := s!"task-{replacementPlan.id}-{contractStep.id}"
    let observed ← observeArtifact root materialized {
      entryId := "replacement-contract-evidence", taskEntryId := replacementTaskId
      taskVerificationId := some "replacement-contract"
      operation := "inspect replacement output", result := "replacement output exists"
      successful := true }
    let currentInputs ← evaluateCurrentInputs root observed
    let replacementClosed ← fromExcept <| closeTask observed currentInputs.observations {
      entryId := "replacement-contract-closed", taskEntryId := replacementTaskId }
    fromExcept (validateState replacementClosed)
    let replacementProjection ← match currentProjection? replacementClosed with
      | some value => pure value
      | none => throw (IO.userError "replacement counterexample lost current projection")
    let completionInputs ← evaluateCurrentInputs root replacementClosed
    expect (!criterionEvidenceRecorded replacementProjection criterion)
      "superseded-Task Criterion evidence remained recorded for the replacement Plan"
    expect (!completionReady replacementClosed completionInputs.observations [])
      "superseded-Task Criterion evidence authorized replacement-Plan completion"

  let corrected ← fromExcept <| recordCorrection closed {
    entryId := "correction-completion-gap", content := "change the current expected result" }
  fromExcept (validateState corrected)
  expectCompletionRejected corrected "completion ignored an effective User Correction"

  IO.FS.withTempDir fun root => do
    IO.FS.writeFile (root / "artifact.txt") "observed"
    let inputs ← evaluateCurrentInputs root closed
    -- This counterexample is about completion's treatment of a historical accepted Finding.  Its
    -- immutable Review root predates the final-audit start gate, so reconstruct that valid history
    -- below the public start operation.
    let fixed ← match ← ReviewTarget.freeze root closed .implementation none
        inputs.observations inputs.claimDigests with
      | .ok value => pure value
      | .error message => throw (IO.userError message)
    let reviewed ← fromExcept <| appendEntry closed {
      id := "review-completion-gap", order := nextEntryOrder closed, scope := work.scope
      workId := some work.id, designRevision := some design.id
      payload := .review {
        reviewId := "review-completion-gap", purpose := .implementation, context := .fresh
        targetSourceId := fixed.sourceId, target := fixed.target
        targetSnapshot := fixed.snapshot, targetManifestVersion := fixed.manifestVersion
        targetManifest := fixed.manifest, producerAgentRuns := fixed.producerAgentRuns
        reviewerAgentRun := "reviewer-completion-gap" } }
    let found ← fromExcept <| recordFinding reviewed {
      entryId := "finding-completion-gap", reviewEntryId := "review-completion-gap"
      subject := { kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
      summary := "the accepted implementation still needs remediation" }
    let disposed ← fromExcept <| recordDisposition found {
      entryId := "disposition-completion-gap", findingEntryId := "finding-completion-gap"
      decision := .accepted, reason := "the fixed target establishes the gap" }
    fromExcept (validateState disposed)
    expectCompletionRejected disposed "completion ignored an unresolved accepted Finding"

    let completed ← fromExcept <| completeFocusedWork closed observations []
      "blake3:post-completion-input"
    let postReviewed ← fromExcept <| appendEntry completed {
      id := "review-after-completion", order := nextEntryOrder completed, scope := work.scope
      workId := some work.id, designRevision := some design.id
      payload := .review {
        reviewId := "review-after-completion", purpose := .implementation, context := .fresh
        targetSourceId := fixed.sourceId, target := fixed.target
        targetSnapshot := fixed.snapshot, targetManifestVersion := fixed.manifestVersion
        targetManifest := fixed.manifest, producerAgentRuns := fixed.producerAgentRuns
        reviewerAgentRun := "reviewer-after-completion" } }
    let postFound ← fromExcept <| recordFinding postReviewed {
      entryId := "finding-after-completion", reviewEntryId := "review-after-completion"
      subject := { kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
      summary := "the completed implementation is nonconforming" }
    expect (operationStructurallyApplicable postFound .reviewDisposition)
      "a post-completion Finding had no disposition route"
    let invalidated ← fromExcept <| recordDisposition postFound {
      entryId := "disposition-after-completion"
      findingEntryId := "finding-after-completion"
      decision := .accepted, impact := .implementationDefect
      reason := "the accepted Design already covers this implementation defect" }
    fromExcept (validateState invalidated)
    expect (invalidated.focusedWorkId.isNone &&
      (invalidated.work? work.id).any fun value =>
        value.status == .suspended && value.resumeCondition.isSome)
      "an accepted post-completion implementation defect did not prospectively invalidate completion"
    expect (invalidated.ledgerEntries.any fun entry => match entry.payload with
      | .workCompletion _ => true
      | _ => false)
      "post-completion invalidation deleted the original completion incident"
    let parent := { design with status := DesignStatus.superseded }
    let successor := withCurrentAssurance { design with
      id := "design-source-only-successor", parent := some design.id
      status := .accepted }
    let carriedState : ProjectState := { invalidated with
      acceptedDesignId := some successor.id
      designRevisions := [parent, successor]
      works := invalidated.works.map fun value =>
        if value.id == work.id then { value with designRevision := some successor.id }
        else value }
    expect ((acceptedImplementationFindingIds carriedState work.id successor.id).contains
      "finding-after-completion")
      "an unchanged successor Contract discarded predecessor Finding remediation authority"
    let changedSuccessor := withCurrentAssurance { successor with
      statements := [{ statement with text := "a different requirement" }] }
    let changedState := { carriedState with designRevisions := [parent, changedSuccessor] }
    expect (!(acceptedImplementationFindingIds changedState work.id changedSuccessor.id).contains
      "finding-after-completion")
      "a changed successor Contract inherited predecessor Finding remediation authority"
    let resumed ← fromExcept <| resumeWork invalidated {
      workId := work.id, entryId := "resume-after-completion-defect"
      satisfaction := "the accepted Finding records the exact same-Design remediation basis"
      basisEntryIds := ["disposition-after-completion"]
      agentRun := work.responsibleAgentRun }
    fromExcept (validateState resumed)
    expect (resumed.focusedWorkId == some work.id &&
      (resumed.work? work.id).any (·.status == .active))
      "the prospectively invalidated Work could not resume under the same Design"
    let replacementReady : ProjectState := { resumed with
      implementationPlans := resumed.implementationPlans.map fun value =>
        if value.id == plan.id then { value with status := .superseded } else value }
    fromExcept (validateState replacementReady)

  let claim : LeanClaim := {
    id := "claim-completion-gap"
    elaboratedPropositionDigest := "blake3:elaborated-claim"
    propositionDependencies := ["True"]
    input := {
      statementId := statement.id, statementText := statement.text
      mapping := "the selected proposition represents the accepted Statement"
      proposition := "CompletionDesign.Property", witness := "CompletionDesign.property"
      proofRoot := ".agent-workbench/design/proofs/completion"
      declaredSources := [{
        path := "CompletionDesign.lean", expectedDigest := some "blake3:claim-source" }]
      check := { executable := "lake", arguments := #["build"] }
      toolchain := ProofToolchain.identifier } }
  let claimDesign : DesignRevision := withCurrentAssurance { design with
    sourceDocuments := design.sourceDocuments ++ [{
      target := "file:.agent-workbench/design/proofs/completion/CompletionDesign.lean"
      mediaKind := "lean", snapshot := "blake3:claim-source" }]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { selectedIds := [claim.id] }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }]
    leanClaims := [claim] }
  let noClaimReceipt : ProjectState := { closed with
    designRevisions := [claimDesign]
    ledgerEntries := closed.ledgerEntries.map fun entry =>
      match entry.payload with
      | .artifactObservation evidence => { entry with
          payload := .artifactObservation { evidence with
            assuranceBinding := some <| claimDesign.assuranceBindingForCriterion
              work.responsibleAgentRun criterion.id } }
      | _ => entry }
  fromExcept (validateState noClaimReceipt)
  expectCompletionRejected noClaimReceipt "completion accepted a selected Claim without a receipt"

end AgentWorkbenchTest.Completion
