use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use super::{correction_state::*, *};

const BASE64URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(crate) fn encode_opaque_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        encoded.push(BASE64URL_ALPHABET[(first >> 2) as usize] as char);
        let second_index = (first & 0x03) << 4 | chunk.get(1).copied().unwrap_or(0) >> 4;
        encoded.push(BASE64URL_ALPHABET[second_index as usize] as char);
        if let Some(second) = chunk.get(1).copied() {
            let third_index = (second & 0x0f) << 2 | chunk.get(2).copied().unwrap_or(0) >> 6;
            encoded.push(BASE64URL_ALPHABET[third_index as usize] as char);
        }
        if let Some(third) = chunk.get(2).copied() {
            encoded.push(BASE64URL_ALPHABET[(third & 0x3f) as usize] as char);
        }
    }
    format!("b64:{encoded}")
}

#[cfg(test)]
pub(crate) fn encode_opaque_task_ref(value: &str) -> String {
    format!("@task/{}", encode_opaque_component(value))
}

pub(crate) fn decode_opaque_component(value: &str, label: &str) -> Result<String> {
    let encoded = value
        .strip_prefix("b64:")
        .with_context(|| format!("{label} requires a b64: canonical opaque component"))?;
    if encoded.is_empty() || encoded.len() % 4 == 1 {
        bail!("{label} has an invalid base64url length");
    }
    let mut bytes = Vec::with_capacity(encoded.len() * 3 / 4);
    let mut accumulator = 0_u16;
    let mut bit_count = 0_u8;
    for byte in encoded.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => bail!("{label} is not unpadded base64url"),
        };
        accumulator = accumulator << 6 | u16::from(value);
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            bytes.push((accumulator >> bit_count) as u8);
            accumulator &= (1_u16 << bit_count) - 1;
        }
    }
    if accumulator != 0 {
        bail!("{label} has noncanonical trailing bits");
    }
    let decoded =
        String::from_utf8(bytes).with_context(|| format!("{label} is not valid UTF-8"))?;
    if encode_opaque_component(&decoded) != value {
        bail!("{label} is not canonically encoded");
    }
    Ok(decoded)
}

pub(super) fn decode_opaque_task_ref(value: &str) -> Result<Option<String>> {
    let Some(component) = value.strip_prefix("@task/") else {
        return Ok(None);
    };
    decode_opaque_component(component, "task reference").map(Some)
}

pub(crate) fn parse_decomposition_reconciliation_target(
    target: &str,
) -> Result<(i64, i64, String)> {
    let parts = target.split('/').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!(
            "decomposition-plan-reconcile target requires design/work/b64:<project-relative-plan-path>"
        );
    }
    let design = parse_positive_decimal(parts[0])?;
    let work = parse_positive_decimal(parts[1])?;
    let path = decode_opaque_component(parts[2], "Decomposition Plan path")?;
    validate_project_relative_markdown_path(&path)?;
    Ok((design, work, path))
}

fn parse_positive_decimal(value: &str) -> Result<i64> {
    let parsed = value.parse::<i64>()?;
    if parsed <= 0 || value != parsed.to_string() {
        bail!("transition ids and order must be positive")
    }
    Ok(parsed)
}

fn validate_project_relative_markdown_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || value.contains('\\')
        || !value.ends_with(".md")
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        bail!("Decomposition Plan path must be a contained project-relative Markdown path");
    }
    Ok(())
}

pub(crate) fn parse_correction_tokens(surfaces: &str) -> Result<Vec<CorrectionToken>> {
    let mut parsed = Vec::new();
    let mut phase_aliases = Vec::<String>::new();
    let mut has_decomposition = false;
    let mut transition_effects = HashSet::<String>::new();
    for raw in surfaces.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            bail!("correction surfaces contain an empty token");
        }
        if let Some(rest) = token.strip_prefix("transition:") {
            let (verb, target) = rest
                .split_once(':')
                .context("transition token requires transition:<verb>:<target>")?;
            if !matches!(
                verb,
                "design-decompose"
                    | "design-reconcile"
                    | "decomposition-plan-reconcile"
                    | "task-accept-out-of-scope"
                    | "phase-create"
                    | "phase-assign"
                    | "phase-dependency-add"
                    | "phase-dependency-satisfy"
                    | "phase-dependency-accept"
                    | "stale-accept"
                    | "stale-close"
            ) || target.trim().is_empty()
            {
                bail!("unsupported correction transition token: {token}");
            }
            validate_correction_transition_target(verb, target, has_decomposition, &phase_aliases)?;
            let effect_key = if verb == "phase-create" {
                let parts = target.split('/').collect::<Vec<_>>();
                format!(
                    "phase-create:{}/{}/{}/{}/{}",
                    parts[0], parts[1], parts[3], parts[4], parts[5]
                )
            } else {
                format!("{verb}:{target}")
            };
            if !transition_effects.insert(effect_key) {
                bail!("duplicate correction transition effect is not allowed");
            }
            if matches!(
                verb,
                "design-decompose" | "design-reconcile" | "decomposition-plan-reconcile"
            ) {
                has_decomposition = true;
            }
            if verb == "phase-create" {
                phase_aliases.push(target.split('/').nth(2).unwrap().to_string());
            }
            parsed.push(CorrectionToken {
                kind: "transition".to_string(),
                operation: verb.to_string(),
                target: target.to_string(),
            });
            continue;
        }
        let mut parts = token.splitn(3, ':');
        let kind = parts.next().unwrap_or_default();
        let operation = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or_default();
        if kind == "repository" {
            bail!(
                "repository surfaces are not source-correction authority; use a scoped implementation remediation closure"
            );
        }
        if !matches!(kind, "design" | "plan" | "docs" | "workflow")
            || !matches!(operation, "edit" | "create" | "delete")
            || target.is_empty()
            || !target.ends_with(".md")
            || target.starts_with('/')
            || target.contains('\\')
            || target
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
            || !target
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
        {
            bail!("invalid typed correction surface: {token}");
        }
        match kind {
            "plan" if !target.starts_with("plans/") => bail!("plan surface must be below plans/"),
            "docs" if target != "README.md" && !target.starts_with("docs/") => {
                bail!("docs surface must be README.md or below docs/")
            }
            "workflow"
                if !target.starts_with(".agents/skills/agent-workbench/")
                    && !target.starts_with("skills/agent-workbench/") =>
            {
                bail!("workflow surface must be inside the Agent Workbench skill")
            }
            _ => {}
        }
        parsed.push(CorrectionToken {
            kind: "file".to_string(),
            operation: operation.to_string(),
            target: format!("{kind}:{target}"),
        });
    }
    Ok(parsed)
}

pub(super) fn declares_typed_correction(surfaces: &str) -> bool {
    surfaces.split(',').all(|raw| {
        let token = raw.trim();
        ["transition:", "design:", "plan:", "docs:", "workflow:"]
            .iter()
            .any(|prefix| token.starts_with(prefix))
    })
}

pub(super) fn validate_correction_transition_target(
    verb: &str,
    target: &str,
    has_decomposition: bool,
    phase_aliases: &[String],
) -> Result<()> {
    let positive = parse_positive_decimal;
    let valid_phase_ref = |value: &str| {
        phase_aliases.iter().any(|alias| alias == value)
            || value.parse::<i64>().is_ok_and(|id| id > 0)
    };
    match verb {
        "design-decompose" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 2 {
                bail!("design-decompose target requires design/work")
            }
            positive(parts[0])?;
            positive(parts[1])?;
        }
        "decomposition-plan-reconcile" => {
            parse_decomposition_reconciliation_target(target)?;
        }
        "design-reconcile" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 3 {
                bail!("design-reconcile target requires design/work/canonical-checklist")
            }
            positive(parts[0])?;
            positive(parts[1])?;
            if parts[2] == "@checklist" {
                if !has_decomposition {
                    bail!(
                        "@checklist requires an earlier design decomposition or reconciliation token"
                    )
                }
            } else {
                positive(parts[2])?;
            }
        }
        "task-accept-out-of-scope" => {
            if decode_opaque_task_ref(target)?.is_some() {
                if !has_decomposition {
                    bail!(
                        "opaque task reference requires an earlier design decomposition or reconciliation token"
                    )
                }
            } else if target.starts_with("@task/") {
                bail!("task reference requires canonical @task/b64:<opaque-key> syntax")
            } else {
                positive(target)?;
            }
        }
        "phase-create" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 6 {
                bail!("phase-create target requires work/design/alias/kind/order/key")
            }
            positive(parts[0])?;
            positive(parts[1])?;
            positive(parts[4])?;
            let alias_key = parts[2].strip_prefix('@').unwrap_or_default();
            if alias_key.is_empty()
                || !alias_key.chars().all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_')
                })
                || parts[3].trim().is_empty()
                || parts[5].is_empty()
                || !parts[5].chars().all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_')
                })
            {
                bail!("phase-create alias, kind, or key is invalid")
            }
            if phase_aliases.iter().any(|alias| alias == parts[2]) {
                bail!("phase-create alias is duplicated")
            }
        }
        "phase-assign" => {
            let (phase, task) = target
                .split_once('/')
                .context("phase-assign target requires phase/task")?;
            if !valid_phase_ref(phase) {
                bail!("phase assignment requires an earlier same-closure phase alias")
            }
            if decode_opaque_task_ref(task)?.is_some() {
                if !has_decomposition {
                    bail!(
                        "opaque task reference requires an earlier design decomposition or reconciliation token"
                    )
                }
            } else if task.starts_with("@task/") {
                bail!("task reference requires canonical @task/b64:<opaque-key> syntax")
            } else {
                positive(task)?;
            }
        }
        "phase-dependency-add" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 3
                || !valid_phase_ref(parts[0])
                || !valid_phase_ref(parts[1])
                || !matches!(parts[2], "blocks" | "requires")
            {
                bail!("phase dependency requires earlier phase aliases and blocks|requires")
            }
        }
        "phase-dependency-satisfy" | "phase-dependency-accept" => {
            positive(target)?;
        }
        "stale-accept" | "stale-close" => {
            let (kind, id) = target
                .split_once('/')
                .context("stale target requires type/id")?;
            if !matches!(
                kind,
                "task_derivation"
                    | "checklist"
                    | "validation_gate"
                    | "coverage_item"
                    | "review_plan"
            ) {
                bail!("invalid stale record type")
            }
            positive(id)?;
        }
        _ => bail!("unsupported correction transition {verb}"),
    }
    Ok(())
}

pub(crate) fn validate_correction_surfaces(surfaces: &str) -> Result<()> {
    let tokens = parse_correction_tokens(surfaces)?;
    if tokens.is_empty() {
        bail!("correction contract has no typed surfaces");
    }
    Ok(())
}

pub(super) fn correction_design_root(
    conn: &rusqlite::Connection,
    finding_id: i64,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        select dp.root_path
        from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        left join design_versions dv on dv.id = p.design_version_id
        left join design_packages dp on dp.id = dv.design_package_id
        where f.id = ?1
        "#,
        params![finding_id],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.flatten())
    .map_err(Into::into)
}

pub(super) fn stale_contract_tuple(
    conn: &rusqlite::Connection,
    project_id: i64,
    kind: &str,
    record_id: i64,
) -> Result<(i64, i64, i64, i64)> {
    let (rank, design_id, work_id) = match kind {
        "task_derivation" => conn.query_row(
            r#"select 0, r.design_version_id, coalesce(t.work_unit_id,0)
               from task_derivations td join design_requirements r on r.id=td.design_requirement_id
               join tasks t on t.id=td.task_id where td.id=?1 and td.project_id=?2"#,
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "checklist" => conn.query_row(
            "select 1, design_version_id, work_unit_id from checklists where id=?1 and project_id=?2",
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "validation_gate" => conn.query_row(
            r#"select 2, coalesce(r.design_version_id,0), coalesce(vg.work_unit_id,t.work_unit_id,0)
               from validation_gates vg left join design_requirements r on r.id=vg.design_requirement_id
               left join tasks t on t.id=vg.task_id where vg.id=?1 and vg.project_id=?2"#,
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "coverage_item" => conn.query_row(
            r#"select 3, r.design_version_id, coalesce(c.work_unit_id,t.work_unit_id,0)
               from coverage_items c join design_requirements r on r.id=c.design_requirement_id
               left join tasks t on t.id=c.task_id where c.id=?1 and c.project_id=?2"#,
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "review_plan" => conn.query_row(
            "select 4, coalesce(design_version_id,0), work_unit_id from review_plans where id=?1 and project_id=?2",
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        _ => bail!("invalid stale record type"),
    }
    .optional()?
    .context("declared stale record does not exist in this project")?;
    Ok((rank, design_id, work_id, record_id))
}

pub(super) fn validate_declared_stale_order(
    conn: &rusqlite::Connection,
    project_id: i64,
    tokens: &[CorrectionToken],
) -> Result<()> {
    let mut previous = None;
    for token in tokens.iter().filter(|token| {
        token.kind == "transition"
            && matches!(token.operation.as_str(), "stale-accept" | "stale-close")
    }) {
        let (kind, record_id) = token
            .target
            .split_once('/')
            .context("stale target requires type/id")?;
        let tuple = stale_contract_tuple(conn, project_id, kind, record_id.parse()?)?;
        if previous.is_some_and(|prior| tuple <= prior) {
            bail!("declared stale transition tokens must be in ascending global tuple order");
        }
        previous = Some(tuple);
    }
    Ok(())
}

pub(super) fn validate_correction_transition_preflight(
    conn: &rusqlite::Connection,
    project_id: i64,
    closure_id: i64,
    finding_id: i64,
) -> Result<()> {
    let (work_unit_id, design_version_id): (i64, Option<i64>) = conn.query_row(
        r#"select p.work_unit_id, p.design_version_id
           from findings f join review_runs r on r.id=f.review_run_id
           join review_plans p on p.id=r.review_plan_id where f.id=?1"#,
        params![finding_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut stmt = conn.prepare(
        "select token_ordinal, operation, target from correction_tokens where closure_id=?1 and token_kind='transition' order by token_ordinal",
    )?;
    let rows = stmt.query_map(params![closure_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let transitions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (token_ordinal, operation, target) in transitions {
        match operation.as_str() {
            "design-decompose" => {
                let (design, work) = parse_pair(&target)?;
                if work != work_unit_id
                    || !correction_design_accepts(conn, design_version_id, design)?
                {
                    bail!("design-decompose target is outside the correction owner or design");
                }
                crate::traceability::validate_design_decomposition_in(
                    conn, project_id, design, work,
                )?;
            }
            "decomposition-plan-reconcile" => {
                let (design, work, _) = parse_decomposition_reconciliation_target(&target)?;
                if work != work_unit_id
                    || !correction_design_accepts(conn, design_version_id, design)?
                {
                    bail!(
                        "decomposition-plan-reconcile target is outside the correction owner or design"
                    );
                }
            }
            "design-reconcile" => {
                let parts = target.split('/').collect::<Vec<_>>();
                let design = parts[0].parse::<i64>()?;
                let work = parts[1].parse::<i64>()?;
                if work != work_unit_id
                    || !correction_design_accepts(conn, design_version_id, design)?
                {
                    bail!("design-reconcile target is outside the correction owner or design");
                }
                crate::traceability::validate_design_decomposition_scope_in(
                    conn, project_id, design, work,
                )?;
                if parts[2] == "@checklist" {
                    let preceding_owner: bool = conn.query_row(
                        r#"
                        select exists(
                          select 1 from correction_tokens prior
                          where prior.closure_id=?1 and prior.token_kind='transition'
                            and prior.token_ordinal<?2
                            and prior.operation in ('design-decompose','design-reconcile')
                            and (
                              prior.target=?3
                              or prior.target like ?3||'/%'
                            )
                        )
                        "#,
                        params![closure_id, token_ordinal, format!("{design}/{work}")],
                        |row| row.get(0),
                    )?;
                    if !preceding_owner {
                        bail!(
                            "@checklist is not produced by an earlier decomposition transition for this design and work"
                        );
                    }
                    continue;
                }
                let checklist = parts[2].parse::<i64>()?;
                conn.query_row(
                    "select 1 from checklists where id=?1 and project_id=?2 and design_version_id=?3 and work_unit_id=?4 and status='active'",
                    params![checklist, project_id, design, work],
                    |_| Ok(()),
                )
                .optional()?
                .context("canonical reconciliation checklist is outside the correction owner or design")?;
            }
            "task-accept-out-of-scope" if decode_opaque_task_ref(&target)?.is_none() => {
                let task_id = target.parse::<i64>()?;
                conn.query_row(
                    r#"select 1 from tasks t
                       where t.id=?1 and t.work_unit_id=?2 and t.status in ('open','blocked')
                         and (?3 is null or exists(
                           select 1 from task_derivations td
                           join design_requirements r on r.id=td.design_requirement_id
                           join design_versions v on v.id=r.design_version_id
                           join design_versions current_v on current_v.id=?3
                           join design_requirements current_r on current_r.design_version_id=current_v.id
                             and current_r.requirement_key=r.requirement_key
                           where td.task_id=t.id and v.design_package_id=current_v.design_package_id
                             and td.status in ('active','stale','closed')
                         ))"#,
                    params![task_id, work_unit_id, design_version_id],
                    |_| Ok(()),
                )
                .optional()?
                .context("task transition target is outside the open correction owner/design")?;
            }
            "phase-create" => {
                let parts = target.split('/').collect::<Vec<_>>();
                if parts.len() != 6 {
                    bail!("phase-create target requires work/design/alias/kind/order/key");
                }
                let work = parts[0].parse::<i64>()?;
                let design = parts[1].parse::<i64>()?;
                if work != work_unit_id
                    || !correction_design_accepts(conn, design_version_id, design)?
                {
                    bail!("phase-create target is outside the correction owner or design");
                }
            }
            "phase-assign" => {
                let (phase, task) = target
                    .split_once('/')
                    .context("phase-assign target requires phase/task")?;
                if !phase.starts_with('@') {
                    resolve_phase_ref(conn, 0, 0, work_unit_id, phase)?;
                }
                if decode_opaque_task_ref(task)?.is_none() {
                    let task_id = task.parse::<i64>()?;
                    conn.query_row(
                        "select 1 from tasks where id=?1 and work_unit_id=?2 and status in ('open','blocked')",
                        params![task_id, work_unit_id],
                        |_| Ok(()),
                    )
                    .optional()?
                    .context("phase assignment task is outside the correction owner")?;
                }
            }
            "phase-dependency-add" => {
                let parts = target.split('/').collect::<Vec<_>>();
                if parts.len() != 3 {
                    bail!("phase-dependency-add target requires from/to/type");
                }
                for phase in &parts[..2] {
                    if !phase.starts_with('@') {
                        resolve_phase_ref(conn, 0, 0, work_unit_id, phase)?;
                    }
                }
            }
            "phase-dependency-satisfy" | "phase-dependency-accept" => {
                ensure_phase_dependency_owner(conn, target.parse()?, work_unit_id)?;
            }
            "stale-accept" | "stale-close" => {
                let (kind, id) = target
                    .split_once('/')
                    .context("stale target requires type/id")?;
                stale_contract_tuple(conn, project_id, kind, id.parse()?)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn correction_design_accepts(
    conn: &rusqlite::Connection,
    source_design: Option<i64>,
    target_design: i64,
) -> Result<bool> {
    let Some(source_design) = source_design else {
        return Ok(false);
    };
    conn.query_row(
        r#"
        select exists(
          select 1
          from design_versions source
          join design_versions target
            on target.design_package_id=source.design_package_id
          where source.id=?1 and target.id=?2
            and target.version_number>=source.version_number
            and target.status='approved'
        )
        "#,
        params![source_design, target_design],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn record_correction_tokens(
    conn: &rusqlite::Connection,
    root: &Path,
    project_id: i64,
    closure_id: i64,
    surfaces: &str,
    design_root: Option<&str>,
) -> Result<i64> {
    let tokens = parse_correction_tokens(surfaces)?;
    if tokens.is_empty() {
        bail!("correction contract has no typed surfaces");
    }
    validate_reconciliation_surface_bindings(root, design_root, &tokens)?;
    validate_declared_stale_order(conn, project_id, &tokens)?;
    for (index, token) in tokens.iter().enumerate() {
        let (pre_state, pre_hash) = match token.kind.as_str() {
            "file" => {
                let path = correction_file_path(root, design_root, token)?;
                let exists = path.is_file();
                match token.operation.as_str() {
                    "edit" | "delete" if !exists => bail!(
                        "{} requires an existing regular file: {}",
                        token.operation,
                        path.display()
                    ),
                    "create" if exists => {
                        bail!("create requires an absent target: {}", path.display())
                    }
                    _ => {}
                }
                (
                    Some(if exists { "file" } else { "absent" }.to_string()),
                    exists.then(|| file_sha256(&path)).transpose()?,
                )
            }
            "transition" => (transition_pre_state(conn, &token.operation)?, None),
            _ => unreachable!(),
        };
        conn.execute(
            r#"
            insert into correction_tokens(
                project_id, closure_id, token_ordinal, token_kind, operation,
                target, pre_state, pre_hash, status, created_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', current_timestamp)
            "#,
            params![
                project_id,
                closure_id,
                index as i64 + 1,
                token.kind,
                token.operation,
                token.target,
                pre_state,
                pre_hash
            ],
        )?;
    }
    Ok(tokens.len() as i64)
}

fn validate_reconciliation_surface_bindings(
    root: &Path,
    design_root: Option<&str>,
    tokens: &[CorrectionToken],
) -> Result<()> {
    for transition in tokens.iter().filter(|token| {
        token.kind == "transition" && token.operation == "decomposition-plan-reconcile"
    }) {
        let (_, _, project_relative_path) =
            parse_decomposition_reconciliation_target(&transition.target)?;
        let package_root = design_root.context(
            "Decomposition Plan reconciliation requires an exact imported design package",
        )?;
        let canonical_root = root
            .canonicalize()
            .with_context(|| format!("project root does not exist: {}", root.display()))?;
        let package_path = if Path::new(package_root).is_absolute() {
            PathBuf::from(package_root)
        } else {
            root.join(package_root)
        };
        let canonical_package = package_path.canonicalize().with_context(|| {
            format!(
                "correction Design Package does not exist: {}",
                package_path.display()
            )
        })?;
        let expected_prefix = canonical_package
            .strip_prefix(&canonical_root)
            .context("correction Design Package is outside the selected project")?;
        let relative_plan = Path::new(&project_relative_path)
            .strip_prefix(expected_prefix)
            .context("Decomposition Plan path is outside the correction Design Package")?;
        let relative_plan = relative_plan.to_string_lossy();
        let file_token = tokens.iter().find(|token| {
            token.kind == "file"
                && matches!(token.operation.as_str(), "edit" | "create")
                && token.target == format!("design:{relative_plan}")
        });
        let file_token = file_token.context(
            "decomposition-plan-reconcile path requires a same-closure design edit or create surface",
        )?;
        let resolved = correction_file_path(root, design_root, file_token)?;
        let resolved_relative = if resolved.exists() {
            resolved.canonicalize()?
        } else {
            resolved
        };
        let resolved_relative = resolved_relative
            .strip_prefix(&canonical_root)
            .context("Decomposition Plan path escaped the project root")?;
        if resolved_relative != Path::new(&project_relative_path) {
            bail!("encoded Decomposition Plan path does not match its design correction surface");
        }
    }
    Ok(())
}

pub(super) fn effective_file_pre_hash(
    conn: &rusqlite::Connection,
    closure_id: i64,
    operation: &str,
    target: &str,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        with recursive inherited(closure_id, depth) as (
          select ?1, 0
          union all
          select predecessor.id, inherited.depth + 1
          from inherited
          join closures predecessor
            on predecessor.superseded_by_closure_id=inherited.closure_id
          where exists(
            select 1 from correction_tokens token
            where token.closure_id=predecessor.id
              and token.token_kind='file'
              and token.operation=?2
              and token.target=?3
          )
        )
        select token.pre_hash
        from inherited
        join correction_tokens token on token.closure_id=inherited.closure_id
          and token.token_kind='file'
          and token.operation=?2
          and token.target=?3
        order by inherited.depth desc, token.token_ordinal
        limit 1
        "#,
        params![closure_id, operation, target],
        |row| row.get(0),
    )
    .optional()?
    .context("correction file token has no effective precondition")
}

pub(super) fn transition_pre_state(
    conn: &rusqlite::Connection,
    operation: &str,
) -> Result<Option<String>> {
    let table = match operation {
        "design-decompose" => Some(("checklist_max", "checklists")),
        "phase-create" => Some(("phase_max", "work_phases")),
        "phase-dependency-add" => Some(("phase_dependency_max", "work_phase_dependencies")),
        _ => None,
    };
    table
        .map(|(label, table)| {
            let max_id: i64 = conn.query_row(
                &format!("select coalesce(max(id),0) from {table}"),
                [],
                |row| row.get(0),
            )?;
            Ok(format!("{label}:{max_id}"))
        })
        .transpose()
}

pub(super) fn ensure_correction_prestate_unchanged(
    conn: &rusqlite::Connection,
    root: &Path,
    closure_id: i64,
    design_root: Option<&str>,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "select token_kind, operation, target, pre_state, pre_hash from correction_tokens where closure_id = ?1 order by token_ordinal",
    )?;
    let rows = stmt.query_map(params![closure_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let stored = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (token_kind, operation, target, pre_state, pre_hash) in stored {
        if token_kind == "transition" {
            let current = transition_pre_state(conn, &operation)?;
            if current != pre_state {
                bail!(
                    "correction transition pre-state changed after closure registration; supersede the closure before correction-begin: {operation}:{target}"
                );
            }
            continue;
        }
        let token = CorrectionToken {
            kind: token_kind,
            operation,
            target,
        };
        let path = correction_file_path(root, design_root, &token)?;
        let state = if path.is_file() { "file" } else { "absent" };
        let hash = path.is_file().then(|| file_sha256(&path)).transpose()?;
        if pre_state.as_deref() != Some(state) || pre_hash != hash {
            bail!(
                "correction source changed after closure registration; supersede the closure before correction-begin: {}",
                path.display()
            );
        }
    }
    Ok(())
}

pub(super) fn correction_file_path(
    root: &Path,
    design_root: Option<&str>,
    token: &CorrectionToken,
) -> Result<PathBuf> {
    let (kind, target) = token
        .target
        .split_once(':')
        .context("invalid stored file token")?;
    let base = match kind {
        "design" | "plan" => root.join(design_root.context(
            "design and plan correction surfaces require an exact imported design package",
        )?),
        "docs" | "workflow" | "repository" => root.to_path_buf(),
        _ => bail!("invalid stored correction file kind"),
    };
    let canonical_base = base
        .canonicalize()
        .with_context(|| format!("correction surface root does not exist: {}", base.display()))?;
    let path = base.join(target);
    let containment = if path.exists() {
        path.canonicalize()?
    } else {
        let mut parent = path.parent().context("correction target has no parent")?;
        while !parent.exists() {
            parent = parent
                .parent()
                .context("correction target has no existing parent")?;
        }
        parent.canonicalize()?
    };
    if !containment.starts_with(&canonical_base) {
        bail!("correction surface escapes its allowed root");
    }
    Ok(path)
}

pub(super) fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
