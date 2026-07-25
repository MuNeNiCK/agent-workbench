---
name: agent-workbench
description: Use when a coding agent needs durable work, design, review, evidence, interruption, recovery, and completion state through the native Agent Workbench CLI.
license: MIT
---

# Agent Workbench

Use the native `agent-workbench` executable directly. The Skill is guidance
only; it does not bootstrap, download, wrap, or route the product.

## State

Choose an explicit state file. Private Workbench metadata belongs under a
project state area by default, but the user may place it elsewhere:

```sh
agent-workbench --state .agent-workbench/state.sqlite3 init \
  <owner> <outcome> <completion-boundary>
```

Never inspect or edit the SQLite file directly. Managed-project source and its
toolchain are independent of the Workbench state location.

## Recover before acting

At the beginning of work and after interruption, run:

```sh
agent-workbench --state <state-path> status
agent-workbench --state <state-path> next
```

`status` is read-only. `next` returns either one revision-bound executable
command with its constraints or one concrete blocker. Run the printed command;
do not reconstruct a different action.

A state initialized by an older or interrupted setup may return a
revision-bound `start` command:

```sh
agent-workbench --state <state-path> start \
  <revision> <owner> <outcome> <completion-boundary>
```

An active work item returns `continue`. A resumable item returns `resume`.
Projection damage returns `repair`. Each command rechecks the current state and
rejects a stale action.

## Mutations

Design, review, decomposition, evidence, and completion mutations use one typed
JSON request per transaction:

```sh
agent-workbench --state <state-path> apply <request.json>
```

Every request contains a unique `operation`, the exact `expectedRevision`, a
supported `command`, and that command's fields. The native CLI parses the
request, while the authoritative Lean transition kernel decides acceptance.
The CLI does not duplicate policy.

Read [request-format.md](references/request-format.md) for the supported request
shapes. Read [native-workflow.md](references/native-workflow.md) for the normal
design-to-completion sequence.

## Review authority

Review claims are advisory. The caller records a separate adjudication with a
reason and remains responsible for adoption. Review observations do not become
requirements merely because a reviewer raised them.

## Validation

Record positive observations required by the current accepted design. Do not
turn a rejected or removed approach into a permanent inverse requirement or an
absence test. Evidence must retain its exact revision, producer, observation,
repository snapshot, and artifact identity.

## Errors

Unknown commands, malformed JSON, stale revisions, invalid transitions, and
state corruption fail with a non-zero exit. Do not retry a changed request
under the same operation identity. On an uncertain external effect, reconcile
the remote observation before retrying.
