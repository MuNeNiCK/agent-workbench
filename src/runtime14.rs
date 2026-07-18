use std::{
    fs,
    fs::File,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution14 {
    pub owner_handle: String,
    pub owner_state: String,
    pub state_revision: String,
    pub blocker: Option<String>,
    pub legal_actions: Vec<String>,
    pub selected_action: String,
}

#[derive(Debug, Clone)]
pub struct Status14 {
    pub open_work: i64,
    pub active_activations: i64,
    pub resolutions: Vec<Resolution14>,
}

#[derive(Debug, Clone)]
pub struct Record14 {
    pub handle: String,
    pub state: String,
    pub revision: i64,
}

#[derive(Debug, Clone)]
pub struct ResumeCheck14 {
    pub snapshot_handle: String,
    pub current_digest: String,
    pub recorded_digest: String,
    pub changed_components: Vec<String>,
    pub result: String,
}

#[derive(Debug, Clone)]
pub struct Integrity14 {
    pub quick_check: String,
    pub foreign_key_violations: i64,
    pub manifest_digest: String,
}

#[derive(Debug, Clone)]
pub struct Claim14 {
    pub handle: String,
    pub target_handle: String,
    pub outcome: String,
}

#[derive(Debug, Clone)]
pub struct Decision14 {
    pub handle: String,
    pub target_handle: String,
    pub resulting_state: String,
}

#[derive(Debug, Clone)]
enum MutationIntent14 {
    Continue,
    Project {
        action: &'static str,
    },
    Correction {
        handle: String,
        action: &'static str,
    },
    CorrectionValidationEvidence {
        profile: String,
    },
    CorrectionCommandProfile {
        work: String,
        profile: Option<String>,
        action: &'static str,
    },
    ReviewWaive {
        handle: String,
    },
    ReviewProgress {
        work: String,
        plan: Option<String>,
    },
    Finding {
        handle: String,
        action: &'static str,
    },
    Work {
        handle: String,
        action: &'static str,
    },
    Resume,
}

struct RuntimeConnection14 {
    conn: Connection,
    _lock: File,
}

impl Deref for RuntimeConnection14 {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl DerefMut for RuntimeConnection14 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

pub fn is_runtime(root: &Path) -> Result<bool> {
    crate::update::is_schema14_root(root)
}

pub fn status(root: &Path) -> Result<Status14> {
    let conn = open(root)?;
    let open_work = conn.query_row(
        "select count(*) from records where kind='work' and state in ('open','blocked')",
        [],
        |row| row.get(0),
    )?;
    let active_activations = conn.query_row(
        "select count(*) from records where kind='activation' and state='active'",
        [],
        |row| row.get(0),
    )?;
    Ok(Status14 {
        open_work,
        active_activations,
        resolutions: resolve_all(&conn)?,
    })
}

pub fn integrity(root: &Path) -> Result<Integrity14> {
    let conn = open(root)?;
    let quick_check: String = conn.query_row("pragma quick_check", [], |row| row.get(0))?;
    let foreign_key_violations: i64 =
        conn.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    let manifest_digest = conn.query_row(
        "select manifest_digest from schema_metadata where singleton=1",
        [],
        |row| row.get(0),
    )?;
    Ok(Integrity14 {
        quick_check,
        foreign_key_violations,
        manifest_digest,
    })
}

pub fn start_work(root: &Path, title: &str) -> Result<Record14> {
    start_work_for_design(root, title, None)
}

pub fn start_work_for_design(
    root: &Path,
    title: &str,
    design_version_id: Option<i64>,
) -> Result<Record14> {
    if title.trim().is_empty() {
        bail!("work title cannot be blank");
    }
    mutate_intent(
        root,
        MutationIntent14::Project {
            action: "work-start",
        },
        |tx| {
            let resolution = resolve_project(tx)?;
            require_selected(&resolution, "work start")?;
            let id = next_numeric_id(tx, "work")?;
            let handle = format!("work:{id}");
            let design = design_version_id.map(|id| format!("design_version:{id}"));
            if let Some(design) = &design
                && record_state(tx, design, "design_version")? != "approved"
            {
                bail!("design work requires an approved design version");
            }
            insert_record(
                tx,
                &handle,
                "work",
                "open",
                design.as_deref(),
                None,
                None,
                Some(title),
                None,
                None,
            )?;
            let activation_id = next_numeric_id(tx, "activation")?;
            insert_record(
                tx,
                &format!("activation:{activation_id}"),
                "activation",
                "active",
                Some(&handle),
                None,
                None,
                Some("active work"),
                None,
                None,
            )?;
            Ok(Record14 {
                handle,
                state: "open".to_string(),
                revision: 1,
            })
        },
    )
}

pub fn activate_work(root: &Path, work_id: i64) -> Result<Record14> {
    let handle = format!("work:{work_id}");
    mutate_intent(
        root,
        MutationIntent14::Work {
            handle: handle.clone(),
            action: "activate",
        },
        |tx| {
            let state = record_state(tx, &handle, "work")?;
            if state != "open" {
                bail!("work activation requires open work");
            }
            if active_activation(tx)?.is_some() {
                bail!("another work activation is already active");
            }
            let activation_id = next_numeric_id(tx, "activation")?;
            let activation = format!("activation:{activation_id}");
            insert_record(
                tx,
                &activation,
                "activation",
                "active",
                Some(&handle),
                None,
                None,
                Some("active work"),
                None,
                None,
            )?;
            Ok(Record14 {
                handle: activation,
                state: "active".to_string(),
                revision: 1,
            })
        },
    )
}

pub fn follow_up_work(
    root: &Path,
    source_work_id: i64,
    title: &str,
    reason: &str,
) -> Result<Record14> {
    mutate_intent(
        root,
        MutationIntent14::Project {
            action: "follow-up",
        },
        |tx| {
            if active_activation(tx)?.is_some() {
                bail!("cannot start follow-up while another work activation is active");
            }
            let source = format!("work:{source_work_id}");
            if !matches!(
                record_state(tx, &source, "work")?.as_str(),
                "closed" | "abandoned"
            ) {
                bail!("follow-up source must be closed or abandoned");
            }
            let id = next_numeric_id(tx, "work")?;
            let handle = format!("work:{id}");
            insert_record(
                tx,
                &handle,
                "work",
                "open",
                None,
                Some(&source),
                Some(&id.to_string()),
                Some(title),
                None,
                Some(reason),
            )?;
            let activation_id = next_numeric_id(tx, "activation")?;
            insert_record(
                tx,
                &format!("activation:{activation_id}"),
                "activation",
                "active",
                Some(&handle),
                None,
                None,
                Some("follow-up work"),
                None,
                None,
            )?;
            Ok(Record14 {
                handle,
                state: "open".into(),
                revision: 1,
            })
        },
    )
}

pub fn transition_work(root: &Path, work_id: i64, action: &str, reason: &str) -> Result<Record14> {
    let target = format!("work:{work_id}");
    let next = match action {
        "block" => "blocked",
        "unblock" => "open",
        "abandon" => "abandoned",
        "reopen" => "open",
        _ => bail!("unsupported work transition {action}"),
    };
    if action != "reopen" {
        return mutate_intent(
            root,
            MutationIntent14::Work {
                handle: target.clone(),
                action: match action {
                    "unblock" => "unblock",
                    "abandon" => "abandon",
                    _ => "transition",
                },
            },
            |tx| {
                let record = if action == "abandon" {
                    update_record_state_unowned(tx, &target, "work", action, next, reason)?
                } else {
                    update_record_state(tx, &target, "work", action, next, reason)?
                };
                if action == "abandon" {
                    let activation: Option<String> = tx.query_row(
                        "select handle from records where kind='activation' and owner_handle=?1 and state in ('active','suspended') order by created_at desc limit 1",
                        params![target],
                        |row| row.get(0),
                    ).optional()?;
                    if let Some(activation) = activation {
                        update_record_state_unowned(
                            tx,
                            &activation,
                            "activation",
                            "abandon",
                            "abandoned",
                            reason,
                        )?;
                    }
                }
                Ok(record)
            },
        );
    }
    mutate_intent(
        root,
        MutationIntent14::Work {
            handle: target.clone(),
            action: "reopen",
        },
        |tx| {
            if active_activation(tx)?.is_some() {
                bail!("cannot reopen while another work activation is active");
            }
            let record = update_record_state_unowned(tx, &target, "work", action, next, reason)?;
            let activation_id = next_numeric_id(tx, "activation")?;
            insert_record(
                tx,
                &format!("activation:{activation_id}"),
                "activation",
                "active",
                Some(&target),
                None,
                None,
                Some("reopened work"),
                None,
                None,
            )?;
            Ok(record)
        },
    )
}

pub fn add_task(
    root: &Path,
    work_id: i64,
    title: &str,
    priority: &str,
    details: Option<&str>,
) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "task")?;
        let handle = format!("task:{id}");
        insert_record(
            tx,
            &handle,
            "task",
            "open",
            Some(&work),
            None,
            None,
            Some(title),
            Some(priority),
            details,
        )?;
        Ok(Record14 {
            handle,
            state: "open".to_string(),
            revision: 1,
        })
    })
}

pub fn transition_task(root: &Path, task_id: i64, action: &str, reason: &str) -> Result<Record14> {
    let target = format!("task:{task_id}");
    let next = match action {
        "block" => "blocked",
        "unblock" => "open",
        "close" => "closed",
        "accept-out-of-scope" => "accepted_out_of_scope",
        _ => bail!("unsupported task transition {action}"),
    };
    transition_record(root, &target, "task", action, next, reason)
}

pub fn create_phase(
    root: &Path,
    work_id: i64,
    key: &str,
    title: &str,
    order: i64,
) -> Result<Record14> {
    if key.trim().is_empty() || order <= 0 {
        bail!("phase key must be nonblank and order must be positive");
    }
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "phase")?;
        let handle = format!("phase:{id}");
        insert_record(
            tx,
            &handle,
            "phase",
            "open",
            Some(&work),
            None,
            Some(key),
            Some(title),
            None,
            Some(&order.to_string()),
        )?;
        tx.execute(
            "update records set ordinal=?1,details=null where handle=?2",
            params![order, handle],
        )?;
        Ok(Record14 {
            handle,
            state: "open".to_string(),
            revision: 1,
        })
    })
}

pub fn assign_task(root: &Path, phase_id: i64, task_id: i64) -> Result<String> {
    mutate(root, |tx| {
        let phase = format!("phase:{phase_id}");
        let task = format!("task:{task_id}");
        let phase_owner = record_owner(tx, &phase, "phase")?;
        let task_owner = record_owner(tx, &task, "task")?;
        if phase_owner != task_owner {
            bail!("phase and task must have the same work owner");
        }
        ensure_active_owner(tx, &phase_owner)?;
        if record_state(tx, &phase, "phase")? != "open"
            || !matches!(
                record_state(tx, &task, "task")?.as_str(),
                "open" | "blocked"
            )
        {
            bail!("membership requires an open phase and nonterminal task");
        }
        let id = next_numeric_relation_id(tx, "membership")?;
        let handle = format!("membership:{id}");
        let now = now()?;
        tx.execute(
            "insert into relations(handle,project_handle,kind,source_handle,target_handle,state,revision,created_at,updated_at) values(?1,'project:current','membership',?2,?3,'recorded',1,?4,?4)",
            params![handle, phase, task, now],
        )?;
        insert_relation_event(tx, &handle, "assigned", None, "recorded", 1, &now)?;
        Ok(handle)
    })
}

pub fn add_review_policy(
    root: &Path,
    work_id: i64,
    name: &str,
    reviewer_limit: i64,
) -> Result<Record14> {
    if name.trim().is_empty() || reviewer_limit < 1 {
        bail!("review policy requires a name and positive reviewer limit");
    }
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "review_policy")?;
        let handle = format!("review_policy:{id}");
        insert_record(
            tx,
            &handle,
            "review_policy",
            "active",
            Some(&work),
            None,
            Some(name),
            Some(name),
            None,
            None,
        )?;
        tx.execute(
            "update records set policy_limit=?1,policy_action='block' where handle=?2",
            params![reviewer_limit, handle],
        )?;
        Ok(Record14 {
            handle,
            state: "active".into(),
            revision: 1,
        })
    })
}

pub fn add_review_plan(
    root: &Path,
    work_id: i64,
    stage: &str,
    policy_id: i64,
    phase_id: Option<i64>,
    required: bool,
) -> Result<Record14> {
    let work = format!("work:{work_id}");
    mutate_intent(
        root,
        MutationIntent14::ReviewProgress {
            work: work.clone(),
            plan: None,
        },
        |tx| {
            ensure_active_owner(tx, &work)?;
            let policy = format!("review_policy:{policy_id}");
            if record_owner(tx, &policy, "review_policy")? != work
                || record_state(tx, &policy, "review_policy")? != "active"
            {
                bail!("review policy is not active for this work");
            }
            let id = next_numeric_id(tx, "review_plan")?;
            let handle = format!("review_plan:{id}");
            insert_record(
                tx,
                &handle,
                "review_plan",
                "pending",
                Some(&work),
                Some(&policy),
                Some(&id.to_string()),
                Some(stage),
                None,
                None,
            )?;
            tx.execute(
                "update records set stage=?1,required=?2 where handle=?3",
                params![stage, i64::from(required), handle],
            )?;
            if let Some(phase_id) = phase_id {
                let phase = format!("phase:{phase_id}");
                if record_owner(tx, &phase, "phase")? != work {
                    bail!("review phase target is outside the work owner");
                }
                insert_relation(tx, "review_target", &handle, &phase, Some(required))?;
            }
            Ok(Record14 {
                handle,
                state: "pending".into(),
                revision: 1,
            })
        },
    )
}

pub fn add_review_claim(
    root: &Path,
    plan_id: i64,
    outcome: &str,
    producer: &str,
    scope_digest: &str,
    evidence_text: Option<&str>,
) -> Result<Claim14> {
    if producer.trim().is_empty() || scope_digest.trim().is_empty() {
        bail!("review claim requires producer and scope digest");
    }
    let plan = format!("review_plan:{plan_id}");
    mutate_intent(
        root,
        MutationIntent14::ReviewProgress {
            work: String::new(),
            plan: Some(plan.clone()),
        },
        |tx| {
            let state = record_state(tx, &plan, "review_plan")?;
            if state != "pending" {
                bail!("review claim requires a pending plan");
            }
            let id = next_claim_id(tx)?;
            let handle = format!("claim:{id}");
            let revision = record_revision(tx, &plan)?;
            tx.execute(
            "insert into claims(handle,project_handle,kind,target_handle,plan_handle,target_revision,outcome,producer,scope_digest,evidence_text,created_at) values(?1,'project:current','review',?2,?2,?3,?4,?5,?6,?7,?8)",
            params![handle, plan, revision, outcome, producer, scope_digest, evidence_text, now()?],
        )?;
            Ok(Claim14 {
                handle,
                target_handle: plan,
                outcome: outcome.into(),
            })
        },
    )
}

pub fn decide_review(
    root: &Path,
    plan_id: i64,
    claim_id: i64,
    adjudication: &str,
    expected_current: &str,
    reason: &str,
) -> Result<Decision14> {
    let plan = format!("review_plan:{plan_id}");
    mutate_intent(
        root,
        MutationIntent14::ReviewProgress {
            work: String::new(),
            plan: Some(plan.clone()),
        },
        |tx| {
            let claim = format!("claim:{claim_id}");
            let (claim_target, outcome): (String, String) = tx
                .query_row(
                    "select target_handle,outcome from claims where handle=?1 and kind='review'",
                    params![claim],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .context("review claim not found")?;
            if claim_target != plan {
                bail!("claim does not target the review plan");
            }
            let current = record_state(tx, &plan, "review_plan")?;
            if current != "pending" {
                bail!("review decision requires a pending plan");
            }
            let resulting = match adjudication {
                "accept" if outcome == "clean" => "clean",
                "accept" => {
                    let policy: String = tx.query_row(
                        "select parent_handle from records where handle=?1",
                        params![plan],
                        |row| row.get(0),
                    )?;
                    let limit: i64 = tx.query_row(
                        "select policy_limit from records where handle=?1",
                        params![policy],
                        |row| row.get(0),
                    )?;
                    let claims: i64 = tx.query_row(
                        "select count(*) from claims where plan_handle=?1 and kind='review'",
                        params![plan],
                        |row| row.get(0),
                    )?;
                    if claims >= limit {
                        "blocked"
                    } else {
                        "pending"
                    }
                }
                "reject" | "needs-evidence" => "pending",
                _ => bail!("review adjudication must be accept, reject, or needs-evidence"),
            };
            append_decision(
                tx,
                "review",
                &plan,
                Some(&claim),
                adjudication,
                resulting,
                expected_current,
                reason,
                None,
            )
        },
    )
}

pub fn waive_review_plan(
    root: &Path,
    plan_id: i64,
    expected_current: &str,
    reason: &str,
    risk: Option<&str>,
) -> Result<Decision14> {
    if reason.trim().is_empty() {
        bail!("review waiver requires a reason");
    }
    let plan = format!("review_plan:{plan_id}");
    mutate_intent(
        root,
        MutationIntent14::ReviewWaive {
            handle: plan.clone(),
        },
        |tx| {
            let state = record_state(tx, &plan, "review_plan")?;
            if !matches!(state.as_str(), "pending" | "blocked") {
                bail!("only pending or blocked review plans can be waived");
            }
            append_decision(
                tx,
                "waiver",
                &plan,
                None,
                "waive",
                "waived",
                expected_current,
                reason,
                risk,
            )
        },
    )
}

pub fn decision_head_for(root: &Path, target: &str) -> Result<String> {
    let conn = open(root)?;
    decision_head(&conn, target)
}

pub fn add_correction(
    root: &Path,
    work_id: i64,
    title: &str,
    severity: &str,
    details: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "correction")?;
        let handle = format!("correction:{id}");
        insert_record(
            tx,
            &handle,
            "correction",
            "recorded",
            Some(&work),
            None,
            Some(&id.to_string()),
            Some(title),
            Some(severity),
            Some(details),
        )?;
        Ok(Record14 {
            handle,
            state: "recorded".into(),
            revision: 1,
        })
    })
}

pub fn link_correction_requirement(
    root: &Path,
    correction_id: i64,
    requirement_handle: &str,
) -> Result<Record14> {
    let correction = format!("correction:{correction_id}");
    mutate_intent(
        root,
        MutationIntent14::Correction {
            handle: correction.clone(),
            action: "link-requirement",
        },
        |tx| {
            if record_state(tx, &correction, "correction")? != "recorded" {
                bail!("requirement link requires a recorded correction");
            }
            let work = record_owner(tx, &correction, "correction")?;
            ensure_active_owner(tx, &work)?;
            if record_state(tx, requirement_handle, "requirement")? != "active" {
                bail!("correction requirement must be active");
            }
            let relation =
                insert_relation(tx, "trace", &correction, requirement_handle, Some(true))?;
            tx.execute(
                "update relations set expected_target_revision=(select revision from records where handle=?1) where handle=?2",
                params![requirement_handle, relation],
            )?;
            update_record_state(
                tx,
                &correction,
                "correction",
                "link-requirement",
                "designed",
                "approved requirement linked",
            )
        },
    )
}

fn require_current_passing_correction_usage(
    conn: &Connection,
    correction: &str,
    usage: &str,
) -> Result<()> {
    let work = record_owner(conn, correction, "correction")?;
    let (profile, result, recorded_profile_revision, usage_created):
        (String, String, Option<i64>, String) = conn
        .query_row(
            "select owner_handle,coalesce(details,''),ordinal,created_at from records where handle=?1 and kind='command_usage' and state='recorded'",
            params![usage],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .context("command usage not found")?;
    if result != "pass" {
        bail!("correction validation requires a passing command usage");
    }
    let (profile_owner, profile_state, profile_revision, profile_updated):
        (String, String, i64, String) = conn.query_row(
        "select owner_handle,state,revision,updated_at from records where handle=?1 and kind='command_profile'",
        params![profile],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if profile_owner != work || profile_state != "fixed" {
        bail!("correction validation requires a fixed command profile from the same work");
    }
    if recorded_profile_revision != Some(profile_revision) {
        bail!("correction validation usage is stale for the command profile revision");
    }
    let (requirement_state, requirement_revision, expected_revision, design_state, design_updated):
        (String, i64, Option<i64>, String, String) = conn.query_row(
        "select r.state,r.revision,t.expected_target_revision,d.state,d.updated_at from relations t join records r on r.handle=t.target_handle and r.kind='requirement' join records d on d.handle=r.owner_handle and d.kind='design_version' where t.kind='trace' and t.source_handle=?1 and t.state='recorded'",
        params![correction],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).context("approved correction requirement link not found")?;
    if requirement_state != "active"
        || expected_revision != Some(requirement_revision)
        || design_state != "approved"
    {
        bail!("correction validation requires the current approved requirement revision");
    }
    let usage_time = OffsetDateTime::parse(&usage_created, &Rfc3339)?;
    let profile_time = OffsetDateTime::parse(&profile_updated, &Rfc3339)?;
    let design_time = OffsetDateTime::parse(&design_updated, &Rfc3339)?;
    if usage_time <= profile_time || usage_time <= design_time {
        bail!("correction validation usage must be recorded after approval and profile fixation");
    }
    Ok(())
}

fn current_passing_correction_usage(conn: &Connection, correction: &str) -> Result<Option<String>> {
    let work = record_owner(conn, correction, "correction")?;
    let mut stmt = conn.prepare(
        "select u.handle from records u join records p on p.handle=u.owner_handle where u.kind='command_usage' and u.state='recorded' and p.kind='command_profile' and p.owner_handle=?1 order by u.created_at desc,u.handle desc",
    )?;
    let usages = stmt
        .query_map(params![work], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(usages
        .into_iter()
        .find(|usage| require_current_passing_correction_usage(conn, correction, usage).is_ok()))
}

pub fn link_correction_validation(
    root: &Path,
    correction_id: i64,
    usage_handle: &str,
) -> Result<Record14> {
    let correction = format!("correction:{correction_id}");
    mutate_intent(
        root,
        MutationIntent14::Correction {
            handle: correction.clone(),
            action: "link-validation",
        },
        |tx| {
            if record_state(tx, &correction, "correction")? != "designed" {
                bail!("validation link requires a designed correction");
            }
            let work = record_owner(tx, &correction, "correction")?;
            ensure_active_owner(tx, &work)?;
            require_current_passing_correction_usage(tx, &correction, usage_handle)?;
            insert_relation(tx, "evidence_target", &correction, usage_handle, Some(true))?;
            update_record_state(
                tx,
                &correction,
                "correction",
                "link-validation",
                "validated",
                "current fixed validation linked",
            )
        },
    )
}

pub fn resolve_correction(root: &Path, correction_id: i64, reason: &str) -> Result<Record14> {
    let correction = format!("correction:{correction_id}");
    mutate_intent(
        root,
        MutationIntent14::Correction {
            handle: correction.clone(),
            action: "resolve",
        },
        |tx| update_record_state(tx, &correction, "correction", "resolve", "resolved", reason),
    )
}

pub fn except_correction(
    root: &Path,
    correction_id: i64,
    expected_current: &str,
    reason: &str,
    risk: &str,
) -> Result<Decision14> {
    let correction = format!("correction:{correction_id}");
    mutate_intent(
        root,
        MutationIntent14::Correction {
            handle: correction.clone(),
            action: "except",
        },
        |tx| {
            let state = record_state(tx, &correction, "correction")?;
            if !matches!(state.as_str(), "recorded" | "designed") {
                bail!("only recorded or designed corrections can be excepted");
            }
            append_decision(
                tx,
                "exception",
                &correction,
                None,
                "except",
                "excepted",
                expected_current,
                reason,
                Some(risk),
            )
        },
    )
}

pub fn start_kpt(root: &Path, work_id: i64, summary: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "kpt_review")?;
        let handle = format!("kpt_review:{id}");
        insert_record(
            tx,
            &handle,
            "kpt_review",
            "open",
            Some(&work),
            None,
            Some(&id.to_string()),
            Some(summary),
            None,
            None,
        )?;
        Ok(Record14 {
            handle,
            state: "open".into(),
            revision: 1,
        })
    })
}

pub fn add_kpt_item(
    root: &Path,
    review_id: i64,
    item_type: &str,
    title: &str,
    severity: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        let review = format!("kpt_review:{review_id}");
        if record_state(tx, &review, "kpt_review")? != "open" {
            bail!("KPT item requires an open review");
        }
        let work = record_owner(tx, &review, "kpt_review")?;
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "kpt_item")?;
        let handle = format!("kpt_item:{id}");
        insert_record(
            tx,
            &handle,
            "kpt_item",
            "open",
            Some(&work),
            Some(&review),
            Some(&id.to_string()),
            Some(title),
            Some(severity),
            Some(item_type),
        )?;
        Ok(Record14 {
            handle,
            state: "open".into(),
            revision: 1,
        })
    })
}

pub fn transition_kpt_item(
    root: &Path,
    item_id: i64,
    action: &str,
    reason: &str,
) -> Result<Record14> {
    let next = match action {
        "convert" => "converted",
        "dismiss" => "dismissed",
        _ => bail!("unsupported KPT item action"),
    };
    transition_record(
        root,
        &format!("kpt_item:{item_id}"),
        "kpt_item",
        action,
        next,
        reason,
    )
}

pub fn close_kpt(root: &Path, review_id: i64) -> Result<Record14> {
    let review = format!("kpt_review:{review_id}");
    let conn = open(root)?;
    let open_items: i64 = conn.query_row(
        "select count(*) from records where parent_handle=?1 and kind='kpt_item' and state='open'",
        params![review],
        |row| row.get(0),
    )?;
    if open_items != 0 {
        bail!("KPT review has open items");
    }
    drop(conn);
    transition_record(
        root,
        &review,
        "kpt_review",
        "close",
        "closed",
        "all items disposed",
    )
}

pub fn add_requirement(root: &Path, work_id: i64, key: &str, title: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "requirement")?;
        let handle = format!("requirement:{id}");
        insert_record(
            tx,
            &handle,
            "requirement",
            "active",
            Some(&work),
            None,
            Some(key),
            Some(title),
            None,
            None,
        )?;
        Ok(Record14 {
            handle,
            state: "active".into(),
            revision: 1,
        })
    })
}

pub fn add_command_profile(
    root: &Path,
    work_id: i64,
    name: &str,
    command: &str,
) -> Result<Record14> {
    let work = format!("work:{work_id}");
    mutate_intent(
        root,
        MutationIntent14::CorrectionCommandProfile {
            work: work.clone(),
            profile: None,
            action: "add",
        },
        |tx| {
            ensure_active_owner(tx, &work)?;
            let id = next_numeric_id(tx, "command_profile")?;
            let handle = format!("command_profile:{id}");
            insert_record(
                tx,
                &handle,
                "command_profile",
                "candidate",
                Some(&work),
                None,
                Some(name),
                Some(name),
                None,
                Some(command),
            )?;
            Ok(Record14 {
                handle,
                state: "candidate".into(),
                revision: 1,
            })
        },
    )
}

pub fn transition_command_profile(
    root: &Path,
    profile_id: i64,
    action: &str,
    reason: &str,
) -> Result<Record14> {
    let next = match action {
        "prefer" => "preferred",
        "fix" => "fixed",
        "deprecate" => "deprecated",
        _ => bail!("unsupported command profile action"),
    };
    let profile = format!("command_profile:{profile_id}");
    let work = {
        let conn = open(root)?;
        record_owner(&conn, &profile, "command_profile")?
    };
    mutate_intent(
        root,
        MutationIntent14::CorrectionCommandProfile {
            work,
            profile: Some(profile.clone()),
            action: match action {
                "fix" => "fix",
                "prefer" => "prefer",
                "deprecate" => "deprecate",
                _ => unreachable!(),
            },
        },
        |tx| update_record_state(tx, &profile, "command_profile", action, next, reason),
    )
}

pub fn add_command_usage(
    root: &Path,
    profile_id: i64,
    invocation: &str,
    result: &str,
    output_digest: &str,
) -> Result<Record14> {
    let profile = format!("command_profile:{profile_id}");
    mutate_intent(
        root,
        MutationIntent14::CorrectionValidationEvidence {
            profile: profile.clone(),
        },
        |tx| {
            let (state, profile_revision): (String, i64) = tx.query_row(
                "select state,revision from records where handle=?1 and kind='command_profile'",
                params![profile],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if state == "deprecated" {
                bail!("cannot record usage for a deprecated command profile");
            }
            let work = record_owner(tx, &profile, "command_profile")?;
            ensure_active_owner(tx, &work)?;
            let id = next_numeric_id(tx, "command_usage")?;
            let handle = format!("command_usage:{id}");
            let occurrence: i64 = tx.query_row(
                "select coalesce(max(occurrence),0)+1 from records where kind='command_usage' and owner_handle=?1 and record_key=?2",
                params![profile, invocation],
                |row| row.get(0),
            )?;
            let created = now()?;
            tx.execute(
                "insert into records(handle,project_handle,kind,state,revision,owner_handle,record_key,occurrence,ordinal,title,content_digest,details,created_at,updated_at) values(?1,'project:current','command_usage','recorded',1,?2,?3,?4,?5,?3,?6,?7,?8,?8)",
                params![handle, profile, invocation, occurrence, profile_revision, output_digest, result, created],
            )?;
            insert_record_event(tx, &handle, "created", None, "recorded", 1, &created)?;
            Ok(Record14 {
                handle,
                state: "recorded".into(),
                revision: 1,
            })
        },
    )
}

pub fn create_work_record(
    root: &Path,
    work_id: i64,
    topic: &str,
    details: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "work_record")?;
        let handle = format!("work_record:{id}");
        insert_record(
            tx,
            &handle,
            "work_record",
            "draft",
            Some(&work),
            None,
            Some(&id.to_string()),
            Some(topic),
            None,
            Some(details),
        )?;
        Ok(Record14 {
            handle,
            state: "draft".into(),
            revision: 1,
        })
    })
}

pub fn link_work_record(root: &Path, work_record_id: i64, target: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let record = format!("work_record:{work_record_id}");
        if !matches!(
            record_state(tx, &record, "work_record")?.as_str(),
            "draft" | "complete"
        ) {
            bail!("work record is terminal");
        }
        let target_kind: String = tx
            .query_row(
                "select kind from records where handle=?1",
                params![target],
                |row| row.get(0),
            )
            .context("work record target not found")?;
        if !matches!(
            target_kind.as_str(),
            "repository_commit" | "repository_change" | "repository_comparison" | "command_usage"
        ) {
            bail!("work record target kind is not supported");
        }
        if owning_work(tx, &record)? != owning_work(tx, target)? {
            bail!("work record target is outside its work owner");
        }
        insert_relation(tx, "record_link", &record, target, Some(true))?;
        if record_state(tx, &record, "work_record")? == "draft" {
            update_record_state(
                tx,
                &record,
                "work_record",
                "link",
                "complete",
                "typed evidence linked",
            )
        } else {
            let revision = record_revision(tx, &record)?;
            Ok(Record14 {
                handle: record,
                state: "complete".into(),
                revision,
            })
        }
    })
}

pub fn render_work_record(root: &Path, work_record_id: i64) -> Result<String> {
    let conn = open(root)?;
    let handle = format!("work_record:{work_record_id}");
    let (title, details, state): (String, String, String) = conn.query_row(
        "select coalesce(title,''),coalesce(details,''),state from records where handle=?1 and kind='work_record'",
        params![handle],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).context("work record not found")?;
    let mut links = conn.prepare("select target_handle from relations where kind='record_link' and source_handle=?1 order by target_handle")?;
    let links = links
        .query_map(params![handle], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut output = format!("# {title}\n\nState: {state}\n\n{details}\n");
    if !links.is_empty() {
        output.push_str("\nEvidence:\n");
        for link in links {
            output.push_str(&format!("- {link}\n"));
        }
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
pub fn add_typed_evidence(
    root: &Path,
    kind: &str,
    owner: &str,
    subject: &str,
    producer: &str,
    result: &str,
    content_digest: Option<&str>,
    details: Option<&str>,
) -> Result<String> {
    if !matches!(
        kind,
        "validation"
            | "implementation"
            | "repository"
            | "command_usage"
            | "work_record"
            | "coverage"
            | "update"
    ) {
        bail!("unsupported evidence kind");
    }
    mutate(root, |tx| {
        if owning_work(tx, owner)? != owning_work(tx, subject)? {
            bail!("evidence owner and subject are cross-scoped");
        }
        ensure_active_owner(tx, &owning_work(tx, owner)?)?;
        let id = next_evidence_id(tx)?;
        let handle = format!("evidence:{id}");
        tx.execute(
            "insert into evidence(handle,project_handle,kind,owner_handle,subject_handle,subject_revision,producer,result,content_digest,details,created_at) values(?1,'project:current',?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![handle, kind, owner, subject, record_revision(tx, subject)?, producer, result, content_digest, details, now()?],
        )?;
        Ok(handle)
    })
}

pub fn list_evidence(root: &Path, owner: Option<&str>) -> Result<Vec<(String, String, String)>> {
    let conn = open(root)?;
    let mut stmt = conn.prepare("select handle,kind,result from evidence where (?1 is null or owner_handle=?1) order by handle")?;
    Ok(stmt
        .query_map(params![owner], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn add_rule(root: &Path, work_id: i64, key: &str, details: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "rule")?;
        let handle = format!("rule:{id}");
        insert_record(
            tx,
            &handle,
            "rule",
            "active",
            Some(&work),
            None,
            Some(key),
            Some(key),
            None,
            Some(details),
        )?;
        Ok(Record14 {
            handle,
            state: "active".into(),
            revision: 1,
        })
    })
}

pub fn init_design_package14(root: &Path, design_id: &str, title: &str) -> Result<PathBuf> {
    if design_id.is_empty()
        || !design_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("design id must use lowercase letters, digits, and hyphens");
    }
    let package = root.join(".agent-workbench/designs").join(design_id);
    if package.exists() {
        bail!("design package already exists");
    }
    fs::create_dir_all(package.join("requirements"))?;
    fs::create_dir_all(package.join("validation"))?;
    fs::write(
        package.join("design.yaml"),
        format!(
            "id: {design_id}\ntitle: {title}\nformat: arc42\nversion: 1\nstatus: draft\narc42: {{}}\nrequirements: []\nvalidation: []\n"
        ),
    )?;
    Ok(package)
}

pub fn import_design14(root: &Path, package: &Path, requested_state: &str) -> Result<Record14> {
    let manifest_path = package.join("design.yaml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: yaml_serde::Value = yaml_serde::from_str(&manifest_text)?;
    let design_id = yaml_string(&manifest, "id")?;
    let title = yaml_string(&manifest, "title")?;
    let version = yaml_i64(&manifest, "version")?;
    let mut files = Vec::new();
    if let Some(mapping) = manifest
        .get("arc42")
        .and_then(yaml_serde::Value::as_mapping)
    {
        for value in mapping.values() {
            files.push(yaml_path(value)?);
        }
    }
    for key in ["requirements", "validation"] {
        if let Some(sequence) = manifest.get(key).and_then(yaml_serde::Value::as_sequence) {
            for value in sequence {
                files.push(yaml_path(value)?);
            }
        }
    }
    let mut blocks = Vec::new();
    for relative in files {
        if relative.extension().and_then(|value| value.to_str()) != Some("md") {
            bail!("design manifest entries must be Markdown files");
        }
        let path = package.join(&relative);
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(package.canonicalize()?) || !canonical.is_file() {
            bail!("design manifest path escapes the package or is not a regular file");
        }
        blocks.extend(extract_agent_blocks(&fs::read_to_string(canonical)?)?);
    }
    mutate_intent(
        root,
        MutationIntent14::Project {
            action: "design-import",
        },
        |tx| {
            let package_handle: Option<String> = tx.query_row(
            "select handle from records where kind='design_package' and record_key=?1 and state!='superseded' order by handle desc limit 1",
            params![design_id],
            |row| row.get(0),
        ).optional()?;
            let package_handle = if let Some(handle) = package_handle {
                handle
            } else {
                let id = next_numeric_id(tx, "design_package")?;
                let handle = format!("design_package:{id}");
                insert_record(
                    tx,
                    &handle,
                    "design_package",
                    "draft",
                    None,
                    None,
                    Some(&design_id),
                    Some(&title),
                    None,
                    None,
                )?;
                handle
            };
            let id = next_numeric_id(tx, "design_version")?;
            let handle = format!("design_version:{id}");
            insert_record(
                tx,
                &handle,
                "design_version",
                requested_state,
                Some(&package_handle),
                None,
                Some(&version.to_string()),
                Some(&title),
                None,
                Some(&manifest_text),
            )?;
            for block in &blocks {
                let kind = match block.kind.as_str() {
                    "requirement" => "requirement",
                    "decision" => "design_decision",
                    "validation_gate_template" => "validation_gate",
                    _ => continue,
                };
                let state = match kind {
                    "design_decision" => "accepted",
                    _ => "active",
                };
                let item_id = next_numeric_id(tx, kind)?;
                let item_handle = format!("{kind}:{item_id}");
                insert_record(
                    tx,
                    &item_handle,
                    kind,
                    state,
                    Some(&handle),
                    None,
                    Some(&block.key),
                    Some(&block.key),
                    block.priority.as_deref(),
                    Some(&block.body),
                )?;
            }
            Ok(Record14 {
                handle,
                state: requested_state.into(),
                revision: 1,
            })
        },
    )
}

pub fn approve_design14(root: &Path, design_version_id: i64, summary: &str) -> Result<Record14> {
    mutate_intent(
        root,
        MutationIntent14::Project {
            action: "design-approve",
        },
        |tx| {
            let version = format!("design_version:{design_version_id}");
            let package = record_owner(tx, &version, "design_version")?;
            let record = update_record_state_unowned(
                tx,
                &version,
                "design_version",
                "approve",
                "approved",
                summary,
            )?;
            if record_state(tx, &package, "design_package")? == "draft" {
                update_record_state_unowned(
                    tx,
                    &package,
                    "design_package",
                    "approve",
                    "approved",
                    summary,
                )?;
            }
            Ok(record)
        },
    )
}

#[derive(Debug)]
struct ImportedBlock14 {
    kind: String,
    key: String,
    priority: Option<String>,
    body: String,
}

fn extract_agent_blocks(content: &str) -> Result<Vec<ImportedBlock14>> {
    let mut output = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("```yaml agent-workbench") {
        let after = &rest[start + "```yaml agent-workbench".len()..];
        let after = after
            .strip_prefix('\n')
            .or_else(|| after.strip_prefix("\r\n"))
            .context("invalid design metadata fence")?;
        let end = after
            .find("```")
            .context("unterminated design metadata fence")?;
        let metadata: yaml_serde::Value = yaml_serde::from_str(&after[..end])?;
        let body_start = end + 3;
        let body_end = after[body_start..]
            .find("```yaml agent-workbench")
            .unwrap_or(after.len() - body_start);
        output.push(ImportedBlock14 {
            kind: yaml_string(&metadata, "type")?,
            key: yaml_string(&metadata, "key")?,
            priority: metadata
                .get("priority")
                .and_then(yaml_serde::Value::as_str)
                .map(str::to_string),
            body: after[body_start..body_start + body_end].trim().to_string(),
        });
        rest = &after[body_start..];
    }
    Ok(output)
}

fn yaml_string(value: &yaml_serde::Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(yaml_serde::Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("design metadata requires {key}"))
}
fn yaml_i64(value: &yaml_serde::Value, key: &str) -> Result<i64> {
    value
        .get(key)
        .and_then(yaml_serde::Value::as_i64)
        .with_context(|| format!("design metadata requires {key}"))
}
fn yaml_path(value: &yaml_serde::Value) -> Result<PathBuf> {
    Ok(PathBuf::from(
        value
            .as_str()
            .context("design manifest path must be a string")?,
    ))
}

pub fn derive_task14(
    root: &Path,
    design_version_id: i64,
    requirement_key: &str,
    task_id: i64,
) -> Result<String> {
    mutate(root, |tx| {
        let version = format!("design_version:{design_version_id}");
        let requirement: String = tx.query_row(
            "select handle from records where kind='requirement' and owner_handle=?1 and record_key=?2 and state='active'",
            params![version, requirement_key],
            |row| row.get(0),
        ).context("active design requirement not found")?;
        let task = format!("task:{task_id}");
        ensure_active_owner(tx, &owning_work(tx, &task)?)?;
        insert_relation(tx, "trace", &requirement, &task, Some(true))
    })
}

pub fn add_coverage14(
    root: &Path,
    design_version_id: i64,
    requirement_key: &str,
    task_id: i64,
    state: &str,
    details: &str,
) -> Result<Record14> {
    if !matches!(state, "open" | "covered" | "excepted") {
        bail!("invalid coverage state");
    }
    mutate(root, |tx| {
        let requirement: String = tx.query_row(
            "select handle from records where kind='requirement' and owner_handle=?1 and record_key=?2 and state='active'",
            params![format!("design_version:{design_version_id}"), requirement_key],
            |row| row.get(0),
        ).context("active design requirement not found")?;
        let task = format!("task:{task_id}");
        ensure_active_owner(tx, &owning_work(tx, &task)?)?;
        let id = next_numeric_id(tx, "coverage")?;
        let handle = format!("coverage:{id}");
        insert_record(
            tx,
            &handle,
            "coverage",
            state,
            Some(&requirement),
            Some(&task),
            Some(&id.to_string()),
            Some(requirement_key),
            None,
            Some(details),
        )?;
        Ok(Record14 {
            handle,
            state: state.into(),
            revision: 1,
        })
    })
}

pub fn decompose_design14(
    root: &Path,
    design_version_id: i64,
    work_id: i64,
    title: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        let version = format!("design_version:{design_version_id}");
        if record_state(tx, &version, "design_version")? != "approved" {
            bail!("design version must be approved");
        }
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "checklist")?;
        let checklist = format!("checklist:{id}");
        insert_record(
            tx,
            &checklist,
            "checklist",
            "open",
            Some(&work),
            Some(&version),
            Some(&id.to_string()),
            Some(title),
            None,
            None,
        )?;
        let mut requirements = tx.prepare("select handle,record_key from records where kind='requirement' and owner_handle=?1 and state='active' order by record_key")?;
        let requirements = requirements
            .query_map(params![version], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (ordinal, (requirement, key)) in requirements.into_iter().enumerate() {
            let item_id = next_numeric_id(tx, "checklist_item")?;
            let item = format!("checklist_item:{item_id}");
            insert_record(
                tx,
                &item,
                "checklist_item",
                "open",
                Some(&work),
                Some(&checklist),
                Some(&key),
                Some(&key),
                None,
                None,
            )?;
            tx.execute(
                "update records set ordinal=?1 where handle=?2",
                params![ordinal as i64 + 1, item],
            )?;
            insert_relation(tx, "checklist_target", &item, &requirement, Some(true))?;
        }
        Ok(Record14 {
            handle: checklist,
            state: "open".into(),
            revision: 1,
        })
    })
}

pub fn close_checklist_item14(root: &Path, item_id: i64) -> Result<Record14> {
    transition_record(
        root,
        &format!("checklist_item:{item_id}"),
        "checklist_item",
        "close",
        "closed",
        "completed",
    )
}

pub fn close_checklist14(root: &Path, checklist_id: i64) -> Result<Record14> {
    let checklist = format!("checklist:{checklist_id}");
    let conn = open(root)?;
    let open_items: i64 = conn.query_row("select count(*) from records where kind='checklist_item' and parent_handle=?1 and state not in ('closed','accepted_out_of_scope')", params![checklist], |row| row.get(0))?;
    if open_items != 0 {
        bail!("checklist has open items");
    }
    drop(conn);
    transition_record(
        root,
        &checklist,
        "checklist",
        "close",
        "closed",
        "all items terminal",
    )
}

pub fn design_gate14(
    root: &Path,
    design_version_id: i64,
    implementation: bool,
) -> Result<(bool, Vec<String>)> {
    let conn = open(root)?;
    let version = format!("design_version:{design_version_id}");
    let mut blockers = Vec::new();
    if record_state(&conn, &version, "design_version")? != "approved" {
        blockers.push(format!("{version}: approval required"));
    }
    if implementation {
        let missing: i64 = conn.query_row(
            "select count(*) from records r where r.kind='requirement' and r.owner_handle=?1 and r.state='active' and not exists(select 1 from relations t where t.kind='trace' and t.source_handle=r.handle)",
            params![version], |row| row.get(0)
        )?;
        if missing != 0 {
            blockers.push(format!("{version}: {missing} requirements lack task trace"));
        }
    }
    Ok((blockers.is_empty(), blockers))
}

pub fn add_acceptance14(
    root: &Path,
    target: &str,
    acceptance_type: &str,
    reason: &str,
    risk: Option<&str>,
) -> Result<Record14> {
    mutate(root, |tx| {
        record_state_any(tx, target)?;
        let id = next_numeric_id(tx, "acceptance")?;
        let handle = format!("acceptance:{id}");
        let owner = active_activation(tx)?.map(|(_, work)| work);
        insert_record(
            tx,
            &handle,
            "acceptance",
            "approved",
            owner.as_deref(),
            Some(target),
            Some(&id.to_string()),
            Some(acceptance_type),
            None,
            Some(&format!("{reason}; risk={}", risk.unwrap_or("unspecified"))),
        )?;
        Ok(Record14 {
            handle,
            state: "approved".into(),
            revision: 1,
        })
    })
}

pub fn revoke_acceptance14(root: &Path, acceptance_id: i64, reason: &str) -> Result<Record14> {
    mutate(root, |tx| {
        update_record_state_unowned(
            tx,
            &format!("acceptance:{acceptance_id}"),
            "acceptance",
            "revoke",
            "revoked",
            reason,
        )
    })
}

pub fn dispose_stale14(
    root: &Path,
    stale_id: i64,
    action: &str,
    expected_current: &str,
    reason: &str,
) -> Result<Decision14> {
    let resulting = match action {
        "accept" => "accepted",
        "close" => "closed",
        _ => bail!("unsupported stale action"),
    };
    mutate(root, |tx| {
        let stale = format!("stale_disposition:{stale_id}");
        if record_state(tx, &stale, "stale_disposition")? != "unresolved" {
            bail!("stale disposition is already terminal");
        }
        if let Ok(work) = owning_work(tx, &stale) {
            ensure_active_owner(tx, &work)?;
        }
        append_decision(
            tx,
            "stale",
            &stale,
            None,
            action,
            resulting,
            expected_current,
            reason,
            None,
        )
    })
}

pub fn add_finding(
    root: &Path,
    work_id: i64,
    severity: &str,
    description: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "finding")?;
        let handle = format!("finding:{id}");
        insert_record(
            tx,
            &handle,
            "finding",
            "open",
            Some(&work),
            None,
            Some(&id.to_string()),
            Some(description),
            Some(severity),
            None,
        )?;
        Ok(Record14 {
            handle,
            state: "open".into(),
            revision: 1,
        })
    })
}

pub fn accept_finding(
    root: &Path,
    finding_id: i64,
    expected_current: &str,
    reason: &str,
    risk: &str,
) -> Result<Decision14> {
    let finding = format!("finding:{finding_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: finding.clone(),
            action: "accept",
        },
        |tx| {
            if record_state(tx, &finding, "finding")? != "open" {
                bail!("only an open finding can be accepted out of scope");
            }
            ensure_active_owner(tx, &owning_work(tx, &finding)?)?;
            append_decision(
                tx,
                "exception",
                &finding,
                None,
                "accept-out-of-scope",
                "accepted_out_of_scope",
                expected_current,
                reason,
                Some(risk),
            )
        },
    )
}

pub fn remediate_finding(
    root: &Path,
    finding_id: i64,
    work_id: i64,
    replace: bool,
) -> Result<String> {
    let finding = format!("finding:{finding_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: finding.clone(),
            action: "remediate",
        },
        |tx| {
            let work = format!("work:{work_id}");
            record_state(tx, &finding, "finding")?;
            record_state(tx, &work, "work")?;
            ensure_active_owner(tx, &owning_work(tx, &finding)?)?;
            if replace {
                let existing: Option<(String, i64)> = tx.query_row(
                "select handle,revision from relations where kind='remediation' and source_handle=?1 and state='active' order by handle limit 1",
                params![finding],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional()?;
                let (handle, revision) =
                    existing.context("finding has no active remediation to replace")?;
                transition_relation(
                    tx,
                    &handle,
                    revision,
                    "replace",
                    "superseded",
                    "replacement remediation",
                )?;
            } else {
                let active: bool = tx.query_row("select exists(select 1 from relations where kind='remediation' and source_handle=?1 and state='active')", params![finding], |row| row.get(0))?;
                if active {
                    bail!("finding already has an active remediation");
                }
            }
            let id = next_numeric_relation_id(tx, "remediation")?;
            let handle = format!("remediation:{id}");
            let created = now()?;
            tx.execute("insert into relations(handle,project_handle,kind,source_handle,target_handle,state,revision,created_at,updated_at) values(?1,'project:current','remediation',?2,?3,'active',1,?4,?4)", params![handle, finding, work, created])?;
            insert_relation_event(tx, &handle, "created", None, "active", 1, &created)?;
            Ok(handle)
        },
    )
}

pub fn add_closure(root: &Path, finding_id: i64, contract: &str) -> Result<Record14> {
    let finding = format!("finding:{finding_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: finding.clone(),
            action: "closure-add",
        },
        |tx| {
            if record_state(tx, &finding, "finding")? != "open" {
                bail!("closure requires an open finding");
            }
            let work = record_owner(tx, &finding, "finding")?;
            ensure_active_owner(tx, &work)?;
            let id = next_numeric_id(tx, "closure")?;
            let handle = format!("closure:{id}");
            insert_record(
                tx,
                &handle,
                "closure",
                "draft",
                Some(&work),
                Some(&finding),
                Some(&id.to_string()),
                Some(contract),
                None,
                None,
            )?;
            Ok(Record14 {
                handle,
                state: "draft".into(),
                revision: 1,
            })
        },
    )
}

pub fn supersede_closure(
    root: &Path,
    closure_id: i64,
    expected_current: &str,
    contract: &str,
    reason: &str,
) -> Result<Record14> {
    let closure = format!("closure:{closure_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: closure.clone(),
            action: "supersede",
        },
        |tx| {
            let finding: String = tx
                .query_row(
                    "select parent_handle from records where handle=?1 and kind='closure'",
                    params![closure],
                    |row| row.get(0),
                )
                .context("closure not found")?;
            if !matches!(
                record_state(tx, &closure, "closure")?.as_str(),
                "draft" | "ready"
            ) {
                bail!("only draft or ready closure can be superseded");
            }
            ensure_active_owner(tx, &owning_work(tx, &closure)?)?;
            append_decision(
                tx,
                "correction",
                &closure,
                None,
                "supersede",
                "superseded",
                expected_current,
                reason,
                None,
            )?;
            let id = next_numeric_id(tx, "closure")?;
            let handle = format!("closure:{id}");
            let work = owning_work(tx, &finding)?;
            insert_record(
                tx,
                &handle,
                "closure",
                "draft",
                Some(&work),
                Some(&finding),
                Some(&id.to_string()),
                Some(contract),
                None,
                None,
            )?;
            Ok(Record14 {
                handle,
                state: "draft".into(),
                revision: 1,
            })
        },
    )
}

pub fn ready_closure(root: &Path, closure_id: i64, evidence_contract: &str) -> Result<Record14> {
    let closure = format!("closure:{closure_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: closure.clone(),
            action: "closure-ready",
        },
        |tx| {
            let finding: String = tx.query_row(
                "select parent_handle from records where handle=?1 and kind='closure'",
                params![closure],
                |row| row.get(0),
            )?;
            if record_state(tx, &closure, "closure")? != "draft"
                || record_state(tx, &finding, "finding")? != "open"
            {
                bail!("closure readiness requires draft closure and open finding");
            }
            let work = record_owner(tx, &closure, "closure")?;
            ensure_active_owner(tx, &work)?;
            let ready =
                update_record_state(tx, &closure, "closure", "ready", "ready", evidence_contract)?;
            update_record_state(
                tx,
                &finding,
                "finding",
                "verify",
                "awaiting_verification",
                "ready closure",
            )?;
            let id = next_numeric_id(tx, "closure_attempt")?;
            let attempt = format!("closure_attempt:{id}");
            insert_record(
                tx,
                &attempt,
                "closure_attempt",
                "pending",
                Some(&work),
                Some(&closure),
                Some(&id.to_string()),
                Some("verification attempt"),
                None,
                None,
            )?;
            Ok(Record14 {
                handle: attempt,
                state: "pending".into(),
                revision: ready.revision,
            })
        },
    )
}

pub fn add_verification_claim(
    root: &Path,
    attempt_id: i64,
    outcome: &str,
    producer: &str,
    scope_digest: &str,
    evidence_text: Option<&str>,
) -> Result<Claim14> {
    if !matches!(outcome, "verified" | "not_fixed" | "needs_evidence") {
        bail!("verification outcome must be verified, not_fixed, or needs_evidence");
    }
    let attempt = format!("closure_attempt:{attempt_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: attempt.clone(),
            action: "verification-claim",
        },
        |tx| {
            if record_state(tx, &attempt, "closure_attempt")? != "pending" {
                bail!("verification claim requires a pending closure attempt");
            }
            let id = next_claim_id(tx)?;
            let handle = format!("claim:{id}");
            tx.execute(
            "insert into claims(handle,project_handle,kind,target_handle,attempt_handle,target_revision,outcome,producer,scope_digest,evidence_text,created_at) values(?1,'project:current','verification',?2,?2,1,?3,?4,?5,?6,?7)",
            params![handle, attempt, outcome, producer, scope_digest, evidence_text, now()?],
        )?;
            Ok(Claim14 {
                handle,
                target_handle: attempt,
                outcome: outcome.into(),
            })
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn decide_verification(
    root: &Path,
    finding_id: i64,
    closure_id: i64,
    attempt_id: i64,
    claim_id: i64,
    adjudication: &str,
    expected_current: &str,
    reason: &str,
) -> Result<Decision14> {
    let finding = format!("finding:{finding_id}");
    mutate_intent(
        root,
        MutationIntent14::Finding {
            handle: finding.clone(),
            action: "verification-decide",
        },
        |tx| {
            let closure = format!("closure:{closure_id}");
            let attempt = format!("closure_attempt:{attempt_id}");
            let claim = format!("claim:{claim_id}");
            let (target, outcome): (String, String) = tx
            .query_row(
                "select target_handle,outcome from claims where handle=?1 and kind='verification'",
                params![claim],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .context("verification claim not found")?;
            if target != attempt {
                bail!("verification claim does not target the closure attempt");
            }
            let linked_closure: String = tx.query_row(
                "select parent_handle from records where handle=?1 and kind='closure_attempt'",
                params![attempt],
                |row| row.get(0),
            )?;
            let linked_finding: String = tx.query_row(
                "select parent_handle from records where handle=?1 and kind='closure'",
                params![closure],
                |row| row.get(0),
            )?;
            if linked_closure != closure || linked_finding != finding {
                bail!("finding, closure, and attempt do not form one chain");
            }
            let resulting_attempt = match adjudication {
                "reject" => "claim_rejected",
                "needs-evidence" => "needs_evidence",
                "accept" if outcome == "verified" => "verified",
                "accept" if outcome == "not_fixed" => "not_fixed",
                "accept" if outcome == "needs_evidence" => "needs_evidence",
                "accept" => bail!("unsupported verification claim outcome"),
                _ => bail!("verification adjudication must be accept, reject, or needs-evidence"),
            };
            let decision = append_decision(
                tx,
                "verification",
                &attempt,
                Some(&claim),
                adjudication,
                resulting_attempt,
                expected_current,
                reason,
                None,
            )?;
            match resulting_attempt {
                "verified" => {
                    update_record_state(tx, &closure, "closure", "verified", "verified", reason)?;
                    update_record_state(tx, &finding, "finding", "verified", "closed", reason)?;
                    if let Some((remediation, revision)) = tx.query_row(
                    "select handle,revision from relations where kind='remediation' and source_handle=?1 and state='active' order by handle limit 1",
                    params![finding],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                ).optional()? {
                    transition_relation(tx, &remediation, revision, "verified", "recovered", reason)?;
                }
                }
                "not_fixed" => {
                    update_record_state_unowned(
                        tx,
                        &closure,
                        "closure",
                        "not-fixed",
                        "draft",
                        reason,
                    )?;
                    update_record_state(tx, &finding, "finding", "not-fixed", "open", reason)?;
                }
                _ => {}
            }
            Ok(decision)
        },
    )
}

pub fn add_repository(root: &Path, work_id: i64, name: &str, path: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let work = format!("work:{work_id}");
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "repository")?;
        let handle = format!("repository:{id}");
        insert_record(
            tx,
            &handle,
            "repository",
            "active",
            Some(&work),
            None,
            Some(name),
            Some(name),
            None,
            Some(path),
        )?;
        Ok(Record14 {
            handle,
            state: "active".into(),
            revision: 1,
        })
    })
}

pub fn add_repository_snapshot(root: &Path, repository_id: i64, commit: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let repository = format!("repository:{repository_id}");
        if record_state(tx, &repository, "repository")? != "active" {
            bail!("snapshot requires an active repository");
        }
        let work = record_owner(tx, &repository, "repository")?;
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "repository_snapshot")?;
        let handle = format!("repository_snapshot:{id}");
        insert_record(
            tx,
            &handle,
            "repository_snapshot",
            "draft",
            Some(&repository),
            None,
            Some(&id.to_string()),
            Some(commit),
            None,
            None,
        )?;
        Ok(Record14 {
            handle,
            state: "draft".into(),
            revision: 1,
        })
    })
}

pub fn add_repository_change(
    root: &Path,
    snapshot_id: i64,
    path: &str,
    content_digest: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        let snapshot = format!("repository_snapshot:{snapshot_id}");
        if record_state(tx, &snapshot, "repository_snapshot")? != "draft" {
            bail!("repository change requires a draft snapshot");
        }
        let repository = record_owner(tx, &snapshot, "repository_snapshot")?;
        let work = record_owner(tx, &repository, "repository")?;
        ensure_active_owner(tx, &work)?;
        let id = next_numeric_id(tx, "repository_change")?;
        let handle = format!("repository_change:{id}");
        insert_record(
            tx,
            &handle,
            "repository_change",
            "unclassified",
            Some(&snapshot),
            None,
            Some(path),
            Some(path),
            None,
            None,
        )?;
        tx.execute(
            "update records set content_digest=?1 where handle=?2",
            params![content_digest, handle],
        )?;
        Ok(Record14 {
            handle,
            state: "unclassified".into(),
            revision: 1,
        })
    })
}

pub fn classify_repository_change(
    root: &Path,
    change_id: i64,
    classification: &str,
) -> Result<Record14> {
    transition_record(
        root,
        &format!("repository_change:{change_id}"),
        "repository_change",
        "classify",
        "classified",
        classification,
    )
}

pub fn accept_repository_change(
    root: &Path,
    change_id: i64,
    expected_current: &str,
    reason: &str,
    risk: &str,
) -> Result<Decision14> {
    mutate(root, |tx| {
        let change = format!("repository_change:{change_id}");
        if record_state(tx, &change, "repository_change")? != "unclassified" {
            bail!("only an unclassified repository change can be accepted");
        }
        append_decision(
            tx,
            "acceptance",
            &change,
            None,
            "accept",
            "accepted_exception",
            expected_current,
            reason,
            Some(risk),
        )
    })
}

pub fn finalize_repository_snapshot(root: &Path, snapshot_id: i64) -> Result<Record14> {
    mutate(root, |tx| {
        let snapshot = format!("repository_snapshot:{snapshot_id}");
        if record_state(tx, &snapshot, "repository_snapshot")? != "draft" {
            bail!("only a draft repository snapshot can be finalized");
        }
        let incomplete: i64 = tx.query_row(
            "select count(*) from records where owner_handle=?1 and kind='repository_change' and state not in ('classified','accepted_exception')",
            params![snapshot],
            |row| row.get(0),
        )?;
        if incomplete != 0 {
            bail!("repository snapshot has unclassified changes");
        }
        let digest = repository_snapshot_digest(tx, &snapshot)?;
        let record = update_record_state(
            tx,
            &snapshot,
            "repository_snapshot",
            "finalize",
            "recorded",
            "all changes classified",
        )?;
        tx.execute(
            "update records set content_digest=?1 where handle=?2",
            params![digest, snapshot],
        )?;
        Ok(record)
    })
}

pub fn add_repository_commit(
    root: &Path,
    snapshot_id: i64,
    object_id: &str,
    content_digest: &str,
) -> Result<Record14> {
    if object_id.len() < 40
        || object_id
            .chars()
            .any(|c| !c.is_ascii_hexdigit() || c.is_ascii_uppercase())
    {
        bail!("repository commit requires a lowercase full object id");
    }
    mutate(root, |tx| {
        let snapshot = format!("repository_snapshot:{snapshot_id}");
        if record_state(tx, &snapshot, "repository_snapshot")? != "recorded" {
            bail!("repository commit requires a finalized snapshot");
        }
        let id = next_numeric_id(tx, "repository_commit")?;
        let handle = format!("repository_commit:{id}");
        insert_record(
            tx,
            &handle,
            "repository_commit",
            "recorded",
            Some(&snapshot),
            None,
            Some(object_id),
            Some(object_id),
            None,
            None,
        )?;
        tx.execute(
            "update records set content_digest=?1 where handle=?2",
            params![content_digest, handle],
        )?;
        Ok(Record14 {
            handle,
            state: "recorded".into(),
            revision: 1,
        })
    })
}

pub fn add_repository_comparison(
    root: &Path,
    current_snapshot_id: i64,
    prior_snapshot_id: i64,
) -> Result<Record14> {
    mutate(root, |tx| {
        let current = format!("repository_snapshot:{current_snapshot_id}");
        let prior = format!("repository_snapshot:{prior_snapshot_id}");
        for snapshot in [&current, &prior] {
            if record_state(tx, snapshot, "repository_snapshot")? != "recorded" {
                bail!("repository comparison requires finalized snapshots");
            }
        }
        if record_owner(tx, &current, "repository_snapshot")?
            != record_owner(tx, &prior, "repository_snapshot")?
        {
            bail!("repository comparison snapshots must share a repository");
        }
        let id = next_numeric_id(tx, "repository_comparison")?;
        let handle = format!("repository_comparison:{id}");
        let digest = length_prefixed_digest(&[
            &current,
            &record_revision(tx, &current)?.to_string(),
            &prior,
            &record_revision(tx, &prior)?.to_string(),
        ]);
        insert_record(
            tx,
            &handle,
            "repository_comparison",
            "recorded",
            Some(&current),
            Some(&prior),
            Some(&id.to_string()),
            Some("repository comparison"),
            None,
            None,
        )?;
        tx.execute(
            "update records set content_digest=?1 where handle=?2",
            params![digest, handle],
        )?;
        Ok(Record14 {
            handle,
            state: "recorded".into(),
            revision: 1,
        })
    })
}

pub fn add_dependency(
    root: &Path,
    kind: &str,
    from_handle: &str,
    to_handle: &str,
    details: &str,
) -> Result<String> {
    if !matches!(kind, "work_dependency" | "phase_dependency") {
        bail!("unsupported dependency kind");
    }
    mutate(root, |tx| {
        let entity_kind = if kind == "work_dependency" {
            "work"
        } else {
            "phase"
        };
        let from_work = owning_work(tx, from_handle)?;
        if from_work != owning_work(tx, to_handle)? && kind == "phase_dependency" {
            bail!("phase dependencies must stay inside one work owner");
        }
        record_state(tx, from_handle, entity_kind)?;
        record_state(tx, to_handle, entity_kind)?;
        ensure_active_owner(tx, &from_work)?;
        let id = next_numeric_relation_id(tx, kind)?;
        let handle = format!("{kind}:{id}");
        let created = now()?;
        tx.execute(
            "insert into relations(handle,project_handle,kind,source_handle,target_handle,state,revision,expected_target_revision,details,created_at,updated_at) values(?1,'project:current',?2,?3,?4,'open',1,?5,?6,?7,?7)",
            params![handle, kind, from_handle, to_handle, record_revision(tx, to_handle)?, details, created],
        )?;
        insert_relation_event(tx, &handle, "created", None, "open", 1, &created)?;
        Ok(handle)
    })
}

pub fn satisfy_dependency(root: &Path, relation_handle: &str, reason: &str) -> Result<String> {
    mutate(root, |tx| {
        let source: String = tx
            .query_row(
                "select source_handle from relations where handle=?1",
                params![relation_handle],
                |row| row.get(0),
            )
            .context("dependency not found")?;
        ensure_active_owner(tx, &owning_work(tx, &source)?)?;
        let (state, revision, target, expected): (String, i64, String, i64) = tx.query_row(
            "select state,revision,target_handle,expected_target_revision from relations where handle=?1 and kind in ('work_dependency','phase_dependency')",
            params![relation_handle],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).context("dependency not found")?;
        if state != "open" {
            bail!("only an open dependency can be satisfied");
        }
        let target_state = record_state_any(tx, &target)?;
        let target_revision = record_revision(tx, &target)?;
        if !matches!(
            target_state.as_str(),
            "closed" | "abandoned" | "accepted_out_of_scope"
        ) || target_revision != expected
        {
            bail!("dependency predecessor is not terminal at the expected revision");
        }
        transition_relation(
            tx,
            relation_handle,
            revision,
            "satisfy",
            "satisfied",
            reason,
        )?;
        Ok(relation_handle.into())
    })
}

pub fn accept_dependency(
    root: &Path,
    relation_handle: &str,
    expected_current: &str,
    reason: &str,
    risk: Option<&str>,
) -> Result<Decision14> {
    mutate(root, |tx| {
        let source: String = tx
            .query_row(
                "select source_handle from relations where handle=?1",
                params![relation_handle],
                |row| row.get(0),
            )
            .context("dependency not found")?;
        ensure_active_owner(tx, &owning_work(tx, &source)?)?;
        append_relation_decision(tx, relation_handle, expected_current, reason, risk)
    })
}

pub fn list_relations(
    root: &Path,
    kind: &str,
    source: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let conn = open(root)?;
    let mut stmt = conn.prepare("select handle,state from relations where kind=?1 and (?2 is null or source_handle=?2) order by handle")?;
    Ok(stmt
        .query_map(params![kind, source], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn repository_snapshot_digest(conn: &Connection, snapshot: &str) -> Result<String> {
    let mut stmt = conn.prepare(
        "select record_key,state,coalesce(policy_action,''),coalesce(content_digest,'') from records where owner_handle=?1 and kind='repository_change' order by record_key",
    )?;
    let rows = stmt
        .query_map(params![snapshot], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let values = rows
        .iter()
        .flat_map(|(path, state, action, digest)| {
            [
                path.as_str(),
                state.as_str(),
                action.as_str(),
                digest.as_str(),
            ]
        })
        .collect::<Vec<_>>();
    Ok(length_prefixed_digest(&values))
}

pub fn transition_phase(
    root: &Path,
    phase_id: i64,
    action: &str,
    reason: &str,
) -> Result<Record14> {
    let target = format!("phase:{phase_id}");
    let next = match action {
        "block" => "blocked",
        "unblock" => "open",
        "close" => "closed",
        "accept-out-of-scope" => "accepted_out_of_scope",
        _ => bail!("unsupported phase transition {action}"),
    };
    if next != "closed" {
        return transition_record(root, &target, "phase", action, next, reason);
    }
    mutate(root, |tx| {
        let work = owning_work(tx, &target)?;
        let resolution = resolve_work(tx, &work)?;
        if resolution.blocker.is_some() {
            bail!(
                "phase close is blocked: {}",
                resolution.legal_actions.join(", ")
            );
        }
        let blockers = phase_blockers(tx, &target)?;
        if !blockers.is_empty() {
            bail!("phase close is blocked: {}", blockers.join(", "));
        }
        update_record_state(tx, &target, "phase", action, next, reason)
    })
}

pub fn suspend(root: &Path, reason: &str, next_hint: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let (activation, work) = active_activation(tx)?.context("no active work activation")?;
        let components = semantic_components(tx, &work)?;
        let digest = semantic_digest(&components);
        let snapshot_id = next_snapshot_id(tx)?;
        let snapshot = format!("snapshot:{snapshot_id}");
        let owner_revision = record_revision(tx, &work)?;
        let created = now()?;
        tx.execute(
            "insert into snapshots(handle,project_handle,owner_handle,owner_revision,maturity,semantic_digest,created_at) values(?1,'project:current',?2,?3,'trace-aware',?4,?5)",
            params![snapshot, work, owner_revision, digest, created],
        )?;
        for component in &components {
            tx.execute(
                "insert into snapshot_components(snapshot_handle,component_kind,component_handle,component_state,component_revision,component_digest) values(?1,?2,?3,?4,?5,?6)",
                params![snapshot, component.kind, component.handle, component.state, component.revision.to_string(), component.digest],
            )?;
        }
        update_record_state(
            tx,
            &activation,
            "activation",
            "suspend",
            "suspended",
            &format!("{reason}; next={next_hint}; snapshot={snapshot}"),
        )?;
        Ok(Record14 {
            handle: snapshot,
            state: "recorded".to_string(),
            revision: 1,
        })
    })
}

pub fn resume_check(root: &Path) -> Result<ResumeCheck14> {
    let conn = open(root)?;
    let (_, _, check) = resume_check_conn(&conn)?;
    Ok(check)
}

fn resume_check_conn(conn: &Connection) -> Result<(String, String, ResumeCheck14)> {
    let (activation, work): (String, String) = conn
        .query_row(
            "select handle,owner_handle from records where kind='activation' and state='suspended' order by created_at desc limit 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("no suspended activation")?;
    let (snapshot, recorded): (String, String) = conn
        .query_row(
            "select handle,semantic_digest from snapshots where owner_handle=?1 order by created_at desc limit 1",
            params![work],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("suspended activation has no semantic snapshot")?;
    let components = semantic_components(conn, &work)?;
    let current = semantic_digest(&components);
    let changed = changed_components(conn, &snapshot, &components)?;
    let result = if changed.is_empty() {
        "pass"
    } else {
        "blocked"
    };
    Ok((
        activation,
        work,
        ResumeCheck14 {
            snapshot_handle: snapshot,
            current_digest: current,
            recorded_digest: recorded,
            changed_components: changed,
            result: result.to_string(),
        },
    ))
}

pub fn resume(root: &Path) -> Result<Record14> {
    mutate_intent(root, MutationIntent14::Resume, |tx| {
        let (activation, _, check) = resume_check_conn(tx)?;
        if check.result != "pass" {
            bail!(
                "semantic resume check is blocked: {}",
                check.changed_components.join(",")
            );
        }
        update_record_state(
            tx,
            &activation,
            "activation",
            "resume",
            "active",
            "semantic snapshot matches",
        )
    })
}

pub fn close_work(root: &Path, summary: &str) -> Result<Record14> {
    mutate(root, |tx| {
        let (_, work) = active_activation(tx)?.context("no active work activation")?;
        let resolution = resolve_work(tx, &work)?;
        if resolution.blocker.is_some() {
            bail!(
                "work close is blocked: {}",
                resolution.legal_actions.join(", ")
            );
        }
        let blockers = work_blockers(tx, &work)?;
        if !blockers.is_empty() {
            bail!("work close is blocked: {}", blockers.join(", "));
        }
        let record = update_record_state(tx, &work, "work", "close", "closed", summary)?;
        tx.execute(
            "update records set state='completed',revision=revision+1,updated_at=?1 where kind='activation' and owner_handle=?2 and state='active'",
            params![now()?, work],
        )?;
        Ok(record)
    })
}

pub fn phase_close_ready(root: &Path, phase_id: i64) -> Result<(bool, Vec<String>)> {
    let conn = open(root)?;
    let phase = format!("phase:{phase_id}");
    record_state(&conn, &phase, "phase")?;
    let work = owning_work(&conn, &phase)?;
    let resolution = resolve_work(&conn, &work)?;
    if resolution.blocker.is_some() {
        return Ok((false, resolution.legal_actions));
    }
    let blockers = phase_blockers(&conn, &phase)?;
    Ok((blockers.is_empty(), blockers))
}

fn phase_blockers(conn: &Connection, phase: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "select t.handle from relations m join records t on t.handle=m.target_handle where m.kind='membership' and m.source_handle=?1 and m.state='recorded' and t.state not in ('closed','accepted_out_of_scope') order by t.handle",
    )?;
    let mut blockers = stmt
        .query_map(params![phase], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    blockers.extend(review_plan_blockers(conn, phase)?);
    let mut dependencies = conn.prepare("select handle from relations where kind='phase_dependency' and source_handle=?1 and state='open' order by handle")?;
    blockers.extend(
        dependencies
            .query_map(params![phase], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    );
    Ok(blockers)
}

pub fn work_close_ready(root: &Path) -> Result<(bool, Vec<String>)> {
    let conn = open(root)?;
    let (_, work) = active_activation(&conn)?.context("no active work activation")?;
    let resolution = resolve_work(&conn, &work)?;
    if resolution.blocker.is_some() {
        return Ok((false, resolution.legal_actions));
    }
    let blockers = work_blockers(&conn, &work)?;
    Ok((blockers.is_empty(), blockers))
}

fn work_blockers(conn: &Connection, work: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "select handle from records where owner_handle=?1 and ((kind in ('task','phase','checklist','checklist_item') and state not in ('closed','accepted_out_of_scope')) or (kind='correction' and priority='critical' and state not in ('resolved','excepted')) or (kind='finding' and state not in ('closed','accepted_out_of_scope')) or (kind='review_plan' and required=1 and state not in ('clean','waived'))) order by kind,handle",
    )?;
    let mut blockers = stmt
        .query_map(params![work], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut dependencies = conn.prepare("select handle from relations where kind='work_dependency' and source_handle=?1 and state='open' order by handle")?;
    blockers.extend(
        dependencies
            .query_map(params![work], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    );
    let mut repositories = conn.prepare("select handle from records where owner_handle=?1 and kind='repository' and state='active' order by handle")?;
    for repository in repositories
        .query_map(params![work], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
    {
        let current: Option<String> = conn.query_row(
            "select handle from records where kind='repository_snapshot' and owner_handle=?1 and state='recorded' order by created_at desc,handle desc limit 1",
            params![repository],
            |row| row.get(0),
        ).optional()?;
        let Some(current) = current else {
            blockers.push(format!(
                "{repository}: finalized repository snapshot required"
            ));
            continue;
        };
        let snapshot_count: i64 = conn.query_row("select count(*) from records where kind='repository_snapshot' and owner_handle=?1 and state='recorded'", params![repository], |row| row.get(0))?;
        if snapshot_count > 1 {
            let comparison: bool = conn.query_row(
                "select exists(select 1 from records where kind='repository_comparison' and owner_handle=?1 and state='recorded')",
                params![current],
                |row| row.get(0),
            )?;
            if !comparison {
                blockers.push(format!(
                    "{repository}: current repository comparison required"
                ));
            }
        }
    }
    let design: Option<String> = conn.query_row(
        "select owner_handle from records where handle=?1 and kind='work' and owner_handle like 'design_version:%'",
        params![work],
        |row| row.get(0),
    ).optional()?;
    if let Some(design) = design {
        let mut requirements = conn.prepare("select handle,record_key,revision from records where kind='requirement' and owner_handle=?1 and state='active' order by record_key")?;
        for (requirement, key, revision) in requirements
            .query_map(params![design], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        {
            let task: Option<String> = conn.query_row(
                "select t.target_handle from relations t join records task on task.handle=t.target_handle where t.kind='trace' and t.source_handle=?1 and task.kind='task' and task.owner_handle=?2 order by t.handle limit 1",
                params![requirement, work],
                |row| row.get(0),
            ).optional()?;
            let Some(task) = task else {
                blockers.push(format!("{key}: task trace required"));
                continue;
            };
            let covered: bool = conn.query_row(
                "select exists(select 1 from records where kind='coverage' and owner_handle=?1 and parent_handle=?2 and state in ('covered','excepted'))",
                params![requirement, task],
                |row| row.get(0),
            )?;
            if !covered {
                blockers.push(format!("{key}: current coverage required"));
            }
            let evidence: bool = conn.query_row(
                "select exists(select 1 from evidence where kind='implementation' and owner_handle=?1 and subject_handle=?2 and subject_revision=(select revision from records where handle=?2) and result in ('pass','recorded'))",
                params![work, task],
                |row| row.get(0),
            )?;
            if !evidence {
                blockers.push(format!("{key}: current implementation evidence required at requirement revision {revision}"));
            }
        }
    }
    blockers.sort();
    Ok(blockers)
}

pub fn list_records(root: &Path, kind: &str, owner: Option<&str>) -> Result<Vec<Record14>> {
    let conn = open(root)?;
    let mut stmt = conn.prepare(
        "select handle,state,revision from records where kind=?1 and (?2 is null or owner_handle=?2) order by created_at,handle",
    )?;
    Ok(stmt
        .query_map(params![kind, owner], |row| {
            Ok(Record14 {
                handle: row.get(0)?,
                state: row.get(1)?,
                revision: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn resolve_all(conn: &Connection) -> Result<Vec<Resolution14>> {
    let mut work = conn.prepare(
        "select w.handle from records w where w.kind='work' and w.state in ('open','blocked') order by exists(select 1 from records a where a.kind='activation' and a.owner_handle=w.handle and a.state in ('active','suspended')) desc,w.created_at,w.handle",
    )?;
    let handles = work
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if handles.is_empty() {
        return Ok(vec![resolve_project(conn)?]);
    }
    handles
        .iter()
        .map(|handle| resolve_work(conn, handle))
        .collect()
}

fn resolve_project(conn: &Connection) -> Result<Resolution14> {
    if active_activation(conn)?.is_some() {
        return Ok(Resolution14 {
            owner_handle: "project:current".to_string(),
            owner_state: "active".to_string(),
            state_revision: state_revision(conn, "project:current")?,
            blocker: Some("active_work_exists".to_string()),
            legal_actions: vec!["continue active work".to_string()],
            selected_action: "continue active work".to_string(),
        });
    }
    Ok(Resolution14 {
        owner_handle: "project:current".to_string(),
        owner_state: "idle".to_string(),
        state_revision: state_revision(conn, "project:current")?,
        blocker: None,
        legal_actions: vec!["work start".to_string()],
        selected_action: "work start".to_string(),
    })
}

fn finding_actions(
    conn: &Connection,
    finding: &str,
    state: &str,
) -> Result<Vec<(&'static str, String)>> {
    let finding_id = finding.rsplit_once(':').map_or(finding, |(_, id)| id);
    let closure: Option<(String, String)> = conn
        .query_row(
            "select handle,state from records where kind='closure' and parent_handle=?1 and state in ('draft','ready') order by handle desc limit 1",
            params![finding],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if state == "open" {
        if let Some((closure, "draft")) = closure.as_ref().map(|(h, s)| (h.as_str(), s.as_str())) {
            let closure_id = closure.rsplit_once(':').map_or(closure, |(_, id)| id);
            return Ok(vec![
                (
                    "closure-ready",
                    format!("closure ready {closure_id} --evidence <evidence> --tests <tests>"),
                ),
                (
                    "supersede",
                    format!(
                        "closure supersede {closure_id} --invariant <contract> --surfaces <surfaces> --fix-plan <plan> --tests <tests> --verification <verification> --reason <reason> --expected-current {}",
                        decision_head(conn, closure)?
                    ),
                ),
                (
                    "accept",
                    format!(
                        "finding accept-out-of-scope {finding_id} --reason <reason> --risk <risk> --expected-current {}",
                        decision_head(conn, finding)?
                    ),
                ),
            ]);
        }
        return Ok(vec![
            (
                "closure-add",
                format!("closure add --finding {finding_id} --invariant <contract>"),
            ),
            (
                "remediate",
                format!("finding remediate {finding_id} --work <work-id>"),
            ),
            (
                "accept",
                format!(
                    "finding accept-out-of-scope {finding_id} --reason <reason> --risk <risk> --expected-current {}",
                    decision_head(conn, finding)?
                ),
            ),
        ]);
    }
    let (closure, _) = closure.context("awaiting finding has no ready closure")?;
    let closure_id = closure
        .rsplit_once(':')
        .map_or(closure.as_str(), |(_, id)| id);
    let attempt: String = conn.query_row(
        "select handle from records where kind='closure_attempt' and parent_handle=?1 and state='pending' order by handle desc limit 1",
        params![closure],
        |row| row.get(0),
    )?;
    let attempt_id = attempt
        .rsplit_once(':')
        .map_or(attempt.as_str(), |(_, id)| id);
    let claim: Option<String> = conn.query_row(
        "select handle from claims where kind='verification' and target_handle=?1 order by handle desc limit 1",
        params![attempt],
        |row| row.get(0),
    ).optional()?;
    if let Some(claim) = claim {
        let claim_id = claim.rsplit_once(':').map_or(claim.as_str(), |(_, id)| id);
        Ok(vec![(
            "verification-decide",
            format!(
                "finding decide {finding_id} --closure {closure_id} --attempt {attempt_id} --claim {claim_id} --decision <accept|reject|needs-evidence> --reason <reason> --expected-current {}",
                decision_head(conn, &attempt)?
            ),
        )])
    } else {
        Ok(vec![(
            "verification-claim",
            format!(
                "finding verify --finding {finding_id} --closure {closure_id} --attempt {attempt_id} --result <verified|not_fixed|needs_evidence>"
            ),
        )])
    }
}

fn resolve_work(conn: &Connection, work: &str) -> Result<Resolution14> {
    let state = record_state(conn, work, "work")?;
    let work_id = work.rsplit_once(':').map_or(work, |(_, id)| id);
    let activation_state: Option<String> = conn
        .query_row(
            "select state from records where kind='activation' and owner_handle=?1 and state in ('active','suspended') order by created_at desc limit 1",
            params![work],
            |row| row.get(0),
        )
        .optional()?;
    let critical_correction: Option<String> = conn
        .query_row(
            "select handle from records where kind='correction' and owner_handle=?1 and priority='critical' and state not in ('resolved','excepted') order by handle limit 1",
            params![work],
            |row| row.get(0),
        )
        .optional()?;
    let blocked_plan: Option<String> = conn
        .query_row(
            "select p.handle from records p where p.kind='review_plan' and p.owner_handle=?1 and p.required=1 and p.state='blocked' order by p.handle limit 1",
            params![work],
            |row| row.get(0),
        )
        .optional()?;
    let finding: Option<(String, String)> = conn
        .query_row(
            "select handle,state from records where kind='finding' and owner_handle=?1 and state not in ('closed','accepted_out_of_scope') order by handle limit 1",
            params![work],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (blocker, actions, selected) = if let Some(correction) = critical_correction {
        let id = correction
            .rsplit_once(':')
            .map_or(correction.as_str(), |(_, id)| id);
        let correction_state = record_state(conn, &correction, "correction")?;
        let command = match correction_state.as_str() {
            "recorded" => format!("correction link-requirement {id} --requirement <handle>"),
            "designed" => {
                let usage = current_passing_correction_usage(conn, &correction)?;
                if let Some(usage) = usage {
                    format!("correction link-validation {id} --usage {usage}")
                } else if let Some(profile) = conn.query_row(
                    "select handle from records where kind='command_profile' and state='fixed' and owner_handle=?1 order by handle limit 1",
                    params![work],
                    |row| row.get::<_, String>(0),
                ).optional()? {
                    let profile_id = profile
                        .rsplit_once(':')
                        .map_or(profile.as_str(), |(_, id)| id);
                    format!("command usage add --profile {profile_id} --command <command> --result pass --output-digest <digest>")
                } else if let Some(profile) = conn.query_row(
                    "select handle from records where kind='command_profile' and state in ('candidate','preferred') and owner_handle=?1 order by handle limit 1",
                    params![work],
                    |row| row.get::<_, String>(0),
                ).optional()? {
                    format!(
                        "command fix {} --reason <reason>",
                        profile.rsplit_once(':').map_or(profile.as_str(), |(_, id)| id)
                    )
                } else {
                    "command add --name <name> --command <command>".to_string()
                }
            }
            "validated" => format!("correction resolve {id} --reason <reason>"),
            _ => format!("correction inspect {id}"),
        };
        (
            Some("critical_correction".to_string()),
            vec![
                command.clone(),
                format!(
                    "correction except {id} --expected-current {} --reason <reason> --risk <risk>",
                    decision_head(conn, &correction)?
                ),
            ],
            command,
        )
    } else if let Some((finding, finding_state)) = finding {
        let actions = finding_actions(conn, &finding, &finding_state)?;
        let command = actions[0].1.clone();
        (
            Some("finding_obligation".to_string()),
            actions.into_iter().map(|(_, command)| command).collect(),
            command,
        )
    } else if let Some(plan) = blocked_plan {
        let head = decision_head(conn, &plan)?;
        let command = format!(
            "review plan waive {} --expected-current {} --reason <reason>",
            plan.rsplit_once(':').map_or(plan.as_str(), |(_, id)| id),
            head
        );
        (
            Some("blocked_review_plan".to_string()),
            vec![command.clone()],
            command,
        )
    } else {
        match (state.as_str(), activation_state.as_deref()) {
            ("blocked", _) => (
                Some("work_blocked".to_string()),
                vec![
                    format!("work unblock {work_id} --reason <reason>"),
                    format!("work abandon {work_id} --reason <reason>"),
                ],
                format!("work unblock {work_id} --reason <reason>"),
            ),
            ("open", Some("suspended")) => (
                Some("suspended".to_string()),
                vec![
                    "resume-check".to_string(),
                    format!("work abandon {work_id} --reason <reason>"),
                ],
                "resume-check".to_string(),
            ),
            ("open", Some("active")) => (
                None,
                vec![
                    "continue".to_string(),
                    "work suspend".to_string(),
                    "work close".to_string(),
                ],
                "continue".to_string(),
            ),
            ("open", None) => (
                None,
                vec![
                    format!("work activate {work_id}"),
                    format!("work abandon {work_id} --reason <reason>"),
                ],
                format!("work activate {work_id}"),
            ),
            _ => (Some("terminal".to_string()), Vec::new(), "none".to_string()),
        }
    };
    Ok(Resolution14 {
        owner_handle: work.to_string(),
        owner_state: state,
        state_revision: state_revision(conn, work)?,
        blocker,
        legal_actions: actions,
        selected_action: selected,
    })
}

fn require_selected(resolution: &Resolution14, action: &str) -> Result<()> {
    if resolution.selected_action != action {
        bail!(
            "action is not resolver-selected; selected action: {}",
            resolution.selected_action
        );
    }
    Ok(())
}

fn transition_record(
    root: &Path,
    target: &str,
    kind: &str,
    action: &str,
    next: &str,
    reason: &str,
) -> Result<Record14> {
    mutate(root, |tx| {
        update_record_state(tx, target, kind, action, next, reason)
    })
}

fn update_record_state(
    tx: &Transaction<'_>,
    target: &str,
    kind: &str,
    action: &str,
    next: &str,
    reason: &str,
) -> Result<Record14> {
    let owner = owning_work(tx, target)?;
    if kind != "activation" {
        ensure_active_owner(tx, &owner)?;
    }
    update_record_state_unowned(tx, target, kind, action, next, reason)
}

fn update_record_state_unowned(
    tx: &Transaction<'_>,
    target: &str,
    kind: &str,
    action: &str,
    next: &str,
    reason: &str,
) -> Result<Record14> {
    let (current, revision): (String, i64) = tx
        .query_row(
            "select state,revision from records where handle=?1 and kind=?2",
            params![target, kind],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("record not found")?;
    if !legal_transition(kind, &current, next) {
        bail!("illegal {kind} transition {current} -> {next}");
    }
    let now = now()?;
    tx.execute(
        "update records set state=?1,revision=revision+1,details=case when ?2='' then details else ?2 end,updated_at=?3 where handle=?4 and revision=?5",
        params![next, reason, now, target, revision],
    )?;
    insert_record_event(tx, target, action, Some(&current), next, revision + 1, &now)?;
    Ok(Record14 {
        handle: target.to_string(),
        state: next.to_string(),
        revision: revision + 1,
    })
}

fn legal_transition(kind: &str, from: &str, to: &str) -> bool {
    match kind {
        "task" => matches!(
            (from, to),
            ("open", "blocked")
                | ("blocked", "open")
                | ("open", "closed")
                | ("blocked", "closed")
                | ("open", "accepted_out_of_scope")
                | ("blocked", "accepted_out_of_scope")
        ),
        "phase" => matches!(
            (from, to),
            ("open", "blocked")
                | ("blocked", "open")
                | ("open", "closed")
                | ("open", "accepted_out_of_scope")
                | ("blocked", "accepted_out_of_scope")
        ),
        "work" => matches!(
            (from, to),
            ("open", "closed")
                | ("open", "blocked")
                | ("blocked", "open")
                | ("open", "abandoned")
                | ("blocked", "abandoned")
                | ("closed", "open")
                | ("abandoned", "open")
        ),
        "activation" => matches!(
            (from, to),
            ("active", "suspended")
                | ("suspended", "active")
                | ("active", "completed")
                | ("active", "abandoned")
                | ("suspended", "abandoned")
        ),
        "correction" => matches!(
            (from, to),
            ("recorded", "designed")
                | ("designed", "validated")
                | ("validated", "resolved")
                | ("recorded", "excepted")
                | ("designed", "excepted")
        ),
        "kpt_item" => matches!((from, to), ("open", "converted") | ("open", "dismissed")),
        "kpt_review" => matches!((from, to), ("open", "closed")),
        "command_profile" => matches!(
            (from, to),
            ("candidate", "preferred")
                | ("candidate", "fixed")
                | ("preferred", "fixed")
                | ("candidate", "deprecated")
                | ("preferred", "deprecated")
                | ("fixed", "deprecated")
        ),
        "finding" => matches!(
            (from, to),
            ("open", "awaiting_verification")
                | ("awaiting_verification", "closed")
                | ("awaiting_verification", "open")
                | ("open", "accepted_out_of_scope")
        ),
        "closure" => matches!(
            (from, to),
            ("draft", "ready")
                | ("ready", "draft")
                | ("ready", "verified")
                | ("draft", "superseded")
                | ("ready", "superseded")
        ),
        "closure_attempt" => matches!(
            (from, to),
            ("pending", "verified")
                | ("pending", "not_fixed")
                | ("pending", "needs_evidence")
                | ("pending", "claim_rejected")
                | ("pending", "superseded")
        ),
        "repository_change" => matches!(
            (from, to),
            ("unclassified", "classified") | ("unclassified", "accepted_exception")
        ),
        "repository_snapshot" => matches!(
            (from, to),
            ("draft", "recorded") | ("draft", "superseded") | ("recorded", "superseded")
        ),
        "work_record" => matches!(
            (from, to),
            ("draft", "complete") | ("draft", "superseded") | ("complete", "superseded")
        ),
        "design_package" | "design_version" => matches!(
            (from, to),
            ("draft", "approved") | ("draft", "superseded") | ("approved", "superseded")
        ),
        "checklist" | "checklist_item" => matches!(
            (from, to),
            ("open", "closed") | ("open", "accepted_out_of_scope")
        ),
        "acceptance" => matches!((from, to), ("approved", "revoked")),
        "stale_disposition" => matches!(
            (from, to),
            ("unresolved", "accepted") | ("unresolved", "closed")
        ),
        _ => false,
    }
}

#[derive(Debug)]
struct Component {
    kind: String,
    handle: String,
    state: Option<String>,
    revision: i64,
    digest: String,
}

fn semantic_components(conn: &Connection, work: &str) -> Result<Vec<Component>> {
    let mut stmt = conn.prepare(
        "with recursive owned(handle) as (select ?1 union select r.handle from records r join owned o on r.owner_handle=o.handle or r.parent_handle=o.handle) select kind,handle,state,revision,coalesce(content_digest,'') from records where handle in owned and kind!='activation' order by kind,handle",
    )?;
    let mut components = stmt
        .query_map(params![work], |row| {
            let kind: String = row.get(0)?;
            let handle: String = row.get(1)?;
            let state: String = row.get(2)?;
            let revision: i64 = row.get(3)?;
            let content: String = row.get(4)?;
            let digest =
                length_prefixed_digest(&[&kind, &handle, &state, &revision.to_string(), &content]);
            Ok(Component {
                kind: if handle == work {
                    "owner".to_string()
                } else if kind.starts_with("repository") || kind == "command_usage" {
                    "repository".to_string()
                } else {
                    "obligation".to_string()
                },
                handle,
                state: Some(state),
                revision,
                digest,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut relations = conn.prepare(
        "with recursive owned(handle) as (select ?1 union select r.handle from records r join owned o on r.owner_handle=o.handle or r.parent_handle=o.handle) select kind,handle,state,revision,source_handle,target_handle from relations where source_handle in owned or target_handle in owned order by kind,handle",
    )?;
    components.extend(
        relations
            .query_map(params![work], |row| {
                let kind: String = row.get(0)?;
                let handle: String = row.get(1)?;
                let state: String = row.get(2)?;
                let revision: i64 = row.get(3)?;
                let source: String = row.get(4)?;
                let target: String = row.get(5)?;
                Ok(Component {
                    kind: "obligation".into(),
                    handle,
                    state: Some(state.clone()),
                    revision,
                    digest: length_prefixed_digest(&[
                        &kind,
                        &state,
                        &revision.to_string(),
                        &source,
                        &target,
                    ]),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    );
    let mut evidence = conn.prepare(
        "with recursive owned(handle) as (select ?1 union select r.handle from records r join owned o on r.owner_handle=o.handle or r.parent_handle=o.handle) select 'claim',handle,outcome,target_revision,producer,scope_digest from claims where target_handle in owned union all select 'decision',handle,resulting_state,expected_target_revision,'owner',value from decisions where target_handle in owned union all select 'evidence',handle,result,subject_revision,producer,coalesce(content_digest,'') from evidence where owner_handle in owned or subject_handle in owned order by 1,2",
    )?;
    components.extend(
        evidence
            .query_map(params![work], |row| {
                let item_kind: String = row.get(0)?;
                let handle: String = row.get(1)?;
                let state: String = row.get(2)?;
                let revision: i64 = row.get(3)?;
                let producer: String = row.get(4)?;
                let digest_value: String = row.get(5)?;
                Ok(Component {
                    kind: "evidence".into(),
                    handle,
                    state: Some(state.clone()),
                    revision,
                    digest: length_prefixed_digest(&[
                        &item_kind,
                        &state,
                        &revision.to_string(),
                        &producer,
                        &digest_value,
                    ]),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    );
    components.sort_by(|left, right| (&left.kind, &left.handle).cmp(&(&right.kind, &right.handle)));
    Ok(components)
}

fn semantic_digest(components: &[Component]) -> String {
    let values = components
        .iter()
        .flat_map(|component| {
            [
                component.kind.as_str(),
                component.handle.as_str(),
                component.state.as_deref().unwrap_or(""),
                component.digest.as_str(),
            ]
        })
        .collect::<Vec<_>>();
    length_prefixed_digest(&values)
}

fn changed_components(
    conn: &Connection,
    snapshot: &str,
    current: &[Component],
) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for component in current {
        let old: Option<(String, Option<String>, String)> = conn
            .query_row(
                "select component_digest,component_state,component_revision from snapshot_components where snapshot_handle=?1 and component_kind=?2 and component_handle=?3",
                params![snapshot, component.kind, component.handle],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if old.as_ref().map(|item| item.0.as_str()) != Some(component.digest.as_str())
            && !terminal_change_is_explained(conn, snapshot, component, old.as_ref())?
        {
            changed.push(component.handle.clone());
        }
    }
    let current_handles = current
        .iter()
        .map(|item| item.handle.as_str())
        .collect::<Vec<_>>();
    let mut stmt = conn.prepare(
        "select component_handle from snapshot_components where snapshot_handle=?1 order by component_handle",
    )?;
    for handle in stmt
        .query_map(params![snapshot], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
    {
        if !current_handles.contains(&handle.as_str()) {
            changed.push(handle);
        }
    }
    changed.sort();
    changed.dedup();
    Ok(changed)
}

fn terminal_change_is_explained(
    conn: &Connection,
    snapshot: &str,
    component: &Component,
    old: Option<&(String, Option<String>, String)>,
) -> Result<bool> {
    if old.is_none() && component.kind == "evidence" && component.handle.starts_with("decision:") {
        let count: i64 = conn.query_row(
            "select count(*) from lifecycle_events e join snapshots s on s.handle=?1 join snapshot_components c on c.snapshot_handle=s.handle and c.component_handle=coalesce(e.target_handle,e.target_relation_handle) where e.decision_handle=?2 and e.created_at>s.created_at and e.from_revision=cast(c.component_revision as integer) and e.to_revision=e.from_revision+1 and e.to_state in ('closed','satisfied','accepted','waived','resolved','excepted','verified','accepted_out_of_scope')",
            params![snapshot, component.handle],
            |row| row.get(0),
        )?;
        return Ok(count == 1);
    }
    let Some((_, old_state, old_revision)) = old else {
        return Ok(false);
    };
    if component.kind != "obligation"
        || !matches!(
            component.state.as_deref(),
            Some(
                "closed"
                    | "satisfied"
                    | "accepted"
                    | "waived"
                    | "resolved"
                    | "excepted"
                    | "verified"
                    | "accepted_out_of_scope"
            )
        )
    {
        return Ok(false);
    }
    let prior_revision: i64 = old_revision.parse().context("invalid snapshot revision")?;
    let snapshot_created: String = conn.query_row(
        "select created_at from snapshots where handle=?1",
        params![snapshot],
        |row| row.get(0),
    )?;
    let count: i64 = conn.query_row(
        "select count(*) from lifecycle_events where (target_handle=?1 or target_relation_handle=?1) and from_state is ?2 and to_state=?3 and from_revision=?4 and to_revision=?5 and created_at>?6 and event_kind in ('close','satisfy','accept','accept-out-of-scope','waive','resolve','except','verified','owner_decision') and (event_kind!='owner_decision' or decision_handle is not null)",
        params![
            component.handle,
            old_state,
            component.state,
            prior_revision,
            component.revision,
            snapshot_created,
        ],
        |row| row.get(0),
    )?;
    Ok(count == 1 && component.revision == prior_revision + 1)
}

fn length_prefixed_digest(values: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn state_revision(conn: &Connection, owner: &str) -> Result<String> {
    if owner.starts_with("work:") {
        return Ok(semantic_digest(&semantic_components(conn, owner)?));
    }
    let mut stmt = conn.prepare(
        "select handle,state,revision from records where handle=?1 or owner_handle=?1 order by handle",
    )?;
    let rows = stmt
        .query_map(params![owner], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut hasher = Sha256::new();
    for (handle, state, revision) in rows {
        for value in [handle, state, revision.to_string()] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[allow(clippy::too_many_arguments)]
fn insert_record(
    tx: &Transaction<'_>,
    handle: &str,
    kind: &str,
    state: &str,
    owner: Option<&str>,
    parent: Option<&str>,
    key: Option<&str>,
    title: Option<&str>,
    priority: Option<&str>,
    details: Option<&str>,
) -> Result<()> {
    let now = now()?;
    tx.execute(
        "insert into records(handle,project_handle,kind,state,revision,owner_handle,parent_handle,record_key,title,priority,details,created_at,updated_at) values(?1,'project:current',?2,?3,1,?4,?5,?6,?7,?8,?9,?10,?10)",
        params![handle, kind, state, owner, parent, key, title, priority, details, now],
    )?;
    insert_record_event(tx, handle, "created", None, state, 1, &now)
}

fn insert_relation(
    tx: &Transaction<'_>,
    kind: &str,
    source: &str,
    target: &str,
    required: Option<bool>,
) -> Result<String> {
    let id = next_numeric_relation_id(tx, kind)?;
    let handle = format!("{kind}:{id}");
    let created = now()?;
    tx.execute(
        "insert into relations(handle,project_handle,kind,source_handle,target_handle,state,revision,required,created_at,updated_at) values(?1,'project:current',?2,?3,?4,'recorded',1,?5,?6,?6)",
        params![handle, kind, source, target, required.map(i64::from), created],
    )?;
    insert_relation_event(tx, &handle, "created", None, "recorded", 1, &created)?;
    Ok(handle)
}

#[allow(clippy::too_many_arguments)]
fn append_decision(
    tx: &Transaction<'_>,
    kind: &str,
    target: &str,
    claim: Option<&str>,
    value: &str,
    resulting_state: &str,
    expected_current: &str,
    reason: &str,
    risk: Option<&str>,
) -> Result<Decision14> {
    if reason.trim().is_empty() {
        bail!("owner decision requires a reason");
    }
    let head = decision_head(tx, target)?;
    if head != expected_current {
        bail!("decision head changed: expected {expected_current}, current {head}");
    }
    let target_revision = record_revision(tx, target)?;
    let id = next_decision_id(tx)?;
    let handle = format!("decision:{id}");
    let predecessor = (head != "none").then_some(head.as_str());
    let created = now()?;
    tx.execute(
        "insert into decisions(handle,project_handle,kind,target_handle,claim_handle,predecessor_handle,value,expected_target_revision,resulting_state,reason,risk,created_at) values(?1,'project:current',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![handle, kind, target, claim, predecessor, value, target_revision, resulting_state, reason, risk, created],
    )?;
    let current = record_state_any(tx, target)?;
    if current != resulting_state {
        tx.execute(
            "update records set state=?1,revision=revision+1,updated_at=?2 where handle=?3 and revision=?4",
            params![resulting_state, created, target, target_revision],
        )?;
        let event_id = next_event_id(tx)?;
        tx.execute(
            "insert into lifecycle_events(handle,project_handle,target_handle,decision_handle,event_kind,from_state,to_state,from_revision,to_revision,created_at) values(?1,'project:current',?2,?3,'owner_decision',?4,?5,?6,?7,?8)",
            params![format!("event:{event_id}"), target, handle, current, resulting_state, target_revision, target_revision + 1, created],
        )?;
    }
    Ok(Decision14 {
        handle,
        target_handle: target.into(),
        resulting_state: resulting_state.into(),
    })
}

fn append_relation_decision(
    tx: &Transaction<'_>,
    relation: &str,
    expected_current: &str,
    reason: &str,
    risk: Option<&str>,
) -> Result<Decision14> {
    let (state, revision): (String, i64) = tx.query_row(
        "select state,revision from relations where handle=?1 and kind in ('work_dependency','phase_dependency')",
        params![relation],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).context("dependency not found")?;
    if state != "open" {
        bail!("only an open dependency can be accepted");
    }
    let head = decision_head_relation(tx, relation)?;
    if head != expected_current {
        bail!("decision head changed: expected {expected_current}, current {head}");
    }
    let id = next_decision_id(tx)?;
    let handle = format!("decision:{id}");
    let predecessor = (head != "none").then_some(head.as_str());
    let created = now()?;
    tx.execute(
        "insert into decisions(handle,project_handle,kind,target_relation_handle,predecessor_handle,value,expected_target_revision,resulting_state,reason,risk,created_at) values(?1,'project:current','dependency',?2,?3,'accept',?4,'accepted',?5,?6,?7)",
        params![handle, relation, predecessor, revision, reason, risk, created],
    )?;
    transition_relation(tx, relation, revision, "accept", "accepted", reason)?;
    Ok(Decision14 {
        handle,
        target_handle: relation.into(),
        resulting_state: "accepted".into(),
    })
}

fn transition_relation(
    tx: &Transaction<'_>,
    relation: &str,
    revision: i64,
    action: &str,
    next: &str,
    details: &str,
) -> Result<()> {
    let current: String = tx.query_row(
        "select state from relations where handle=?1",
        params![relation],
        |row| row.get(0),
    )?;
    let created = now()?;
    tx.execute(
        "update relations set state=?1,revision=revision+1,details=?2,updated_at=?3 where handle=?4 and revision=?5",
        params![next, details, created, relation, revision],
    )?;
    insert_relation_event(
        tx,
        relation,
        action,
        Some(&current),
        next,
        revision + 1,
        &created,
    )
}

fn review_plan_blockers(conn: &Connection, phase: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "select p.handle,p.state from relations t join records p on p.handle=t.source_handle where t.kind='review_target' and t.target_handle=?1 and t.required=1 and p.kind='review_plan' and p.state not in ('clean','waived') order by p.handle",
    )?;
    let plans = stmt
        .query_map(params![phase], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    plans
        .into_iter()
        .map(|(plan, state)| {
            if state == "blocked" {
                let head = decision_head(conn, &plan)?;
                Ok(format!(
                    "review plan waive {} --expected-current {} --reason <reason>",
                    plan.rsplit_once(':').map_or(plan.as_str(), |(_, id)| id),
                    head
                ))
            } else {
                Ok(format!("review plan {} requires an owner decision", plan))
            }
        })
        .collect()
}

fn decision_head(conn: &Connection, target: &str) -> Result<String> {
    Ok(conn
        .query_row(
            "select handle from decisions where target_handle=?1 order by created_at desc,handle desc limit 1",
            params![target],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "none".to_string()))
}

fn decision_head_relation(conn: &Connection, target: &str) -> Result<String> {
    Ok(conn.query_row(
        "select handle from decisions where target_relation_handle=?1 order by created_at desc,handle desc limit 1",
        params![target],
        |row| row.get(0),
    ).optional()?.unwrap_or_else(|| "none".into()))
}

fn record_state_any(conn: &Connection, handle: &str) -> Result<String> {
    conn.query_row(
        "select state from records where handle=?1",
        params![handle],
        |row| row.get(0),
    )
    .context("record not found")
}

fn insert_record_event(
    tx: &Transaction<'_>,
    target: &str,
    event: &str,
    from: Option<&str>,
    to: &str,
    revision: i64,
    created: &str,
) -> Result<()> {
    let id = next_event_id(tx)?;
    tx.execute(
        "insert into lifecycle_events(handle,project_handle,target_handle,event_kind,from_state,to_state,from_revision,to_revision,created_at) values(?1,'project:current',?2,?3,?4,?5,?6,?7,?8)",
        params![format!("event:{id}"), target, event, from, to, if revision > 1 { Some(revision - 1) } else { None }, revision, created],
    )?;
    Ok(())
}

fn insert_relation_event(
    tx: &Transaction<'_>,
    target: &str,
    event: &str,
    from: Option<&str>,
    to: &str,
    revision: i64,
    created: &str,
) -> Result<()> {
    let id = next_event_id(tx)?;
    tx.execute(
        "insert into lifecycle_events(handle,project_handle,target_relation_handle,event_kind,from_state,to_state,from_revision,to_revision,created_at) values(?1,'project:current',?2,?3,?4,?5,?6,?7,?8)",
        params![format!("event:{id}"), target, event, from, to, if revision > 1 { Some(revision - 1) } else { None }, revision, created],
    )?;
    Ok(())
}

fn active_activation(conn: &Connection) -> Result<Option<(String, String)>> {
    conn.query_row(
        "select handle,owner_handle from records where kind='activation' and state='active' order by created_at desc limit 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

fn ensure_active_owner(conn: &Connection, work: &str) -> Result<()> {
    let active: bool = conn.query_row(
        "select exists(select 1 from records where kind='activation' and owner_handle=?1 and state='active')",
        params![work],
        |row| row.get(0),
    )?;
    if !active {
        bail!("work owner is not active");
    }
    Ok(())
}

fn record_state(conn: &Connection, handle: &str, kind: &str) -> Result<String> {
    conn.query_row(
        "select state from records where handle=?1 and kind=?2",
        params![handle, kind],
        |row| row.get(0),
    )
    .context("record not found")
}

fn record_revision(conn: &Connection, handle: &str) -> Result<i64> {
    conn.query_row(
        "select revision from records where handle=?1",
        params![handle],
        |row| row.get(0),
    )
    .context("record not found")
}

fn record_owner(conn: &Connection, handle: &str, kind: &str) -> Result<String> {
    conn.query_row(
        "select coalesce(owner_handle,handle) from records where handle=?1 and kind=?2",
        params![handle, kind],
        |row| row.get(0),
    )
    .context("record not found")
}

fn owning_work(conn: &Connection, handle: &str) -> Result<String> {
    let mut current = handle.to_string();
    for _ in 0..8 {
        let (kind, owner): (String, Option<String>) = conn
            .query_row(
                "select kind,owner_handle from records where handle=?1",
                params![current],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .context("record not found")?;
        if kind == "work" {
            return Ok(current);
        }
        current = owner.context("record has no work owner")?;
    }
    bail!("record ownership chain is too deep")
}

fn next_numeric_id(conn: &Connection, kind: &str) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,length(?1)+2) as integer)),0)+1 from records where kind=?1 and handle glob ?1||':[0-9]*'",
        params![kind],
        |row| row.get(0),
    )?)
}

fn next_numeric_relation_id(conn: &Connection, kind: &str) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,length(?1)+2) as integer)),0)+1 from relations where kind=?1 and handle glob ?1||':[0-9]*'",
        params![kind],
        |row| row.get(0),
    )?)
}

fn next_event_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,7) as integer)),0)+1 from lifecycle_events where handle glob 'event:[0-9]*'",
        [],
        |row| row.get(0),
    )?)
}

fn next_claim_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,7) as integer)),0)+1 from claims where handle glob 'claim:[0-9]*'",
        [],
        |row| row.get(0),
    )?)
}

fn next_decision_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,10) as integer)),0)+1 from decisions where handle glob 'decision:[0-9]*'",
        [],
        |row| row.get(0),
    )?)
}

fn next_evidence_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,10) as integer)),0)+1 from evidence where handle glob 'evidence:[0-9]*'",
        [],
        |row| row.get(0),
    )?)
}

fn next_snapshot_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "select coalesce(max(cast(substr(handle,10) as integer)),0)+1 from snapshots where handle glob 'snapshot:[0-9]*'",
        [],
        |row| row.get(0),
    )?)
}

fn mutate<T>(root: &Path, operation: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
    mutate_intent(root, MutationIntent14::Continue, operation)
}

fn mutate_intent<T>(
    root: &Path,
    intent: MutationIntent14,
    operation: impl FnOnce(&Transaction<'_>) -> Result<T>,
) -> Result<T> {
    let _lock = crate::update::acquire_project_lock(root)?;
    let mut conn = open_unlocked(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    authorize_mutation(&tx, &intent)?;
    let result = operation(&tx)?;
    tx.commit()?;
    Ok(result)
}

fn authorize_mutation(conn: &Connection, intent: &MutationIntent14) -> Result<()> {
    let explicit_work = match intent {
        MutationIntent14::Work { handle, .. } => Some(handle.clone()),
        MutationIntent14::Resume => conn
            .query_row(
                "select owner_handle from records where kind='activation' and state='suspended' order by created_at desc limit 1",
                [],
                |row| row.get(0),
            )
            .optional()?,
        MutationIntent14::Correction { handle, .. }
        | MutationIntent14::ReviewWaive { handle }
        | MutationIntent14::Finding { handle, .. } => Some(owning_work(conn, handle)?),
        MutationIntent14::Continue => active_activation(conn)?.map(|(_, work)| work),
        MutationIntent14::CorrectionValidationEvidence { profile } => {
            Some(owning_work(conn, profile)?)
        }
        MutationIntent14::CorrectionCommandProfile { work, .. } => Some(work.clone()),
        MutationIntent14::ReviewProgress { work, plan } => match plan {
            Some(plan) => Some(owning_work(conn, plan)?),
            None => Some(work.clone()),
        },
        MutationIntent14::Project { .. } => active_activation(conn)?.map(|(_, work)| work),
    };
    let Some(work) = explicit_work else {
        if let MutationIntent14::Project { action } = intent {
            let resolution = resolve_project(conn)?;
            if matches!(
                *action,
                "work-start" | "follow-up" | "design-import" | "design-approve"
            ) && resolution.blocker.is_none()
            {
                return Ok(());
            }
            bail!(
                "action is blocked by resolver; selected action: {}",
                resolution.selected_action
            );
        }
        let resolution = resolve_project(conn)?;
        bail!(
            "mutation requires an explicit owner; selected action: {}",
            resolution.selected_action
        );
    };
    let resolution = resolve_work(conn, &work)?;
    let permitted = match (&resolution.blocker, intent) {
        (None, _) => true,
        (Some(blocker), MutationIntent14::Correction { handle, action })
            if blocker == "critical_correction" =>
        {
            let selected: Option<String> = conn.query_row(
                "select handle from records where kind='correction' and owner_handle=?1 and priority='critical' and state not in ('resolved','excepted') order by handle limit 1",
                params![work],
                |row| row.get(0),
            ).optional()?;
            selected.as_deref() == Some(handle.as_str())
                && ((*action == "except")
                    || matches!(
                        (record_state(conn, handle, "correction")?.as_str(), *action),
                        ("recorded", "link-requirement")
                            | ("designed", "link-validation")
                            | ("validated", "resolve")
                    ))
        }
        (Some(blocker), MutationIntent14::CorrectionValidationEvidence { profile })
            if blocker == "critical_correction" =>
        {
            let state: Option<String> = conn.query_row(
                "select state from records where kind='correction' and owner_handle=?1 and priority='critical' and state not in ('resolved','excepted') order by handle limit 1",
                params![work],
                |row| row.get(0),
            ).optional()?;
            state.as_deref() == Some("designed")
                && record_state(conn, profile, "command_profile")? == "fixed"
                && resolution.selected_action.starts_with(&format!(
                    "command usage add --profile {} ",
                    profile
                        .rsplit_once(':')
                        .map_or(profile.as_str(), |(_, id)| id)
                ))
        }
        (
            Some(blocker),
            MutationIntent14::CorrectionCommandProfile {
                profile, action, ..
            },
        ) if blocker == "critical_correction" => match (profile, *action) {
            (None, "add") => resolution.selected_action.starts_with("command add "),
            (Some(profile), "fix") => {
                let id = profile
                    .rsplit_once(':')
                    .map_or(profile.as_str(), |(_, id)| id);
                resolution
                    .selected_action
                    .starts_with(&format!("command fix {id} "))
            }
            _ => false,
        },
        (Some(blocker), MutationIntent14::ReviewWaive { handle })
            if blocker == "blocked_review_plan" =>
        {
            let selected: Option<String> = conn.query_row(
                "select handle from records where kind='review_plan' and owner_handle=?1 and required=1 and state='blocked' order by handle limit 1",
                params![work],
                |row| row.get(0),
            ).optional()?;
            selected.as_deref() == Some(handle.as_str())
        }
        (Some(blocker), MutationIntent14::Finding { handle, action })
            if blocker == "finding_obligation" =>
        {
            let finding = finding_for_target(conn, handle)?;
            let selected: Option<String> = conn.query_row(
                "select handle from records where kind='finding' and owner_handle=?1 and state not in ('closed','accepted_out_of_scope') order by handle limit 1",
                params![work],
                |row| row.get(0),
            ).optional()?;
            selected.as_deref() == Some(finding.as_str())
                && finding_actions(conn, &finding, &record_state(conn, &finding, "finding")?)?
                    .iter()
                    .any(|(permitted, _)| permitted == action)
        }
        (Some(blocker), MutationIntent14::Work { action, .. }) if blocker == "work_blocked" => {
            matches!(*action, "unblock" | "abandon")
        }
        (Some(blocker), MutationIntent14::Work { action, .. }) if blocker == "suspended" => {
            *action == "abandon"
        }
        (Some(blocker), MutationIntent14::Work { action, .. }) if blocker == "terminal" => {
            *action == "reopen"
        }
        (Some(blocker), MutationIntent14::Resume) if blocker == "suspended" => true,
        _ => false,
    };
    if !permitted {
        bail!(
            "action is blocked by resolver; selected action: {}",
            resolution.selected_action
        );
    }
    Ok(())
}

fn finding_for_target(conn: &Connection, handle: &str) -> Result<String> {
    if handle.starts_with("finding:") {
        return Ok(handle.to_string());
    }
    let (kind, parent): (String, Option<String>) = conn.query_row(
        "select kind,parent_handle from records where handle=?1",
        params![handle],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    match (kind.as_str(), parent) {
        ("closure", Some(finding)) => Ok(finding),
        ("closure_attempt", Some(closure)) => finding_for_target(conn, &closure),
        _ => bail!("finding mutation target is outside a finding chain"),
    }
}

fn open(root: &Path) -> Result<RuntimeConnection14> {
    let lock = crate::update::acquire_project_read_lock(root)?;
    let conn = open_unlocked(root)?;
    Ok(RuntimeConnection14 { conn, _lock: lock })
}

fn open_unlocked(root: &Path) -> Result<Connection> {
    let path = ledger_path(root);
    let conn = Connection::open(path)?;
    crate::update::verify_schema14(&conn)?;
    conn.execute_batch("pragma foreign_keys=on;")?;
    Ok(conn)
}

fn ledger_path(root: &Path) -> PathBuf {
    root.join(".agent-workbench/ledger.sqlite")
}

fn now() -> Result<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_schema14_work_task_phase_suspend_resume_and_close_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        assert_eq!(
            status(temp.path()).unwrap().resolutions[0].selected_action,
            "work start"
        );
        let work = start_work(temp.path(), "ordinary workflow").unwrap();
        assert_eq!(work.handle, "work:1");
        let task = add_task(temp.path(), 1, "implement", "high", None).unwrap();
        let phase = create_phase(temp.path(), 1, "implementation", "Implementation", 1).unwrap();
        assert_eq!(assign_task(temp.path(), 1, 1).unwrap(), "membership:1");
        let snapshot = suspend(temp.path(), "pause", "resume").unwrap();
        assert_eq!(snapshot.handle, "snapshot:1");
        assert_eq!(resume_check(temp.path()).unwrap().result, "pass");
        assert_eq!(resume(temp.path()).unwrap().state, "active");
        assert_eq!(
            transition_task(temp.path(), 1, "close", "done")
                .unwrap()
                .state,
            "closed"
        );
        assert_eq!(
            transition_phase(temp.path(), 1, "close", "done")
                .unwrap()
                .state,
            "closed"
        );
        assert_eq!(close_work(temp.path(), "complete").unwrap().state, "closed");
        assert_eq!(task.handle, "task:1");
        assert_eq!(phase.handle, "phase:1");
    }

    #[test]
    fn update_lock_excludes_schema14_mutations_until_release() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "lock coordination").unwrap();
        let lock = crate::update::acquire_project_lock(temp.path()).unwrap();
        let error = add_task(temp.path(), 1, "blocked during update", "high", None).unwrap_err();
        assert!(error.to_string().contains("another Agent Workbench update"));
        drop(lock);
        assert_eq!(
            add_task(temp.path(), 1, "after update", "high", None)
                .unwrap()
                .state,
            "open"
        );
    }

    #[test]
    fn abandon_terminates_active_and_suspended_activation_and_task_can_close_from_blocked() {
        for suspend_first in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            crate::update::init_fresh(temp.path()).unwrap();
            start_work(temp.path(), "abandon lifecycle").unwrap();
            add_task(temp.path(), 1, "blocked task", "high", None).unwrap();
            transition_task(temp.path(), 1, "block", "waiting").unwrap();
            assert_eq!(
                transition_task(temp.path(), 1, "close", "completed while blocked")
                    .unwrap()
                    .state,
                "closed"
            );
            if suspend_first {
                suspend(temp.path(), "pause", "abandon").unwrap();
            }
            assert_eq!(
                transition_work(temp.path(), 1, "abandon", "stop work")
                    .unwrap()
                    .state,
                "abandoned"
            );
            let status = status(temp.path()).unwrap();
            assert_eq!(status.active_activations, 0);
            assert_eq!(status.resolutions[0].selected_action, "work start");
            assert_eq!(
                transition_work(temp.path(), 1, "reopen", "resume later")
                    .unwrap()
                    .state,
                "open"
            );
        }
    }

    #[test]
    fn semantic_resume_reports_real_task_revision_changes() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "resume delta").unwrap();
        add_task(temp.path(), 1, "task", "medium", None).unwrap();
        suspend(temp.path(), "pause", "resume").unwrap();
        let conn = open(temp.path()).unwrap();
        conn.execute(
            "update records set state='blocked',revision=revision+1 where handle='task:1'",
            [],
        )
        .unwrap();
        let check = resume_check(temp.path()).unwrap();
        assert_eq!(check.result, "blocked");
        assert_eq!(check.changed_components, vec!["task:1"]);
    }

    #[test]
    fn semantic_resume_accepts_one_exact_terminal_child_event() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "terminal resume").unwrap();
        add_task(temp.path(), 1, "task", "medium", None).unwrap();
        suspend(temp.path(), "pause", "resume").unwrap();
        let conn = open(temp.path()).unwrap();
        conn.execute(
            "update records set state='closed',revision=2,updated_at='9999-01-01T00:00:00Z' where handle='task:1' and revision=1",
            [],
        )
        .unwrap();
        conn.execute(
            "insert into lifecycle_events(handle,project_handle,target_handle,event_kind,from_state,to_state,from_revision,to_revision,created_at) values('event:terminal-resume','project:current','task:1','close','open','closed',1,2,'9999-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        drop(conn);
        let check = resume_check(temp.path()).unwrap();
        assert_eq!(check.result, "pass");
        assert!(check.changed_components.is_empty());
        assert_eq!(resume(temp.path()).unwrap().state, "active");
    }

    #[test]
    fn blocked_required_plan_has_one_resolver_selected_waiver_exit() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "review lifecycle").unwrap();
        create_phase(temp.path(), 1, "review", "Review", 1).unwrap();
        add_review_policy(temp.path(), 1, "one reviewer", 1).unwrap();
        add_review_plan(temp.path(), 1, "phase", 1, Some(1), true).unwrap();
        add_review_claim(
            temp.path(),
            1,
            "changes-required",
            "external:one",
            "scope-a",
            None,
        )
        .unwrap();
        add_review_plan(temp.path(), 1, "phase", 1, Some(1), true).unwrap();
        add_review_claim(temp.path(), 2, "clean", "external:two", "scope-b", None).unwrap();
        decide_review(temp.path(), 2, 2, "accept", "none", "accept clean review").unwrap();
        decide_review(temp.path(), 1, 1, "accept", "none", "accept review result").unwrap();
        let blocked = add_task(temp.path(), 1, "unrelated", "low", None).unwrap_err();
        assert!(blocked.to_string().contains("review plan waive 1"));
        assert!(add_review_plan(temp.path(), 1, "phase", 1, Some(1), true).is_err());

        let resolution = status(temp.path()).unwrap().resolutions.remove(0);
        let (_, blockers) = phase_close_ready(temp.path(), 1).unwrap();
        assert_eq!(resolution.blocker.as_deref(), Some("blocked_review_plan"));
        assert_eq!(resolution.legal_actions, blockers);
        assert_eq!(resolution.selected_action, blockers[0]);

        let head = decision_head_for(temp.path(), "review_plan:1").unwrap();
        let waiver = waive_review_plan(
            temp.path(),
            1,
            &head,
            "fresh clean sibling covers the required scope",
            Some("review plan one remains historical evidence"),
        )
        .unwrap();
        assert_eq!(waiver.resulting_state, "waived");
        assert!(phase_close_ready(temp.path(), 1).unwrap().0);
        let plans = list_records(temp.path(), "review_plan", None).unwrap();
        assert_eq!(plans[0].state, "waived");
        assert_eq!(plans[1].state, "clean");
    }

    #[test]
    fn critical_correction_requires_requirement_and_fixed_usage_while_kpt_has_no_effect() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        let package = temp.path().join("correction-design");
        fs::create_dir_all(package.join("requirements")).unwrap();
        fs::write(
            package.join("design.yaml"),
            "id: correction\ntitle: Correction\nformat: arc42\nversion: 1\nstatus: draft\narc42: {}\nrequirements: [requirements/main.md]\nvalidation: []\n",
        )
        .unwrap();
        fs::write(
            package.join("requirements/main.md"),
            "# Requirements\n\n## Correction\n```yaml agent-workbench\ntype: requirement\nkey: REQ-CORRECTION\npriority: critical\nstatus: active\n```\n\nPrevent regression.\n",
        )
        .unwrap();
        import_design14(temp.path(), &package, "draft").unwrap();
        approve_design14(temp.path(), 1, "approved correction requirement").unwrap();
        start_work(temp.path(), "correction lifecycle").unwrap();
        start_kpt(temp.path(), 1, "review failure").unwrap();
        add_kpt_item(temp.path(), 1, "problem", "regression", "critical").unwrap();
        transition_kpt_item(temp.path(), 1, "convert", "create follow-up").unwrap();
        close_kpt(temp.path(), 1).unwrap();
        add_correction(
            temp.path(),
            1,
            "prevent regression",
            "critical",
            "enforce it",
        )
        .unwrap();
        let before = list_records(temp.path(), "task", None).unwrap().len();
        let blocked = add_task(temp.path(), 1, "unrelated", "low", None).unwrap_err();
        assert!(
            blocked
                .to_string()
                .contains("correction link-requirement 1")
        );
        assert_eq!(
            list_records(temp.path(), "task", None).unwrap().len(),
            before
        );
        assert!(!work_close_ready(temp.path()).unwrap().0);
        assert_eq!(
            status(temp.path()).unwrap().resolutions[0].selected_action,
            "correction link-requirement 1 --requirement <handle>"
        );

        link_correction_requirement(temp.path(), 1, "requirement:1").unwrap();
        assert!(
            status(temp.path()).unwrap().resolutions[0]
                .selected_action
                .starts_with("command add --name")
        );
        add_command_profile(temp.path(), 1, "test", "cargo test").unwrap();
        assert!(
            status(temp.path()).unwrap().resolutions[0]
                .selected_action
                .starts_with("command fix 1 --reason")
        );
        transition_command_profile(temp.path(), 1, "fix", "approved validation").unwrap();
        {
            let conn = open(temp.path()).unwrap();
            let created = now().unwrap();
            conn.execute(
                "insert into records(handle,project_handle,kind,state,revision,owner_handle,record_key,title,details,created_at,updated_at) values('command_profile:2','project:current','command_profile','fixed',2,'work:1','other','other','cargo check',?1,?1)",
                params![created],
            )
            .unwrap();
        }
        assert!(
            status(temp.path()).unwrap().resolutions[0]
                .selected_action
                .starts_with("command usage add --profile 1")
        );
        assert!(add_command_usage(temp.path(), 2, "cargo check", "pass", "wrong-profile").is_err());
        add_command_usage(temp.path(), 1, "cargo test", "fail", "failed-output").unwrap();
        assert!(link_correction_validation(temp.path(), 1, "command_usage:1").is_err());
        assert!(
            status(temp.path()).unwrap().resolutions[0]
                .selected_action
                .starts_with("command usage add --profile 1")
        );
        add_command_usage(temp.path(), 1, "cargo test", "pass", "output-digest").unwrap();
        link_correction_validation(temp.path(), 1, "command_usage:2").unwrap();
        assert!(!work_close_ready(temp.path()).unwrap().0);
        resolve_correction(temp.path(), 1, "validated behavior is in use").unwrap();
        transition_command_profile(temp.path(), 2, "deprecate", "unused profile").unwrap();
        assert!(work_close_ready(temp.path()).unwrap().0);
    }

    #[test]
    fn verification_claim_matrix_only_accept_verified_closes_finding() {
        for (adjudication, outcome, attempt_state, finding_state, closure_state) in [
            ("accept", "verified", "verified", "closed", "verified"),
            ("accept", "not_fixed", "not_fixed", "open", "draft"),
            (
                "accept",
                "needs_evidence",
                "needs_evidence",
                "awaiting_verification",
                "ready",
            ),
            (
                "reject",
                "verified",
                "claim_rejected",
                "awaiting_verification",
                "ready",
            ),
            (
                "reject",
                "not_fixed",
                "claim_rejected",
                "awaiting_verification",
                "ready",
            ),
            (
                "needs-evidence",
                "verified",
                "needs_evidence",
                "awaiting_verification",
                "ready",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            crate::update::init_fresh(temp.path()).unwrap();
            start_work(temp.path(), "verification matrix").unwrap();
            add_finding(temp.path(), 1, "high", "broken behavior").unwrap();
            add_closure(temp.path(), 1, "exact closure contract").unwrap();
            ready_closure(temp.path(), 1, "tests and evidence").unwrap();
            add_verification_claim(temp.path(), 1, outcome, "external", "scope", None).unwrap();
            decide_verification(
                temp.path(),
                1,
                1,
                1,
                1,
                adjudication,
                "none",
                "owner adjudication",
            )
            .unwrap();
            assert_eq!(
                list_records(temp.path(), "closure_attempt", None).unwrap()[0].state,
                attempt_state
            );
            assert_eq!(
                list_records(temp.path(), "finding", None).unwrap()[0].state,
                finding_state
            );
            assert_eq!(
                list_records(temp.path(), "closure", None).unwrap()[0].state,
                closure_state
            );
        }
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "invalid verification outcome").unwrap();
        add_finding(temp.path(), 1, "high", "broken behavior").unwrap();
        add_closure(temp.path(), 1, "exact closure contract").unwrap();
        ready_closure(temp.path(), 1, "tests and evidence").unwrap();
        assert!(
            add_verification_claim(temp.path(), 1, "unknown", "external", "scope", None).is_err()
        );
    }

    #[test]
    fn finding_mutations_are_exactly_the_resolver_legal_set() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "finding resolver").unwrap();
        add_finding(temp.path(), 1, "high", "broken behavior").unwrap();
        let resolution = status(temp.path()).unwrap().resolutions.remove(0);
        assert!(
            resolution
                .legal_actions
                .iter()
                .any(|action| action.starts_with("closure add "))
        );
        add_closure(temp.path(), 1, "restore invariant").unwrap();
        let blocked = remediate_finding(temp.path(), 1, 1, false).unwrap_err();
        assert!(blocked.to_string().contains("closure ready 1"));
        ready_closure(temp.path(), 1, "tests and evidence").unwrap();
        add_verification_claim(temp.path(), 1, "verified", "external", "scope", None).unwrap();
        let duplicate =
            add_verification_claim(temp.path(), 1, "verified", "external-two", "scope", None)
                .unwrap_err();
        assert!(duplicate.to_string().contains("finding decide 1"));
    }

    #[test]
    fn repository_snapshot_counts_only_after_all_changes_are_terminal() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "repository evidence").unwrap();
        add_repository(temp.path(), 1, "main", ".").unwrap();
        add_repository_snapshot(temp.path(), 1, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .unwrap();
        add_repository_change(temp.path(), 1, "src/lib.rs", "digest-a").unwrap();
        add_repository_change(temp.path(), 1, "README.md", "digest-b").unwrap();
        classify_repository_change(temp.path(), 1, "implementation").unwrap();
        assert!(finalize_repository_snapshot(temp.path(), 1).is_err());
        accept_repository_change(temp.path(), 2, "none", "expected docs change", "low").unwrap();
        finalize_repository_snapshot(temp.path(), 1).unwrap();
        assert!(work_close_ready(temp.path()).unwrap().0);
        assert!(classify_repository_change(temp.path(), 2, "documentation").is_err());
    }

    #[test]
    fn dependencies_have_satisfy_and_reasoned_accept_exits() {
        let temp = tempfile::tempdir().unwrap();
        crate::update::init_fresh(temp.path()).unwrap();
        start_work(temp.path(), "first").unwrap();
        close_work(temp.path(), "done").unwrap();
        follow_up_work(temp.path(), 1, "second", "continue").unwrap();
        add_dependency(
            temp.path(),
            "work_dependency",
            "work:2",
            "work:1",
            "requires first",
        )
        .unwrap();
        assert!(!work_close_ready(temp.path()).unwrap().0);
        satisfy_dependency(temp.path(), "work_dependency:1", "predecessor closed").unwrap();
        assert!(work_close_ready(temp.path()).unwrap().0);

        create_phase(temp.path(), 2, "one", "One", 1).unwrap();
        create_phase(temp.path(), 2, "two", "Two", 2).unwrap();
        add_dependency(
            temp.path(),
            "phase_dependency",
            "phase:2",
            "phase:1",
            "ordered",
        )
        .unwrap();
        accept_dependency(
            temp.path(),
            "phase_dependency:1",
            "none",
            "safe exception",
            Some("low"),
        )
        .unwrap();
        assert!(phase_close_ready(temp.path(), 2).unwrap().0);
    }
}
