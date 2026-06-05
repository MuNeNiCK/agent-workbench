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

- `agent-workbench status`
- `agent-workbench next`
- `agent-workbench rules applicable --scope current`
- `agent-workbench resume-check --maturity basic`
- `agent-workbench gate resume-ready --dry-run`
- `agent-workbench record create`
- `agent-workbench kpt start --from corrections`

Later phases add design import, traceability, review runs, validation evidence,
and repository-state gates.

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
- Use `agent-workbench gate resume-ready --dry-run` for read-only resume checks
  in review contexts; use `resume-check` only when a ledger row should be
  recorded.
- Record reusable validation commands with `command fixed add` and command runs
  with `command usage add` so work records can link to stable evidence.
- Create work records with `record create` and link commands, commits, and files
  when they are available.
- Use `work suspend`, `work interrupt`, `work reopen`, and `work follow-up` to
  preserve the activation stack before switching tasks.
- Treat accepted design decisions as durable constraints until the user changes them.
