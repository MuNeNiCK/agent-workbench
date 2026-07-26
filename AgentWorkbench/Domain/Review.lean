import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Review

open AgentWorkbench.Domain

inductive Purpose
  | design
  | decomposition
  | designConformance
  | implementationQuality
deriving DecidableEq, Repr, BEq

structure FrozenScope where
  design : Option DesignId
  work : WorkId
  phase : Option String := none
  repositorySnapshot : String
  artifactDigest : String
  purpose : Purpose
deriving DecidableEq, Repr

structure Plan where
  id : ReviewPlanId
  owner : String
  reviewer : String
  adjudicator : String
  caller : String
  scope : FrozenScope
deriving DecidableEq, Repr

structure AuthorityException where
  key : String
  plan : ReviewPlanId
  scope : FrozenScope
  owner : String
  reviewer : String
  adjudicator : String
  caller : String
  authorizedBy : String
  reason : String
deriving DecidableEq, Repr

inductive ObservationKind
  | risk
  | proposal
deriving DecidableEq, Repr, BEq

structure Observation where
  key : String
  kind : ObservationKind
  summary : String
  evidence : String
deriving DecidableEq, Repr

structure Claim where
  id : ReviewId
  plan : ReviewPlanId
  work : WorkId
  epoch : CompletionEpoch
  claim : ReviewClaim
  reviewer : String := ""
  scope : Option FrozenScope := none
  observations : List Observation := []
deriving DecidableEq, Repr

inductive ObservationDecision
  | accepted
  | rejected
  | rescoped
  | deferred
  | needsEvidence
deriving DecidableEq, Repr, BEq

structure AdoptionRationale where
  necessity : String
  simplerAlternativesInsufficient : String
  boundedScope : String
  complexityCost : String
deriving DecidableEq, Repr

structure ObservationDisposition where
  observation : String
  decision : ObservationDecision
  reason : String
  changesAuthority : Bool := false
  successorDesign : Option DesignId := none
  adoptionRationale : Option AdoptionRationale := none
deriving DecidableEq, Repr

structure Adjudication where
  review : ReviewId
  decision : OwnerDecision
  adjudicator : String := ""
  reason : String
  observations : List ObservationDisposition := []
deriving DecidableEq, Repr

structure ClosureAttempt where
  attempt : Nat
  evidenceDigest : String
  repositorySnapshot : String
deriving DecidableEq, Repr

inductive VerificationResult
  | verified
  | notFixed
  | needsEvidence
deriving DecidableEq, Repr, BEq

structure Finding where
  key : String
  review : ReviewId
  blocking : Bool
  authority : String
  failureAccount : String
  invariant : String
  remediationSurfaces : List String
  accepted : Bool
  adjudicated : Bool
  decisionReason : String := ""
  closureAttempts : List ClosureAttempt := []
deriving DecidableEq, Repr

structure Verification where
  finding : String
  attempt : Nat
  verifier : String
  scope : FrozenScope
  evidenceDigest : String
  result : VerificationResult
  adjudicator : String := ""
  adjudicated : Bool := false
  accepted : Bool
deriving DecidableEq, Repr

def independent (plan : Plan) : Bool :=
  !plan.owner.isEmpty && !plan.reviewer.isEmpty && !plan.adjudicator.isEmpty &&
  !plan.caller.isEmpty && plan.reviewer != plan.owner &&
  plan.reviewer != plan.adjudicator && plan.reviewer != plan.caller

def exceptionExact (plan : Plan) (exception : AuthorityException) : Bool :=
  exception.plan == plan.id && exception.scope == plan.scope &&
  exception.owner == plan.owner &&
  exception.reviewer == plan.reviewer &&
  exception.adjudicator == plan.adjudicator &&
  exception.caller == plan.caller &&
  exception.authorizedBy == "user" && !exception.reason.isEmpty

def scopeExact (plan : Plan) (claim : Claim) : Bool :=
  claim.plan == plan.id && claim.work == plan.scope.work &&
    claim.reviewer == plan.reviewer && claim.scope == some plan.scope

def planWellFormed (plan : Plan) : Bool :=
  !plan.scope.repositorySnapshot.isEmpty && !plan.scope.artifactDigest.isEmpty &&
  plan.scope.phase.all (fun phase => !phase.isEmpty) &&
  !plan.owner.isEmpty && !plan.reviewer.isEmpty && !plan.adjudicator.isEmpty &&
  !plan.caller.isEmpty

def findingWellFormed (finding : Finding) : Bool :=
  !finding.key.isEmpty && !finding.authority.isEmpty &&
  !finding.failureAccount.isEmpty && !finding.invariant.isEmpty &&
  !finding.remediationSurfaces.isEmpty &&
  finding.remediationSurfaces.all (fun surface => !surface.isEmpty) &&
  (!finding.accepted || finding.adjudicated) &&
  (!finding.adjudicated || !finding.decisionReason.isEmpty)

def observationWellFormed (observation : Observation) : Bool :=
  !observation.key.isEmpty && !observation.summary.isEmpty

def claimWellFormed (claim : Claim) : Bool :=
  claim.observations.all observationWellFormed &&
  (claim.observations.map (·.key)).Nodup

def adoptionRationaleWellFormed (rationale : AdoptionRationale) : Bool :=
  !rationale.necessity.isEmpty &&
  !rationale.simplerAlternativesInsufficient.isEmpty &&
  !rationale.boundedScope.isEmpty &&
  !rationale.complexityCost.isEmpty

def dispositionWellFormed (observation : Observation)
    (disposition : ObservationDisposition) : Bool :=
  let adoptedProposal :=
    observation.kind == .proposal &&
    disposition.decision == .accepted
  disposition.observation == observation.key && !disposition.reason.isEmpty &&
  (adoptedProposal ==
    disposition.adoptionRationale.any adoptionRationaleWellFormed) &&
  (!disposition.changesAuthority ||
    (adoptedProposal &&
      disposition.successorDesign.isSome &&
      disposition.adoptionRationale.any adoptionRationaleWellFormed)) &&
  (disposition.successorDesign.isNone || disposition.changesAuthority)

def adjudicationExact (claim : Claim) (adjudication : Adjudication) : Bool :=
  adjudication.review == claim.id && !adjudication.adjudicator.isEmpty &&
  !adjudication.reason.isEmpty &&
  (adjudication.observations.map (·.observation)).Nodup &&
  claim.observations.all (fun observation =>
    adjudication.observations.any (dispositionWellFormed observation)) &&
  adjudication.observations.all (fun disposition =>
    claim.observations.any (·.key == disposition.observation))

def attemptWellFormed (attempt : ClosureAttempt) : Bool :=
  attempt.attempt > 0 && !attempt.evidenceDigest.isEmpty &&
  !attempt.repositorySnapshot.isEmpty

def latestAttempt? (finding : Finding) : Option ClosureAttempt :=
  finding.closureAttempts.getLast?

def verificationForAttempt? (finding : String) (attempt : Nat)
    (verifications : List Verification) : Option Verification :=
  verifications.reverse.find? fun verification =>
    verification.finding == finding && verification.attempt == attempt

def mayStartAttempt (finding : Finding)
    (verifications : List Verification) : Bool :=
  match latestAttempt? finding with
  | none => true
  | some attempt =>
      (verificationForAttempt? finding.key attempt.attempt verifications).any
        fun verification =>
          verification.adjudicated && verification.accepted &&
          verification.result != .verified

def sameContext (left right : FrozenScope) : Bool :=
  left.design == right.design && left.work == right.work &&
    left.phase == right.phase && left.purpose == right.purpose

def sameArtifactScope (left right : FrozenScope) : Bool :=
  left.design == right.design && left.work == right.work &&
    left.phase == right.phase &&
  left.repositorySnapshot == right.repositorySnapshot &&
  left.artifactDigest == right.artifactDigest

def latestPlanFor? (design : Option DesignId) (work : WorkId)
    (purpose : Purpose) (plans : List Plan) : Option Plan :=
  plans.reverse.find? fun plan =>
    plan.scope.design == design && plan.scope.work == work &&
    plan.scope.phase.isNone && plan.scope.purpose == purpose

def latestPlanForContext? (scope : FrozenScope) (plans : List Plan) : Option Plan :=
  plans.reverse.find? fun plan => sameContext plan.scope scope

def isLatestPlan (plan : Plan) (plans : List Plan) : Bool :=
  (latestPlanForContext? plan.scope plans).any (·.id == plan.id)

def verificationExact (finding : Finding) (claim : Claim)
    (verification : Verification) : Bool :=
  match latestAttempt? finding with
  | none => false
  | some attempt =>
      verification.finding == finding.key &&
      verification.attempt == attempt.attempt &&
      verification.result == .verified &&
      verification.adjudicated && verification.accepted &&
      !verification.verifier.isEmpty && verification.verifier != claim.reviewer &&
      claim.scope.any (sameContext verification.scope) &&
      verification.scope.repositorySnapshot == attempt.repositorySnapshot &&
      verification.evidenceDigest == attempt.evidenceDigest &&
      !verification.evidenceDigest.isEmpty

def scopeFindingsClosed (scope : FrozenScope) (claims : List Claim)
    (findings : List Finding) (verifications : List Verification) : Bool :=
  findings.all fun finding =>
    match claims.find? (·.id == finding.review) with
    | none => false
    | some claim =>
        !claim.scope.any (sameContext scope) || !finding.blocking ||
          (finding.adjudicated &&
            (!finding.accepted ||
              verifications.any fun verification =>
                verificationExact finding claim verification &&
                verification.scope.repositorySnapshot ==
                  scope.repositorySnapshot &&
                verification.scope.artifactDigest == scope.artifactDigest))

def latestClaimFor (plan : Plan) (claims : List Claim) : Option Claim :=
  claims.reverse.find? (scopeExact plan)

def claimAcceptsFindings (claim : Claim) (plans : List Plan)
    (claims : List Claim) : Bool :=
  claim.claim == .findings && plans.any fun plan =>
    scopeExact plan claim &&
      (latestClaimFor plan claims).any (·.id == claim.id)

def scopeReady (plan : Plan) (claims : List Claim)
    (adjudications : List Adjudication) (findings : List Finding)
    (verifications : List Verification) : Bool :=
  match latestClaimFor plan claims with
  | none => false
  | some claim =>
      claim.claim == .clean &&
      adjudications.any (fun decision =>
        decision.decision == .accepted &&
          adjudicationExact claim decision) &&
      scopeFindingsClosed plan.scope claims findings verifications

def acceptedReviews (decisions : List Adjudication) : List ReviewId :=
  decisions.filterMap fun decision =>
    if decision.decision == .accepted then some decision.review else none

def UniqueClaimIds (claims : List Claim) : Prop :=
  (claims.map (·.id)).Nodup

def UniqueAdjudications (adjudications : List Adjudication) : Prop :=
  (adjudications.map (·.review)).Nodup

def AdjudicationsReferenceClaims (claims : List Claim)
    (adjudications : List Adjudication) : Prop :=
  (adjudications.all fun adjudication =>
    claims.any (adjudicationExact · adjudication)) = true

def ValidReviewState (claims : List Claim) (adjudications : List Adjudication) : Prop :=
  UniqueClaimIds claims ∧
  UniqueAdjudications adjudications ∧
  AdjudicationsReferenceClaims claims adjudications

end AgentWorkbench.Domain.Review
