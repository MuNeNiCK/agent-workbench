# Concepts

## Project state

The Agent Skill resolves the managed project and uses:

```text
.agent-workbench/state.sqlite3
```

Workbench state is private project metadata. Do not inspect or edit the SQLite
database directly. The repository's policy controls whether the complete
`.agent-workbench` area is tracked or ignored.

## Work and activation

A work item names one coherent outcome and its completion boundary. An
activation represents the currently executing interval. Interruptions suspend
one activation and may start another; recovery preserves the exact return
point.

Aggregate work may have ordered phases and dependencies. A rescope changes the
outcome or owner. A split creates genuinely independent work lifecycles.
Blocked work is a suspension with explicit resume conditions. Closed work is
not rewritten; continuing the outcome creates a successor follow-up tied to the
terminal predecessor.

## Design and decomposition

An accepted design is immutable. A reviewed decomposition connects its
requirements to implementation work, tasks, checklists, and validation gates.
Historical or rejected approaches are not converted into inverse requirements.

## Evidence

Evidence records a positive observation required by the accepted design. It is
bound to the exact revision, producer, command, repository snapshot, and
artifact identity that produced it.

Repository evidence can additionally retain the exact commit and changed-file
set. KPT observations and learning candidates remain non-authoritative work
records. A separate user correction and authority transition records any later
decision to give a learning normative effect.

## External operations

Release and other external operations retain their work, kind, and artifact
identity. If the result is uncertain, observe the remote state before retrying.
