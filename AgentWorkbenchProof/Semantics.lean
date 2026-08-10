import AgentWorkbench.Decision.Completion

namespace AgentWorkbenchProof

open AgentWorkbench

theorem structurally_current_assurance_is_closed_and_has_no_accepted_omission
    (state : ProjectState) (design : DesignRevision)
    (current : designAssuranceStructurallyCurrent state design = true) :
    design.assuranceClosed = true ∧
      acceptedAssuranceOmissionForDesign state design.id = false := by
  simpa [designAssuranceStructurallyCurrent] using current

theorem accepted_assurance_omission_blocks_structural_assurance
    (state : ProjectState) (design : DesignRevision)
    (omitted : acceptedAssuranceOmissionForDesign state design.id = true) :
    designAssuranceStructurallyCurrent state design = false := by
  simp [designAssuranceStructurallyCurrent, omitted]

theorem schema_zero_design_is_not_structurally_assured
    (state : ProjectState) (design : DesignRevision)
    (legacy : design.assuranceSchemaVersion = 0) :
    designAssuranceStructurallyCurrent state design = false := by
  simp [designAssuranceStructurallyCurrent, DesignRevision.assuranceClosed, legacy]

theorem closed_assurance_uses_the_exact_statement_contract_universe
    (design : DesignRevision) (closed : design.assuranceClosed = true) :
    design.effectiveAssuranceContracts.map (·.statementId) =
      design.statementCoverage.map (·.statementId) := by
  simp [DesignRevision.assuranceClosed] at closed
  grind

theorem closed_critical_contract_has_complete_failure_partition
    (design : DesignRevision) (contract : AssuranceContract)
    (closed : design.assuranceClosed = true)
    (member : contract ∈ design.effectiveAssuranceContracts)
    (critical : contract.implementationRequired = true) :
    contract.counterexamples.map (·.failureClass) = AssuranceFailureClass.all := by
  simp [DesignRevision.assuranceClosed, List.all_eq_true] at closed
  have contractClosed := closed.2 contract member
  simp [critical] at contractClosed
  grind

theorem current_artifact_evidence_has_exact_assurance_binding
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (record : ArtifactObservationRecord)
    (payload : entry.payload = .artifactObservation record)
    (current : evidenceEntryCurrent projection observations entry = true) :
    match record.criterionId with
    | some id => projection.design.assuranceBindingCurrentForCriterion
        record.producerAgentRun id record.assuranceBinding = true
    | none => projection.design.assuranceBindingCurrentForTask
        record.producerAgentRun record.assuranceBinding = true := by
  simp [evidenceEntryCurrent, payload] at current
  cases criterion : record.criterionId with
  | none => simpa [criterion] using current.2.1.2
  | some id => simpa [criterion] using current.2.1.2

/-- A reusable receipt is current only when every production identity component agrees. -/
theorem reusable_receipt_has_complete_current_identity
    (claim : LeanClaim) (current : CurrentClaimDigest) (receipt : LeanProofReceiptRecord)
    (reusable : canReuseReceipt claim current receipt = true) :
    receipt.kernelAccepted = true ∧
    claim.input.toolchain = ProofToolchain.identifier ∧
    current.claimId = claim.id ∧ receipt.claimId = claim.id ∧
    current.claimInput = claim.input ∧ receipt.claimInput = claim.input ∧
    current.elaboratedPropositionDigest = claim.elaboratedPropositionDigest ∧
    receipt.elaboratedPropositionDigest = claim.elaboratedPropositionDigest ∧
    current.propositionDependencies = claim.propositionDependencies ∧
    receipt.propositionDependencies = claim.propositionDependencies ∧
    receipt.assumptionDependencies = claim.input.assumptions.mergeSort (· < ·) ∧
    receipt.sourceDigests = current.sourceDigests ∧
    receipt.inputDigest = current.inputDigest ∧
    receipt.toolchain = ProofToolchain.identifier := by
  simp [canReuseReceipt] at reusable
  grind

/-- The production completion decision exposes all of its required authorities; a status or Review
alone cannot make this theorem's premise true. -/
theorem completion_ready_has_complete_current_authority
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest)
    (ready : completionReady state observations digests = true) :
    ∃ projection plan,
      currentProjection? state = some projection ∧
      state.currentPlanFor? projection.work.id = some plan ∧
      projection.design.sourceArchiveAvailable = true ∧
      designAssuranceStructurallyCurrent state projection.design = true ∧
      requiredTasksClosed projection observations = true ∧
      projection.design.acceptanceCriteria.all
        (criterionHasEvidence projection observations) = true ∧
      projection.design.leanClaims.all (claimHasReceipt projection digests) = true ∧
      noBlockingEntries state projection observations = true := by
  unfold completionReady at ready
  split at ready
  · simp at ready
  · rename_i projection projectionEq
    cases planEq : state.currentPlanFor? projection.work.id with
    | none => simp [planEq] at ready
    | some plan =>
        refine ⟨projection, plan, projectionEq, planEq, ?_⟩
        simp [planEq] at ready
        grind

/-- A Review entry is advisory: it is never current artifact or command evidence by itself. -/
theorem review_entry_is_not_current_evidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (review : ReviewRecord)
    (payload : entry.payload = .review review) :
    evidenceEntryCurrent projection observations entry = false := by
  simp [evidenceEntryCurrent, payload]

/-- A Review conclusion is likewise not current artifact or command evidence. -/
theorem review_conclusion_is_not_current_evidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (conclusion : ReviewConclusionRecord)
    (payload : entry.payload = .reviewConclusion conclusion) :
    evidenceEntryCurrent projection observations entry = false := by
  simp [evidenceEntryCurrent, payload]

/-- Applied or unapplied project learning cannot become acceptance evidence by itself. -/
theorem kpt_entry_is_not_current_evidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (record : KPTRecord)
    (payload : entry.payload = .kpt record) :
    evidenceEntryCurrent projection observations entry = false := by
  simp [evidenceEntryCurrent, payload]

/-- Legacy command records without captured inputs are readable history, never current evidence. -/
theorem command_without_input_snapshots_is_not_current_evidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (record : CommandExecutionRecord)
    (payload : entry.payload = .commandExecution record)
    (legacy : record.inputSnapshots = none) :
    evidenceEntryCurrent projection observations entry = false := by
  cases target : record.target <;> cases snapshot : record.snapshot <;>
    simp [evidenceEntryCurrent, payload, target, snapshot, legacy]

/-- A current command result carries an explicit input snapshot list. -/
theorem current_command_evidence_has_input_snapshots
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (record : CommandExecutionRecord)
    (payload : entry.payload = .commandExecution record)
    (current : evidenceEntryCurrent projection observations entry = true) :
    record.inputSnapshots.isSome = true := by
  cases inputs : record.inputSnapshots with
  | none =>
      cases target : record.target <;> cases snapshot : record.snapshot <;>
        simp [evidenceEntryCurrent, payload, target, snapshot, inputs] at current
  | some value => rfl

theorem current_command_evidence_has_environment_identity
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) (record : CommandExecutionRecord)
    (payload : entry.payload = .commandExecution record)
    (current : evidenceEntryCurrent projection observations entry = true) :
    record.environmentSnapshots.isSome = true := by
  cases environment : record.environmentSnapshots with
  | none =>
      cases target : record.target <;> cases snapshot : record.snapshot <;>
        simp [evidenceEntryCurrent, payload, target, snapshot, environment] at current
  | some value => rfl

theorem replaced_current_disposition_is_not_accepted
    (state : ProjectState) (findingId workId : String)
    (entry : LedgerEntry) (disposition : ReviewDispositionRecord)
    (current : state.findingDisposition? findingId workId = some entry)
    (payload : entry.payload = .reviewDisposition disposition)
    (replaced : disposition.decision = .replaced) :
    state.findingAccepted findingId workId = false := by
  simp [ProjectState.findingAccepted, current, payload, replaced]

/-- Current evidence cannot cross the current Work or Design binding. -/
theorem current_evidence_has_exact_work_and_design
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry)
    (current : evidenceEntryCurrent projection observations entry = true) :
    entry.workId = some projection.work.id ∧
      entry.designRevision = some projection.design.id := by
  unfold evidenceEntryCurrent at current
  grind

/-- Completion's blocker decision cannot hide an unresolved current User Correction. -/
theorem no_blockers_resolves_every_current_correction
    (state : ProjectState) (projection : CurrentProjection)
    (observations : List TargetObservation)
    (entry : LedgerEntry) (correction : UserCorrectionRecord)
    (member : entry ∈ projection.entries)
    (payload : entry.payload = .userCorrection correction)
    (clear : noBlockingEntries state projection observations = true) :
    correction.resolvedByEntryId.isSome = true ∨
      correction.incorporatedIn = some projection.design.id := by
  simp only [noBlockingEntries, List.all_eq_true] at clear
  have accepted := clear entry member
  simp [payload] at accepted
  grind

end AgentWorkbenchProof
