# Workflow

## Recover first

At the beginning of every session:

```bash
agent-workbench --state <state-path> status
agent-workbench --state <state-path> next
```

`status` is read-only. `next` prints one revision-bound action or one concrete
blocker. Execute that action exactly instead of reconstructing a similar
command.

## Initialize

```bash
agent-workbench --state <state-path> init \
  <owner> <outcome> <completion-boundary>
```

The response includes the new revision. Run `next` and then its printed
`continue` command.

## Mutate through typed requests

Write one JSON request and apply it:

```bash
agent-workbench --state <state-path> apply request.json
```

Every request contains a unique operation identity, the exact expected
revision, and one supported command. An exact retry returns the same receipt.
Reusing an operation identity with changed content is rejected.

## Complete

Before completion, record the accepted design and decomposition, required
reviews and caller decisions, positive validation evidence, repository state,
and completion plan. Complete ordered phases only after their dependencies,
tasks, and phase reviews are ready.

After completing the work, start a fresh process and run `status` and `next`
again. The recovered terminal state is the final check.
