import AgentWorkbench.Decision.ProofReuse
import AgentWorkbench.Decision.Finding

namespace AgentWorkbench

structure TargetObservation where
  target : String
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CompletionInput where
  work : Work
  design : DesignRevision
  plan : ImplementationPlan
  currentEntries : List LedgerEntry
  observations : List TargetObservation
  claimDigests : List CurrentClaimDigest
  deriving Repr, DecidableEq, Lean.ToJson

def completionInput
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Except String CompletionInput := do
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw "completion input requires a current Work and Design"
  let plan ← match state.currentPlanFor? projection.work.id with
    | some value => pure value
    | none => throw "completion input requires a current materialized Plan"
  pure {
    work := projection.work
    design := projection.design
    plan := plan
    currentEntries := projection.entries
    observations := observations
    claimDigests := digests }

def currentSnapshot? (observations : List TargetObservation) (target : String) : Option String :=
  uniqueBy? observations (·.target) target |>.map (·.snapshot)

def evidenceEntryCurrent
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) : Bool :=
  entry.workId == some projection.work.id &&
  entry.designRevision == some projection.design.id &&
  match entry.payload with
  | .artifactObservation evidence =>
      evidence.successful &&
      currentSnapshot? observations evidence.target == some evidence.snapshot
  | .commandExecution evidence =>
      match evidence.target, evidence.snapshot with
      | some target, some snapshot =>
          evidence.successful &&
          currentSnapshot? observations target == some snapshot &&
          evidence.environmentSnapshots.any (fun environment => environment.all fun input =>
            currentSnapshot? observations input.target == some input.snapshot) &&
          evidence.inputSnapshots.any (fun inputs => inputs.all fun input =>
            currentSnapshot? observations input.target == some input.snapshot) &&
          projection.entries.any (fun candidate =>
            candidate.id == evidence.profileEntryId &&
            match candidate.payload with | .commandProfile _ => true | _ => false)
      | _, _ => false
  | _ => false

def taskClosedWithCurrentEvidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (task : TaskRecord) : Bool :=
  task.closed &&
        task.verificationTaskEntryId.isSome &&
        !task.verificationEvidenceEntryIds.isEmpty &&
        task.verificationEvidenceEntryIds.length == task.verificationCriterionIds.length &&
        task.verificationEvidenceEntryIds.all fun evidenceId =>
          projection.entries.any fun evidenceEntry =>
            evidenceEntry.id == evidenceId &&
            evidenceEntryCurrent projection observations evidenceEntry &&
            match evidenceEntry.payload with
            | .artifactObservation evidence =>
                evidence.taskEntryId == task.verificationTaskEntryId
            | .commandExecution evidence =>
                evidence.taskEntryId == task.verificationTaskEntryId
            | _ => false

def requiredTasksClosed
    (projection : CurrentProjection) (observations : List TargetObservation) : Bool :=
  projection.entries.all (fun entry =>
    match entry.payload with
    | .task task => !task.required || taskClosedWithCurrentEvidence projection observations task
    | _ => true)

def criterionEvidenceRecorded
    (projection : CurrentProjection) (criterion : AcceptanceCriterion) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .artifactObservation evidence =>
        criterion.evidenceKind == "artifact" &&
        entry.workId == some projection.work.id &&
        entry.designRevision == some projection.design.id &&
        evidence.criterionId == criterion.id && evidence.target == criterion.target &&
        evidence.successful
    | .commandExecution evidence =>
        criterion.evidenceKind == "command" &&
        entry.workId == some projection.work.id &&
        entry.designRevision == some projection.design.id &&
        evidence.criterionId == some criterion.id && evidence.target == some criterion.target &&
        evidence.successful
    | _ => false)

def claimReceiptRecorded (projection : CurrentProjection) (claim : LeanClaim) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .leanProofReceipt receipt =>
        entry.workId == some projection.work.id &&
        entry.designRevision == some projection.design.id &&
        receipt.claimId == claim.id && receipt.kernelAccepted
    | _ => false)

def criterionHasEvidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (criterion : AcceptanceCriterion) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .artifactObservation evidence =>
        criterion.evidenceKind == "artifact" &&
        evidenceEntryCurrent projection observations entry &&
        evidence.criterionId == criterion.id &&
        evidence.target == criterion.target &&
        !evidence.operation.isEmpty && !evidence.result.isEmpty
    | .commandExecution evidence =>
        criterion.evidenceKind == "command" &&
        evidenceEntryCurrent projection observations entry &&
        evidence.criterionId == some criterion.id &&
        evidence.target == some criterion.target &&
        currentSnapshot? observations criterion.target == evidence.snapshot
    | _ => false)

def claimHasReceipt
    (projection : CurrentProjection) (digests : List CurrentClaimDigest)
    (claim : LeanClaim) : Bool :=
  match uniqueBy? digests (·.claimId) claim.id with
  | none => false
  | some current => projection.entries.any (fun entry =>
      match entry.payload with
      | .leanProofReceipt receipt =>
          entry.workId == some projection.work.id &&
          entry.designRevision == some projection.design.id &&
          canReuseReceipt claim current receipt
      | _ => false)

theorem criterionHasEvidence_implies_recorded
    (projection : CurrentProjection) (observations : List TargetObservation)
    (criterion : AcceptanceCriterion) :
    criterionHasEvidence projection observations criterion = true →
      criterionEvidenceRecorded projection criterion = true := by
  simp only [criterionHasEvidence, criterionEvidenceRecorded, List.any_eq_true]
  rintro ⟨entry, member, hmatch⟩
  refine ⟨entry, member, ?_⟩
  cases payload : entry.payload <;>
    simp [payload, evidenceEntryCurrent] at hmatch ⊢
  all_goals grind

theorem claimHasReceipt_implies_recorded
    (projection : CurrentProjection) (digests : List CurrentClaimDigest)
    (claim : LeanClaim) :
    claimHasReceipt projection digests claim = true →
      claimReceiptRecorded projection claim = true := by
  unfold claimHasReceipt
  split
  · simp
  · simp only [claimReceiptRecorded, List.any_eq_true]
    rintro ⟨entry, member, hmatch⟩
    refine ⟨entry, member, ?_⟩
    cases payload : entry.payload <;>
      simp [payload, canReuseReceipt] at hmatch ⊢
    all_goals grind

def acceptedFindingResolved
    (state : ProjectState) (projection : CurrentProjection) (observations : List TargetObservation)
    (findingEntry : LedgerEntry) (finding : FindingRecord) : Bool :=
  let accepted := findingDispositionIn? projection.entries findingEntry.id projection.work.id
    |>.any fun entry => match entry.payload with
    | .reviewDisposition disposition =>
        disposition.decision == .accepted
    | _ => false
  if !accepted then true else
  projection.entries.any (fun entry =>
    match entry.payload with
    | .reviewVerification verification =>
        verification.findingEntryId == findingEntry.id &&
        verification.reviewId == finding.reviewId && verification.resolved &&
        currentSnapshot? observations verification.target == some verification.snapshot &&
        projection.entries.any (fun evidenceEntry =>
          evidenceEntry.id == verification.evidenceEntryId &&
          evidenceEntryCurrent projection observations evidenceEntry &&
          findingRemediationBindingCurrent state findingEntry evidenceEntry finding
            verification.target &&
          match evidenceEntry.payload with
          | .artifactObservation evidence =>
              evidence.target == verification.target &&
              evidence.snapshot == verification.snapshot
          | .commandExecution evidence =>
              evidence.target == some verification.target &&
              evidence.snapshot == some verification.snapshot
          | _ => false)
    | _ => false)

def noBlockingEntries
    (state : ProjectState) (projection : CurrentProjection)
    (observations : List TargetObservation) : Bool :=
  projection.entries.all (fun entry =>
    match entry.payload with
    | .finding finding => acceptedFindingResolved state projection observations entry finding
    | .userCorrection correction =>
        correction.resolvedByEntryId.isSome ||
          correction.incorporatedIn == some projection.design.id
    | _ => true)

def completionReady
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection =>
      projection.design.sourceArchiveAvailable &&
      (state.currentPlanFor? projection.work.id).isSome &&
      requiredTasksClosed projection observations &&
      projection.design.acceptanceCriteria.all
        (criterionHasEvidence projection observations) &&
      projection.design.leanClaims.all (claimHasReceipt projection digests) &&
      noBlockingEntries state projection observations

end AgentWorkbench
