# Quick Start

## Requirements

- Linux x86_64 with glibc-compatible runtime.
- GitHub CLI with `gh skill`.
- A coding agent that supports Agent Skills.
- Network access to GitHub Releases and the GitHub API on first use.
- `curl`, `sed`, `tar`, and `sha256sum` available on the host.
- A writable user cache directory.

Agent Workbench is distributed through the Agent Skill. The installed skill
fetches the released Linux x86_64 CLI when an agent first uses it.

## Install the skill

Install for your user account:

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench \
  --scope user \
  --agent <target-agent>
```

Install only for the current project:

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench \
  --scope project \
  --agent <target-agent>
```

Use the agent name that matches your tool. `gh skill install --help` lists the
supported `--agent` values.

## Initialize a project

After installation, ask your coding agent to initialize the repository:

```text
Use $agent-workbench for this project and initialize it.
```

The agent creates managed project state, checks the current workbench state, and
records the project as initialized.

## Expected project data

After initialization, the project has product-managed operational state. Access
it through Agent Workbench commands; explicit exports require a destination.

## Start work

Ask the agent to open a concrete work unit:

```text
Use $agent-workbench and start a work unit for expanding the public docs.
```

The agent should use the work unit to record decisions, validation evidence,
review findings, and close readiness.

## Check status

Ask the agent:

```text
Use $agent-workbench and report the current workbench status.
```

The response should tell you whether the project is initialized, what work is
active, and what action is next.

If the response says the phase is blocked, ask the agent to resolve that blocker
before implementation. A blocked phase means Agent Workbench has review,
finding, gate, or work-state evidence that must be handled first.

`finding remediation` is the narrow exception for a valid close-review fix.
The finding remains open while the owning work unit implements its registered
closure contract. `closure ready` ends that interval and requires an exact
finding-fix resume review plus matching verification.

When the owner has no active activation, run the exact `work remediate
--finding <id>` command printed by `status` or `next`. Do not substitute a
generic activation. For source-only design or documentation corrections, run
the printed `closure correction-begin` and stay inside its typed contract.

## Normal human flow

Most users only need this loop:

1. Install the Agent Skill with `gh skill install`.
2. Ask the agent to initialize the project.
3. Ask the agent to start or resume work with `$agent-workbench`.
4. Ask the agent to report blockers, evidence, and close readiness before it
   claims completion.

For design-driven work, the agent should keep using the work unit that owns the
decomposed tasks and checklists. After `implementation-ready` passes,
`agent-workbench next` should tell the agent whether to continue active work,
resume suspended work, activate an open inactive work unit, or start new work.
Design-derived implementation uses explicit implementation intent, for example
`agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>`.
Start-based implementation intent is rejected; activate the work unit produced
by `decompose design`.
For aggregate work that needs feature or milestone grouping, ask the agent to
create work phases with `phase create`, assign tasks with `phase assign`, and
inspect `phase inventory`. Before moving or splitting a phase, the agent should
run `phase rescope --dry-run` or `phase split --dry-run` and resolve the
printed dependency or trace-decision blockers. Phase reviews can use
`review-context ... --phase <phase-id>`, and phase completion uses
`phase close-ready` before `phase close`.
Before close, the agent should close completed checklist items with
`checklist item close` and then close the parent checklist with
`checklist close`.
