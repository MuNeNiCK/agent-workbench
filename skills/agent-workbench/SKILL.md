---
name: agent-workbench
description: Use when managing long-running coding-agent work with structured project memory, task ledgers, design decisions, review loops, handoff records, and Markdown exports for Codex or Claude Code workflows.
license: MIT
---

# Agent Workbench

Use this skill when a project needs durable agent operating state: checklists,
design decisions, review findings, handoffs, or compatibility notes that should
survive across sessions without forcing the agent to grep old Markdown logs.

## Current Scope

This is the initial scaffold. The planned shape is:

- Skill instructions that teach agents when and how to use structured memory.
- A SQLite-backed CLI for project-local memory.
- Optional Markdown exports for human-readable project notes.

## References

Detailed references will be added once the memory model and CLI contract are
settled.

## Rules

- Prefer structured project memory over broad Markdown history searches.
- Use `agent-workbench status` and `agent-workbench next` before long-running
  work when the CLI is available.
- Keep handoffs concise and queryable.
- Treat accepted design decisions as durable constraints until the user changes them.
