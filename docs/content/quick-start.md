# Quick Start

## Requirements

- Linux x86_64.
- GitHub CLI with `gh skill`.
- A coding agent that supports Agent Skills.

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
Use $agent-workbench for this project and initialize the ledger.
```

The agent creates `.agent-workbench/ledger.sqlite`, checks current workbench
state, and records the project as initialized.

## Expected project data

After initialization, the project has:

```text
.agent-workbench/
  ledger.sqlite
  designs/
  exports/
  logs/
```

These files are project operational data. Decide per repository whether to keep
them local, archive them, or commit them.

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

The response should tell you whether the ledger is initialized, what work is
active, and what action is next.

## Normal human flow

Most users only need this loop:

1. Install the Agent Skill with `gh skill install`.
2. Ask the agent to initialize the project.
3. Ask the agent to start or resume work with `$agent-workbench`.
4. Ask the agent to report blockers, evidence, and close readiness before it
   claims completion.
