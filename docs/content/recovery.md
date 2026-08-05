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

The ownership mechanism is a project-local SQLite transaction. It is released when the process
connection closes; there is no user-managed lock file or stale-lock deletion procedure. Read-only
operations such as `context`, `describe`, `ready`, history, and entity lookup do not take this lock.

## Failed operations

Invalid input, unknown fields, inapplicable intent, a failed command, or an ordinarily failing proof
does not create successful evidence or a successful state transition. Managed proof output layouts
are restored on the operation's success and handled failure paths.

If an external condition caused failure, correct it, read current context again, and retry the same
semantic intent. Never forge a receipt, delete a private lock, or edit SQLite state.

These are application guarantees, not claims of durability across hardware failure, storage
corruption, forced machine power loss, or a non-conforming network filesystem.
