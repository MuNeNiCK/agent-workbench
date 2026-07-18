#[cfg(test)]
mod authority;
#[cfg(test)]
mod commands;
#[cfg(test)]
mod coverage;
#[cfg(test)]
mod db;
#[cfg(test)]
mod design;
#[cfg(test)]
mod doctor;
#[cfg(test)]
mod identity;
#[cfg(test)]
mod kpt;
#[cfg(test)]
mod phases;
#[cfg(test)]
mod planning;
#[cfg(test)]
mod records;
#[cfg(test)]
mod repository;
#[cfg(test)]
mod review;
#[cfg(test)]
mod review_context;
#[cfg(test)]
mod rules;
mod runtime14;
#[cfg(test)]
mod task_identity;
#[cfg(test)]
mod traceability;
mod update;
#[cfg(test)]
mod work;

#[cfg(test)]
pub use authority::{
    AuthorityEventOutcome, AuthorityEventRecord, AuthorityRecord, NewAuthorityEvent,
    OwnerDecisionOutcome, add_authority_event, list_authorities, list_authority_events,
};
#[cfg(test)]
pub use commands::{
    CommandDeviationOutcome, CommandOutcome, CommandProfileRecord, CommandUsageListQuery,
    CommandUsageOutcome, CommandUsageRecord, NewCommandDeviation, NewCommandProfile,
    NewCommandPromotion, NewCommandUsage, NewCommandUsageWithRepositorySnapshot,
    add_command_deviation, add_command_usage, add_command_usage_with_repository_snapshot,
    add_fixed_command, add_preferred_command, deprecate_command_profile, list_command_profiles,
    list_command_usages, promote_command_usage,
};
#[cfg(test)]
pub use coverage::{
    CoverageItemListQuery, CoverageItemOutcome, CoverageItemRecord, NewCoverageItem,
    add_coverage_item, list_coverage_items,
};
#[cfg(test)]
pub use db::{
    ActiveWorkUnit, FindingRemediation, InitOutcome, IntegrityPredicateStatus, NextAction,
    OwnerAction, PhaseBlocker, ProjectIntegrityStatus, ProjectStatus, SourceCorrection,
    default_design_root, default_export_root, default_ledger_path, default_log_root, init_project,
    next_action, project_status,
};
#[cfg(test)]
pub use design::{
    DesignDecisionListQuery, DesignDecisionRecord, DesignExceptionAcceptanceOutcome,
    DesignPackageImport, DesignPackageImportOutcome, DesignPackageInitOutcome, DesignReadyCheck,
    DesignReadyItem, DesignReadyOutcome, DesignRequirementListQuery, DesignRequirementRecord,
    DesignVersionApproval, DesignVersionApprovalOutcome, GeneralAcceptanceOutcome,
    NewDesignExceptionAcceptance, NewDesignPackage, NewGeneralAcceptance,
    ValidationGateTemplateListQuery, ValidationGateTemplateRecord, accept_design_exception,
    add_general_acceptance, approve_design_version, design_ready, import_design_package,
    init_design_package, list_design_decisions, list_design_requirements,
    list_validation_gate_templates,
};
#[cfg(test)]
pub use doctor::{
    ValidationLinkAuditChange, ValidationLinkAuditRun, ValidationLinkChange,
    ValidationLinkDiagnosis, ValidationLinkRepairOutcome, ValidationLinkRunDiagnosis,
    diagnose_validation_links, list_validation_link_audit, repair_validation_links,
    repair_validation_links_with_backup_notice,
};
#[cfg(test)]
pub use kpt::{
    KptItemCommandProfileConversion, KptItemCommandProfileConversionOutcome,
    KptItemConversionOutcome, KptItemDecisionConversion, KptItemDecisionConversionOutcome,
    KptItemDesignVersionConversion, KptItemDesignVersionConversionOutcome, KptItemOutcome,
    KptItemRecord, KptItemReviewPolicyConversion, KptItemReviewPolicyConversionOutcome,
    KptItemTaskConversion, KptReviewCloseOutcome, KptReviewOutcome, KptReviewRecord, NewKptItem,
    NewKptReview, add_kpt_item, close_kpt_review, convert_kpt_item_to_command_profile,
    convert_kpt_item_to_decision, convert_kpt_item_to_design_version,
    convert_kpt_item_to_review_policy, convert_kpt_item_to_task, list_kpt_items, list_kpt_reviews,
    start_kpt_review,
};
#[cfg(test)]
pub use phases::{
    NewPhaseDependency, NewPhaseTraceDecision, NewWorkPhase, PhaseAcceptanceOutcome,
    PhaseCloseOutcome, PhaseCloseReadyItem, PhaseCloseReadyOutcome, PhaseDependencyOutcome,
    PhaseDependencyRecord, PhaseInventory, PhaseRescope, PhaseRescopeBlocker, PhaseRescopeOutcome,
    PhaseReviewTargetOutcome, PhaseSplit, PhaseTaskOutcome, PhaseTraceDecisionOutcome,
    PhaseTraceRecord, WorkPhaseOutcome, WorkPhaseRecord, accept_phase_dependency,
    accept_phase_out_of_scope, add_phase_dependency, add_phase_review_target, assign_task_to_phase,
    close_phase, create_phase, decide_phase_trace, list_phase_dependencies, list_phase_trace,
    list_phases, phase_close_ready, phase_inventory, phase_rescope, phase_split,
    satisfy_phase_dependency, show_phase,
};
#[cfg(test)]
pub use planning::{
    DecisionOutcome, DecisionRecord, NewDecision, NewTask, TaskAcceptanceOutcome, TaskCloseOutcome,
    TaskListQuery, TaskOutcome, TaskRecord, accept_task_out_of_scope, add_decision, add_task,
    close_task, list_decisions, list_tasks,
};
#[cfg(test)]
pub use records::{
    NewWorkRecord, NewWorkRecordCommand, NewWorkRecordCommit, NewWorkRecordFile,
    NewWorkRecordGitCommit, NewWorkRecordGitFile, WorkRecordEntry, WorkRecordLinkOutcome,
    WorkRecordOutcome, add_work_record_command, add_work_record_commit, add_work_record_file,
    add_work_record_git_commit, add_work_record_git_file, create_work_record,
    export_work_record_markdown, list_work_records,
};
#[cfg(test)]
pub use repository::{
    GitCommitOutcome, GitFileChangeOutcome, NewGitCommit, NewGitFileChange, NewRepository,
    NewRepositoryDirtyEntry, NewRepositorySnapshot, NewRepositorySnapshotComparison,
    NewRepositoryStateClassification, RepositoryDirtyEntryOutcome, RepositoryOutcome,
    RepositoryRecord, RepositorySnapshotComparisonOutcome, RepositorySnapshotOutcome,
    RepositorySnapshotRecord, RepositoryStateClassificationOutcome, add_git_commit,
    add_git_file_change, add_repository, add_repository_dirty_entry, add_repository_snapshot,
    add_repository_snapshot_comparison, add_repository_state_classification, list_repositories,
    list_repository_snapshots, resolve_git_commit_id,
};
#[cfg(test)]
pub use review::{
    AdjudicationDecision, AdjudicationInput, ClosureOutcome, ClosureReady, ClosureReadyOutcome,
    ClosureSupersession, ClosureSupersessionOutcome, CorrectionBeginOutcome,
    CorrectionTransitionOutcome, FindingClassificationOutcome, FindingDisposition,
    FindingLifecycle, FindingOutOfScope, FindingOutOfScopeOutcome, FindingOutcome, FindingRecord,
    FindingVerificationOutcome, NewClosure, NewFinding, NewFindingVerification, NewReviewPlan,
    NewReviewPlanTarget, NewReviewPolicy, NewReviewRun, NewReviewScope, ReviewClaim,
    ReviewPlanOutcome, ReviewPlanRecord, ReviewPlanTargetOutcome, ReviewPlanTargetRecord,
    ReviewPlanWaiver, ReviewPlanWaiverOutcome, ReviewPolicyOutcome, ReviewPolicyRecord,
    ReviewRunOutcome, ReviewRunRecord, ReviewScopeOutcome, ReviewScopeRecord, VerificationClaim,
    accept_finding_out_of_scope, add_closure, add_finding, add_finding_verification,
    add_review_plan, add_review_plan_target, add_review_policy, add_review_run,
    add_review_run_with_finding_result, adjudicate_review, adjudicate_verification,
    apply_correction_transition, begin_correction, classify_finding, correct_terminal_review,
    decide_finding, finding_fix_context_ref, finding_lifecycle_transition, list_findings,
    list_review_plan_targets, list_review_plans, list_review_policies, list_review_runs,
    list_review_scopes, ready_closure, reopen_finding_epoch, start_review_scope, supersede_closure,
    waive_review_plan,
};
#[cfg(test)]
pub use review_context::{
    ReviewContextDocument, ReviewContextQuery, render_finding_fix_context, render_review_context,
    review_context_ref, review_context_ref_with_phase,
};
#[cfg(test)]
pub use rules::{
    NewUserCorrection, RuleQuery, RuleRecord, UserCorrectionOutcome, UserCorrectionRecord,
    add_user_correction, applicable_rules, list_user_corrections,
};
pub use runtime14::{
    Claim14, Decision14, Integrity14, Record14, Resolution14, ResumeCheck14, Status14,
    accept_dependency as accept_dependency14, accept_finding as accept_finding14,
    accept_repository_change as accept_repository_change14, activate_work as activate_work14,
    add_acceptance14 as add_acceptance_schema14, add_closure as add_closure14,
    add_command_profile as add_command_profile14, add_command_usage as add_command_usage14,
    add_correction as add_correction14, add_coverage14 as add_coverage_schema14,
    add_dependency as add_dependency14, add_finding as add_finding14,
    add_kpt_item as add_kpt_item14, add_repository as add_repository14,
    add_repository_change as add_repository_change14,
    add_repository_commit as add_repository_commit14,
    add_repository_comparison as add_repository_comparison14,
    add_repository_snapshot as add_repository_snapshot14, add_requirement as add_requirement14,
    add_review_claim as add_review_claim14, add_review_plan as add_review_plan14,
    add_review_policy as add_review_policy14, add_rule as add_rule14, add_task as add_task14,
    add_typed_evidence as add_typed_evidence14, add_verification_claim as add_verification_claim14,
    approve_design14 as approve_design_schema14, assign_task as assign_task14,
    classify_repository_change as classify_repository_change14,
    close_checklist_item14 as close_checklist_item_schema14,
    close_checklist14 as close_checklist_schema14, close_kpt as close_kpt14,
    close_work as close_work14, create_phase as create_phase14,
    create_work_record as create_work_record14, decide_review as decide_review14,
    decide_verification as decide_verification14, decision_head_for as decision_head_for14,
    decompose_design14 as decompose_design_schema14, derive_task14 as derive_task_schema14,
    design_gate14 as design_gate_schema14, dispose_stale14 as dispose_stale_schema14,
    except_correction as except_correction14,
    finalize_repository_snapshot as finalize_repository_snapshot14,
    follow_up_work as follow_up_work14, import_design14 as import_design_schema14,
    init_design_package14 as init_design_package_schema14, integrity as integrity14,
    is_runtime as is_schema14_runtime,
    link_correction_requirement as link_correction_requirement14,
    link_correction_validation as link_correction_validation14,
    link_work_record as link_work_record14, list_evidence as list_evidence14,
    list_records as list_records14, list_relations as list_relations14,
    phase_close_ready as phase_close_ready14, ready_closure as ready_closure14,
    remediate_finding as remediate_finding14, render_work_record as render_work_record14,
    resolve_correction as resolve_correction14, resume as resume_work14,
    resume_check as resume_check14, revoke_acceptance14 as revoke_acceptance_schema14,
    satisfy_dependency as satisfy_dependency14, start_kpt as start_kpt14,
    start_work as start_work14, start_work_for_design as start_work_for_design14,
    status as status14, supersede_closure as supersede_closure14, suspend as suspend_work14,
    transition_command_profile as transition_command_profile14,
    transition_kpt_item as transition_kpt_item14, transition_phase as transition_phase14,
    transition_task as transition_task14, transition_work as transition_work14,
    waive_review_plan as waive_review_plan14, work_close_ready as work_close_ready14,
};
#[cfg(test)]
pub use task_identity::{
    TaskIdentityAmbiguityOutput, TaskIdentityApplyOutput, TaskIdentityAuditOutput,
    TaskIdentityAuthorityOutput, TaskIdentityAuthorityRequest, TaskIdentityDecisionOutput,
    TaskIdentityDecisionRequest, TaskIdentityPlanOutput, apply_task_identity, audit_task_identity,
    decide_task_identity_ambiguity, list_task_identity_ambiguities, plan_task_identity,
    record_task_identity_authority,
};
#[cfg(test)]
pub use traceability::{
    ChecklistItemListQuery, ChecklistItemOutcome, ChecklistItemRecord, ChecklistOutcome,
    ChecklistRecord, DesignDecomposition, DesignDecompositionOutcome,
    ImplementationEvidenceListQuery, ImplementationEvidenceOutcome, ImplementationEvidenceRecord,
    ImplementationReadyCheck, ImplementationReadyItem, ImplementationReadyOutcome,
    NewImplementationEvidence, NewImplementationEvidenceWithGit, NewTaskDerivation,
    NewValidationRun, StaleRecord, StaleRecordDisposition, StaleRecordDispositionOutcome,
    TaskDerivationListQuery, TaskDerivationOutcome, TaskDerivationRecord,
    ValidationGateContextQuery, ValidationGateContextRecord, ValidationGateSelection,
    ValidationGateSelectionOutcome, ValidationRunListQuery, ValidationRunOutcome,
    ValidationRunRecord, accept_stale_record, add_implementation_evidence,
    add_implementation_evidence_with_git, add_validation_run, close_checklist,
    close_checklist_item, close_stale_record, decompose_design, derive_task_from_requirement,
    implementation_ready, list_checklist_items, list_checklists, list_implementation_evidence,
    list_stale_records, list_task_derivations, list_validation_gate_context, list_validation_runs,
    select_validation_gate,
};
pub use update::{
    RestoreOutcome, UpdatePlan, UpdateResetOutcome, dry_run as update_dry_run,
    init_fresh as init_schema14_project, is_schema14_root, reset as update_reset,
    restore as update_restore, schema_profile as schema14_profile,
};
#[cfg(test)]
pub use work::{
    CloseOutcome, CloseReadyItem, CloseReadyOutcome, FollowUpOutcome, InterruptOutcome,
    NewWorkFork, ResumeCheckOutcome, ResumeOutcome, ResumeReadyItem, ResumeReadyOutcome,
    SuspendOutcome, WorkActivate, WorkForkOutcome, WorkForkSource, WorkOutcome, WorkRemediate,
    WorkRemediateOutcome, WorkReopen, WorkStart, WorkStatusOutcome, abandon_work, activate_work,
    block_work, close_active_work, close_ready, create_follow_up_work, fork_work, interrupt_work,
    remediate_work, reopen_work, resume_check, resume_check_basic, resume_ready,
    resume_ready_basic, resume_work, start_work, start_work_with_options, suspend_work,
    unblock_work,
};

#[cfg(test)]
mod tests;
