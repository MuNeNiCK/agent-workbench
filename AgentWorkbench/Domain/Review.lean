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
  repositorySnapshot : String
  artifactDigest : String
  purpose : Purpose
deriving DecidableEq, Repr

structure Plan where
  id : ReviewPlanId
  owner : String
  reviewer : String
  adjudicator : String
  scope : FrozenScope
deriving DecidableEq, Repr

structure AuthorityException where
  key : String
  plan : ReviewPlanId
  scope : FrozenScope
  owner : String
  reviewer : String
  adjudicator : String
  authorizedBy : String
  reason : String
deriving DecidableEq, Repr

structure Claim where
  id : ReviewId
  plan : ReviewPlanId
  work : WorkId
  epoch : CompletionEpoch
  claim : ReviewClaim
  reviewer : String := ""
  scope : Option FrozenScope := none
deriving DecidableEq, Repr

structure Adjudication where
  review : ReviewId
  decision : OwnerDecision
  adjudicator : String := ""
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
  invariant : String
  remediationSurfaces : List String
  accepted : Bool
  adjudicated : Bool
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
  plan.reviewer != plan.owner && plan.reviewer != plan.adjudicator

def exceptionExact (plan : Plan) (exception : AuthorityException) : Bool :=
  exception.plan == plan.id && exception.scope == plan.scope &&
  exception.owner == plan.owner &&
  exception.reviewer == plan.reviewer &&
  exception.adjudicator == plan.adjudicator &&
  exception.authorizedBy == "user" && !exception.reason.isEmpty

def scopeExact (plan : Plan) (claim : Claim) : Bool :=
  claim.plan == plan.id && claim.work == plan.scope.work &&
    claim.reviewer == plan.reviewer && claim.scope == some plan.scope

def planWellFormed (plan : Plan) : Bool :=
  !plan.scope.repositorySnapshot.isEmpty && !plan.scope.artifactDigest.isEmpty &&
  !plan.owner.isEmpty && !plan.reviewer.isEmpty && !plan.adjudicator.isEmpty

def findingWellFormed (finding : Finding) : Bool :=
  !finding.key.isEmpty && !finding.invariant.isEmpty &&
  !finding.remediationSurfaces.isEmpty &&
  finding.remediationSurfaces.all (fun surface => !surface.isEmpty)

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
  left.purpose == right.purpose

def sameArtifactScope (left right : FrozenScope) : Bool :=
  left.design == right.design && left.work == right.work &&
  left.repositorySnapshot == right.repositorySnapshot &&
  left.artifactDigest == right.artifactDigest

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
        decision.review == claim.id && decision.decision == .accepted) &&
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
    claims.any (·.id == adjudication.review)) = true

def ValidReviewState (claims : List Claim) (adjudications : List Adjudication) : Prop :=
  UniqueClaimIds claims ∧
  UniqueAdjudications adjudications ∧
  AdjudicationsReferenceClaims claims adjudications

end AgentWorkbench.Domain.Review
