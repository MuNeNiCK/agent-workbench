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
- `agent-workbench design init`
- `agent-workbench design import`
- `agent-workbench trace derive-task`
- `agent-workbench gate design-ready --dry-run`
- `agent-workbench gate implementation-ready --dry-run`
- `agent-workbench gate close-ready --dry-run`
- `agent-workbench gate resume-ready --maturity basic|trace-aware|repo-aware --dry-run`
- `agent-workbench gate record`
- `agent-workbench gate run list`
- `agent-workbench repository add`
- `agent-workbench repository snapshot add`
- `agent-workbench repository dirty add`
- `agent-workbench repository classify add`
- `agent-workbench repository compare add`
- `agent-workbench resume-check --maturity basic|trace-aware|repo-aware`
- `agent-workbench record create`
- `agent-workbench kpt start --from corrections`

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
- Use read-only gates before state-changing steps: `design-ready`,
  `implementation-ready`, `close-ready`, and `resume-ready`.
- Use `resume-check` only when a ledger row should be recorded for an actual
  resume operation.
- Record reusable validation commands with `command fixed add` and command runs
  with `command usage add` so work records can link to stable evidence.
- Record selected validation gate results with `gate record`; list prior runs
  with `gate run list`.
- Record repository snapshots, dirty entries, classifications, and resume
  comparisons when repository state matters to close or resume decisions.
- Create work records with `record create` and link commands, commits, and files
  when they are available.
- Use `work suspend`, `work interrupt`, `work reopen`, and `work follow-up` to
  preserve the activation stack before switching tasks.
- Treat accepted design decisions as durable constraints until the user changes them.
