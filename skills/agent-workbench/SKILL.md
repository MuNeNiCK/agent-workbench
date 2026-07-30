---
name: agent-workbench
description: Use when a coding agent needs durable work, accepted design, proportional assurance, review decisions, interruption, recovery, and exact completion state.
license: MIT
---

# Agent Workbench

Run every project action through:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh <action> [arguments...]
```

The Skill acquires and verifies the native runtime itself. It acquires the
pinned portable Lean tool only after a formal assurance is selected. The user
does not install or operate either tool and does not provide glibc.

## Recover first

At the beginning of a session and after interruption, run `status` and `next`.
Both describe the project in project language. Do not inspect or edit
`.agent-workbench/state.sqlite3`; the using repository decides whether the
whole `.agent-workbench` directory is tracked or ignored.

Initialize a project with one outcome and its first concrete task:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh init \
  "<outcome>" "<first task>"
```

## Authority

Classify each source effect before recording it:

- `record-design` records caller-stated design;
- `propose-design` records an ordinary or complexity-adding agent proposal
  without authority;
- `accept-design` records the caller's reasoned acceptance;
- `accept-complex-design` additionally records necessity, why a simpler option
  is insufficient, bounded scope, and maintenance cost;
- `record-instruction` records an immediately binding operating instruction;
- `record-question` and `reject-proposal` remain non-authoritative context.

One statement may require more than one of these effects. A rejected approach
does not become an inverse requirement or an absence test.

When one caller source contains a design clause, operating instruction,
question, and/or new Work request, use `record-source-effects` once so the
classified effects share one source and commit atomically.

## Command Profiles and KPT

Command Profiles are durable, structured project facts, not a command runner.
Record exact argv, optional project-relative cwd, purpose, project or current
Work scope, and `required`, `recommended`, or `discouraged` disposition.
Repository and agent profiles remain proposals until the caller accepts them.
Project and Work profiles both remain applicable; never invent precedence.
When alternatives serve the same purpose, use the caller-selected exact key.
Only `add-evidence` freezes that accepted profile into an EvidenceSpec, with
an explicit caller selection reason. A later same-lineage EvidenceSpec cannot
silently remove a required binding.

A required profile has no agent-reasoned bypass. Record a caller-authorized
accepted replacement and select that exact profile instead. A recommended
profile may record an agent-reasoned deviation. A discouraged profile is
guidance and never creates an absence test.

KPT records `keep`, `problem`, and `try` as non-blocking project memory at
project or current Work scope. It never changes assurance, Review, next action,
or completion by itself. An agent-authored correction remains a parallel
candidate until the caller adopts it; it cannot hide caller-owned KPT.
Supply one stable project-language author identity across that author's KPT
actions; per-action operation tokens are provenance, not author identity.
Adoption names that author and is valid only for a proposal correcting the
exact current caller entry.
Use `kpt-history` to inspect immutable predecessors, relations, provenance, and
authority after correction.
Use the atomic KPT conclusion actions when the same source also records an
instruction, an unaccepted Design candidate, or a Command Profile. A Design
candidate still requires the ordinary fresh design Review and caller
acceptance flow. `accept-design-with-kpt` may accompany that existing
acceptance transition only after all ordinary review and assurance
prerequisites already pass.

## Work and assurance

Use `add-task` for a small fix. Use `add-task-for-design` when the task
implements accepted design. Phase assignment is optional presentation only and
never affects readiness or completion.

Each accepted DesignItem selects `formal`, `evidence`, `mixed`, or `none`.
For non-formal observations, use `add-evidence` then `record-evidence`. When
the same Evidence key is selected by more than one Design, supply the optional
Design key to both operations; Workbench binds currentness to that exact
Design basis. `status` and `next` render that Design selector whenever it is
needed, so do not guess between same-key obligations.

For formal assurance:

1. Create project-domain-only modules under `.agent-workbench/formal/` or
   another caller-selected project path.
2. Run `preview-formal` against the still-unaccepted design, its selected
   modules, and a project-domain oracle that prints concrete meaning examples.
   A product adapter is optional and is used only for external implementation
   conformance.
3. Request a fresh exact design review with `request-design-review`, then
   record the clean result or every observation.
4. Let the caller accept or reject the reviewed design with a recorded reason.
5. Add the implementation Task. The verified result selected by
   `preview-formal` remains reusable for that exact accepted design.
6. If a declared product surface later changes, run
   `formal-check <assurance-key> [design-key]` again. Supply the Design key
   when more than one current Design uses the same assurance key. It reads
   the selected artifacts from
   project state; the caller cannot substitute a different target at check time.

`formal-check` is the only public route that can produce a formal result. It
checks the actual module closure, hashes sources, oleans, implementation
surfaces and oracle, and, when selected, compares every input-only case with an
ordinary product-boundary adapter. Expected results must not appear in cases or
adapters.

When the oracle and product disagree, `preview-formal` records and presents the
counterexample instead of discarding it. The checked formal meaning can then be
reviewed and accepted before the product is corrected. Work completion remains
blocked until `formal-check` observes passing product conformance. An adapter
timeout, process failure, malformed JSON, or output-limit failure is recorded
as an execution failure, never as a semantic counterexample. An oracle failure
produces no reviewable formal result.

If a repository change is outside every declared implementation surface, do
not rerun unrelated proofs merely because some file changed. Request a bounded
`reuse` Review over the changed artifact to decide whether the surface
declaration remains complete. A clean reuse decision preserves the existing
formal result and Review identities; uncertainty is limited to that binding.
A stale binding makes only its assurance claim pending; `status`, `next`, and
unrelated project work remain available.

## Review and interruption

Reviews are advisory. Use `request-review` and `record-review`, then let the
caller use `resolve-review` with a reason. A complexity-adding proposal also
requires necessity, why a simpler option is insufficient, bounded scope, and
maintenance cost through `adopt-complex-review-proposal`. Ordinary proposals
use `adopt-review-proposal`; in both cases the caller owns adoption.

Keep reviewer execution provenance distinct:

- Resume an existing reviewer context only to continue the same Review lineage,
  such as completing an interrupted inspection or explaining its observations.
- A requested fresh Review uses a different reviewer execution with no inherited
  implementation or prior-Review context. Do not resume a prior reviewer and
  record that result as fresh.
- Verification by a resumed reviewer may confirm its earlier observation, but
  it does not replace a required fresh Review of the corrected exact artifact.

Workbench records the exact Review scope and reviewer identity; it cannot
inspect an agent runtime's hidden context. The operating agent must preserve
this distinction when it starts or resumes the external reviewer.

Use `correct-review` with the intended outcome, Task, and artifact to supersede
a mistaken target without relinking history.
Use `interrupt` for urgent work and finish its selected boundary before using
`return` to restore the saved outcome.
If a saved assumption changed, use `replan-return` with the caller's selected
outcome and reason; silent retargeting is rejected.
The caller may also use that explicit, reasoned operation to replace a pending
return plan before the interrupting Work is complete; ordinary `return` cannot
abandon unfinished Work.
Use `start-work` and `switch-work` for independent work without an automatic
return plan.

## Completion

Run `complete`. It succeeds only when every positive member of the current
accepted completion boundary is current and satisfied. Creating unrelated
tasks, reviews, phases, proofs, or evidence never changes that result.

See [request-format.md](references/request-format.md) for action signatures and
[native-workflow.md](references/native-workflow.md) for the normal sequence.
