---
name: agent-workbench
description: Use when managing long-running coding-agent work with structured project memory, task ledgers, design decisions, review loops, work records, and Markdown exports for Codex or Claude Code workflows.
license: MIT
---

# Agent Workbench

Use this skill when a project needs durable agent operating state: checklists,
design decisions, review findings, work records, or compatibility notes that
should survive across sessions without forcing the agent to grep old Markdown
logs.

## Current Scope

This skill can use a project-local Rust CLI when it is installed or available
in the repository:

- `agent-workbench init`
- `agent-workbench status`
- `agent-workbench next`
- `agent-workbench rules applicable --scope current`
- `agent-workbench authority list`
- `agent-workbench design init`
- `agent-workbench design import`
- `agent-workbench design refresh`
- `agent-workbench design approve`
- `agent-workbench acceptance add`
- `agent-workbench authority event add`
- `agent-workbench correction add`
- `agent-workbench correction list`
- `agent-workbench decision add`
- `agent-workbench decision search`
- `agent-workbench command fixed add`
- `agent-workbench command prefer`
- `agent-workbench command list`
- `agent-workbench command usage add`
- `agent-workbench command usage list`
- `agent-workbench command usage promote`
- `agent-workbench command deviation add`
- `agent-workbench command deprecate`
- `agent-workbench task add`
- `agent-workbench task list`
- `agent-workbench task close`
- `agent-workbench task accept-out-of-scope`
- `agent-workbench decompose design`
- `agent-workbench trace derive-task`
- `agent-workbench checklist list`
- `agent-workbench stale list`
- `agent-workbench gate design-ready --dry-run`
- `agent-workbench gate implementation-ready --dry-run`
- `agent-workbench gate close-ready --dry-run`
- `agent-workbench gate resume-ready --maturity basic|trace-aware|repo-aware --dry-run`
- `agent-workbench gate select`
- `agent-workbench gate record`
- `agent-workbench gate run list`
- `agent-workbench review scope start`
- `agent-workbench review policy add`
- `agent-workbench review plan add`
- `agent-workbench review plan target add`
- `agent-workbench review run add`
- `agent-workbench finding add`
- `agent-workbench finding classify`
- `agent-workbench finding verify`
- `agent-workbench closure add`
- `agent-workbench review-context`
- `agent-workbench evidence add`
- `agent-workbench coverage add`
- `agent-workbench export design`
- `agent-workbench export plan`
- `agent-workbench repository add`
- `agent-workbench repository list`
- `agent-workbench repository snapshot add`
- `agent-workbench repository snapshot list`
- `agent-workbench repository dirty add`
- `agent-workbench repository classify add`
- `agent-workbench repository compare add`
- `agent-workbench repository commit add`
- `agent-workbench repository file add`
- `agent-workbench work start`
- `agent-workbench work suspend`
- `agent-workbench work interrupt`
- `agent-workbench work resume`
- `agent-workbench work reopen`
- `agent-workbench work follow-up`
- `agent-workbench work close`
- `agent-workbench work block`
- `agent-workbench work unblock`
- `agent-workbench work abandon`
- `agent-workbench work fork`
- `agent-workbench resume-check --maturity basic|trace-aware|repo-aware`
- `agent-workbench record create`
- `agent-workbench record command add`
- `agent-workbench record commit add`
- `agent-workbench record file add`
- `agent-workbench record link command`
- `agent-workbench record link commit`
- `agent-workbench record link file`
- `agent-workbench record export`
- `agent-workbench kpt start --from corrections`
- `agent-workbench kpt item add`
- `agent-workbench kpt item list`
- `agent-workbench kpt item convert`

See `references/cli-workflow.md` for the normal operating flow and
`references/repository-validation.md` for validation and repository evidence
commands.

## References

Use the local design references in this repository when changing the skill or
CLI. Do not treat old Markdown exports as standing policy unless they are linked
from structured ledger state.

## Rules

- Prefer structured project memory over broad Markdown history searches.
- Use `agent-workbench status` and `agent-workbench next` before long-running
  work when the CLI is available.
- Use `agent-workbench rules applicable --scope current` before acting on a
  resumed or interrupted work unit.
- Before planning, editing, or reviewing, run `agent-workbench correction list`
  and apply active user corrections for the current scope.
- Before choosing validation or test commands, run `agent-workbench command list`
  and prefer applicable fixed or preferred command profiles.
- When repeated corrections, command drift, recurring findings, or recurring
  close/resume failures appear, propose or run `agent-workbench kpt start` and
  inspect items with `agent-workbench kpt item list`.
- Use read-only gates before state-changing steps: `design-ready`,
  `implementation-ready`, `close-ready`, and `resume-ready`.
- For design-derived implementation work, create the required review plans and
  clean runs before relying on gates: `design_review` for `design-ready`,
  `design_task_decomposition` for `implementation-ready`, and both
  `design_implementation_diff` plus `implementation_review` for `close-ready`.
- Use `decompose design` for normal design-to-plan conversion. Use
  `trace derive-task` for explicit manual links or corrections.
- Record implementation evidence and coverage items for design-derived tasks
  before closing the work unit.
- Use `review-context` when launching a review agent so the prompt is focused
  on the relevant design version, work unit, or review kind. Copy the printed
  `context_ref` into `review run add --target <context_ref>`; design-derived
  gates require clean fresh runs tied to that context.
- Use `resume-check` only when a ledger row should be recorded for an actual
  resume operation.
- Record reusable validation commands with `command fixed add` and command runs
  with `command usage add` so work records can link to stable evidence.
- Record selected validation gate results with `gate record`; list prior runs
  with `gate run list`.
- Register every Git boundary that can affect the task. For repo-aware resume,
  every registered repository needs suspend and current snapshots plus classified
  comparisons.
- Record repository snapshots, dirty entries, classifications, resume
  comparisons, close comparisons, commits, and file changes when repository
  state matters to close or resume decisions.
- Create work records with `record create` and link commands, commits, and files
  when they are available. Manual commit/path links can be recorded first; later
  `repository commit add` and `repository file add` entries backfill structured
  Git identities when the match is unambiguous.
- Use `work suspend`, `work interrupt`, `work reopen`, and `work follow-up` to
  preserve the activation stack before switching tasks.
- Treat accepted design decisions as durable constraints until the user changes them.
