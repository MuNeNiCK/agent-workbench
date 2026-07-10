# Concepts

## Ledger

The ledger is a project-local SQLite database at:

```text
.agent-workbench/ledger.sqlite
```

It is the source of truth for operational state. Markdown files and exports are
useful for people, but the ledger stores the structured relationships that
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

A Design Package is structured design material stored under:

```text
.agent-workbench/designs/<design-id>/
```

It contains human-readable architecture sections plus machine-readable
requirements, decisions, and validation gate templates.

Agent Workbench does not treat arbitrary local notes as standing authority.
Design notes should be converted into a Design Package and imported into the
ledger.

## Requirement

A requirement is a stable, importable statement of expected behavior or
constraint. Requirements use keys such as `REQ-001`.

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
