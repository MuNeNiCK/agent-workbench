import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Policy.Authority

open AgentWorkbench.Domain
open AgentWorkbench.Domain.Review

structure ReviewState where
  claims : List Claim
  adjudications : List Adjudication
deriving DecidableEq, Repr

def authority (state : ReviewState) : List ReviewId :=
  acceptedReviews state.adjudications

def recordClaim (state : ReviewState) (claim : Claim) : ReviewState :=
  { state with claims := state.claims ++ [claim] }

def mayInvoke (plan : Plan) (exceptions : List AuthorityException) : Bool :=
  independent plan || exceptions.any (exceptionExact plan)

def mayAdjudicate (plan : Plan) (claim : Claim) (principal : String) : Bool :=
  scopeExact plan claim && principal == plan.adjudicator &&
  principal != claim.reviewer

def blockingFindingsClosed (review : ReviewId) (claims : List Claim)
    (findings : List Finding) (verifications : List Verification) : Bool :=
  findings.all fun finding =>
    finding.review != review || !finding.blocking ||
      (finding.adjudicated &&
        (!finding.accepted ||
          match claims.find? (·.id == finding.review) with
          | none => false
          | some claim =>
              verifications.any fun verification =>
                Review.verificationExact finding claim verification))

def scopeFindingsClosed (scope : FrozenScope) (claims : List Claim)
    (findings : List Finding) (verifications : List Verification) : Bool :=
  Review.scopeFindingsClosed scope claims findings verifications

theorem review_claim_has_no_authority (state : ReviewState) (claim : Claim) :
    authority (recordClaim state claim) = authority state :=
  rfl

end AgentWorkbench.Policy.Authority
