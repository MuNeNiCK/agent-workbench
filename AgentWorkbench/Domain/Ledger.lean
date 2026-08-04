import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Work

namespace AgentWorkbench

structure TaskRecord where
  criterionId : Option String := none
  description : String
  required : Bool
  closed : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

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

structure CommandProfileRecord where
  purpose : String
  target : Option String := none
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CommandExecutionRecord where
  profileEntryId : String
  criterionId : Option String := none
  target : Option String := none
  snapshot : Option String := none
  command : CommandSpec
  exitCode : Nat
  stdoutDigest : String
  stderrDigest : String
  successful : Bool
  producerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ArtifactObservationRecord where
  criterionId : String
  target : String
  snapshot : String
  operation : String
  result : String
  successful : Bool
  producerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive ReviewPurpose where
  | design
  | implementation
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive ReviewContext where
  | fresh
  | resume
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewRecord where
  reviewId : String
  purpose : ReviewPurpose
  context : ReviewContext
  continuesEntryId : Option String := none
  targetSourceId : String
  target : String
  targetSnapshot : String
  producerAgentRun : String
  reviewerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive FindingSubjectKind where
  | statement
  | criterion
  | assumption
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure FindingSubject where
  kind : FindingSubjectKind
  id : String
  exactQuote : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure FindingRecord where
  reviewId : String
  subject : FindingSubject
  mismatchEvidenceId : String
  summary : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewDispositionRecord where
  findingEntryId : String
  decision : DispositionDecision
  reason : String
  decidedByRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewVerificationRecord where
  reviewId : String
  findingEntryId : String
  reviewEntryId : String
  evidenceEntryId : String
  target : String
  snapshot : String
  verifiedByRun : String
  resolved : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure UserCorrectionRecord where
  content : String
  resolvedByEntryId : Option String := none
  resolutionReason : Option String := none
  incorporatedIn : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure KPTRecord where
  keep : Option String := none
  problem : Option String := none
  tryNext : Option String := none
  appliesKptEntryId : Option String := none
  appliedByEntryId : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ProofSourceDigest where
  path : String
  digest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure LeanProofReceiptRecord where
  claimId : String
  claimInput : ClaimInput
  inputDigest : String
  sourceDigests : List ProofSourceDigest
  toolchain : String
  exitCode : Nat
  outputDigest : String
  kernelAccepted : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive EntryPayload where
  | task (value : TaskRecord)
  | workDesignAdoption (value : WorkDesignAdoptionRecord)
  | workHandoff (value : WorkHandoffRecord)
  | commandProfile (value : CommandProfileRecord)
  | commandExecution (value : CommandExecutionRecord)
  | artifactObservation (value : ArtifactObservationRecord)
  | review (value : ReviewRecord)
  | finding (value : FindingRecord)
  | reviewDisposition (value : ReviewDispositionRecord)
  | reviewVerification (value : ReviewVerificationRecord)
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
