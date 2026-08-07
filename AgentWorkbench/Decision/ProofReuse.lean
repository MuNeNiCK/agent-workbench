import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure CurrentClaimDigest where
  claimId : String
  claimInput : ClaimInput
  elaboratedPropositionDigest : String
  propositionDependencies : List String
  sourceDigests : List ProofSourceDigest
  inputDigest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def canReuseReceipt
    (claim : LeanClaim) (current : CurrentClaimDigest)
    (receipt : LeanProofReceiptRecord) : Bool :=
  receipt.kernelAccepted &&
  claim.input.toolchain == ProofToolchain.identifier &&
  current.claimId == claim.id &&
  receipt.claimId == claim.id &&
  current.claimInput == claim.input &&
  receipt.claimInput == claim.input &&
  current.elaboratedPropositionDigest == claim.elaboratedPropositionDigest &&
  receipt.elaboratedPropositionDigest == claim.elaboratedPropositionDigest &&
  current.propositionDependencies == claim.propositionDependencies &&
  receipt.propositionDependencies == claim.propositionDependencies &&
  receipt.assumptionDependencies == claim.input.assumptions.mergeSort (· < ·) &&
  receipt.sourceDigests == current.sourceDigests &&
  receipt.inputDigest == current.inputDigest &&
  receipt.toolchain == ProofToolchain.identifier

end AgentWorkbench
