# Overview

Agent Workbench gives coding agents durable project memory.

It is meant for repository work that lasts longer than a single clean exchange:
design changes, implementation tasks branch, reviews find issues, validation
commands matter, and later sessions need to know what is still true.

## Why use it

Use Agent Workbench when a normal chat transcript is too fragile:

- Work is interrupted and must resume later.
- A design needs to become implementation tasks.
- Review findings must be tracked until verified.
- Completion needs evidence, not just a summary.
- Multiple agent sessions need the same operating state.
- User corrections should stop repeating across sessions.

Agent Workbench stores that state in the repository so the next agent can query
it before planning or claiming completion.

## Who it is for

- Developers who ask coding agents to do multi-step work.
- Maintainers who want design decisions and validation evidence to be auditable.
- Teams that need a repeatable process for interruption, resume, review, and
  close.
- Contributors who need to understand why work was blocked, reopened, or marked
  complete.

It is not a human task manager. It is an operating layer for agents working in a
project.

## What you get

- Project-local structured workflow state managed through the CLI.
- Work units with active, suspended, blocked, closed, reopened, and follow-up
  states.
- Design Package import for requirements, decisions, and validation gate
  templates.
- Review plans, findings, closures, and verification records.
- Command profiles and validation evidence.
- Repository snapshots, Git commit links, and changed-file evidence.
- Human-readable exports for work history.

## Installation model

Agent Workbench is installed as an Agent Skill with `gh skill install`.

The skill contains the instructions an agent follows. The installed skill also
contains a small Linux x86_64 wrapper that fetches the released CLI and runs it
for the agent.

People normally install the skill and then ask their agent to use
`$agent-workbench`. The agent handles the CLI calls through the skill.

## Stored data

Agent Workbench keeps private operational state in the project. It can contain
task titles, command records, design summaries, review findings, Git references,
and user corrections, so agents access it through classified inspection and
explicit export commands.

## Next step

Start with [Quick Start](quick-start.md) to install the skill and initialize a
repository.
