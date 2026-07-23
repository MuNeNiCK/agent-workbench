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
  { id := ⟨1⟩, status := .open }

def workTwo : Domain.Work.WorkUnit :=
  { id := ⟨2⟩, status := .open }

def activationOne : Domain.Work.Activation :=
  { id := ⟨1⟩, work := workOne.id, status := .active, readyToResume := false }

def suspension : Domain.Work.SuspensionContext :=
  { reason := "independent workflow review"
    returnPoint := "activate reviewed implementation plan"
    assumptions := ["design-v1", "repository-snapshot-v1"]
    resumeConditions := ["trace complete", "corrections resolved"] }

def activationTwo : Domain.Work.Activation :=
  { id := ⟨2⟩, work := workTwo.id, status := .suspended
    readyToResume := false, suspension := some suspension }

def designVersion : Domain.Design.DesignVersion :=
  { id := ⟨1⟩
    revision := ⟨1⟩
    owner := "owner"
    contentDigest := "sha256:design-v1"
    requirements := [
      { key := "REQ-003", active := true },
      { key := "REQ-004", active := true },
      { key := "REQ-005", active := true },
      { key := "REQ-006", active := true },
      { key := "REQ-009", active := true }]
    approved := false }

def scope (stage digest : String) (work : WorkId) : Domain.Review.FrozenScope :=
  { design := some designVersion.id
    work
    repositorySnapshot := "commit:workflow-v1"
    artifactDigest := digest
    stage }

def reviewPlan (id : Nat) (stage digest : String) (work : WorkId) :
    Domain.Review.Plan :=
  { id := ⟨id⟩
    owner := "owner"
    reviewer := s!"reviewer-{id}"
    adjudicator := "owner"
    scope := scope stage digest work
    userAuthorizedException := none }

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

  let designPlan := reviewPlan 1 "design" designVersion.contentDigest workOne.id
  let state ← execute (.recordReviewPlan state.revision designPlan) state
    "design review plan failed"
  let designClaim := claimFor 1 designPlan .clean
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
  expect (state.designs.any (fun version =>
    version.id == designVersion.id && version.approved))
    "approved design version was not durable"
  expect (Kernel.Gates.designReadyState designVersion.id state)
    "approved design did not pass design readiness"

  let state ← execute (.registerWork state.revision workTwo) state
    "implementation work registration failed"
  reject (.registerSuspendedActivation state.revision activationTwo) state
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
      items := [{
        key := "governed-workflow"
        requirements := ["REQ-003", "REQ-004", "REQ-005", "REQ-006", "REQ-009"]
        implementationWork := ["kernel workflow transitions"]
        completionChecks := ["workflow laws"]
        validationGates := ["GATE-003", "GATE-004", "GATE-005", "GATE-006", "GATE-009"] }]
      reviewer := decompositionPlan.reviewer
      adjudicator := decompositionPlan.adjudicator
      accepted := true }
  let state ← execute (.recordDecomposition state.revision decomposition) state
    "reviewed decomposition failed"
  expect (Policy.Traceability.ready
    { designVersion with approved := true } decomposition)
    "complete reviewed trace did not become ready"
  expect (Kernel.Gates.traceReadyState designVersion.id workTwo.id state)
    "reviewed decomposition did not pass trace readiness"

  let correction : Domain.Design.Correction :=
    { key := "COR-1", scope := "workflow"
      statement := "resume only after current readiness is re-established"
      resolved := false }
  let state ← execute (.recordUserCorrection state.revision correction) state
    "durable correction failed"
  expect (!Kernel.Gates.correctionsReadyState correction.scope state)
    "unresolved correction passed correction readiness"
  reject (.registerSuspendedActivation state.revision activationTwo) state
    "unresolved correction did not invalidate activation readiness"
  let state ← execute (.resolveUserCorrection state.revision correction.key) state
    "correction resolution failed"
  expect (Kernel.Gates.correctionsReadyState correction.scope state)
    "resolved correction still blocked correction readiness"
  let rule : Domain.Design.LearnedRule :=
    { key := "RULE-1", correction := correction.key, scope := correction.scope
      statement := correction.statement }
  let state ← execute (.promoteCorrection state.revision rule) state
    "learning promotion failed"
  expect (state.learnedRules.contains rule) "promoted learning was not durable"
  let state ← execute (.registerSuspendedActivation state.revision activationTwo) state
    "traceable activation failed"

  let state ← execute
    (.suspendWork state.revision workOne.id activationOne.id suspension)
    state "active work suspension failed"
  let state ← execute
    (.confirmResumeReadiness state.revision workTwo.id activationTwo.id)
    state "resume readiness confirmation failed"
  expect (Kernel.Gates.resumeReadyState workTwo.id activationTwo.id state)
    "confirmed activation did not pass resume readiness"
  let state ← execute
    (.resumeWork state.revision workTwo.id activationTwo.id)
    state "reviewed implementation activation failed"
  expect ((Domain.Work.activeFor state.activations workTwo.id).isSome)
    "resumed work is not the unique active frame"

  let findingPlan := reviewPlan 3 "implementation" "sha256:implementation-v1" workTwo.id
  let state ← execute (.recordReviewPlan state.revision findingPlan) state
    "finding review plan failed"
  let findingClaim := claimFor 3 findingPlan .findings
  let state ← execute (.recordReviewClaim state.revision findingClaim) state
    "finding review claim failed"
  let finding : Domain.Review.Finding :=
    { key := "FINDING-1"
      review := findingClaim.id
      blocking := true
      invariant := "resume evidence remains exact"
      remediationSurfaces := ["AgentWorkbench/Kernel/Decide.lean"]
      accepted := false
      adjudicated := false
      closed := false }
  let state ← execute (.recordReviewFinding state.revision finding) state
    "finding recording failed"
  reject (.closeReviewFinding state.revision finding.key) state
    "unadjudicated finding closure"
  let state ← execute
    (.adjudicateReviewFinding state.revision finding.key "owner" true)
    state "finding adjudication failed"
  let state ← execute (.closeReviewFinding state.revision finding.key) state
    "finding remediation closure failed"
  reject
    (.verifyReviewFinding state.revision {
      finding := finding.key, verifier := findingClaim.reviewer
      scope := findingPlan.scope, accepted := true })
    state "reviewer self-verification"
  let verification : Domain.Review.Verification :=
    { finding := finding.key, verifier := "fresh-verifier"
      scope := findingPlan.scope, accepted := true }
  let state ← execute (.verifyReviewFinding state.revision verification) state
    "independent finding verification failed"
  expect (Policy.Authority.blockingFindingsClosed findingClaim.id
    state.reviewFindings state.findingVerifications)
    "verified accepted blocking finding remained open"
  expect (!Kernel.Gates.reviewReadyState findingPlan.id state)
    "a findings review incorrectly replaced the required fresh clean review"

  let freshPlan := reviewPlan 4 "implementation" "sha256:implementation-v2" workTwo.id
  let state ← execute (.recordReviewPlan state.revision freshPlan) state
    "fresh review plan failed"
  let freshClaim := claimFor 4 freshPlan .clean
  let state ← execute (.recordReviewClaim state.revision freshClaim) state
    "fresh clean review claim failed"
  let state ← execute
    (.recordReviewAdjudication state.revision (adjudicationFor freshClaim))
    state "fresh clean review adjudication failed"
  expect (Kernel.Gates.reviewReadyState freshPlan.id state)
    "fresh clean adjudicated review did not pass readiness"

  let obligation : Domain.Evidence.Obligation :=
    { work := workTwo.id
      key := "GATE-006"
      revision := state.revision
      commandProfile := "workflow-laws"
      invocation := ".lake/build/bin/workflow-laws"
      repository := "agent-workbench"
      snapshot := "commit:workflow-v1"
      artifactDigest := "sha256:workflow-laws"
      current := true
      requirements := ["REQ-006"] }
  let state ← execute (.recordObligation state.revision obligation) state
    "traceable obligation failed"
  let badEvidence : Domain.Evidence.Evidence :=
    { id := ⟨1⟩
      work := workTwo.id
      obligation := obligation.key
      revision := state.revision
      commandProfile := obligation.commandProfile
      invocation := obligation.invocation
      exitCode := 0
      repository := obligation.repository
      snapshot := obligation.snapshot
      artifactDigest := obligation.artifactDigest
      current := true
      requirements := obligation.requirements }
  reject (.recordEvidence state.revision badEvidence) state
    "evidence without provenance"
  let exactEvidence :=
    { badEvidence with producer := "workflow-law-runner", observedAt := "revision-current" }
  let state ← execute (.recordEvidence state.revision exactEvidence) state
    "exact evidence recording failed"
  let currentObligation := { obligation with revision := state.revision }
  expect (Domain.Evidence.exactFor
    { exactEvidence with revision := state.revision } currentObligation)
    "exact evidence did not match its obligation"
  expect (!Domain.Evidence.exactFor
    { exactEvidence with revision := state.revision, snapshot := "commit:stale" }
    currentObligation)
    "stale-scope evidence matched its obligation"
  expect (Kernel.Gates.evidenceExactState workTwo.id obligation.key state)
    "exact traceable evidence did not pass evidence readiness"

  IO.println "workflow laws: pass"

end AgentWorkbench.Tests.WorkflowLaws

def main : IO Unit :=
  AgentWorkbench.Tests.WorkflowLaws.run
