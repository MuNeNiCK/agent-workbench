---
name: agent-workbench
description: Use when a coding agent needs durable work, design, review, evidence, interruption, recovery, and completion state through Agent Workbench.
license: MIT
---

# Agent Workbench

Use the wrapper bundled with this Skill:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh <args>
```

The wrapper acquires the pinned static Linux x86_64 runtime, verifies its
published checksum, caches it, and invokes it. In a source checkout it uses the
already-built Lean executable. Users do not separately install or operate the
runtime.

## Project state

Run the wrapper from the managed project. It resolves the project root and keeps
private state at `.agent-workbench/state.sqlite3`. An existing state file is
found by walking parent directories; a fresh Git worktree uses its Git root,
and a non-Git project uses the current directory.

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh init \
  <owner> <outcome> <completion-boundary>
```

Never inspect or edit the SQLite file directly. The using repository—not
Workbench and not an individual agent—decides whether `.agent-workbench` is
ignored, tracked, copied, or shared.

## Recover before acting

At the beginning of work and after interruption, run:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh status
sh <installed-skill-dir>/scripts/agent-workbench.sh next
```

`status` is read-only. `next` returns either one revision-bound executable
command with its constraints or one concrete blocker. Run the printed command;
do not reconstruct a different action.

A state initialized by an older or interrupted setup may return a
revision-bound `start` command:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh start \
  <revision> <owner> <outcome> <completion-boundary>
```

An active work item returns `continue`. A resumable item returns `resume`.
Projection damage returns `repair`. Each command rechecks the current state and
rejects a stale action.

## Mutations

Design, review, decomposition, evidence, and completion mutations use one typed
JSON request per transaction:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh apply <request.json>
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
