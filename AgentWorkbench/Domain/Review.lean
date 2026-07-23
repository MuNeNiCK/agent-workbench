import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Review

open AgentWorkbench.Domain

structure FrozenScope where
  design : Option DesignId
  work : WorkId
  repositorySnapshot : String
  artifactDigest : String
  stage : String
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

structure Finding where
  key : String
  review : ReviewId
  blocking : Bool
  invariant : String
  remediationSurfaces : List String
  accepted : Bool
  adjudicated : Bool
  closed : Bool
  closureEvidence : String := ""
  closureSnapshot : String := ""
deriving DecidableEq, Repr

structure Verification where
  finding : String
  verifier : String
  scope : FrozenScope
  evidenceDigest : String
  claimFixed : Bool
  adjudicator : String := ""
  adjudicated : Bool := false
  accepted : Bool
deriving DecidableEq, Repr

def independent (plan : Plan) : Bool :=
  !plan.owner.isEmpty && !plan.reviewer.isEmpty && !plan.adjudicator.isEmpty &&
  plan.reviewer != plan.owner && plan.reviewer != plan.adjudicator

def exceptionExact (plan : Plan) (exception : AuthorityException) : Bool :=
  exception.plan == plan.id && exception.owner == plan.owner &&
  exception.reviewer == plan.reviewer &&
  exception.adjudicator == plan.adjudicator &&
  exception.authorizedBy == "user" && !exception.reason.isEmpty

def scopeExact (plan : Plan) (claim : Claim) : Bool :=
  claim.plan == plan.id && claim.work == plan.scope.work &&
    claim.reviewer == plan.reviewer && claim.scope == some plan.scope

def planWellFormed (plan : Plan) : Bool :=
  !plan.scope.repositorySnapshot.isEmpty && !plan.scope.artifactDigest.isEmpty &&
  !plan.scope.stage.isEmpty && !plan.owner.isEmpty && !plan.reviewer.isEmpty &&
  !plan.adjudicator.isEmpty

def findingWellFormed (finding : Finding) : Bool :=
  !finding.key.isEmpty && !finding.invariant.isEmpty &&
  !finding.remediationSurfaces.isEmpty &&
  finding.remediationSurfaces.all (fun surface => !surface.isEmpty)

def sameContext (left right : FrozenScope) : Bool :=
  left.design == right.design && left.work == right.work &&
  left.stage == right.stage

def verificationExact (finding : Finding) (claim : Claim)
    (verification : Verification) : Bool :=
  verification.finding == finding.key && verification.claimFixed &&
  verification.adjudicated && verification.accepted &&
  !verification.verifier.isEmpty && verification.verifier != claim.reviewer &&
  claim.scope.any (sameContext verification.scope) &&
  verification.scope.repositorySnapshot == finding.closureSnapshot &&
  verification.evidenceDigest == finding.closureEvidence &&
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
              (finding.closed && verifications.any fun verification =>
                verificationExact finding claim verification)))

def scopeReady (plan : Plan) (claims : List Claim)
    (adjudications : List Adjudication) (findings : List Finding)
    (verifications : List Verification) : Bool :=
  match claims.reverse.find? (scopeExact plan) with
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
