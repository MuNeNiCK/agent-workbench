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

def latestAcceptedReviewClaim (plan : ReviewPlanId) (work : WorkId)
    (epoch : CompletionEpoch) (claims : List Review.Claim)
    (adjudications : List Review.Adjudication) : Option Review.Claim :=
  claims.foldl (init := none) fun latest claim =>
    if claim.plan == plan && claim.work == work && claim.epoch == epoch &&
        adjudications.any (fun decision =>
          decision.review == claim.id && decision.decision == .accepted) then
      some claim
    else
      latest

def reviewsReady (state : Lifecycle.CompletionState) (claims : List Review.Claim)
    (adjudications : List Review.Adjudication) : Bool :=
  state.plan.reviews.all fun plan =>
    match latestAcceptedReviewClaim plan state.plan.work state.epoch
        claims adjudications with
    | some claim => claim.claim == .clean
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
    (obligations : List Evidence.Obligation) : Bool :=
  let owned := Evidence.forWork obligations target
  !owned.isEmpty && owned.all (obligationSatisfied evidence)

def traceReady (target : WorkId) (designs : List Design.DesignVersion)
    (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) : Bool :=
  designs.any fun design =>
    approvals.any fun approval =>
      approval.design == design.id && decompositions.any fun decomposition =>
        decomposition.work == target &&
        Traceability.ready design approval decomposition

def implementationReviewReady (target : WorkId) (plans : List Review.Plan)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (findings : List Review.Finding)
    (verifications : List Review.Verification) : Bool :=
  plans.any fun plan =>
    plan.scope.work == target && plan.scope.stage == "implementation" &&
    Review.scopeReady plan claims adjudications findings verifications

def correctionsReady (target : WorkId)
    (decompositions : List Design.Decomposition)
    (corrections : List Design.Correction) : Bool :=
  let design := (decompositions.find? (·.work == target)).map (·.design)
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
  obligationsReady target evidence obligations &&
  (Work.activeFor activations target).isSome &&
  Work.workIsOpen work target &&
  authoritativeReady target work claims adjudications lifecycle &&
  traceReady target designs approvals decompositions &&
  implementationReviewReady target reviewPlans claims adjudications findings verifications &&
  correctionsReady target decompositions corrections

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
    obligationsReady target evidence obligations = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1.1.1.1.1.1

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
  exact accepted.1.1.1.2

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
  exact accepted.1.1.1.1.1.2

end AgentWorkbench.Policy.Completion
