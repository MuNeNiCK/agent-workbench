mod authority;
mod commands;
mod coverage;
mod db;
mod decomposition;
mod design;
mod doctor;
mod identity;
mod kpt;
mod phases;
mod planning;
mod records;
mod release;
mod release_operator;
mod repository;
mod review;
mod review_context;
mod rules;
mod task_identity;
mod traceability;
mod update;
mod work;

pub use authority::{
    AuthorityEventOutcome, AuthorityEventRecord, AuthorityRecord, NewAuthorityEvent,
    OwnerDecisionOutcome, add_authority_event, list_authorities, list_authority_events,
};
pub use commands::{
    CommandDeviationOutcome, CommandOutcome, CommandProfileListFilter, CommandProfileRecord,
    CommandUsageListQuery, CommandUsageOutcome, CommandUsageRecord, NewCommandDeviation,
    NewCommandProfile, NewCommandPromotion, NewCommandUsage, NewCommandUsageWithRepositorySnapshot,
    add_command_deviation, add_command_usage, add_command_usage_with_repository_snapshot,
    add_fixed_command, add_preferred_command, deprecate_command_profile, list_command_profiles,
    list_command_profiles_filtered, list_command_usages, promote_command_usage,
};
pub use coverage::{
    CoverageItemListFilter, CoverageItemListQuery, CoverageItemOutcome, CoverageItemRecord,
    NewCoverageItem, add_coverage_item, list_coverage_items, list_coverage_items_filtered,
};
pub use db::{
    ActiveWorkUnit, FindingRemediation, InitOutcome, IntegrityPredicateStatus, NextAction,
    OwnerAction, PhaseBlocker, ProjectIntegrityStatus, ProjectStatus, SourceCorrection,
    default_design_root, default_export_root, default_ledger_path, default_log_root, init_project,
    init_project_with_name, next_action, next_action_for, project_status, project_status_for,
};
pub use decomposition::{
    DecompositionApplication, DecompositionApplicationOutcome, DecompositionGapRecord,
    DecompositionImport, DecompositionItemRecord, DecompositionMappingRecord,
    DecompositionPlanCandidate, DecompositionPlanQuery, DecompositionPlanRecord,
    DecompositionPlanResolution, DecompositionPlanTransitionOutcome,
    DecompositionReconciliationApplication, DecompositionReconciliationOutcome,
    DecompositionReconciliationProjection, DecompositionRevise, DecompositionSharedBindingRecord,
    DecompositionSliceRecord, DecompositionValidate, PlanReviewOwnerResolution,
    PlanReviewOwnerState,
};
pub use decomposition::{
    apply_decomposition_plan, import_decomposition_plan, preview_decomposition_reconciliation,
    reconcile_decomposition_plan, resolve_decomposition_plan, revise_decomposition_plan,
    show_decomposition_plan, validate_decomposition_plan,
};
pub use design::{
    DesignDecisionListQuery, DesignDecisionRecord, DesignExceptionAcceptanceOutcome,
    DesignPackageImport, DesignPackageImportOutcome, DesignPackageInitOutcome, DesignReadyCheck,
    DesignReadyItem, DesignReadyOutcome, DesignRequirementListQuery, DesignRequirementRecord,
    DesignVersionApproval, DesignVersionApprovalOutcome, DesignVersionInspection,
    GeneralAcceptanceOutcome, NewDesignExceptionAcceptance, NewDesignPackage, NewGeneralAcceptance,
    ValidationGateTemplateListQuery, ValidationGateTemplateRecord, accept_design_exception,
    add_general_acceptance, approve_design_version, design_ready, import_design_package,
    init_design_package, inspect_design_version, inspect_design_version_ref, list_design_decisions,
    list_design_requirements, list_validation_gate_templates, resolve_design_version_ref,
};
pub use doctor::{
    ValidationLinkArtifactOutcome, ValidationLinkAuditChange, ValidationLinkAuditRun,
    ValidationLinkChange, ValidationLinkDiagnosis, ValidationLinkRepairOutcome,
    ValidationLinkRunDiagnosis, diagnose_validation_link, diagnose_validation_links,
    list_validation_link_audit, repair_validation_link, repair_validation_links,
    repair_validation_links_with_backup_notice, retire_validation_link,
};
pub use kpt::{
    KptConversionAlreadyCommitted, KptItemCommandProfileConversion,
    KptItemCommandProfileConversionOutcome, KptItemConversionOutcome, KptItemConversionReceipt,
    KptItemConversionRecord, KptItemConversionTarget, KptItemCorrectionConversion,
    KptItemCorrectionConversionOutcome, KptItemDecisionConversion,
    KptItemDecisionConversionOutcome, KptItemDesignVersionConversion,
    KptItemDesignVersionConversionOutcome, KptItemDismissalOutcome, KptItemDismissalReceipt,
    KptItemDismissalRequest, KptItemOutcome, KptItemRecord, KptItemReviewPolicyConversion,
    KptItemReviewPolicyConversionOutcome, KptItemRuleConversion, KptItemRuleConversionOutcome,
    KptItemSourceBinding, KptItemTaskConversion, KptReviewCloseOutcome, KptReviewOutcome,
    KptReviewRecord, NewKptItem, NewKptReview, add_kpt_item, close_kpt_review,
    convert_kpt_item_to_command_profile, convert_kpt_item_to_correction,
    convert_kpt_item_to_decision, convert_kpt_item_to_design_version,
    convert_kpt_item_to_review_policy, convert_kpt_item_to_rule, convert_kpt_item_to_task,
    dismiss_kpt_item, list_kpt_items, list_kpt_reviews, start_kpt_review,
};
pub use phases::{
    NewPhaseDependency, NewPhaseTraceDecision, NewWorkPhase, PhaseAcceptanceOutcome,
    PhaseCloseOutcome, PhaseCloseReadyItem, PhaseCloseReadyOutcome, PhaseDependencyListFilter,
    PhaseDependencyOutcome, PhaseDependencyRecord, PhaseInventory, PhaseRescope,
    PhaseRescopeBlocker, PhaseRescopeOutcome, PhaseReviewTargetOutcome, PhaseSplit,
    PhaseTaskOutcome, PhaseTraceDecisionOutcome, PhaseTraceRecord, WorkPhaseOutcome,
    WorkPhaseRecord, accept_phase_dependency, accept_phase_out_of_scope, add_phase_dependency,
    add_phase_review_target, assign_task_to_phase, close_phase, create_phase, decide_phase_trace,
    list_phase_dependencies, list_phase_dependencies_filtered, list_phase_trace, list_phases,
    phase_close_ready, phase_inventory, phase_rescope, phase_split, satisfy_phase_dependency,
    show_phase,
};
pub use planning::{
    CorrectionSupportTask, DecisionListFilter, DecisionOutcome, DecisionRecord, NewDecision,
    NewTask, TaskAcceptanceOutcome, TaskCloseOutcome, TaskCompletionRevision,
    TaskCompletionRevisionOutcome, TaskListQuery, TaskOutcome, TaskRecord,
    accept_task_out_of_scope, add_correction_support_task, add_decision, add_task, close_task,
    list_decisions, list_decisions_filtered, list_tasks, revise_task_completion,
};
pub use records::{
    NewWorkRecord, NewWorkRecordCommand, NewWorkRecordCommit, NewWorkRecordFile,
    NewWorkRecordGitCommit, NewWorkRecordGitFile, WorkRecordEntry, WorkRecordLinkOutcome,
    WorkRecordOutcome, add_work_record_command, add_work_record_commit, add_work_record_file,
    add_work_record_git_commit, add_work_record_git_file, create_work_record,
    export_work_record_markdown, list_work_records,
};
pub use release::{ReleaseSubjectRecord, ReleaseTransitionOutcome};
pub use release_operator::{
    OperatorReleaseAssemble, OperatorReleaseAuthorityMutation, OperatorReleaseMutation,
    OperatorReleaseSupersession, operator_assemble_release, operator_inspect_release,
    operator_publish_release_assets, operator_publish_release_source, operator_reconcile_release,
    operator_retry_release, operator_supersede_release, operator_verify_release_remote,
    operator_withdraw_release,
};
pub use repository::{
    GitCommitOutcome, GitFileChangeOutcome, NewGitCommit, NewGitFileChange, NewRepository,
    NewRepositoryDirtyEntry, NewRepositorySnapshot, NewRepositorySnapshotComparison,
    NewRepositoryStateClassification, RepositoryDirtyEntryOutcome, RepositoryOutcome,
    RepositoryRecord, RepositorySnapshotComparisonOutcome, RepositorySnapshotOutcome,
    RepositorySnapshotRecord, RepositoryStateClassificationOutcome, add_git_commit,
    add_git_file_change, add_repository, add_repository_dirty_entry, add_repository_snapshot,
    add_repository_snapshot_comparison, add_repository_state_classification, list_repositories,
    list_repository_snapshots, resolve_git_commit_id, resolve_repository_head_commit,
};
pub use review::{
    AddResultFindingRequest, CancelResultStageRequest, CompleteResultStageRequest,
    CreateResultStageRequest, InvocationOutcome, InvocationRequest, InvocationTerminal,
    InvocationTransitionRequest, ResultStageOutcome, ReviewProvenanceIssue,
    ReviewProvenanceOutcome, add_result_finding, cancel_result_stage, complete_result_stage,
    create_result_stage, issue_review_provenance, request_invocation, transition_invocation,
};
pub use review::{
    AdjudicationDecision, AdjudicationInput, ClosureOutcome, ClosureReady, ClosureReadyOutcome,
    ClosureSupersession, ClosureSupersessionOutcome, CorrectionBeginOutcome,
    CorrectionTransitionOutcome, DecisionContinuationApply, DecisionContinuationApplyOutcome,
    DecisionContinuationRecord, FindingClassificationOutcome, FindingDesignRecovery,
    FindingDesignRecoveryOutcome, FindingDisposition, FindingLifecycle, FindingListFilter,
    FindingOutOfScope, FindingOutOfScopeOutcome, FindingOutcome, FindingRecord,
    FindingVerificationOutcome, NewClosure, NewDecisionContinuation, NewFinding,
    NewFindingVerification, NewFindingVerificationForAttempt, NewReviewPlan, NewReviewPlanTarget,
    NewReviewPolicy, NewReviewRun, NewReviewScope, ReviewClaim, ReviewPlanOutcome,
    ReviewPlanRecord, ReviewPlanSupersession, ReviewPlanSupersessionOutcome,
    ReviewPlanTargetOutcome, ReviewPlanTargetRecord, ReviewPlanWaiver, ReviewPlanWaiverOutcome,
    ReviewPolicyOutcome, ReviewPolicyRecord, ReviewRunOutcome, ReviewRunRecord, ReviewScopeOutcome,
    ReviewScopeRecord, ReviewerMigrationBinding, ReviewerMigrationBindingOutcome,
    VerificationClaim, accept_finding_out_of_scope, add_closure, add_decision_continuation,
    add_finding, add_finding_verification, add_finding_verification_for_attempt, add_review_plan,
    add_review_plan_target, add_review_policy, add_review_run, add_review_run_with_finding_result,
    adjudicate_owner, adjudicate_review, adjudicate_verification, apply_correction_transition,
    apply_decision_continuation, begin_correction, bind_migration_reviewer, classify_finding,
    correct_terminal_review, decide_finding, finding_fix_context_ref, finding_lifecycle_transition,
    list_findings, list_findings_filtered, list_review_plan_targets, list_review_plans,
    list_review_policies, list_review_runs, list_review_scopes, ready_closure,
    recover_finding_design, reopen_finding_epoch, show_decision_continuation, start_review_scope,
    supersede_closure, supersede_review_plan, waive_review_plan,
};
pub use review_context::{
    ReviewContextDocument, ReviewContextQuery, render_finding_fix_context, render_review_context,
    review_context_ref, review_context_ref_with_phase,
};
pub use rules::{
    NewUserCorrection, RuleQuery, RuleRecord, UserCorrectionListFilter, UserCorrectionOutcome,
    UserCorrectionRecord, UserCorrectionStatusRecord, add_user_correction, applicable_rules,
    list_user_corrections, list_user_corrections_filtered,
};
pub use task_identity::{
    TaskIdentityAmbiguityOutput, TaskIdentityApplyOutput, TaskIdentityAuditOutput,
    TaskIdentityAuthorityOutput, TaskIdentityAuthorityRequest, TaskIdentityDecisionOutput,
    TaskIdentityDecisionRequest, TaskIdentityPlanOutput, apply_task_identity, audit_task_identity,
    decide_task_identity_ambiguity, list_task_identity_ambiguities, plan_task_identity,
    record_task_identity_authority,
};
pub use traceability::{
    ChecklistItemListQuery, ChecklistItemOutcome, ChecklistItemRecord, ChecklistListFilter,
    ChecklistOutcome, ChecklistRecord, DesignDecomposition, DesignDecompositionOutcome,
    ImplementationEvidenceListQuery, ImplementationEvidenceOutcome, ImplementationEvidenceRecord,
    ImplementationReadyCheck, ImplementationReadyItem, ImplementationReadyOutcome,
    NewImplementationEvidence, NewImplementationEvidenceWithGit, NewTaskDerivation,
    NewValidationRun, StaleRecord, StaleRecordDisposition, StaleRecordDispositionOutcome,
    TaskDerivationListFilter, TaskDerivationListQuery, TaskDerivationOutcome, TaskDerivationRebind,
    TaskDerivationRebindOutcome, TaskDerivationRecord, ValidationGateContextQuery,
    ValidationGateContextRecord, ValidationGateSelection, ValidationGateSelectionOutcome,
    ValidationRunListQuery, ValidationRunOutcome, ValidationRunRecord, accept_stale_record,
    add_implementation_evidence, add_implementation_evidence_with_git, add_validation_run,
    close_checklist, close_checklist_item, close_stale_record, decompose_design,
    derive_task_from_requirement, design_version_for_work, implementation_ready,
    implementation_ready_for_work, list_checklist_items, list_checklists, list_checklists_filtered,
    list_implementation_evidence, list_stale_records, list_stale_records_filtered,
    list_task_derivations, list_task_derivations_filtered, list_validation_gate_context,
    list_validation_runs, rebind_task_derivation, select_validation_gate,
};
pub use update::{
    UpdateApplyOutcome, UpdateAuthorityOutcome, UpdateDecisionAuthority, UpdateDecisionOutcome,
    UpdateInspection, UpdateRecoveryAuthorityInput, UpdateRestoreOutcome, apply_update,
    apply_update_operation, decide_update, decide_update_with_authority, inspect_update,
    record_update_recovery_authority, restore_update, restore_update_operation,
};
pub use work::{
    CloseOutcome, CloseReadyItem, CloseReadyOutcome, CloseWorkOutcome, FollowUpOutcome,
    InterruptOutcome, NewWorkFork, ResumeCheckOutcome, ResumeOutcome, ResumeReadyItem,
    ResumeReadyOutcome, SuspendOutcome, WorkActivate, WorkForkOutcome, WorkForkSource, WorkOutcome,
    WorkRemediate, WorkRemediateOutcome, WorkReopen, WorkStart, WorkStatusOutcome, abandon_work,
    activate_work, block_work, close_active_work, close_ready, close_ready_for, close_work,
    create_follow_up_work, fork_work, interrupt_work, remediate_work, reopen_work, resume_check,
    resume_check_basic, resume_check_for, resume_ready, resume_ready_basic, resume_ready_for,
    resume_work, start_work, start_work_with_options, suspend_work, unblock_work,
};

#[cfg(test)]
mod tests;
