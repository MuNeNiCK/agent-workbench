use super::*;

fn record_close_prerequisites(root: &std::path::Path, work: &WorkOutcome) {
    create_work_record(
        root,
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "close evidence",
            work_performed: Some("recorded close prerequisites"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
}

fn record_clean_repository_snapshot(root: &std::path::Path, work: &WorkOutcome) {
    add_repository(
        root,
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    add_repository_snapshot(
        root,
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
}

mod closure;
mod integrity;
mod lifecycle;
mod memory;
mod resume;
