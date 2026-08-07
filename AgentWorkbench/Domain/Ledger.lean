import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Task
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Correction
import AgentWorkbench.Domain.Kpt

namespace AgentWorkbench

structure WorkDesignAdoptionRecord where
  predecessor : String
  successor : String
  impactDisposition : String
  adoptedByRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkHandoffRecord where
  predecessorRun : String
  successorRun : String
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkWithdrawalRecord where
  correctionEntryId : String
  reason : String
  withdrawnByRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- Immutable authority created atomically with successful Work completion. -/
structure WorkCompletionRecord where
  workId : String
  designRevision : String
  planId : String
  inputRevision : Nat
  inputDigest : String
  completedByRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignRejectionRecord where
  designId : String
  reason : String
  rejectedByRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- Closed set of entries that may become immutable project history. -/
inductive EntryPayload where
  | task (value : TaskRecord)
  | workDesignAdoption (value : WorkDesignAdoptionRecord)
  | workHandoff (value : WorkHandoffRecord)
  | workWithdrawal (value : WorkWithdrawalRecord)
  | workCompletion (value : WorkCompletionRecord)
  | designRejection (value : DesignRejectionRecord)
  | commandProfile (value : CommandProfileRecord)
  | commandExecution (value : CommandExecutionRecord)
  | artifactObservation (value : ArtifactObservationRecord)
  | review (value : ReviewRecord)
  | finding (value : FindingRecord)
  | reviewDisposition (value : ReviewDispositionRecord)
  | reviewVerification (value : ReviewVerificationRecord)
  | reviewHandoff (value : ReviewHandoffRecord)
  | reviewConclusion (value : ReviewConclusionRecord)
  | userCorrection (value : UserCorrectionRecord)
  | kpt (value : KPTRecord)
  | leanProofReceipt (value : LeanProofReceiptRecord)
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure LedgerEntry where
  id : String
  order : Nat
  scope : String
  workId : Option String := none
  designRevision : Option String := none
  supersedes : List String := []
  payload : EntryPayload
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
