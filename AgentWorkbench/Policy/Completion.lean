import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority

namespace AgentWorkbench.Policy.Completion

open AgentWorkbench.Domain

def relatedWorkTerminal (work : List Work.WorkUnit)
    (requirements : List Lifecycle.RelatedWorkRequirement) : Bool :=
  requirements.all fun requirement =>
    work.any fun unit => unit.id == requirement.work &&
      (unit.status == .closed || unit.status == .abandoned)

def latestReviewClaim (plan : ReviewPlanId) (work : WorkId)
    (epoch : CompletionEpoch) (claims : List Review.Claim) : Option Review.Claim :=
  claims.foldl (init := none) fun latest claim =>
    if claim.plan == plan && claim.work == work && claim.epoch == epoch then
      some claim
    else
      latest

def reviewsReady (state : Lifecycle.CompletionState) (claims : List Review.Claim)
    (adjudications : List Review.Adjudication) : Bool :=
  state.plan.reviews.all fun plan =>
    match latestReviewClaim plan state.plan.work state.epoch claims with
    | some claim =>
        claim.claim == .clean && adjudications.any (fun decision =>
          decision.review == claim.id && decision.decision == .accepted)
    | none => false

def authoritativeReady (target : WorkId) (work : List Work.WorkUnit)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (lifecycle : List Lifecycle.CompletionState) : Bool :=
  match Lifecycle.forWork lifecycle target with
  | none => false
  | some state =>
      relatedWorkTerminal work state.plan.relatedWork &&
      Lifecycle.recordsReady state &&
      reviewsReady state claims adjudications

def obligationSatisfied (evidence : List Evidence.Evidence)
    (obligation : Evidence.Obligation) : Bool :=
  obligation.current && evidence.any (Evidence.exactFor · obligation)

def obligationsReady (target : WorkId) (evidence : List Evidence.Evidence)
    (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion)
    (decompositions : List Design.Decomposition) : Bool :=
  let owned := Evidence.forWork obligations target
  match decompositions.reverse.find? (·.work == target) with
  | none => false
  | some decomposition =>
      match designs.find? fun design =>
          design.id == decomposition.design &&
          design.revision == decomposition.designRevision with
      | none => false
      | some design =>
          let active := (design.requirements.filter (·.active)).map (·.key)
          !owned.isEmpty &&
          owned.all (fun obligation =>
            obligation.design == design.id &&
            obligation.designRevision == design.revision &&
            obligationSatisfied evidence obligation) &&
          active.all fun requirement =>
            owned.any (fun obligation =>
              obligation.requirements.contains requirement)

def traceReady (target : WorkId) (designs : List Design.DesignVersion)
    (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) : Bool :=
  match decompositions.reverse.find? (·.work == target) with
  | none => false
  | some decomposition =>
      designs.any fun design =>
        design.id == decomposition.design &&
        design.revision == decomposition.designRevision &&
        approvals.any fun approval =>
          approval.design == design.id &&
          Traceability.ready design approval decomposition

def purposeReviewReady (target : WorkId) (design : Option DesignId)
    (purpose : Review.Purpose) (plans : List Review.Plan)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (findings : List Review.Finding)
    (verifications : List Review.Verification) : Bool :=
  match Review.latestPlanFor? design target purpose plans with
  | none => false
  | some plan =>
      Review.scopeReady plan claims adjudications findings verifications

def requiredReviewPurposes : List Review.Purpose :=
  [.designConformance, .implementationQuality]

def requiredReviewsReady (target : WorkId) (work : List Work.WorkUnit)
    (plans : List Review.Plan)
    (decompositions : List Design.Decomposition)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (findings : List Review.Finding)
    (verifications : List Review.Verification) : Bool :=
  let design :=
    (decompositions.reverse.find? (·.work == target)).map (·.design)
  match Review.latestPlanFor? design target .designConformance plans with
  | none => false
  | some conformance =>
      match Review.latestPlanFor? design target .implementationQuality plans with
      | none => false
      | some quality =>
          work.any (fun unit =>
            unit.id == target && unit.status == .open &&
            conformance.owner == unit.owner && quality.owner == unit.owner) &&
          Review.sameArtifactScope conformance.scope quality.scope &&
          requiredReviewPurposes.all fun purpose =>
            purposeReviewReady target design purpose plans claims adjudications
              findings verifications

abbrev CompletionBinding := String × String

def completionBinding? (target : WorkId) (plans : List Review.Plan)
    (decompositions : List Design.Decomposition) : Option CompletionBinding :=
  let design :=
    (decompositions.reverse.find? (·.work == target)).map (·.design)
  match Review.latestPlanFor? design target .designConformance plans,
      Review.latestPlanFor? design target .implementationQuality plans with
  | some conformance, some quality =>
      if Review.sameArtifactScope conformance.scope quality.scope then
        some (conformance.scope.repositorySnapshot,
          conformance.scope.artifactDigest)
      else none
  | _, _ => none

def completionBindingReady (target : WorkId) (binding : CompletionBinding)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence)
    (obligations : List Evidence.Obligation) : Bool :=
  match Lifecycle.forWork lifecycle target with
  | none => false
  | some state =>
      (state.repositories.all fun record =>
        record.status == .classified &&
        record.snapshotDigest == binding.1) &&
      (state.validations.all fun record =>
        record.status == .passed &&
        record.artifactDigest == binding.2) &&
      let current := obligations.filter fun obligation =>
        obligation.work == target && obligation.current
      !current.isEmpty &&
      current.all fun obligation =>
        obligation.snapshot == binding.1 &&
        obligation.artifactDigest == binding.2 &&
        evidence.any fun item =>
          Evidence.exactFor item obligation &&
          item.snapshot == binding.1 &&
          item.artifactDigest == binding.2

def correctionsReady (target : WorkId)
    (decompositions : List Design.Decomposition)
    (corrections : List Design.Correction) : Bool :=
  let design := (decompositions.reverse.find? (·.work == target)).map (·.design)
  !corrections.any fun correction =>
    !correction.resolved && Design.correctionApplies correction target design

def closeable (target : WorkId) (work : List Work.WorkUnit)
    (activations : List Work.Activation) (claims : List Review.Claim)
    (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition)
    (corrections : List Design.Correction) : Bool :=
  obligationsReady target evidence obligations designs decompositions &&
  (Work.activeFor activations target).isSome &&
  Work.workIsOpen work target &&
  authoritativeReady target work claims adjudications lifecycle &&
  traceReady target designs approvals decompositions &&
  requiredReviewsReady target work reviewPlans decompositions claims
    adjudications findings verifications &&
  correctionsReady target decompositions corrections &&
  (completionBinding? target reviewPlans decompositions).any fun binding =>
    completionBindingReady target binding lifecycle evidence obligations

theorem completion_requires_current_obligations (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) (corrections : List Design.Correction)
    (accepted : closeable target work activations claims adjudications reviewPlans
      findings verifications lifecycle evidence obligations designs approvals
      decompositions corrections = true) :
    obligationsReady target evidence obligations designs decompositions = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1.1.1.1.1.1.1

theorem completion_requires_authoritative_lifecycle (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) (corrections : List Design.Correction)
    (accepted : closeable target work activations claims adjudications reviewPlans
      findings verifications lifecycle evidence obligations designs approvals
      decompositions corrections = true) :
    authoritativeReady target work claims adjudications lifecycle = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1.1.1.1.2

theorem completion_requires_active_target (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) (corrections : List Design.Correction)
    (accepted : closeable target work activations claims adjudications reviewPlans
      findings verifications lifecycle evidence obligations designs approvals
      decompositions corrections = true) :
    (Work.activeFor activations target).isSome = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1.1.1.1.1.1.2

end AgentWorkbench.Policy.Completion
