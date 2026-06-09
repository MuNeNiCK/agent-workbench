mod authority;
mod commands;
mod coverage;
mod db;
mod design;
mod kpt;
mod planning;
mod records;
mod repository;
mod review;
mod rules;
mod traceability;
mod work;

pub use authority::{
    AuthorityEventOutcome, AuthorityEventRecord, NewAuthorityEvent, add_authority_event,
    list_authority_events,
};
pub use commands::{
    CommandDeviationOutcome, CommandOutcome, CommandProfileRecord, CommandUsageListQuery,
    CommandUsageOutcome, CommandUsageRecord, NewCommandDeviation, NewCommandProfile,
    NewCommandUsage, NewCommandUsageWithRepositorySnapshot, add_command_deviation,
    add_command_usage, add_command_usage_with_repository_snapshot, add_fixed_command,
    list_command_profiles, list_command_usages,
};
pub use coverage::{
    CoverageItemListQuery, CoverageItemOutcome, CoverageItemRecord, NewCoverageItem,
    add_coverage_item, list_coverage_items,
};
pub use db::{
    ActiveWorkUnit, InitOutcome, NextAction, ProjectStatus, default_design_root,
    default_export_root, default_ledger_path, default_log_root, init_project, next_action,
    project_status,
};
pub use design::{
    DesignDecisionListQuery, DesignDecisionRecord, DesignPackageImport, DesignPackageImportOutcome,
    DesignPackageInitOutcome, DesignReadyCheck, DesignReadyItem, DesignReadyOutcome,
    DesignRequirementListQuery, DesignRequirementRecord, DesignVersionApproval,
    DesignVersionApprovalOutcome, NewDesignExceptionAcceptance, NewDesignPackage,
    ValidationGateTemplateListQuery, ValidationGateTemplateRecord, accept_design_exception,
    approve_design_version, design_ready, import_design_package, init_design_package,
    list_design_decisions, list_design_requirements, list_validation_gate_templates,
};
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
pub use planning::{
    DecisionOutcome, DecisionRecord, NewDecision, NewTask, TaskAcceptanceOutcome, TaskCloseOutcome,
    TaskListQuery, TaskOutcome, TaskRecord, accept_task_out_of_scope, add_decision, add_task,
    close_task, list_decisions, list_tasks,
};
pub use records::{
    NewWorkRecord, NewWorkRecordCommand, NewWorkRecordCommit, NewWorkRecordFile,
    NewWorkRecordGitCommit, NewWorkRecordGitFile, WorkRecordEntry, WorkRecordLinkOutcome,
    WorkRecordOutcome, add_work_record_command, add_work_record_commit, add_work_record_file,
    add_work_record_git_commit, add_work_record_git_file, create_work_record,
    export_work_record_markdown, list_work_records,
};
pub use repository::{
    GitCommitOutcome, GitFileChangeOutcome, NewGitCommit, NewGitFileChange, NewRepository,
    NewRepositoryDirtyEntry, NewRepositorySnapshot, NewRepositorySnapshotComparison,
    NewRepositoryStateClassification, RepositoryDirtyEntryOutcome, RepositoryOutcome,
    RepositoryRecord, RepositorySnapshotComparisonOutcome, RepositorySnapshotOutcome,
    RepositorySnapshotRecord, RepositoryStateClassificationOutcome, add_git_commit,
    add_git_file_change, add_repository, add_repository_dirty_entry, add_repository_snapshot,
    add_repository_snapshot_comparison, add_repository_state_classification, list_repositories,
    list_repository_snapshots,
};
pub use review::{
    ClosureOutcome, FindingClassificationOutcome, FindingOutcome, FindingRecord,
    FindingVerificationOutcome, NewClosure, NewFinding, NewFindingVerification, NewReviewPlan,
    NewReviewPolicy, NewReviewRun, NewReviewScope, ReviewPlanOutcome, ReviewPlanRecord,
    ReviewPlanTargetRecord, ReviewPolicyOutcome, ReviewPolicyRecord, ReviewRunOutcome,
    ReviewRunRecord, ReviewScopeOutcome, ReviewScopeRecord, add_closure, add_finding,
    add_finding_verification, add_review_plan, add_review_policy, add_review_run, classify_finding,
    list_findings, list_review_plan_targets, list_review_plans, list_review_policies,
    list_review_runs, list_review_scopes, start_review_scope,
};
pub use rules::{
    NewUserCorrection, RuleQuery, RuleRecord, UserCorrectionOutcome, UserCorrectionRecord,
    add_user_correction, applicable_rules, list_user_corrections,
};
pub use traceability::{
    ChecklistRecord, DesignDecomposition, DesignDecompositionOutcome,
    ImplementationEvidenceListQuery, ImplementationEvidenceOutcome, ImplementationEvidenceRecord,
    ImplementationReadyCheck, ImplementationReadyItem, ImplementationReadyOutcome,
    NewImplementationEvidence, NewImplementationEvidenceWithGit, NewTaskDerivation,
    NewValidationRun, StaleRecord, TaskDerivationListQuery, TaskDerivationOutcome,
    TaskDerivationRecord, ValidationGateSelection, ValidationGateSelectionOutcome,
    ValidationRunListQuery, ValidationRunOutcome, ValidationRunRecord, add_implementation_evidence,
    add_implementation_evidence_with_git, add_validation_run, decompose_design,
    derive_task_from_requirement, implementation_ready, list_checklists,
    list_implementation_evidence, list_stale_records, list_task_derivations, list_validation_runs,
    select_validation_gate,
};
pub use work::{
    CloseOutcome, CloseReadyItem, CloseReadyOutcome, FollowUpOutcome, InterruptOutcome,
    NewWorkFork, ResumeCheckOutcome, ResumeOutcome, ResumeReadyItem, ResumeReadyOutcome,
    SuspendOutcome, WorkForkOutcome, WorkForkSource, WorkOutcome, close_active_work, close_ready,
    create_follow_up_work, fork_work, interrupt_work, reopen_work, resume_check,
    resume_check_basic, resume_ready, resume_ready_basic, resume_work, start_work, suspend_work,
};

#[cfg(test)]
mod tests;
