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

theorem review_claim_has_no_authority (state : ReviewState) (claim : Claim) :
    authority (recordClaim state claim) = authority state :=
  rfl

end AgentWorkbench.Policy.Authority
