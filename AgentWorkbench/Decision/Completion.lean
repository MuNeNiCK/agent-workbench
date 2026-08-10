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

def commandEvidenceMatchesCurrentProfile
    (projection : CurrentProjection) (evidence : CommandExecutionRecord) : Bool :=
  projection.entries.any fun candidate =>
    candidate.id == evidence.profileEntryId &&
    match candidate.payload with
    | .commandProfile profile =>
        profile.command.executable == evidence.command.executable &&
        profile.command.arguments == evidence.command.arguments &&
        profile.command.environment == evidence.command.environment &&
        profile.taskEntryId == evidence.taskEntryId &&
        profile.outputScope == evidence.outputScope &&
        profile.target == evidence.target &&
        evidence.inputSnapshots.any (fun snapshots =>
          snapshots.map (·.target) == profile.inputTargets.getD []) &&
        evidence.environmentSnapshots.any (fun snapshots =>
          snapshots.map (·.target) == profile.command.environment.toList.map ("env:" ++ ·)) &&
        evidence.criterionId.all (profile.criterionIds.getD []).contains &&
        evidence.taskVerificationId.all (profile.taskVerificationIds.getD []).contains
    | _ => false

def evidenceEntryCurrent
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) : Bool :=
  entry.workId == some projection.work.id &&
  entry.designRevision == some projection.design.id &&
  match entry.payload with
  | .artifactObservation evidence =>
      evidence.successful &&
      (match evidence.criterionId with
        | some id => projection.design.assuranceBindingCurrentForCriterion
            evidence.producerAgentRun id evidence.assuranceBinding
        | none => projection.design.assuranceBindingCurrentForTask
            evidence.producerAgentRun evidence.assuranceBinding) &&
      currentSnapshot? observations evidence.target == some evidence.snapshot
  | .commandExecution evidence =>
      match evidence.target, evidence.snapshot with
      | some target, some snapshot =>
          evidence.successful &&
          (match evidence.criterionId with
            | some id => projection.design.assuranceBindingCurrentForCriterion
                evidence.producerAgentRun id evidence.assuranceBinding
            | none => projection.design.assuranceBindingCurrentForTask
                evidence.producerAgentRun evidence.assuranceBinding) &&
          currentSnapshot? observations target == some snapshot &&
          evidence.environmentSnapshots.any (fun environment => environment.all fun input =>
            currentSnapshot? observations input.target == some input.snapshot) &&
          evidence.inputSnapshots.any (fun inputs => inputs.all fun input =>
            currentSnapshot? observations input.target == some input.snapshot) &&
          commandEvidenceMatchesCurrentProfile projection evidence
      | _, _ => false
  | _ => false

private def evidenceTaskBinding?
    (entry : LedgerEntry) : Option (String × String) :=
  match entry.payload with
  | .artifactObservation evidence => do
      let taskEntryId ← evidence.taskEntryId
      let outputScope ← evidence.outputScope
      pure (taskEntryId, outputScope)
  | .commandExecution evidence => do
      let taskEntryId ← evidence.taskEntryId
      let outputScope ← evidence.outputScope
      pure (taskEntryId, outputScope)
  | _ => none

/-- Evidence applies to a current Plan Task either while it is directly bound to that open Task,
or after closure/replacement only when the current Task explicitly retains the exact evidence and
its original source Task. The latter preserves intentional closed-Task inheritance without making
all evidence from a superseded Task current. -/
def evidenceBoundToCurrentTask
    (projection : CurrentProjection) (entry : LedgerEntry)
    (accepts : TaskRecord → String → Bool) : Bool :=
  match evidenceTaskBinding? entry with
  | none => false
  | some (sourceTaskId, outputScope) =>
      projection.entries.any fun taskEntry => match taskEntry.payload with
      | .task task =>
          task.planId.isSome && task.required && !task.retired &&
          task.outputScopes.contains outputScope && accepts task outputScope &&
          if task.closed then
            task.verificationTaskEntryId == some sourceTaskId &&
              task.verificationEvidenceEntryIds.contains entry.id
          else taskEntry.id == sourceTaskId
      | _ => false

def evidenceBoundToCurrentTaskCriterion
    (projection : CurrentProjection) (entry : LedgerEntry)
    (criterion : AcceptanceCriterion) : Bool :=
  evidenceBoundToCurrentTask projection entry fun task outputScope =>
    task.verificationCriterionIds.contains criterion.id && outputScope == criterion.target

def evidenceBoundToCurrentTaskVerification
    (projection : CurrentProjection) (entry : LedgerEntry) : Bool :=
  match entry.payload with
  | .artifactObservation evidence =>
      evidence.taskVerificationId.any fun verificationId =>
        evidenceBoundToCurrentTask projection entry fun task outputScope =>
          task.taskVerificationContracts.any fun contract =>
            contract.id == verificationId && contract.kind == .artifact &&
              contract.target == outputScope
  | .commandExecution evidence =>
      evidence.taskVerificationId.any fun verificationId =>
        evidenceBoundToCurrentTask projection entry fun task outputScope =>
          task.taskVerificationContracts.any fun contract =>
            contract.id == verificationId && contract.kind == .command &&
              contract.target == outputScope
  | _ => false

def taskClosedWithCurrentEvidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (task : TaskRecord) : Bool :=
  let evidenceFor (predicate : LedgerEntry → Bool) :=
    task.verificationEvidenceEntryIds.any fun evidenceId =>
      projection.entries.any fun evidenceEntry =>
        evidenceEntry.id == evidenceId &&
        evidenceEntryCurrent projection observations evidenceEntry && predicate evidenceEntry
  task.closed && task.verificationTaskEntryId.isSome &&
    !task.verificationEvidenceEntryIds.isEmpty &&
    task.verificationEvidenceEntryIds.length ==
      task.verificationCriterionIds.length + task.taskVerificationContracts.length &&
    task.verificationCriterionIds.all (fun criterionId => evidenceFor fun entry =>
      match entry.payload with
      | .artifactObservation evidence =>
          evidence.taskEntryId == task.verificationTaskEntryId &&
          evidence.criterionId == some criterionId && evidence.taskVerificationId.isNone
      | .commandExecution evidence =>
          evidence.taskEntryId == task.verificationTaskEntryId &&
          evidence.criterionId == some criterionId && evidence.taskVerificationId.isNone
      | _ => false) &&
    task.taskVerificationContracts.all (fun contract => evidenceFor fun entry =>
      match entry.payload with
      | .artifactObservation evidence =>
          contract.kind == .artifact && evidence.taskEntryId == task.verificationTaskEntryId &&
          evidence.taskVerificationId == some contract.id && evidence.criterionId.isNone &&
          evidence.target == contract.target
      | .commandExecution evidence =>
          contract.kind == .command && evidence.taskEntryId == task.verificationTaskEntryId &&
          evidence.taskVerificationId == some contract.id && evidence.criterionId.isNone &&
          evidence.target == some contract.target
      | _ => false)

def currentPlanTaskEntries
    (state : ProjectState) (projection : CurrentProjection) : List LedgerEntry :=
  match state.currentPlanFor? projection.work.id with
  | none => []
  | some plan => projection.entries.filter fun entry => match entry.payload with
    | .task task => task.planId == some plan.id && task.required && !task.retired
    | _ => false

def staleClosedTaskLineages
    (projection : CurrentProjection) (observations : List TargetObservation)
    (tasks : List LedgerEntry) : List String :=
  tasks.filterMap fun entry => match entry.payload with
    | .task task =>
        if task.closed && !taskClosedWithCurrentEvidence projection observations task then
          task.lineageId
        else none
    | _ => none

def hasStaleClosedTasks
    (state : ProjectState) (observations : List TargetObservation) : Bool :=
  currentProjection? state |>.any fun projection =>
    let tasks := currentPlanTaskEntries state projection
    !(staleClosedTaskLineages projection observations tasks).isEmpty

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
        evidence.criterionId == some criterion.id && evidence.target == criterion.target &&
        evidence.successful && projection.design.assuranceBindingCurrentForCriterion
          evidence.producerAgentRun criterion.id evidence.assuranceBinding &&
        evidenceBoundToCurrentTaskCriterion projection entry criterion
    | .commandExecution evidence =>
        criterion.evidenceKind == "command" &&
        entry.workId == some projection.work.id &&
        entry.designRevision == some projection.design.id &&
        evidence.criterionId == some criterion.id && evidence.target == some criterion.target &&
        evidence.successful && projection.design.assuranceBindingCurrentForCriterion
          evidence.producerAgentRun criterion.id evidence.assuranceBinding &&
        evidenceBoundToCurrentTaskCriterion projection entry criterion
    | _ => false)

def claimReceiptRecorded (projection : CurrentProjection) (claim : LeanClaim) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .leanProofReceipt receipt =>
        entry.workId == some projection.work.id &&
        entry.designRevision == some projection.design.id &&
        receipt.claimId == claim.id && receipt.kernelAccepted &&
        projection.design.assuranceBindingCurrentForClaim
          projection.work.responsibleAgentRun claim.id receipt.assuranceBinding
    | _ => false)

def criterionHasEvidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (criterion : AcceptanceCriterion) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .artifactObservation evidence =>
        criterion.evidenceKind == "artifact" &&
        evidenceEntryCurrent projection observations entry &&
        evidence.criterionId == some criterion.id &&
        evidence.target == criterion.target &&
        evidenceBoundToCurrentTaskCriterion projection entry criterion &&
        !evidence.operation.isEmpty && !evidence.result.isEmpty
    | .commandExecution evidence =>
        criterion.evidenceKind == "command" &&
        evidenceEntryCurrent projection observations entry &&
        evidence.criterionId == some criterion.id &&
        evidence.target == some criterion.target &&
        evidenceBoundToCurrentTaskCriterion projection entry criterion &&
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
          canReuseReceipt claim current receipt &&
          projection.design.assuranceBindingCurrentForClaim
            projection.work.responsibleAgentRun claim.id receipt.assuranceBinding
      | _ => false)

def designClaimHasReceipt
    (state : ProjectState) (design : DesignRevision) (digests : List CurrentClaimDigest)
    (claim : LeanClaim) : Bool :=
  match uniqueBy? digests (·.claimId) claim.id with
  | none => false
  | some current => state.ledgerEntries.any fun entry =>
      entry.designRevision == some design.id && match entry.payload with
      | .leanProofReceipt receipt =>
          let producer := receipt.assuranceBinding.map (·.producerAgentRun) |>.getD ""
          canReuseReceipt claim current receipt &&
            design.assuranceBindingCurrentForClaim producer claim.id receipt.assuranceBinding
      | _ => false

def acceptedAssuranceOmissionForDesign
    (state : ProjectState) (designId : String) : Bool :=
  state.ledgerEntries.any fun findingEntry =>
    findingEntry.designRevision == some designId && match findingEntry.payload with
    | .finding _ => findingEntry.workId.any fun workId =>
        (state.findingDisposition? findingEntry.id workId).any fun dispositionEntry =>
          match dispositionEntry.payload with
          | .reviewDisposition disposition =>
              disposition.decision == .accepted && disposition.impact == .assuranceOmission
          | _ => false
    | _ => false

def designAssuranceStructurallyCurrent
    (state : ProjectState) (design : DesignRevision) : Bool :=
  design.assuranceClosed && !acceptedAssuranceOmissionForDesign state design.id

def designAssuranceCurrent
    (state : ProjectState) (design : DesignRevision) (digests : List CurrentClaimDigest) : Bool :=
  designAssuranceStructurallyCurrent state design &&
    design.leanClaims.all (designClaimHasReceipt state design digests)

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
      designAssuranceStructurallyCurrent state projection.design &&
      (state.currentPlanFor? projection.work.id).isSome &&
      requiredTasksClosed projection observations &&
      projection.design.acceptanceCriteria.all
        (criterionHasEvidence projection observations) &&
      projection.design.leanClaims.all (claimHasReceipt projection digests) &&
      noBlockingEntries state projection observations

end AgentWorkbench
