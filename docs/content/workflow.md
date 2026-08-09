# Everyday workflow

The user states an outcome and corrections in ordinary language. The coding agent owns the Design,
Implementation Plan, state transitions, implementation, and evidence. Workbench does not ask the
user to approve routine transitions, and Review is not inserted as a universal development loop.

## Start a new project outcome

```text
Use $agent-workbench for this repository. I want <outcome>.
Research the project, explain material design choices, implement the result, and verify completion.
```

For a project with no accepted Design, the agent follows the discoverable state route:

1. Initialize the project-local runtime and private Design workspace.
2. Start one Work carrying the requested outcome. Its immutable baseline is empty until the initial
   Design is accepted.
3. Write coherent Markdown sources below `.agent-workbench/design/product/` and
   `.agent-workbench/design/implementation/`. For a selected Lean Claim, also finish its complete
   local proof-source closure below `.agent-workbench/design/proofs/` before proposal.
4. Inspect and propose those exact sources as an immutable Design candidate. Every content-bearing
   CommonMark unit is classified, and every Statement records explicit Lean-Claim, observable
   Criterion, and implementation choices. Proposal archives the Lean sources, derives their
   digests, rejects undeclared local dependencies, and pins the elaborated proposition identity.
5. If the accepted project policy or risk calls for Design Review, review that immutable candidate.
   Review is otherwise optional.
6. Accept the current candidate head. If it selects Lean Claims, complete their project-local proofs
   and receipts.
7. Write the Work-specific implementation source below
   `.agent-workbench/design/plans/<work-id>/`, propose the complete Plan, and materialize it. This
   creates the exact Task dependency graph; agents cannot add unrelated Tasks manually. Every step
   selects Design Criteria, or declares concrete Task-local command/artifact verification when the
   Design intentionally has no applicable Criterion.
8. Implement dependency-ready Tasks and obtain their Task-bound command or artifact evidence.
9. Close each Task only after its current post-materialization evidence exists.
10. Use `ready`; complete the Work only when every current gap is closed. Completion atomically
    stores one immutable record bound to the exact Work, Design, Plan, responsible run, and digest
    of the pre-commit completion input. A `completed` status without that record is invalid.

This is a dependency route, not review-driven development. The agent discovers the applicable next
operation from the current state instead of inventing a phase or reconstructing it from chat.

## Continue after a session boundary

```text
Use $agent-workbench. Continue the focused work from current project state.
```

The response should identify the user outcome, implemented result, passed or stale evidence, and
remaining result. A null focus does not mean that retained Design or Work history disappeared; the
agent uses the read-only entity and history operations when it needs that retained context.

## Correct a misunderstanding

```text
The accepted behavior is <correct behavior>, not <previous interpretation>.
Record the correction and update the design before continuing.
```

The agent records the Correction immediately. For a Design change it keeps the same Work, suspends
it, proposes and accepts a strict successor, inspects `work adoption-impact`, adopts that successor,
incorporates the Correction, and resumes. A replacement Plan covers the exact baseline-to-successor
Statement delta and materialization reopens every affected Task and transitive dependent.

The Work outcome and immutable baseline do not change during this route. An ancestor-bound Work
cannot resume or perform productive operations until successor adoption is explicit.

## Interrupt, hand off, or withdraw

Suspension retains the Work with a concrete return condition. Handoff changes the responsible agent
run and records provenance without replacing Work. Withdrawal ends Work unsuccessfully and requires
an effective User Correction; it cannot masquerade as completion.

## Verify project results

The agent uses an applicable Command Profile rather than guessing a build or test command. It shows
the resolved argv and executes that same profile against an open dependency-ready Task. Artifact
criteria are observed against their current targets. Selected Lean Claims use the pinned
project-local toolchain.

If a target, profile, Design, Task materialization epoch, or proof input changes, earlier evidence no
longer counts. Failed or interrupted managed commands restore uncommitted output; proof build output
is isolated, serialized, and restored before another mutation proceeds.

## Use review when it adds evidence

Design Review examines one immutable DesignRevision. Implementation Review examines one fixed
Design/Plan/Task/evidence manifest. Findings are advisory until the responsible Work agent records a
disposition. An accepted Design Finding becomes a causal basis for candidate amendment; an accepted
Implementation Finding that requires output work must be included in the replacement Plan before it
can be materialized.

Checking a fix for the same implementation target uses `resume`. A newly amended Design is a new
immutable target; the old Design Review remains readable but cannot authorize the new candidate.

## Complete

`ready` derives completion from the current accepted Design, selected Claim receipts, current Plan,
closed verified Tasks, current Criteria evidence, Corrections, and accepted Findings. A clean Git
tree, commit, review message, KPT entry, or agent statement cannot substitute for a missing gap.

After completion, the immutable Work-completion record is the authority that completion occurred;
the status field is only its lifecycle projection. The record is committed in the same SQLite
transaction as the status change and focus removal.
