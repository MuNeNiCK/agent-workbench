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
- arc42-style Markdown sections
- Markdown files under `requirements/`
- Markdown files under `validation/`
- `09-decisions.md`

The architecture sections help people and review agents understand the system.
The machine-readable files let Agent Workbench import requirements, decisions, and
validation gate templates.

Every path declared by the manifest's `arc42`, `requirements`, and `validation`
fields must name a regular file ending in `.md`. JSON, test vectors, fixtures,
generated output, binaries, and other data files are not Design Package files.
Describe their required behavior in Markdown and validate their implementation
through normal repository evidence instead of adding them to `design.yaml`.

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

## Decomposition Plans

An approved Design Version is converted to executable work through a persistent
Decomposition Plan. The normal lifecycle is:

1. Inspect the current slot with `agent-workbench decomposition show
   --design-version <design-version-id> --work <work-unit-id>`.
2. Import an authored or pathless Plan with the exact `decomposition import`
   action printed by the CLI.
3. Follow `decomposition validate` and `decomposition revise` actions until the
   Plan is ready.
4. Review and separately adjudicate the exact Plan.
5. Apply it with `agent-workbench decomposition apply <design-version-id>
   --work <work-unit-id>`.
6. When a successor changes an applied graph, follow the exact
   `decomposition reconcile` action. The predecessor and successor remain
   linked in project-local lineage.

`agent-workbench decompose design` remains the automatic compatibility path.
It atomically creates the generated task graph and its matching applied Plan;
it does not bypass approval, exact-Plan review, or implementation readiness.
