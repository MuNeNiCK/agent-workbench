import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Review

open AgentWorkbench.Domain

structure Claim where
  id : ReviewId
  plan : ReviewPlanId
  work : WorkId
  epoch : CompletionEpoch
  claim : ReviewClaim
deriving DecidableEq, Repr

structure Adjudication where
  review : ReviewId
  decision : OwnerDecision
deriving DecidableEq, Repr

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
