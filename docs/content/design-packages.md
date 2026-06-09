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

Design Packages live under:

```text
.agent-workbench/designs/<design-id>/
```

The package contains:

- `design.yaml`
- arc42-style architecture sections
- `requirements/`
- `validation/`
- `09-decisions.md`

The architecture sections help people and review agents understand the system.
The machine-readable files let the ledger import requirements, decisions, and
validation gate templates.

## Requirements

Requirements are stable records with keys such as `REQ-001`.

They include:

- priority
- affected surfaces
- validation gate keys
- status
- human explanation

Requirements should describe verifiable behavior or constraints.

## Decisions

Decisions use keys such as `DEC-001` and record accepted design choices.

Examples:

- use a project-local SQLite ledger
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
Use $agent-workbench and import the agent-workbench-core design package.
```

The import creates a new design version in the ledger. When the design changes,
derived tasks, checklists, validation gates, review plans, and coverage can
become stale.

## Current project design

This repository has an imported `agent-workbench-core` design package. Its
current purpose is documentation expansion: the README, skill instructions,
workflow references, release wrapper behavior, and imported design package
should describe one coherent install and operating path.
