import AgentWorkbench.Application.Service

open AgentWorkbench
open AgentWorkbench.Domain

namespace AgentWorkbench.Tests.WorkflowLaws

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw <| IO.userError message

def execute (command : Kernel.Decide.Command) (state : Kernel.Replay.State)
    (message : String) : IO Kernel.Replay.State :=
  match Kernel.Decide.decide command state with
  | .ok transaction => pure transaction.result.state
  | .error error => throw <| IO.userError s!"{message}: {repr error}"

def reject (command : Kernel.Decide.Command) (state : Kernel.Replay.State)
    (message : String) : IO Unit :=
  match Kernel.Decide.decide command state with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError s!"{message}: command unexpectedly accepted"

def workOne : Domain.Work.WorkUnit :=
  { id := ⟨1⟩, status := .open, owner := "owner" }

def workTwo : Domain.Work.WorkUnit :=
  { id := ⟨2⟩, status := .open, owner := "owner" }

def activationOne : Domain.Work.Activation :=
  { id := ⟨1⟩, work := workOne.id, status := .active, readyToResume := false }

def designVersion : Domain.Design.DesignVersion :=
  { id := ⟨1⟩
    revision := ⟨1⟩
    owner := "owner"
    contentDigest := "sha256:design-v1"
    requirements := [
      { key := "resume-readiness", active := true },
      { key := "completion-integrity", active := true },
      { key := "review-authority", active := true },
      { key := "evidence-integrity", active := true },
      { key := "durable-corrections", active := true }]
    decisions := ["workflow mutations are kernel transitions"]
    validationGates := [
      "resume-matrix", "trace-matrix", "review-matrix",
      "evidence-matrix", "persistence-matrix"] }

def scope (stage digest : String) (work : WorkId) : Domain.Review.FrozenScope :=
  { design := some designVersion.id
    work
    repositorySnapshot := s!"commit:{digest}"
    artifactDigest := digest
    stage }

def reviewPlan (id : Nat) (stage digest : String) (work : WorkId) :
    Domain.Review.Plan :=
  { id := ⟨id⟩
    owner := "owner"
    reviewer := s!"reviewer-{id}"
    adjudicator := "owner"
    scope := scope stage digest work }

def claimFor (id : Nat) (plan : Domain.Review.Plan)
    (result : ReviewClaim) : Domain.Review.Claim :=
  { id := ⟨id⟩
    plan := plan.id
    work := plan.scope.work
    epoch := ⟨0⟩
    claim := result
    reviewer := plan.reviewer
    scope := some plan.scope }

def adjudicationFor (claim : Domain.Review.Claim) : Domain.Review.Adjudication :=
  { review := claim.id, decision := .accepted, adjudicator := "owner" }

set_option maxRecDepth 2048 in
def run : IO Unit := do
  let state ← execute
    (.initializeWork ⟨0⟩ workOne activationOne)
    Kernel.Replay.emptyState "work initialization failed"
  let state ← execute (.importDesign state.revision designVersion) state
    "immutable design import failed"
  reject (.approveDesign state.revision designVersion.id) state
    "unreviewed design approval"

  let badPlan := {
    reviewPlan 1 "design" designVersion.contentDigest workOne.id with
      reviewer := "owner" }
  reject (.recordReviewPlan state.revision badPlan) state
    "owner self-review"
  let exception : Domain.Review.AuthorityException :=
    { key := "self-review-authority", plan := badPlan.id, scope := badPlan.scope
      owner := badPlan.owner
      reviewer := badPlan.reviewer, adjudicator := badPlan.adjudicator
      authorizedBy := "user", reason := "scoped test exception" }
  let state ← execute (.recordAuthorityException state.revision exception) state
    "explicit user authority exception failed"
  reject (.recordReviewPlan state.revision
    { badPlan with scope := scope "implementation" "different-artifact" workOne.id })
    state "authority exception escaped its frozen scope"
  let state ← execute (.recordReviewPlan state.revision badPlan) state
    "authorized scoped review exception failed"
  reject (.recordReviewPlan state.revision
    { reviewPlan 11 "design" designVersion.contentDigest workOne.id with
      owner := "different-owner" })
    state "review plan owner did not derive from design and work ownership"

  let wrongArtifactPlan :=
    reviewPlan 12 "design" "sha256:different-design" workOne.id
  let state ← execute (.recordReviewPlan state.revision wrongArtifactPlan) state
    "wrong-artifact review plan setup failed"
  let wrongArtifactClaim := claimFor 12 wrongArtifactPlan .clean
  let state ← execute
    (.recordReviewClaim state.revision wrongArtifactClaim) state
    "wrong-artifact review claim setup failed"
  let state ← execute
    (.recordReviewAdjudication state.revision
      (adjudicationFor wrongArtifactClaim))
    state "wrong-artifact adjudication setup failed"
  reject (.approveDesign state.revision designVersion.id) state
    "design approval accepted a review of a different artifact"

  let designPlan := reviewPlan 10 "design" designVersion.contentDigest workOne.id
  let state ← execute (.recordReviewPlan state.revision designPlan) state
    "design review plan failed"
  let designClaim := claimFor 10 designPlan .clean
  reject
    (.recordReviewClaim state.revision { designClaim with scope := none })
    state "review claim without its frozen scope"
  let state ← execute (.recordReviewClaim state.revision designClaim) state
    "design review claim failed"
  reject (.approveDesign state.revision designVersion.id) state
    "advisory claim used as authority"
  reject
    (.recordReviewAdjudication state.revision
      { review := designClaim.id, decision := .accepted,
        adjudicator := designClaim.reviewer })
    state "reviewer self-adjudication"
  let state ← execute
    (.recordReviewAdjudication state.revision (adjudicationFor designClaim))
    state "owner adjudication failed"
  let state ← execute (.approveDesign state.revision designVersion.id) state
    "reviewed design approval failed"
  expect (state.designApprovals.any (·.design == designVersion.id))
    "approved design version was not durable"
  expect (Kernel.Gates.designReadyState designVersion.id state)
    "approved design did not pass design readiness"

  let state ← execute (.registerWork state.revision workTwo) state
    "implementation work registration failed"
  let untracedActivation : Domain.Work.Activation :=
    { id := ⟨2⟩, work := workTwo.id, status := .suspended
      readyToResume := false
      suspension := some {
        reason := "implementation handoff"
        returnPoint := "resume implementation"
        assumptions := ["approved design"]
        resumeConditions := ["reviewed decomposition"] }
      parent := some activationOne.id }
  reject (.registerSuspendedActivation state.revision untracedActivation) state
    "activation without reviewed decomposition"

  let decompositionPlan := reviewPlan 2 "decomposition" "decomposition-v1" workTwo.id
  let state ← execute (.recordReviewPlan state.revision decompositionPlan) state
    "decomposition review plan failed"
  let decompositionClaim := claimFor 2 decompositionPlan .clean
  let state ← execute (.recordReviewClaim state.revision decompositionClaim) state
    "decomposition review claim failed"
  let state ← execute
    (.recordReviewAdjudication state.revision (adjudicationFor decompositionClaim))
    state "decomposition adjudication failed"
  let decomposition : Domain.Design.Decomposition :=
    { key := "decomposition-v1"
      design := designVersion.id
      work := workTwo.id
      designRevision := designVersion.revision
      contentDigest := "decomposition-v1"
      items := [{
        key := "workflow-integrity"
        requirements := [
          "resume-readiness", "completion-integrity", "review-authority",
          "evidence-integrity", "durable-corrections"]
        implementationWork := ["kernel workflow transitions"]
        tasks := ["implement workflow transitions"]
        completionChecks := ["workflow laws"]
        checklists := ["authority and freshness reviewed"]
        validationGates := [
          "resume-matrix", "trace-matrix", "review-matrix",
          "evidence-matrix", "persistence-matrix"] }]
      reviewer := decompositionPlan.reviewer
      adjudicator := decompositionPlan.adjudicator
      accepted := true }
  let state ← execute (.recordDecomposition state.revision decomposition) state
    "reviewed decomposition failed"
  let approval : Domain.Design.Approval :=
    { design := designVersion.id, review := designClaim.id }
  expect (Policy.Traceability.ready designVersion approval decomposition)
    "complete reviewed trace did not become ready"
  expect (Kernel.Gates.traceReadyState designVersion.id workTwo.id state)
    "reviewed decomposition did not pass trace readiness"

  for incomplete in [
      { decomposition with items := decomposition.items.map fun item =>
          { item with requirements := [] } },
      { decomposition with items := decomposition.items.map fun item =>
          { item with implementationWork := [] } },
      { decomposition with items := decomposition.items.map fun item =>
          { item with tasks := [] } },
      { decomposition with items := decomposition.items.map fun item =>
          { item with completionChecks := [] } },
      { decomposition with items := decomposition.items.map fun item =>
          { item with checklists := [] } },
      { decomposition with items := decomposition.items.map fun item =>
          { item with validationGates := [] } }] do
    expect (!Policy.Traceability.ready designVersion approval incomplete)
      "an independently missing trace dimension passed readiness"
  let uncoveredDecomposition :=
    { decomposition with
      key := "uncovered-decomposition"
      items := decomposition.items.map fun item =>
        { item with requirements := ["unknown-requirement"] } }
  reject (.recordDecomposition state.revision uncoveredDecomposition) state
    "decomposition omitted an active design requirement"
  let uncoveredTraceState :=
    { state with decompositions := state.decompositions ++ [uncoveredDecomposition] }
  expect (!Kernel.Gates.traceReadyState
    designVersion.id workTwo.id uncoveredTraceState)
    "trace gate reused an older complete decomposition"

  let implementationPlan :=
    reviewPlan 3 "implementation" "sha256:implementation-v1" workTwo.id
  let state ← execute (.recordReviewPlan state.revision implementationPlan) state
    "implementation review plan failed"
  let initialFindingsClaim := claimFor 98 implementationPlan .findings
  let state ← execute (.recordReviewClaim state.revision initialFindingsClaim) state
    "initial implementation findings claim failed"
  let implementationClaim := claimFor 3 implementationPlan .clean
  let state ← execute (.recordReviewClaim state.revision implementationClaim) state
    "implementation review claim failed"
  let state ← execute
    (.recordReviewAdjudication state.revision (adjudicationFor implementationClaim))
    state "implementation review adjudication failed"

  let obligation : Domain.Evidence.Obligation :=
    { work := workTwo.id
      key := "evidence-matrix"
      revision := state.revision
      commandProfile := "workflow-laws"
      invocation := ".lake/build/bin/workflow-laws"
      repository := "agent-workbench"
      snapshot := implementationPlan.scope.repositorySnapshot
      artifactDigest := "sha256:workflow-laws"
      current := true
      kind := .test
      requirements := ["resume-readiness", "evidence-integrity"]
      expectedProducer := "workflow-law-runner"
      expectedObservation := "observation-1"
      design := designVersion.id
      designRevision := designVersion.revision }
  let state ← execute (.recordObligation state.revision obligation) state
    "traceable obligation failed"
  let evidenceOne : Domain.Evidence.Evidence :=
    { id := ⟨1⟩
      work := workTwo.id
      obligation := obligation.key
      revision := obligation.revision
      commandProfile := obligation.commandProfile
      invocation := obligation.invocation
      exitCode := 0
      repository := obligation.repository
      snapshot := obligation.snapshot
      artifactDigest := obligation.artifactDigest
      current := true
      kind := obligation.kind
      requirements := obligation.requirements
      producer := obligation.expectedProducer
      observedAt := "observation-1"
      design := obligation.design
      designRevision := obligation.designRevision }
  reject (.recordEvidence state.revision { evidenceOne with producer := "" }) state
    "evidence without provenance"
  let state ← execute (.recordEvidence state.revision evidenceOne) state
    "exact evidence recording failed"

  let basisOne : Domain.Work.ReadinessBasis :=
    { design := designVersion.id
      designRevision := designVersion.revision
      decompositionKey := decomposition.key
      decompositionDigest := decomposition.contentDigest
      repositorySnapshot := implementationPlan.scope.repositorySnapshot
      obligationKeys := [obligation.key]
      evidenceRevision := evidenceOne.revision
      reviewPlan := implementationPlan.id }
  let childSuspension : Domain.Work.SuspensionContext :=
    { reason := "independent workflow review"
      returnPoint := "activate reviewed implementation plan"
      assumptions := ["design-v1", "repository-snapshot-v1"]
      resumeConditions := ["trace complete", "corrections resolved"]
      basis := some basisOne }
  let activationTwo : Domain.Work.Activation :=
    { id := ⟨2⟩, work := workTwo.id, status := .suspended
      readyToResume := false, suspension := some childSuspension
      parent := some activationOne.id }
  reject
    (.registerSuspendedActivation state.revision
      { activationTwo with readyToResume := true })
    state "pre-confirmed activation bypass"
  let state ← execute (.registerSuspendedActivation state.revision activationTwo) state
    "traceable activation registration failed"
  let parentSuspension : Domain.Work.SuspensionContext :=
    { reason := "child implementation"
      returnPoint := "parent completion"
      assumptions := ["child returns"]
      resumeConditions := ["child terminal"] }
  let state ← execute
    (.suspendWork state.revision workOne.id activationOne.id parentSuspension)
    state "active work suspension failed"
  expect (Kernel.Replay.readinessCurrent
    workTwo.id activationTwo.id basisOne state)
    "exact readiness basis did not become current"
  let closedParentState :=
    { state with activations := state.activations.map fun activation =>
        if activation.id == activationOne.id then
          { activation with status := .closed }
        else activation }
  expect (!Kernel.Replay.readinessCurrent
    workTwo.id activationTwo.id basisOne closedParentState)
    "readiness accepted a closed stack parent"
  let missingParentState :=
    { state with activations := state.activations.map fun activation =>
        if activation.id == activationTwo.id then
          { activation with parent := some ⟨99⟩ }
        else activation }
  expect (!Kernel.Replay.readinessCurrent
    workTwo.id activationTwo.id basisOne missingParentState)
    "readiness accepted a missing stack parent"
  for staleBasis in [
      { basisOne with design := ⟨99⟩ },
      { basisOne with designRevision := designVersion.revision.next },
      { basisOne with decompositionKey := "wrong-decomposition" },
      { basisOne with decompositionDigest := "wrong-decomposition" },
      { basisOne with repositorySnapshot := "wrong-snapshot" },
      { basisOne with obligationKeys := ["wrong-obligation"] },
      { basisOne with evidenceRevision := evidenceOne.revision.next },
      { basisOne with reviewPlan := ⟨99⟩ }] do
    expect (!Kernel.Replay.readinessCurrent
      workTwo.id activationTwo.id staleBasis state)
      "stale readiness basis passed exact resume checks"
  let state ← execute
    (.confirmResumeReadiness state.revision workTwo.id activationTwo.id basisOne)
    state "resume readiness confirmation failed"
  expect (Kernel.Gates.resumeReadyState workTwo.id activationTwo.id state)
    "confirmed activation did not pass resume readiness"
  let laterSnapshotFinding : Domain.Review.Finding :=
    { key := "later-snapshot-resume-finding"
      review := initialFindingsClaim.id
      blocking := true
      invariant := "finding verification remains snapshot exact"
      remediationSurfaces := ["AgentWorkbench/Kernel/Replay.lean"]
      accepted := false
      adjudicated := false
      closed := false }
  reject (.recordReviewFinding state.revision laterSnapshotFinding) state
    "historical findings claim accepted a later finding"
  let laterSnapshotScope :=
    { implementationPlan.scope with
      repositorySnapshot := "commit:later-snapshot" }
  let laterSnapshotVerification : Domain.Review.Verification :=
    { finding := laterSnapshotFinding.key
      verifier := "later-snapshot-verifier"
      scope := laterSnapshotScope
      evidenceDigest := "sha256:later-fix"
      claimFixed := true
      adjudicator := "owner"
      adjudicated := true
      accepted := true }
  let crossSnapshotFinding :=
    { laterSnapshotFinding with
      accepted := true
      adjudicated := true
      closed := true
      closureEvidence := laterSnapshotVerification.evidenceDigest
      closureSnapshot := laterSnapshotScope.repositorySnapshot }
  let crossSnapshotState :=
    { state with
      reviewFindings := state.reviewFindings ++ [crossSnapshotFinding]
      findingVerifications :=
        state.findingVerifications ++ [laterSnapshotVerification] }
  expect (!Kernel.Replay.resumeCurrent
    workTwo.id activationTwo.id crossSnapshotState)
    "verification at a later snapshot reactivated an older readiness basis"
  let wrongArtifactScope :=
    { implementationPlan.scope with artifactDigest := "sha256:other-artifact" }
  let wrongArtifactVerification :=
    { laterSnapshotVerification with
      scope := wrongArtifactScope }
  let wrongArtifactFinding :=
    { laterSnapshotFinding with
      accepted := true
      adjudicated := true
      closed := true
      closureEvidence := wrongArtifactVerification.evidenceDigest
      closureSnapshot := wrongArtifactScope.repositorySnapshot }
  let wrongArtifactState :=
    { state with
      reviewFindings := state.reviewFindings ++ [wrongArtifactFinding]
      findingVerifications :=
        state.findingVerifications ++ [wrongArtifactVerification] }
  expect (!Kernel.Replay.resumeCurrent
    workTwo.id activationTwo.id wrongArtifactState)
    "verification of another artifact reactivated readiness"
  let unadjudicatedBlockingFinding : Domain.Review.Finding :=
    { key := "unadjudicated-resume-finding"
      review := implementationClaim.id
      blocking := true
      invariant := "unadjudicated findings invalidate readiness"
      remediationSurfaces := ["AgentWorkbench/Kernel/Replay.lean"]
      accepted := false
      adjudicated := false
      closed := false }
  let unadjudicatedFindingState :=
    { state with
      reviewFindings := state.reviewFindings ++ [unadjudicatedBlockingFinding] }
  expect (!Kernel.Replay.resumeCurrent
    workTwo.id activationTwo.id unadjudicatedFindingState)
    "an unadjudicated blocking finding did not invalidate resume readiness"
  let newerFindingsClaim :=
    { implementationClaim with id := ⟨99⟩, claim := ReviewClaim.findings }
  let unadjudicatedClaimState :=
    { state with claims := state.claims ++ [newerFindingsClaim] }
  expect (!Kernel.Replay.resumeCurrent
    workTwo.id activationTwo.id unadjudicatedClaimState)
    "older clean claim remained resumable after a newer unadjudicated claim"
  let newerClaimState :=
    { unadjudicatedClaimState with
      adjudications := unadjudicatedClaimState.adjudications ++
        [adjudicationFor newerFindingsClaim] }
  expect (!Kernel.Replay.resumeCurrent workTwo.id activationTwo.id newerClaimState)
    "older clean claim remained resumable after a newer findings claim"
  let newerImplementationPlan :=
    reviewPlan 99 "implementation" "sha256:implementation-v2" workTwo.id
  let newerPlanState :=
    { state with reviewPlans := state.reviewPlans ++ [newerImplementationPlan] }
  expect (!Kernel.Replay.resumeCurrent workTwo.id activationTwo.id newerPlanState)
    "older review plan remained resumable after a newer plan was recorded"
  let newerObligation : Domain.Evidence.Obligation :=
    { work := obligation.work
      key := "additional-current-evidence"
      revision := obligation.revision
      commandProfile := obligation.commandProfile
      invocation := obligation.invocation
      repository := obligation.repository
      snapshot := obligation.snapshot
      artifactDigest := obligation.artifactDigest
      current := true
      kind := obligation.kind
      requirements := obligation.requirements
      expectedProducer := obligation.expectedProducer
      expectedObservation := "observation-newer"
      design := obligation.design
      designRevision := obligation.designRevision }
  let newerEvidence : Domain.Evidence.Evidence :=
    { id := ⟨99⟩
      work := newerObligation.work
      obligation := newerObligation.key
      revision := newerObligation.revision
      commandProfile := newerObligation.commandProfile
      invocation := newerObligation.invocation
      exitCode := 0
      repository := newerObligation.repository
      snapshot := newerObligation.snapshot
      artifactDigest := newerObligation.artifactDigest
      current := true
      kind := newerObligation.kind
      requirements := newerObligation.requirements
      producer := newerObligation.expectedProducer
      observedAt := newerObligation.expectedObservation
      design := newerObligation.design
      designRevision := newerObligation.designRevision }
  reject (.recordObligation state.revision newerObligation) state
    "later obligation reused a frozen evidence revision"
  let newerObligationState :=
    { state with
      obligations := state.obligations ++ [newerObligation]
      evidence := state.evidence ++ [newerEvidence] }
  expect (!Kernel.Replay.resumeCurrent
    workTwo.id activationTwo.id newerObligationState)
    "older evidence basis remained resumable with a different current obligation"
  let newerDecomposition :=
    { decomposition with
      key := "decomposition-v2"
      contentDigest := decomposition.contentDigest }
  let newerDecompositionState :=
    { state with decompositions := state.decompositions ++ [newerDecomposition] }
  expect (!Kernel.Replay.resumeCurrent
    workTwo.id activationTwo.id newerDecompositionState)
    "older trace basis remained resumable after a newer decomposition was recorded"

  let correction : Domain.Design.Correction :=
    { key := "resume-readiness-correction", scope := "workflow"
      statement := "resume only after current readiness is re-established"
      resolved := false, work := some workTwo.id, design := some designVersion.id }
  let state ← execute (.recordUserCorrection state.revision correction) state
    "durable correction failed"
  expect (!Kernel.Gates.correctionsReadyState correction.scope state)
    "unresolved correction passed correction readiness"
  reject (.resumeWork state.revision workTwo.id activationTwo.id) state
    "stale confirmation resumed after correction"
  let state ← execute (.resolveUserCorrection state.revision correction.key) state
    "correction resolution failed"
  let rule : Domain.Design.LearnedRule :=
    { key := "resume-readiness-rule", correction := correction.key, scope := correction.scope
      statement := correction.statement }
  let state ← execute (.promoteCorrection state.revision rule) state
    "learning promotion failed"

  let obligationTwo :=
    { { obligation with revision := state.revision } with
      expectedObservation := "observation-2" }
  let state ← execute (.recordObligation state.revision obligationTwo) state
    "replacement obligation failed"
  let evidenceTwo :=
    { evidenceOne with
      id := ⟨2⟩
      revision := obligationTwo.revision
      observedAt := "observation-2" }
  let state ← execute (.recordEvidence state.revision evidenceTwo) state
    "replacement evidence failed"
  expect (state.evidence.any fun item =>
    item.id == evidenceOne.id && item.revision == evidenceOne.revision)
    "historical evidence revision was mutated"
  let historicalObligation ←
    match state.obligations.find? fun item =>
        item.work == obligation.work && item.key == obligation.key &&
        item.revision == obligation.revision with
    | some item => pure item
    | none => throw <| IO.userError "historical obligation was discarded"
  let historicalEvidence ←
    match state.evidence.find? (·.id == evidenceOne.id) with
    | some item => pure item
    | none => throw <| IO.userError "historical evidence was discarded"
  expect (Domain.Evidence.historicalExact
    historicalEvidence historicalObligation)
    "historical evidence lost its exact obligation provenance"
  expect (!Domain.Evidence.historicalExact
    { historicalEvidence with producer := "different-producer" }
    historicalObligation)
    "mutated historical provenance remained valid"
  let basisTwo := { basisOne with evidenceRevision := evidenceTwo.revision }
  let revisedSuspension := { childSuspension with basis := some basisTwo }
  let state ← execute
    (.reviseSuspension state.revision workTwo.id activationTwo.id revisedSuspension)
    state "suspension basis revision failed"
  let state ← execute
    (.confirmResumeReadiness state.revision workTwo.id activationTwo.id basisTwo)
    state "current resume readiness confirmation failed"
  let state ← execute
    (.resumeWork state.revision workTwo.id activationTwo.id)
    state "reviewed implementation activation failed"
  expect ((Domain.Work.activeFor state.activations workTwo.id).isSome)
    "resumed work is not the unique active frame"
  expect (state.activations.any fun activation =>
    activation.id == activationOne.id && activation.status == .suspended)
    "child activation lost its stack-return parent"

  let findingPlan := reviewPlan 4 "implementation" "sha256:implementation-v1" workTwo.id
  let state ← execute (.recordReviewPlan state.revision findingPlan) state
    "finding review plan failed"
  let findingClaim := claimFor 4 findingPlan .findings
  let state ← execute (.recordReviewClaim state.revision findingClaim) state
    "finding review claim failed"
  let finding : Domain.Review.Finding :=
    { key := "resume-evidence-finding"
      review := findingClaim.id
      blocking := true
      invariant := "resume evidence remains exact"
      remediationSurfaces := ["AgentWorkbench/Kernel/Decide.lean"]
      accepted := false
      adjudicated := false
      closed := false }
  let state ← execute (.recordReviewFinding state.revision finding) state
    "finding recording failed"
  reject (.closeReviewFinding state.revision finding.key
    "sha256:fix" "commit:workflow-v2") state
    "unadjudicated finding closure"
  let state ← execute
    (.adjudicateReviewFinding state.revision finding.key "owner" true)
    state "finding adjudication failed"
  let bypassPlan := reviewPlan 5 "implementation" "sha256:implementation-v2" workTwo.id
  let state ← execute (.recordReviewPlan state.revision bypassPlan) state
    "bypass review plan failed"
  let bypassClaim := claimFor 5 bypassPlan .clean
  let state ← execute (.recordReviewClaim state.revision bypassClaim) state
    "bypass clean claim failed"
  let state ← execute
    (.recordReviewAdjudication state.revision (adjudicationFor bypassClaim))
    state "bypass claim adjudication failed"
  expect (!Kernel.Gates.reviewReadyState bypassPlan.id state)
    "fresh clean claim bypassed an accepted open blocking finding"

  let state ← execute (.closeReviewFinding state.revision finding.key
    "sha256:fix" bypassPlan.scope.repositorySnapshot) state
    "finding remediation closure failed"
  reject
    (.verifyReviewFinding state.revision {
      finding := finding.key, verifier := findingClaim.reviewer
      scope := bypassPlan.scope, evidenceDigest := "sha256:fix"
      claimFixed := true, accepted := false })
    state "reviewer self-verification"
  let verification : Domain.Review.Verification :=
    { finding := finding.key, verifier := "fresh-verifier"
      scope := bypassPlan.scope, evidenceDigest := "sha256:fix"
      claimFixed := true, accepted := false }
  let state ← execute (.verifyReviewFinding state.revision verification) state
    "independent finding verification failed"
  expect (!Kernel.Gates.reviewReadyState bypassPlan.id state)
    "unadjudicated verification changed readiness"
  let state ← execute
    (.adjudicateFindingVerification state.revision finding.key "owner")
    state "verification adjudication failed"
  expect (Policy.Authority.blockingFindingsClosed findingClaim.id
    state.claims state.reviewFindings state.findingVerifications)
    "verified accepted blocking finding remained open"
  expect (!Kernel.Gates.reviewReadyState findingPlan.id state)
    "a findings review incorrectly replaced the required fresh clean review"

  let freshPlan := reviewPlan 6 "implementation" "sha256:implementation-v2" workTwo.id
  let state ← execute (.recordReviewPlan state.revision freshPlan) state
    "fresh review plan failed"
  let freshClaim := claimFor 6 freshPlan .clean
  let state ← execute (.recordReviewClaim state.revision freshClaim) state
    "fresh clean review claim failed"
  let state ← execute
    (.recordReviewAdjudication state.revision (adjudicationFor freshClaim))
    state "fresh clean review adjudication failed"
  expect (Kernel.Gates.reviewReadyState freshPlan.id state)
    "fresh clean adjudicated review did not pass readiness"

  for wrong in [
      { evidenceTwo with work := ⟨99⟩ },
      { evidenceTwo with obligation := "wrong-obligation" },
      { evidenceTwo with kind := .build },
      { evidenceTwo with commandProfile := "wrong-profile" },
      { evidenceTwo with invocation := "wrong-invocation" },
      { evidenceTwo with exitCode := 1 },
      { evidenceTwo with design := ⟨99⟩ },
      { evidenceTwo with designRevision := evidenceTwo.designRevision.next },
      { evidenceTwo with artifactDigest := "sha256:wrong" },
      { evidenceTwo with repository := "wrong-repository" },
      { evidenceTwo with snapshot := "wrong-snapshot" },
      { evidenceTwo with revision := evidenceTwo.revision.next },
      { evidenceTwo with current := false },
      { evidenceTwo with requirements := ["wrong-requirement"] },
      { evidenceTwo with producer := "wrong-producer" },
      { evidenceTwo with observedAt := "wrong-observation" }] do
    expect (!Domain.Evidence.exactFor wrong obligationTwo)
      "independently mismatched evidence matched its obligation"
  expect (Kernel.Gates.evidenceExactState workTwo.id obligationTwo.key state)
    "exact traceable evidence did not pass evidence readiness"

  let publicStore ←
    match Application.Service.execute Application.Service.bootstrapCommand
        Application.Service.initialStore with
    | .ok transaction => pure transaction.result
    | .error error => throw <| IO.userError s!"public bootstrap failed: {repr error}"
  let publicCorrection : Domain.Design.Correction :=
    { key := "PUBLIC-COR", scope := "workflow", statement := "persist"
      resolved := false, work := some workOne.id }
  let publicStore ←
    match Application.Service.execute
        (.recordUserCorrection publicStore.ledger.storedHead publicCorrection)
        publicStore with
    | .ok transaction => pure transaction.result
    | .error error => throw <| IO.userError s!"public correction failed: {repr error}"
  let replacementSession := { publicStore with active := none }
  let recoveryAction ←
    match (Application.Service.resolve replacementSession).value with
    | .action action => pure action
    | .blocked blocker =>
        throw <| IO.userError s!"replacement session recovery blocked: {repr blocker}"
  let recoveredStore ←
    match Application.Service.executeRecovery recoveryAction replacementSession with
    | .ok transaction => pure transaction.adopted.result
    | .error error =>
        throw <| IO.userError s!"replacement session replay failed: {repr error}"
  let recoveredState ←
    match (Application.Service.status recoveredStore).value.currentState? with
    | some recovered => pure recovered
    | none => throw <| IO.userError "replacement session projection unavailable"
  expect (recoveredState.corrections.contains publicCorrection)
    "replacement session replay lost durable correction"
  expect (recoveredState.reviewPlans.isEmpty)
    "replacement session introduced planning state before correction recovery"
  expect ((Application.Service.queryGate
      (.correctionsReady publicCorrection.scope) recoveredStore).value ==
        .blocked "an applicable durable user correction remains unresolved")
    "recovered correction was not visible to the public readiness gate"
  let resolvedStore ←
    match Application.Service.execute
        (.resolveUserCorrection recoveredStore.ledger.storedHead publicCorrection.key)
        recoveredStore with
    | .ok transaction => pure transaction.result
    | .error error => throw <| IO.userError s!"public correction resolution failed: {repr error}"
  let publicRule : Domain.Design.LearnedRule :=
    { key := "PUBLIC-RULE", correction := publicCorrection.key
      scope := publicCorrection.scope, statement := publicCorrection.statement }
  let promotedStore ←
    match Application.Service.execute
        (.promoteCorrection resolvedStore.ledger.storedHead publicRule) resolvedStore with
    | .ok transaction => pure transaction.result
    | .error error => throw <| IO.userError s!"public correction promotion failed: {repr error}"
  match (Application.Service.status promotedStore).value.currentState? with
  | some promoted =>
      expect (promoted.corrections.any fun item =>
        item.key == publicCorrection.key && item.resolved)
        "promoted correction lost its resolved provenance"
      expect (promoted.learnedRules.contains publicRule)
        "replacement session lost promoted correction provenance"
  | none => throw <| IO.userError "promoted correction projection unavailable"

  IO.println "workflow laws: pass"

end AgentWorkbench.Tests.WorkflowLaws

def main : IO Unit :=
  AgentWorkbench.Tests.WorkflowLaws.run
