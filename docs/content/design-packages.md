# Design Packages

This page is for maintainers and contributors who want design material to drive
agent work.

## Why Design Packages exist

Free-form design notes are useful, but they are hard for agents to use safely.
They do not provide stable requirement IDs, validation expectations, or links to
tasks and evidence.

Agent Workbench converts design material into a Design Package before treating it
as authority.

## Package layout

Create a package with `agent-workbench design init <design-id>`. The command
prints the project-local package location to the invoking user.

The package contains:

- `design.yaml`
- arc42-style architecture sections
- `requirements/`
- `validation/`
- `09-decisions.md`

The architecture sections help people and review agents understand the system.
The machine-readable files let Agent Workbench import requirements, decisions, and
validation gate templates.

## Requirements

Requirements use stable package-owned keys.

They include:

- priority
- affected surfaces
- validation gate keys
- status
- human explanation

Requirements should describe verifiable behavior or constraints.

## Decisions

Decisions use stable package-owned keys and record accepted design choices.

Examples:

- keep structured operational state project-local
- represent execution stack state with work-unit activations
- distribute the CLI through a release asset used by the skill wrapper

Accepted decisions are durable constraints until superseded.

## Validation gate templates

Validation gate templates define expected checks for requirements. They are not
completed validation runs. They are design-level expectations that later work
can select and satisfy.

## Importing design

After updating a Design Package, ask the agent to import it:

```text
Use $agent-workbench and import the design package created for this project.
```

The import creates a new design version. When the design changes,
derived tasks, checklists, validation gates, review plans, and coverage can
become stale. Package identities and contents remain project-local unless the
user explicitly exports a classified view to a chosen destination.
