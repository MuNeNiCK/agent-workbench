# Concepts

## Design Package

A Design Package distinguishes:

- caller-stated project design awaiting explicit acceptance;
- accepted goals, requirements, constraints, decisions, structure, and facts;
- binding caller operating instructions;
- agent proposals and unresolved questions; and
- rejected proposals retained only as context.

A rejection does not create an inverse requirement or a test that a discarded
approach is absent. Tasks and assurance come from accepted design or the
accepted Work boundary.

Agent proposals distinguish ordinary corrections from additions that increase
product complexity. Only the latter require a recorded necessity,
simpler-alternative analysis, bounded scope, and maintenance cost.

Each accepted design statement selects `formal`, `evidence`, `mixed`, or
`none`. Dependencies are explicit so a later correction invalidates only the
declared affected scope.

## Work, Task, and Phase

Work is one caller outcome with a positive completion boundary. A Task is one
executable unit within that Work and may freeze the accepted design versions it
implements.

Phase is optional display metadata: a name and order for grouping Tasks. It has
no lifecycle, dependency, review, readiness, or completion effect. A small fix
does not need a Phase.

## Assurance

Formal assurance records the exact checked Lean modules, imported closure,
oracle, generated artifacts, and declared product implementation surfaces. If a
declared surface changes, the affected assurance becomes pending and the public
Skill requires `formal-check` again. `status`, `next`, and unrelated project
work remain available. Stale-currentness identities cross the private process
boundary through a bounded file, so their count cannot consume a single
environment-string slot and disable those operations.
Pure logical contracts select Lean contract/proof modules plus a project-domain
oracle that exposes concrete meaning examples. External product conformance
additionally selects product surfaces and an input-only adapter/case set.

External Evidence records a positive observation, method, environment,
acceptance condition, trusted boundary, and artifact identity.

## Review and completion

A review freezes the outcome identity, exact design versions, Task, purpose,
and artifact it examined.
Reviewer observations are advisory. The caller accepts, rejects, rescopes,
defers, or requests evidence with a reason.

Completion is computed from the current Work boundary. Unrelated Tasks, Phases,
reviews, proofs, or Evidence do not become completion conditions.

## Project memory

State is stored in `.agent-workbench/state.sqlite3`. Do not edit it directly.
The repository using Agent Workbench controls all Git treatment of
`.agent-workbench`.
