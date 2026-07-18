# Concepts

## Product principles

Agent Workbench is an operational layer for coding agents, not a substitute for
the user's product decisions. Its durable state must preserve the original work,
accepted design decisions, user corrections, and the evidence needed to resume
without switching to whichever problem is most recent.

The following are compatibility requirements:

- Simplifying the implementation must not silently remove a supported use case
  or CLI capability. A replacement must preserve the observable workflow and be
  documented before the old surface is removed.
- A gate and the lifecycle resolver must agree. Every recovery action printed by
  a gate must be executable in that same state, with the exact target and command
  needed to make progress.
- Ordinary status, planning, and lifecycle commands must not rewrite the schema
  as a side effect. Incompatible state changes use an explicit `update` command
  with inspection, a staged atomic update, a content-addressed backup, integrity
  checks, and a reversible restore.
- New mechanisms require a concrete user need that existing ordinary commands
  cannot meet. Review evidence and owner decisions need clear audit records, but
  the default local workflow does not require signing keys, trust stores,
  capabilities, system paths, or an external administrator.
- Historical releases are evidence for regression analysis, not the product
  specification. Current documented use cases, accepted design, corrections,
  and tests define the behavior that must be preserved.

## Project state

Agent Workbench keeps structured operational state inside the project. It is
the source of truth for agent workflow. Explicit exports are
useful for people, while the managed project state stores the relationships that
agents need before they plan, resume, review, or close work.

## Work unit

A work unit is a durable unit of agent work, such as "expand public docs",
"fix release wrapper", or "import the current design".

Only one work activation can be active at a time. This matters because agents
often discover interruptions while working. Agent Workbench records whether work
is active, suspended, blocked, closed, reopened, or followed up.

## Activation stack

The activation stack is how Agent Workbench preserves interruptions.

Example:

```text
docs work is active
  -> release wrapper issue blocks docs verification
  -> docs work is suspended
  -> wrapper work becomes active
  -> wrapper work closes
  -> docs work resumes only after a resume check
```

Resume is a gate, not just a reminder. It can block if assumptions, design
state, repository state, or review state changed while the work was suspended.

## Design Package

A Design Package is structured design material created with `design init`.
Its manifest declares Markdown authority files only: every `arc42`,
`requirements`, and `validation` path must end in `.md`. Arbitrary data and
implementation fixtures do not become design authority by being placed in the
package directory.
It contains human-readable design sections plus machine-readable
requirements, decisions, and validation gate templates.

Agent Workbench does not treat arbitrary local notes as standing authority.
Design notes should be converted into a Design Package and imported through the
CLI.

## Requirement

A requirement is a stable, importable statement of expected behavior or
constraint. Their stable keys belong to the package that defines them.

Requirements can be linked to tasks, validation gates, implementation evidence,
coverage records, findings, and validation runs.

## Validation gate

A validation gate is a structured expectation that can be checked before moving
work forward.

Common readiness gates:

- `design-ready`
- `implementation-ready`
- `close-ready`
- `resume-ready`

Gates are read-only by default. If they block, they should report the missing
evidence or next action.

## Review

Reviews are semantic checks. They answer questions that mechanical gates cannot,
such as whether an implementation matches the design or whether a finding fix is
actually complete.

Agent Workbench separates review types:

- design review
- design task decomposition review
- design-implementation diff review
- implementation review

Fresh reviews are for unbiased judgment. Resume reviews are for verifying known
finding fixes.

## Evidence

Evidence is the material proof behind work:

- command usage
- validation runs
- repository snapshots
- Git commits
- changed files
- implementation evidence
- review findings and closures
- scoped remediation and immutable finding-verification attempts
- work records

The close-ready gate uses evidence to decide whether work can safely close.

## Command profile

A command profile records validation commands that are fixed, preferred, or
known to be project-specific.

This matters because agents often guess test commands. Agent Workbench can store
the commands that should be used for a repository and record when an agent
deviates from them.
