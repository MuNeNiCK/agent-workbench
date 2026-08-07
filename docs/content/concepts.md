# Concepts and terminology

These terms may appear in agent reports and diagnostic references. They are public Workbench
concepts, not steps the user must manually create.

## DesignRevision

A DesignRevision is one immutable version of the project design accepted for Workbench decisions. It
contains exact archived private Markdown bytes, their canonical CommonMark source-unit graph,
design statements, explicit assumptions, acceptance criteria, and optional Lean claims.

The coding agent constructs the design from the user's outcome and project research. The user is not
required to write a Workbench design package. When a requirement changes, a strict successor keeps
the old revision as history and becomes the new accepted design.

## Work

Work is one requested outcome, such as “add account deletion” or “fix concurrent proof execution.”
It retains the accepted design binding, responsible agent run, remaining tasks, evidence, findings,
and an optional resume condition. Only one Work can be focused at a time.

Suspending or handing off Work does not replace its outcome. A successor design is adopted explicitly
before predecessor-bound Work can resume.

## Implementation Plan and Task

An Implementation Plan is an immutable, Work-bound mapping from the Work's original Design baseline
to its currently adopted Design. It covers every added, modified, and removed Statement plus accepted
Implementation Findings, gives construction dependencies and output scopes, and has no Task
authority until materialized.

A Task is one required construction unit derived atomically from a materialized Plan step. There is
no manual Task-add route. Replacing a Plan preserves unaffected lineages and reopens every changed
step and transitive dependent. Required open Tasks block completion, and closing one requires current
post-materialization evidence for that exact Task and output scope.

Successful completion creates one immutable Work-completion record in the authoritative ledger. It
binds the Work, adopted Design, current Plan, responsible run, prior state revision, and canonical
completion-input digest. Work status alone is not completion authority.

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
working directory, relevant environment, purpose, optional target, and the files or other inputs on
which the result depends. The agent shows the resolved command and executes that same resolution,
avoiding tool-name and argument guessing. A successful run records the observed state of every
declared input. If an input changes later, evidence from that run is stale and the command must be
run again.

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
