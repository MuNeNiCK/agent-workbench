import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Review

open AgentWorkbench.Domain

structure Claim where
  id : ReviewId
  claim : ReviewClaim
deriving DecidableEq, Repr

structure Adjudication where
  review : ReviewId
  decision : OwnerDecision
deriving DecidableEq, Repr

def acceptedReviews (decisions : List Adjudication) : List ReviewId :=
  decisions.filterMap fun decision =>
    if decision.decision == .accepted then some decision.review else none

end AgentWorkbench.Domain.Review
