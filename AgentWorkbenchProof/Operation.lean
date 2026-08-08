import AgentWorkbench.Application.Mutation
import AgentWorkbench.Application.Query
import AgentWorkbenchProof.State

namespace AgentWorkbenchProof

open AgentWorkbench

/-- External capabilities are classified exhaustively from the production mutation type. -/
inductive ExternalEffectClass where
  | none
  | workspaceInitialization
  | designSourceCaptureAndProof
  | planSourceCapture
  | currentInputObservation
  | artifactObservation
  | commandExecution
  | proofExecution
  | reviewTargetCapture
  deriving Repr, DecidableEq

def externalEffectClass : Mutation → ExternalEffectClass
  | .init => .workspaceInitialization
  | .designPropose _ | .designAmend _ => .designSourceCaptureAndProof
  | .planPropose _ | .planReplace _ => .planSourceCapture
  | .planMaterialize _ | .taskClose _ | .taskReopenStale |
      .workComplete => .currentInputObservation
  | .artifactObserve _ => .artifactObservation
  | .commandRun _ => .commandExecution
  | .proofRun _ => .proofExecution
  | .reviewStart _ | .reviewResume _ => .reviewTargetCapture
  | .designAccept _ | .designReject _
  | .workStart _ | .workFocus _ | .workSuspend _ _ | .workResume _
  | .workHandoff _ _ _ _ | .workAdoptDesign _ | .workWithdraw _
  | .profileDefine _ | .profileReplace _
  | .correctionRecord _ | .correctionSupersede _ | .correctionResolve _
  | .correctionIncorporate _ | .kptRecord _ | .kptApply _
  | .reviewHandoff _ | .reviewFinding _ | .reviewDisposition _
  | .reviewConclude _ | .reviewVerify _ => .none

theorem external_class_agrees_with_transition_boundary (mutation : Mutation) :
    mutation.operation == .init ||
      mutation.pureTransition?.isNone = (externalEffectClass mutation != .none) := by
  cases mutation <;> rfl

theorem every_mutation_operation_is_listed (mutation : Mutation) :
    Operation.all.contains mutation.operation = true := by
  cases mutation <;> simp [Mutation.operation, Operation.all]

theorem every_mutation_is_classified_as_mutating (mutation : Mutation) :
    mutation.operation.kind = .mutation := by
  cases mutation <;> rfl

theorem every_query_is_classified_as_read_only (query : Query) :
    query.operation.kind = .query := by
  cases query <;> rfl

theorem successful_pure_mutation_is_valid
    (mutation : Mutation) (prior next : ProjectState)
    (success : mutation.executePure prior = .ok next) :
    AgentWorkbench.ValidProjectState next := by
  cases transition : mutation.pureTransition? with
  | none => simp [Mutation.executePure, transition] at success
  | some value =>
      cases applied : value prior with
      | error message => simp [Mutation.executePure, transition, applied] at success
      | ok candidate =>
          cases post : semanticTransitionPostcondition mutation.operation prior candidate with
          | false => simp [Mutation.executePure, transition, applied, post] at success
          | true =>
              have validatedResult : validated candidate = .ok next := by
                simpa [Mutation.executePure, transition, applied, post] using success
              exact validated_preserves candidate next validatedResult

theorem successful_pure_mutation_preserves_authority
    (mutation : Mutation) (prior next : ProjectState)
    (success : mutation.executePure prior = .ok next) :
    pureTransitionPostcondition prior next = true := by
  unfold Mutation.executePure at success
  cases transition : mutation.pureTransition? with
  | none => simp [transition] at success
  | some value =>
      cases applied : value prior with
      | error message => simp [transition, applied] at success
      | ok candidate =>
          cases post : semanticTransitionPostcondition mutation.operation prior candidate with
          | false => simp [transition, applied, post] at success
          | true =>
              have equal : candidate = next :=
                (validated_success candidate next (by
                  simpa [transition, applied, post] using success)).1.symm
              have semanticNext : semanticTransitionPostcondition mutation.operation prior next = true := by
                simpa [equal] using post
              simp [semanticTransitionPostcondition] at semanticNext
              exact semanticNext.1

theorem every_prepared_mutation_is_classified_as_mutating (prepared : PreparedMutation) :
    prepared.operation.kind = .mutation := by
  cases prepared with
  | direct mutation => exact every_mutation_is_classified_as_mutating mutation
  | designPropose | designAmend | planPropose | planReplace
  | planMaterialize | taskClose | taskReopenStale | workComplete | artifactObservation | commandExecution
  | proofReceipt | reviewStart | reviewResume => rfl

theorem successful_prepared_mutation_is_valid
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.execute prior = .ok next) :
    AgentWorkbench.ValidProjectState next := by
  cases applied : prepared.transition prior with
  | error message => simp [PreparedMutation.execute, applied] at success
  | ok candidate =>
      cases post : semanticTransitionPostcondition prepared.operation prior candidate with
      | false => simp [PreparedMutation.execute, applied, post] at success
      | true =>
          exact validated_preserves candidate next (by
            simpa [PreparedMutation.execute, applied, post] using success)

theorem successful_prepared_mutation_preserves_authority
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.execute prior = .ok next) :
    pureTransitionPostcondition prior next = true := by
  cases applied : prepared.transition prior with
  | error message => simp [PreparedMutation.execute, applied] at success
  | ok candidate =>
      cases post : semanticTransitionPostcondition prepared.operation prior candidate with
      | false => simp [PreparedMutation.execute, applied, post] at success
      | true =>
          have equal : candidate = next :=
            (validated_success candidate next (by
              simpa [PreparedMutation.execute, applied, post] using success)).1.symm
          have semanticNext : semanticTransitionPostcondition prepared.operation prior next = true := by
            simpa [equal] using post
          simp [semanticTransitionPostcondition] at semanticNext
          exact semanticNext.1

theorem successful_prepared_mutation_respects_effect_map
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.execute prior = .ok next) :
    transitionEffectsPermitted prepared.operation prior next = true := by
  cases applied : prepared.transition prior with
  | error message => simp [PreparedMutation.execute, applied] at success
  | ok candidate =>
      cases post : semanticTransitionPostcondition prepared.operation prior candidate with
      | false => simp [PreparedMutation.execute, applied, post] at success
      | true =>
          have equal : candidate = next :=
            (validated_success candidate next (by
              simpa [PreparedMutation.execute, applied, post] using success)).1.symm
          have semanticNext : semanticTransitionPostcondition prepared.operation prior next = true := by
            simpa [equal] using post
          simp [semanticTransitionPostcondition] at semanticNext
          exact semanticNext.2

theorem successful_prepared_mutation_preserves_every_existing_work_identity
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.execute prior = .ok next) :
    workIdentityPreserved prior next = true := by
  have post := successful_prepared_mutation_preserves_authority prepared prior next success
  simp [pureTransitionPostcondition] at post
  exact post.1.2

theorem successful_prepared_mutation_preserves_immutable_history
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.execute prior = .ok next) :
    immutableHistoryPreserved prior next = true := by
  have post := successful_prepared_mutation_preserves_authority prepared prior next success
  simp [pureTransitionPostcondition] at post
  exact post.2

theorem successful_applicable_mutation_was_advertised
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.executeApplicable prior = .ok next) :
    operationApplicable prior prepared.currentObservations prepared.currentClaimDigests
      prepared.operation = true := by
  unfold PreparedMutation.executeApplicable at success
  split at success
  · assumption
  · simp at success

theorem successful_applicable_mutation_is_valid
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.executeApplicable prior = .ok next) :
    AgentWorkbench.ValidProjectState next := by
  unfold PreparedMutation.executeApplicable at success
  split at success
  · exact successful_prepared_mutation_is_valid prepared prior next success
  · simp at success

theorem successful_applicable_mutation_preserves_design_history
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.executeApplicable prior = .ok next) :
    validateDesignHistoryInvariant next = .ok () :=
  (successful_applicable_mutation_is_valid prepared prior next success).designHistory

theorem successful_applicable_mutation_preserves_work_lifecycle
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.executeApplicable prior = .ok next) :
    validateWorkLifecycleInvariant next = .ok () :=
  (successful_applicable_mutation_is_valid prepared prior next success).workLifecycle

theorem successful_applicable_mutation_preserves_plan_and_tasks
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.executeApplicable prior = .ok next) :
    validatePlanTaskInvariant next = .ok () :=
  (successful_applicable_mutation_is_valid prepared prior next success).planTask

theorem successful_applicable_mutation_preserves_ledger_authority
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.executeApplicable prior = .ok next) :
    validateLedgerAuthorityInvariant next = .ok () :=
  (successful_applicable_mutation_is_valid prepared prior next success).ledgerAuthority

/-- Completion is advertised only after the persisted state has a current Plan and every
non-observational completion input has a possible witness. External freshness is rechecked under
the mutation lock. -/
theorem advertised_completion_has_current_request
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest)
    (advertised : operationApplicable state observations digests .workComplete = true) :
    ∃ projection plan,
      currentProjection? state = some projection ∧
      state.currentPlanFor? projection.work.id = some plan ∧
      projection.design.sourceArchiveAvailable = true ∧
      projection.design.acceptanceCriteria.all
        (criterionHasEvidence projection observations) = true ∧
      projection.design.leanClaims.all (claimHasReceipt projection digests) = true := by
  have ready : completionReady state observations digests = true := by
    have both : operationStructurallyApplicable state .workComplete = true ∧
        completionReady state observations digests = true := by
      simpa [operationApplicable] using advertised
    exact both.2
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

/-- Plan materialization is advertised only with current receipts, not merely persisted ones. -/
theorem advertised_plan_materialization_has_current_request
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest)
    (advertised : operationApplicable state observations digests .planMaterialize = true) :
    ∃ (projection : CurrentProjection) (candidate : ImplementationPlan),
      currentProjection? state = some projection ∧
      state.implementationPlans.find? (fun plan =>
        plan.workId == projection.work.id && plan.designRevision == projection.design.id &&
          plan.status == .candidate &&
          !(state.implementationPlans.any fun successor =>
            successor.predecessorPlanId == some plan.id && successor.status == .candidate)) =
        some candidate ∧
      projection.design.leanClaims.all (claimHasReceipt projection digests) = true := by
  have structural : planMaterializationStructurallyReady state = true := by
    have both : operationStructurallyApplicable state .planMaterialize = true ∧
        (currentProjection? state).any (fun projection =>
          projection.design.leanClaims.all (claimHasReceipt projection digests)) = true := by
      simpa [operationApplicable] using advertised
    simpa [operationStructurallyApplicable] using both.1
  have current : (currentProjection? state).any (fun projection =>
      projection.design.leanClaims.all (claimHasReceipt projection digests)) = true := by
    have both : operationStructurallyApplicable state .planMaterialize = true ∧
        (currentProjection? state).any (fun projection =>
          projection.design.leanClaims.all (claimHasReceipt projection digests)) = true := by
      simpa [operationApplicable] using advertised
    exact both.2
  unfold planMaterializationStructurallyReady at structural
  split at structural
  · simp at structural
  · rename_i projection projectionEq
    split at structural
    · simp at structural
    · rename_i candidate candidateEq
      refine ⟨projection, candidate, projectionEq, candidateEq, ?_⟩
      simpa [projectionEq] using current

end AgentWorkbenchProof
