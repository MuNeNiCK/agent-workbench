# Failure and recovery

Use the Skill again after a process loss or a new chat session. It reads project state and returns the
current outcome, accepted design, resume condition, remaining tasks, corrections, KPT, findings, and
evidence gaps. Do not reconstruct current state from old conversation text.

## Start with the symptom

| Symptom | Expected check | User or agent action |
|---|---|---|
| No Work is shown | `context` has no focused Work | Inspect retained Work with `work get`/history; resume the intended suspended Work or start a new outcome |
| Resume is rejected after a design change | Work is bound to a replaced design | Keep Work suspended, inspect successor impact, use `work adopt-design`, then resume |
| Completion is rejected | `ready` lists one or more current gaps | Correct the project result or record the required current evidence; do not override readiness |
| A verification result disappeared | Its target, profile, design, Work, or proof input changed | Re-run the current applicable verification rather than restoring the old entry |
| An operation is reported inapplicable | Current selectors/state do not permit it | Read `context` and `describe <operation>`; perform the required semantic transition |
| A review fix cannot be verified | Wrong lineage, reviewer-produced remediation evidence, or stale target | Resume the same Review and use independent current evidence for its fixed target |

For exact statuses and transitions, see [State and transition reference](state-reference.md). For
request shapes and applicability discovery, see [Native operation reference](operation-reference.md).

## Persistent state

State is stored transactionally in `.agent-workbench/state.db`. Do not inspect or edit database rows
to repair workflow state. Use an applicable semantic operation. An unreadable or invalid state must
stop before Workbench infers current authority.

## Concurrent mutations

Every public state-changing operation in one project obtains project-wide exclusive ownership before
reading authoritative state. Ownership remains held through managed external effects, restoration,
and state commit. A concurrent invocation waits, then rechecks applicability against the state it
observes after acquiring ownership.

The ownership mechanism is an operating-system lock on the project-local
`.agent-workbench/mutation.lock` file. It is released automatically when the process handle closes;
the file's continued existence is not a stale lock and must not be deleted as a recovery step.
SQLite then commits the validated state change in one transaction. Read-only operations open SQLite
without schema-creation or migration capability and do not take the mutation lock.

An older schema is therefore not migrated by `context` or another read operation. Re-run the
matching installed Skill's setup entry point. It recognizes the explicit v0.2.7 schema response and
delegates migration to native `init`; it does not reinterpret other database errors as an upgrade.
During a runtime replacement, setup rolls back an interruption before native activation. Once
native `context` or `init` succeeds, setup records a separate activation commit; recovery then keeps
the compatible new runtime and only completes replacement cleanup. Re-running setup is therefore
safe after an interruption immediately following a successful schema migration.

## Failed operations

Invalid input, unknown fields, inapplicable intent, a failed command, or an ordinarily failing proof
does not create successful evidence or a successful state transition. Managed command outputs and
proof build layouts are journaled before change. An uncommitted command output is restored to its
recorded baseline; an output whose state commit completed is retained. Proof build layouts are
restored after success and handled failure, and an interrupted journal is recovered before the next
mutation.

If an external condition caused failure, correct it, read current context again, and retry the same
semantic intent. Never forge a receipt, delete a private lock, or edit SQLite state.

These are application guarantees, not claims of durability across hardware failure, storage
corruption, forced machine power loss, or a non-conforming network filesystem.
