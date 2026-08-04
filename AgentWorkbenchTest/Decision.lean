import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Decision

open AgentWorkbench AgentWorkbenchTest

private def append (state : ProjectState) (entry : LedgerEntry) : IO ProjectState :=
  fromExcept (appendEntry state entry)

def run : IO Unit := do
  let mutableToolchainClaim : LeanClaim :=
    { claim with input := { claim.input with toolchain := "stable" } }
  let mutableToolchainDesign : DesignRevision :=
    { design with id := "design-mutable-toolchain", leanClaims := [mutableToolchainClaim] }
  match proposeDesign .empty mutableToolchainDesign with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "Design accepted a mutable Lean toolchain alias")
  let state ← fromExcept readyState
  expect (operationApplicable .empty "design propose" &&
    !operationApplicable .empty "task add" && operationApplicable state "task add")
    "native operation applicability did not follow current state"
  expect (completionReady state observations digests) "valid state was not ready"
  expect (!completionReady state
    [{ target := criterion.target, snapshot := "snapshot-b" }] digests)
    "stale evidence remained ready"
  expect (!completionReady state observations
    [{
      claimId := claim.id
      claimInput := claim.input
      sourceDigests := [{ path := "Proof.lean", digest := "source-a" }]
      inputDigest := "proof-input-b" }])
    "changed proof input reused an old receipt"
  let semanticTask ← fromExcept (addTask state {
    entryId := "entry-semantic-task", criterionId := some criterion.id
    description := "a request cannot select binding or order", required := true })
  let createdTask ← match semanticTask.entry? "entry-semantic-task" with
    | some value => pure value
    | none => throw (IO.userError "semantic Task operation created no entry")
  expect (createdTask.order == 4 && createdTask.scope == work.scope &&
    createdTask.workId == some work.id && createdTask.designRevision == some design.id &&
    createdTask.supersedes.isEmpty)
    "semantic Task operation did not derive its system-owned fields"
  let closedSemantic ← fromExcept (closeTask semanticTask {
    entryId := "entry-semantic-task-closed", taskEntryId := createdTask.id })
  let closedEntry ← match closedSemantic.entry? "entry-semantic-task-closed" with
    | some value => pure value
    | none => throw (IO.userError "semantic Task close created no entry")
  expect (closedEntry.order == 5 && closedEntry.supersedes == [createdTask.id])
    "semantic Task close did not derive exact supersession"

  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw (IO.userError "current projection was absent")
  expect (projection.entries.length == 3) "projection lost current entries"
  let context ← match currentContext? state observations digests with
    | some value => pure value
    | none => throw (IO.userError "current context was absent")
  expect context.unfinishedRequiredTasks.isEmpty
    "current context exposed a completed required task"
  expect (context.criterionGaps.isEmpty && context.claimGaps.isEmpty)
    "current context reported a gap for current evidence"
  let staleContext ← match currentContext? state
      [{ target := criterion.target, snapshot := "snapshot-b" }] digests with
    | some value => pure value
    | none => throw (IO.userError "stale current context was absent")
  expect (staleContext.criterionGaps.length == 1)
    "current context did not expose stale criterion evidence"

  match completeFocusedWork state
      [{ target := criterion.target, snapshot := "snapshot-b" }] digests with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "completion accepted stale evidence")
  let completed ← fromExcept (completeFocusedWork state observations digests)
  expect (completed.focusedWorkId.isNone &&
    completed.works.any (fun item => item.id == work.id && item.status == .completed))
    "completion did not close the ready focused Work"

  let commandCriterion : AcceptanceCriterion :=
    { criterion with evidenceKind := "command" }
  let commandDesign : DesignRevision :=
    { id := "design-command", producerAgentRun := design.producerAgentRun
      statements := design.statements, acceptanceCriteria := [commandCriterion] }
  let commandWork : Work :=
    { id := "work-command", outcome := work.outcome, scope := work.scope
      designRevision := commandDesign.id, status := .focused
      responsibleAgentRun := work.responsibleAgentRun
      delegatedReviewDecisions := work.delegatedReviewDecisions }
  let commandProposed ← fromExcept (proposeDesign .empty commandDesign)
  let commandAccepted ← fromExcept (acceptDesign commandProposed commandDesign.id)
  let commandStarted ← fromExcept (startWork commandAccepted commandWork)
  let withProfile ← fromExcept (defineProfile commandStarted {
    entryId := "profile-command-a", purpose := "current command evidence"
    target := some commandCriterion.target, command := { executable := "true" } })
  let withCommandEvidence ← append withProfile {
    id := "entry-command-evidence", order := 2, scope := commandWork.scope
    workId := some commandWork.id, designRevision := some commandDesign.id
    payload := .commandExecution {
      profileEntryId := "profile-command-a", criterionId := some commandCriterion.id
      target := some commandCriterion.target, snapshot := some "snapshot-a"
      command := { executable := "true", workingDirectory := some "." }
      exitCode := 0, stdoutDigest := "stdout", stderrDigest := "stderr"
      successful := true, producerAgentRun := commandWork.responsibleAgentRun } }
  let commandObservations := [{ target := commandCriterion.target, snapshot := "snapshot-a" }]
  expect (completionReady withCommandEvidence commandObservations [])
    "current Command Profile evidence did not satisfy its command criterion"
  let replacedProfile ← fromExcept (replaceProfile withCommandEvidence {
    entryId := "profile-command-b", profileEntryId := "profile-command-a"
    purpose := "replacement command", target := some commandCriterion.target
    command := { executable := "false" } })
  expect (!completionReady replacedProfile commandObservations [])
    "evidence from a superseded Command Profile remained current for readiness"

  let suspended ← fromExcept (suspendWork state work.id "continue current Work")
  let resumed ← fromExcept (focusWork suspended work.id)
  let resumedWork ← match resumed.work? work.id with
    | some value => pure value
    | none => throw (IO.userError "resume removed Work")
  expect (resumedWork.outcome == work.outcome && resumedWork.resumeCondition.isSome)
    "suspend/resume lost Work continuity"
  let handedOff ← fromExcept (handoffWork state work.id "entry-handoff" "agent-2"
    "continue the same Work in a new agent run")
  let handedWork ← match handedOff.work? work.id with
    | some value => pure value
    | none => throw (IO.userError "handoff removed Work")
  expect (handedWork.responsibleAgentRun == "agent-2" &&
    handedWork.outcome == work.outcome && handedWork.designRevision == work.designRevision &&
    handedWork.status == work.status)
    "handoff replaced Work context instead of its responsible run"
  let handedOffAgain ← fromExcept (handoffWork handedOff work.id "entry-handoff-2" "agent-3"
    "continue the same Work through another agent run")
  let handedWorkAgain ← match handedOffAgain.work? work.id with
    | some value => pure value
    | none => throw (IO.userError "second handoff removed Work")
  expect (handedWorkAgain.responsibleAgentRun == "agent-3")
    "a later handoff invalidated the immutable earlier handoff"
  let otherWork : Work :=
    { id := "work-2", outcome := "a separate focused outcome", scope := work.scope
      designRevision := work.designRevision, status := .focused
      responsibleAgentRun := "agent-other"
      delegatedReviewDecisions := work.delegatedReviewDecisions }
  let otherFocused ← fromExcept (startWork suspended otherWork)
  match handoffWork otherFocused work.id "entry-cross-work-handoff" "agent-cross"
      "must not borrow another Work's applicability" with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError
      "handoff used the focused Work's applicability to mutate a different suspended Work")

  let successor : DesignRevision := { design with id := "design-2", parent := some design.id }
  let proposedSuccessor ← fromExcept (proposeDesign suspended successor)
  let acceptedSuccessor ← fromExcept (acceptDesign proposedSuccessor successor.id)
  match focusWork acceptedSuccessor work.id with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "predecessor-bound Work resumed without design adoption")
  let laterSuccessor : DesignRevision :=
    { design with id := "design-3", parent := some successor.id }
  let proposedLater ← fromExcept (proposeDesign acceptedSuccessor laterSuccessor)
  let acceptedLater ← fromExcept (acceptDesign proposedLater laterSuccessor.id)
  let adopted ← fromExcept (adoptDesignForWork acceptedLater work.id "entry-adopt"
    "all predecessor evidence must be re-observed" "agent-1")
  let finalSuccessor : DesignRevision :=
    { design with id := "design-4", parent := some laterSuccessor.id }
  let proposedFinal ← fromExcept (proposeDesign adopted finalSuccessor)
  let acceptedFinal ← fromExcept (acceptDesign proposedFinal finalSuccessor.id)
  let adoptedAgain ← fromExcept (adoptDesignForWork acceptedFinal work.id "entry-adopt-2"
    "re-evaluate against the next accepted successor" "agent-1")
  let adoptedWorkAgain ← match adoptedAgain.work? work.id with
    | some value => pure value
    | none => throw (IO.userError "second design adoption removed Work")
  expect (adoptedWorkAgain.designRevision == finalSuccessor.id)
    "a later design adoption invalidated the immutable earlier adoption"
  let refocused ← fromExcept (focusWork adopted work.id)
  expect (!completionReady refocused observations digests)
    "successor design reused predecessor-bound completion evidence"

  let kptState ← append state {
    id := "entry-kpt", order := 4, scope := work.scope, workId := some work.id
    designRevision := some design.id
    payload := .kpt { tryNext := some "use a smaller verification command" } }
  expect (completionReady kptState observations digests)
    "KPT became an implicit completion requirement"
  let invalidKptApplication : LedgerEntry := {
    id := "entry-invalid-kpt-application", order := 5, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .kpt {
      keep := some "invalid self-application", appliesKptEntryId := some "entry-kpt"
      appliedByEntryId := some "entry-kpt" } }
  match appendEntry kptState invalidKptApplication with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "KPT accepted an action that did not follow its Try")
  let correctionState ← append state {
    id := "entry-correction", order := 4, scope := work.scope, workId := some work.id
    designRevision := some design.id
    payload := .userCorrection { content := "change the required outcome" } }
  expect (!completionReady correctionState observations digests)
    "open user correction did not block completion"
  let correctionAction ← fromExcept (addTask correctionState {
    entryId := "entry-correction-action", criterionId := none
    description := "apply the current correction", required := false })
  let correctionResolved ← fromExcept (resolveCorrection correctionAction {
    entryId := "entry-correction-resolved", correctionEntryId := "entry-correction"
    actionEntryId := "entry-correction-action", reason := "the later action applied the correction" })
  expect (completionReady correctionResolved observations digests)
    "explicit action-bound correction resolution did not restore readiness"
  let resolvedContext ← match currentContext? correctionResolved observations digests with
    | some value => pure value
    | none => throw (IO.userError "context disappeared after correction resolution")
  expect resolvedContext.effectiveUserCorrections.isEmpty
    "resolved correction remained current intent"
  let invalidIncorporatedCorrection : LedgerEntry := {
    id := "entry-incorporated-correction", order := 4, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .userCorrection {
      content := "clarify the accepted outcome"
      incorporatedIn := some design.id } }
  match appendEntry state invalidIncorporatedCorrection with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "a correction claimed incorporation by its own prior design")
  let correctionSuspended ← fromExcept (suspendWork correctionState work.id "adopt correction design")
  let correctionSuccessor : DesignRevision :=
    { design with id := "design-correction-2", parent := some design.id }
  let correctionProposed ← fromExcept (proposeDesign correctionSuspended correctionSuccessor)
  let correctionAccepted ← fromExcept (acceptDesign correctionProposed correctionSuccessor.id)
  let correctionAdopted ← fromExcept (adoptDesignForWork correctionAccepted work.id
    "entry-correction-adopt" "the correction is not yet incorporated" "agent-1")
  let correctionFocused ← fromExcept (focusWork correctionAdopted work.id)
  let carriedContext ← match currentContext? correctionFocused [] [] with
    | some value => pure value
    | none => throw (IO.userError "context disappeared after correction design adoption")
  expect (carriedContext.effectiveUserCorrections.length == 1)
    "unincorporated correction disappeared after successor adoption"
  let incorporatedCorrection ← append correctionFocused {
    id := "entry-correction-incorporated", order := 6, scope := work.scope
    workId := some work.id, designRevision := some design.id
    supersedes := ["entry-correction"]
    payload := .userCorrection {
      content := "change the required outcome"
      incorporatedIn := some correctionSuccessor.id } }
  let incorporatedContext ← match currentContext? incorporatedCorrection [] [] with
    | some value => pure value
    | none => throw (IO.userError "context disappeared after successor incorporation")
  expect incorporatedContext.effectiveUserCorrections.isEmpty
    "successor-incorporated correction remained unresolved"

  let invalidReview : LedgerEntry := {
    id := "entry-invalid-review", order := 4, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-invalid", purpose := .implementation, context := .fresh
      targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-a"
      producerAgentRun := "agent-1", reviewerAgentRun := "agent-1" } }
  match appendEntry state invalidReview with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "review by its target producer was accepted")
  let unboundReview : LedgerEntry := {
    id := "entry-unbound-review", order := 4, scope := work.scope
    workId := some work.id, designRevision := none
    payload := .review {
      reviewId := "review-unbound", purpose := .implementation, context := .fresh
      targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-a"
      producerAgentRun := "agent-1", reviewerAgentRun := "reviewer-unbound" } }
  match appendEntry state unboundReview with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "Review without a Design binding was accepted")

  let designReviewed ← append state {
    id := "entry-design-review", order := 4, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-design", purpose := .design, context := .fresh
      targetSourceId := design.id
      target := "design:design-1", targetSnapshot := "design-snapshot"
      producerAgentRun := "designer-1", reviewerAgentRun := "reviewer-design" } }
  let statementFinding ← append designReviewed {
    id := "entry-statement-finding", order := 5, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .finding {
      reviewId := "review-design", subject := {
        kind := .statement, id := statement.id, exactQuote := statement.text }
      mismatchEvidenceId := "entry-evidence", summary := "statement mismatch" } }
  let _ ← append statementFinding {
    id := "entry-assumption-finding", order := 6, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .finding {
      reviewId := "review-design", subject := {
        kind := .assumption, id := statement.id
        exactQuote := "artifact observation is externally truthful" }
      mismatchEvidenceId := "entry-evidence", summary := "assumption mismatch" } }

  let reviewed ← append state {
    id := "entry-review", order := 4, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-1", purpose := .implementation, context := .fresh
      targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-a"
      producerAgentRun := "agent-1", reviewerAgentRun := "reviewer-1" } }
  let otherWork : Work := { work with id := "work-2", status := .suspended }
  let multiWork ← fromExcept (validated { reviewed with works := reviewed.works ++ [otherWork] })
  let alternateFocused : Work :=
    { otherWork with status := .focused, scope := "other-project" }
  let originalSuspended := { work with status := .suspended }
  let alternateState ← fromExcept (validated { reviewed with
    focusedWorkId := some alternateFocused.id, works := [originalSuspended, alternateFocused] })
  expect (!operationApplicable alternateState "review resume" &&
    !operationApplicable alternateState "review finding")
    "operation applicability leaked an entry from another Work projection"
  let crossWorkFinding : LedgerEntry := {
    id := "entry-cross-finding", order := 5, scope := otherWork.scope
    workId := some otherWork.id, designRevision := some design.id
    payload := .finding {
      reviewId := "review-1", subject := {
        kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
      mismatchEvidenceId := "entry-evidence", summary := "cross-boundary mismatch" } }
  match appendEntry multiWork crossWorkFinding with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "finding crossed its Review Work binding")
  let withFinding ← append reviewed {
    id := "entry-finding", order := 5, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .finding {
      reviewId := "review-1", subject := {
        kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
      mismatchEvidenceId := "entry-evidence", summary := "artifact mismatch" } }
  let anotherFresh ← append withFinding {
    id := "entry-review-fresh-2", order := 6, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-2", purpose := .implementation, context := .fresh
      targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-a"
      producerAgentRun := "agent-1", reviewerAgentRun := "reviewer-2" } }
  let freshInput ← match reviewInput? anotherFresh "entry-review-fresh-2" with
    | some value => pure value
    | none => throw (IO.userError "fresh review input was absent")
  expect freshInput.lineage.isEmpty
    "fresh review input exposed earlier review/finding/remediation context"
  expect (completionReady withFinding observations digests)
    "an advisory, unaccepted review finding became normative"
  let acceptedFinding ← append withFinding {
    id := "entry-disposition", order := 6, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .reviewDisposition {
      findingEntryId := "entry-finding", decision := .accepted
      reason := "criterion evidence shows the mismatch", decidedByRun := "agent-1" } }
  expect (!completionReady acceptedFinding observations digests)
    "accepted unresolved finding did not block completion"
  let acceptedFindingHandedOff ← fromExcept (handoffWork acceptedFinding work.id
    "entry-disposition-handoff" "agent-2" "continue after recording the disposition")
  expect (!completionReady acceptedFindingHandedOff observations digests)
    "a Work handoff invalidated or erased an earlier disposition"
  let noDelegationWork := { work with delegatedReviewDecisions := [] }
  let noDelegationState ← fromExcept (validated {
    withFinding with works := [noDelegationWork] })
  match recordDisposition noDelegationState {
      entryId := "entry-undelegated-disposition", findingEntryId := "entry-finding"
      decision := .accepted, reason := "attempt decision outside authority" } with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "Review disposition exceeded structured delegation")
  let fakeResume ← append acceptedFinding {
    id := "entry-review-fake-resume", order := 7, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-1", purpose := .implementation, context := .resume
      continuesEntryId := some "entry-review", targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-a", producerAgentRun := "agent-1"
      reviewerAgentRun := "reviewer-1" } }
  let fakeVerification : LedgerEntry := {
    id := "entry-fake-verification", order := 8, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .reviewVerification {
      reviewId := "review-1", findingEntryId := "entry-finding"
      reviewEntryId := "entry-review-fake-resume", evidenceEntryId := "entry-task"
      target := criterion.target, snapshot := "snapshot-a"
      verifiedByRun := "reviewer-1", resolved := true } }
  match appendEntry fakeResume fakeVerification with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "a Task was accepted as review verification evidence")
  let withNewEvidence ← append acceptedFinding {
    id := "entry-evidence-b", order := 7, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .artifactObservation {
      criterionId := criterion.id, target := criterion.target, snapshot := "snapshot-b"
      operation := "verify fix", result := "success", successful := true
      producerAgentRun := "agent-1" } }
  let resumedReview ← append withNewEvidence {
    id := "entry-review-resume", order := 8, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-1", purpose := .implementation, context := .resume
      continuesEntryId := some "entry-review", targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-b", producerAgentRun := "agent-1"
      reviewerAgentRun := "reviewer-1" } }
  let selfProducedEvidence ← append acceptedFinding {
    id := "entry-reviewer-evidence", order := 7, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .artifactObservation {
      criterionId := criterion.id, target := criterion.target, snapshot := "snapshot-b"
      operation := "reviewer produced remediation", result := "success", successful := true
      producerAgentRun := "reviewer-1" } }
  let selfProducedResume ← append selfProducedEvidence {
    id := "entry-reviewer-resume", order := 8, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-1", purpose := .implementation, context := .resume
      continuesEntryId := some "entry-review", targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-b", producerAgentRun := "agent-1"
      reviewerAgentRun := "reviewer-1" } }
  match appendEntry selfProducedResume {
      id := "entry-reviewer-self-verification", order := 9, scope := work.scope
      workId := some work.id, designRevision := some design.id
      payload := .reviewVerification {
        reviewId := "review-1", findingEntryId := "entry-finding"
        reviewEntryId := "entry-reviewer-resume", evidenceEntryId := "entry-reviewer-evidence"
        target := criterion.target, snapshot := "snapshot-b"
        verifiedByRun := "reviewer-1", resolved := true } } with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError
      "reviewer verified a Finding with remediation evidence produced by the same run")
  let verified ← append resumedReview {
    id := "entry-verification", order := 9, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .reviewVerification {
      reviewId := "review-1", findingEntryId := "entry-finding"
      reviewEntryId := "entry-review-resume", evidenceEntryId := "entry-evidence-b"
      target := criterion.target, snapshot := "snapshot-b"
      verifiedByRun := "reviewer-1", resolved := true } }
  let observationsB := [{ target := criterion.target, snapshot := "snapshot-b" }]
  expect (completionReady verified observationsB digests)
    "resumed review verification did not resolve the accepted finding"
  let reviewProfileState ← fromExcept (defineProfile acceptedFinding {
    entryId := "entry-review-profile-a", purpose := "verify review remediation"
    target := some criterion.target, command := { executable := "true" } })
  let reviewCommandEvidence ← append reviewProfileState {
    id := "entry-review-command-evidence", order := 8, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .commandExecution {
      profileEntryId := "entry-review-profile-a", criterionId := none
      target := some criterion.target, snapshot := some "snapshot-a"
      command := { executable := "true", workingDirectory := some "." }
      exitCode := 0, stdoutDigest := "stdout", stderrDigest := "stderr"
      successful := true, producerAgentRun := work.responsibleAgentRun } }
  let reviewCommandResume ← append reviewCommandEvidence {
    id := "entry-review-command-resume", order := 9, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-1", purpose := .implementation, context := .resume
      continuesEntryId := some "entry-review", targetSourceId := "entry-evidence"
      target := criterion.target, targetSnapshot := "snapshot-a"
      producerAgentRun := "agent-1", reviewerAgentRun := "reviewer-1" } }
  let reviewCommandVerified ← append reviewCommandResume {
    id := "entry-review-command-verification", order := 10, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .reviewVerification {
      reviewId := "review-1", findingEntryId := "entry-finding"
      reviewEntryId := "entry-review-command-resume"
      evidenceEntryId := "entry-review-command-evidence"
      target := criterion.target, snapshot := "snapshot-a"
      verifiedByRun := "reviewer-1", resolved := true } }
  expect (completionReady reviewCommandVerified observations digests)
    "current Command Profile evidence did not resolve its Review finding"
  let reviewProfileReplaced ← fromExcept (replaceProfile reviewCommandVerified {
    entryId := "entry-review-profile-b", profileEntryId := "entry-review-profile-a"
    purpose := "replacement review verification"
    target := some criterion.target, command := { executable := "false" } })
  expect (!completionReady reviewProfileReplaced observations digests)
    "Review verification survived replacement of its producing Command Profile"
  let resumeInput ← match reviewInput? verified "entry-review-resume" with
    | some value => pure value
    | none => throw (IO.userError "resumed review input was absent")
  expect (!resumeInput.lineage.isEmpty)
    "resumed review input lost its Finding/remediation lineage"

  let commandCriterion : AcceptanceCriterion :=
    { criterion with id := "criterion-command", evidenceKind := "command" }
  let commandDesign : DesignRevision :=
    { design with id := "design-command", acceptanceCriteria := [commandCriterion] }
  let commandWork : Work :=
    { work with id := "work-command", designRevision := commandDesign.id }
  let commandProposed ← fromExcept (proposeDesign .empty commandDesign)
  let commandAccepted ← fromExcept (acceptDesign commandProposed commandDesign.id)
  let commandStarted ← fromExcept (startWork commandAccepted commandWork)
  let wrongKind : LedgerEntry := {
    id := "wrong-kind", order := 1, scope := commandWork.scope
    workId := some commandWork.id, designRevision := some commandDesign.id
    payload := .artifactObservation {
      criterionId := commandCriterion.id, target := commandCriterion.target
      snapshot := "snapshot-a", operation := "wrong kind", result := "success"
      successful := true, producerAgentRun := commandWork.responsibleAgentRun } }
  match appendEntry commandStarted wrongKind with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "artifact evidence satisfied a command criterion")

  let mut manyKpt := state
  for index in List.range 33 do
    manyKpt ← append manyKpt {
      id := s!"bounded-kpt-{index}", order := nextEntryOrder manyKpt, scope := work.scope
      workId := some work.id, designRevision := some design.id
      payload := .kpt { keep := some s!"bounded reference {index}" } }
  let bounded ← match currentContext? manyKpt observations digests with
    | some value => pure value
    | none => throw (IO.userError "bounded context was absent")
  expect (bounded.relevantKpt.length == currentContextLimit &&
    bounded.truncated.contains "relevantKpt")
    "current context returned an unbounded KPT record set"

  let temporalSuspended ← fromExcept (suspendWork state work.id "test correction chronology")
  let earlySuccessor : DesignRevision :=
    { design with id := "design-early-successor", parent := some design.id }
  let earlyProposed ← fromExcept (proposeDesign temporalSuspended earlySuccessor)
  let earlyAccepted ← fromExcept (acceptDesign earlyProposed earlySuccessor.id)
  let earlyAdopted ← fromExcept (adoptDesignForWork earlyAccepted work.id
    "entry-early-adopt" "successor existed before correction" "agent-1")
  let lateCorrection ← append earlyAdopted {
    id := "entry-late-correction", order := 5, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .userCorrection {
      content := "late correction" } }
  let invalidTemporalIncorporation : LedgerEntry := {
    id := "entry-invalid-temporal-incorporation", order := 6, scope := work.scope
    workId := some work.id, designRevision := some design.id
    supersedes := ["entry-late-correction"]
    payload := .userCorrection {
      content := "late correction"
      incorporatedIn := some earlySuccessor.id } }
  match appendEntry lateCorrection invalidTemporalIncorporation with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "a correction used a Design that predated the correction")

end AgentWorkbenchTest.Decision
