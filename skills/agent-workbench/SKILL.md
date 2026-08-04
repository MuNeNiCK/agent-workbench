---
name: agent-workbench
description: Use Agent Workbench as the project-local source of current accepted design, focused work, user corrections, KPT learning, Command Profiles, review findings, Lean proof receipts, evidence freshness, and completion readiness. Trigger when starting, resuming, handing off, reviewing, verifying, or completing coding-agent work in a project that uses `.agent-workbench`.
---

# Agent Workbench

Use the native Workbench executable for state and decisions. Do not reconstruct its ledger rules in
the Skill or shell.

## Enter a project

1. Find the project root.
2. If `.agent-workbench/bin/agent-workbench` is absent, invoke this Skill's setup script as
   `sh SKILL_DIR/scripts/setup.sh PROJECT_ROOT` on POSIX or
   `SKILL_DIR/scripts/setup.ps1 -ProjectRoot PROJECT_ROOT` on Windows. Do not depend on the
   installed POSIX script retaining an executable mode.
3. Run `.agent-workbench/bin/agent-workbench --project PROJECT_ROOT context`.
4. Run `... describe` and select only from `applicableOperations`. Before using an unfamiliar
   mutation, run `... describe OPERATION`; require `applicable: true` and use only its
   `inputExample` fields.
5. Use only the returned current context for the next action. Query history separately when needed;
   do not treat matching old text as current.

Current Context returns bounded stable references. Retrieve details with `design get`, `work get`,
or `entry get`; page older entries with `history`. Do not replace the bounded context with a full
ledger dump.

The setup script only acquires and verifies the release archive. After setup, invoke the native
binary directly.

## Operate safely

The JSON on stdin is a command-specific machine transport, not a persisted record format. Never
construct a `ProjectState` or `LedgerEntry`, inspect source to infer fields, or add fields absent
from the native contract. Unknown fields and inapplicable operations must fail without a state
revision change.

```text
describe [operation]
design propose | accept | get
work start | focus | resume | suspend | handoff | adopt-design | complete
work get
task add | close
profile define | replace
artifact observe
correction record | supersede | resolve | incorporate
kpt record | apply
review start | resume | finding | disposition | verify | context
entry get
context | history
ready
command show | run
proof digest | run
```

There is no generic mutation or `entry append`. System-owned order, scope, Work/Design binding,
supersession, status, and Design ancestry are derived by the native semantic operation.

`context`, `ready`, and `work complete` compute current target snapshots and Lean input digests
internally; do not supply or guess them.

Use `command show` before execution when presenting a next command. Use `command run` to execute
that same Command Profile resolution and record its argv, cwd, environment, output digests, and
target snapshot. Never replace it with a guessed command or a shell command string.

Treat `ready` as the completion decision. A verbal done report, commit, clean tree, KPT, or review
completion cannot override `ready: false` unless the accepted design explicitly makes it a
criterion.

## Preserve authority

- Treat the accepted DesignRevision as normative. Record a changed requirement as a user
  correction and construct a successor design; do not silently add a task or criterion.
- Treat KPT as retained learning. A Try becomes relevant when a later action applies it; it does
  not become a requirement by itself.
- Suspend before switching work or accepting a successor design. Resume the same Work when its
  return condition is met. Explicitly adopt a successor design before resuming predecessor-bound
  Work.

## Review

Review has exactly two purposes: Design Review and Implementation Review. `fresh` and `resume` are
reviewer-context modes, not review types.

- Use `fresh` for an immutable design or implementation snapshot with no prior review,
  finding, or remediation context. The reviewer run must differ from the target producer run.
- Give a reviewer only `review context` for its Review entry. A fresh result has an empty lineage;
  do not supply ordinary Current Context or a prior conversation to that reviewer.
- Use `resume` to continue the same review identifier and target, including verification of an
  existing finding. Do not create a fresh review to verify its fix.
- Treat findings as advisory until the responsible work agent records an accepted, rejected, or
  replaced disposition under the accepted design. Only an accepted unresolved finding blocks
  completion.
