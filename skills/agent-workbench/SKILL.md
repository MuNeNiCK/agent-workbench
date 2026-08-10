---
name: agent-workbench
description: Use Agent Workbench as the project-local source of current accepted design, focused work, user corrections, KPT learning, Command Profiles, review findings, Lean proof receipts, evidence freshness, and completion readiness. Trigger when starting, resuming, handing off, reviewing, verifying, or completing coding-agent work in a project that uses `.agent-workbench`.
---

# Agent Workbench

Use the native Workbench executable for state and decisions. Do not reconstruct its ledger rules in
the Skill or shell.

## Enter a project

1. Find the project root.
2. Always invoke this Skill's setup script as
   `sh SKILL_DIR/scripts/setup.sh PROJECT_ROOT` on POSIX or
   `SKILL_DIR/scripts/setup.ps1 -ProjectRoot PROJECT_ROOT` on Windows. Do not depend on the
   installed POSIX script retaining an executable mode. Setup compares this Skill's
   `release-version` with the runtime bundle's embedded `skill/agent-workbench/release-version`;
   it acquires the pinned runtime when the executable or marker is absent or different, and is a
   read-only context check when they already match. During replacement, the prior complete bundle
   remains recoverable until the new native runtime successfully completes `context` or `init`.
   Setup then records a distinct activation commit before removing that prior bundle. A failure
   before native activation restores the prior bundle; an interruption after activation retains
   the new runtime that can read any state it migrated, and the next setup completes cleanup.
3. Run `.agent-workbench/bin/agent-workbench --project PROJECT_ROOT context`.
4. Run `... describe` and select only from `applicableOperations`. Before using an unfamiliar
   mutation, run `... describe OPERATION`; require `applicable: true` and use only its
   `inputExample` fields.
5. Use only the returned current context for the next action. Query history separately when needed;
   do not treat matching old text as current.

Current Context returns bounded stable references. Retrieve details with `design get`, `plan get`,
`work get`, or `entry get`; page older entries with `history`. A null focus does not prove that no
Design or retained Work exists. Do not replace the bounded context with a full ledger dump.

The setup script only acquires and verifies the release archive. After setup, invoke the native
binary directly.

## Operate safely

The JSON on stdin is a command-specific machine transport, not a persisted record format. Never
construct a `ProjectState` or `LedgerEntry`, inspect source to infer fields, or add fields absent
from the native contract. Unknown fields and inapplicable operations must fail without a state
revision change.

```text
describe [operation]
design inspect-sources | propose | amend | accept | reject | get | source | diff | export
work start | focus | resume | suspend | handoff | adoption-impact | adopt-design | withdraw | complete | get
plan inspect-sources | propose | replace | materialize | get | source | diff | export
task close | reopen-stale
profile define | replace
artifact observe
correction record | supersede | resolve | incorporate
kpt record | apply
review start | resume | handoff | finding | disposition | conclude | verify | context | inspect
entry get
context | history
ready
command show | run
proof digest | run
```

There is no generic mutation or `entry append`. System-owned order, scope, Work/Design binding,
supersession, status, and Design ancestry are derived by the native semantic operation.

Keep product implementation invariant under Workbench use. Never add, rename, restructure, import,
persist, or branch product source, build inputs, runtime state, or shipped artifacts solely to make
Workbench evidence, Review, proof, or self-application convenient. A helper used only by a Command
Profile remains private below `.agent-workbench` and is declared as a snapshotted Profile input. Put
a verifier in the ordinary tracked project structure only when it has an independent project
verification or release purpose beyond recording Workbench evidence.

For an initial outcome, keep one Work from empty baseline through private Design-source capture,
candidate acceptance, selected Claim receipts, Work-specific Plan proposal/materialization, derived
Tasks, evidence, and completion. Never create Tasks manually or split Design/proof/planning into
replacement Works. A Plan candidate has no productive authority until materialization.

When `context` exposes `task reopen-stale`, invoke that no-input semantic operation before trying to
replace a Profile or record new evidence. It is available only when closing evidence for a current,
required, closed Task has become stale. Workbench atomically reopens those Task lineages and every
closed transitive dependent, clears inherited closing evidence, and preserves the current Plan and
Task contracts. Then use the existing Profile/artifact route to produce current evidence and close
the reopened Tasks in dependency order. Do not create a replacement Plan merely to make stale-Task
re-verification reachable, and do not treat this as a generic manual reopen.

When selecting a Lean Claim, place its proposition, witness, and complete local Lean source closure
below `.agent-workbench/design/proofs/` before `design propose`. Declare every local source; do not
invent or copy `expectedDigest`. Proposal derives the digests from the captured bytes, rejects an
omitted dependency, and stores the pinned elaborated proposition with the immutable Design.

`context`, `ready`, and `work complete` compute current target snapshots and Lean input digests
internally; do not supply or guess them.

Use `command show` before execution when presenting a next command. Use `command run` to execute
that same Command Profile resolution and record its argv, cwd, environment identity, output digests, target
snapshot, and every declared input observation. Declare every input on which the result depends. If
one changes, treat the run as stale and rerun it. Evidence created without input observations is
historical only. Never replace a Profile with a guessed command or a shell command string.

Treat `ready` as the completion decision. A verbal done report, commit, clean tree, KPT, or review
completion cannot override `ready: false` unless the accepted design explicitly makes it a
criterion.

## Preserve authority

- Treat the accepted DesignRevision as normative. Record a changed requirement as a User
  Correction and construct a successor Design from private sources; do not silently add a Task,
  Criterion, Claim, or implementation mechanism.
- Map the complete Work-baseline-to-current-Design delta into one Work-specific Implementation Plan.
  Materialize that Plan to derive Tasks; do not turn already accepted constraints into new
  requirements or repeated confirmation work.
- Treat KPT as retained learning. A Try becomes relevant when a later action applies it; it does
  not become a requirement by itself.
- Suspend before switching work or accepting a successor design. Resume the same Work when its
  return condition is met. `work resume` must record a non-empty satisfaction statement and at
  least one current same-Work/Design ledger entry as its immutable basis; never discard a condition
  by resuming immediately. Explicitly adopt a successor design before resuming predecessor-bound Work.

## Review

Review has exactly two purposes: Design Review and Implementation Review. `fresh` and `resume` are
reviewer-context modes, not review types.

Review is a final audit, not a way to construct assurance. Design Review starts only after the
target's Assurance Contract matrix and selected Claim receipts are current. Fresh Implementation
Review starts only after independent completion readiness. If an accepted Finding is an assurance
omission, stop productive work and return through a strict successor Design, fresh assurance,
adoption, and Plan materialization. Only an implementation defect already covered by a current
Contract is Plan-remediable.

- Use `fresh` for an immutable design or implementation snapshot with no prior review,
  finding, or remediation context. The reviewer run must differ from the target producer run.
- For Implementation Review, treat the returned Design, current Plan, immutable Work identity, and
  producer provenance as one fixed target derived by Workbench. Do not reconstruct or substitute
  these components from conversation or prior state.
- Give a reviewer only `review context` for its Review entry. A fresh result has an empty lineage;
  do not supply ordinary Current Context or a prior conversation to that reviewer.
- Use `resume` to continue the same review identifier and exact target, including verification of an
  implementation fix. An amended Design is a new immutable target: its old Review stays readable but
  cannot authorize the new candidate.
- Treat findings as advisory until the responsible work agent records an accepted, rejected, or
  replaced disposition under the accepted design. Only an accepted unresolved finding blocks
  completion.
- Ground a Design Finding in an exact statement, criterion, or assumption. Ground an
  Implementation Finding in an exact component ID and snapshot from the fixed Review manifest.
