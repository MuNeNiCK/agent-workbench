# Concepts and terminology

These terms may appear in agent reports and diagnostic references. They are public Workbench
concepts, not steps the user must manually create.

## DesignRevision

A DesignRevision is one immutable version of the project design accepted for Workbench decisions. It
contains design statements, explicit assumptions, acceptance criteria, optional Lean claims, and
snapshots of any declared readable design documents.

The coding agent constructs the design from the user's outcome and project research. The user is not
required to write a Workbench design package. When a requirement changes, a strict successor keeps
the old revision as history and becomes the new accepted design.

## Work

Work is one requested outcome, such as “add account deletion” or “fix concurrent proof execution.”
It retains the accepted design binding, responsible agent run, remaining tasks, evidence, findings,
and an optional resume condition. Only one Work can be focused at a time.

Suspending or handing off Work does not replace its outcome. A successor design is adopted explicitly
before predecessor-bound Work can resume.

## Task

A Task is an executable unit within Work. Required open Tasks block completion. A Task cannot silently
invent a new acceptance criterion; those come from the accepted DesignRevision.

## Acceptance criterion and evidence

An acceptance criterion states an observable result, its target, and the required evidence kind:

- `command` evidence runs a current Command Profile and records its exact invocation and result;
- `artifact` evidence records an explicit observation against the current target snapshot.

Evidence is useful only while its Work, DesignRevision, target, and verification method remain
current. Workbench reports a gap instead of reusing displaced evidence.

## Current Context and history

Current Context is the bounded information needed for the next action: accepted design, focused Work,
open required Tasks, applicable profiles, effective corrections, relevant KPT, accepted unresolved
findings, and evidence gaps.

History contains retained older entries. Similar wording in history does not make an old requirement
current. See [State and transition reference](state-reference.md) for the exact projection and status
rules.

## Command Profile

A Command Profile is the project's recorded way to perform a command: executable, argument vector,
working directory, relevant environment, purpose, and optional target. The agent shows the resolved
command and executes that same resolution, avoiding tool-name and argument guessing.

## User correction

A correction is newer user authority that contradicts or changes the current interpretation. While
it is open, completion is blocked. It stops being current only when superseded by a newer correction,
resolved by a later bound action, or incorporated into a strict successor design.

## KPT

KPT retains project learning across sessions:

- **Keep** records something worth continuing.
- **Problem** records an observed difficulty.
- **Try** records a proposed next practice.

KPT does not automatically change design or completion. A Try becomes relevant when a later action
explicitly applies it.

## Review, Finding, and disposition

A Review examines an immutable design or fixed implementation snapshot. A Finding is the reviewer's
advisory observation. The responsible work agent records whether it is accepted, rejected, or
replaced and why. See [Reviews](reviews.md) for context independence and fix verification.

## Lean claim and proof receipt

A Lean claim selects one proposition about the accepted design, its explicit assumptions, source
inputs, and witness. A proof receipt records that the pinned Lean kernel accepted the current witness
for that proposition. Any relevant input change makes the receipt unusable for completion.
