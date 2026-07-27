# Workflow

## Recover first

At the beginning of every session:

```bash
sh <installed-skill-dir>/scripts/agent-workbench.sh status
sh <installed-skill-dir>/scripts/agent-workbench.sh next
```

`status` is read-only. `next` prints one revision-bound action or one concrete
blocker. Execute that action exactly instead of reconstructing a similar
command.

## Initialize

```bash
sh <installed-skill-dir>/scripts/agent-workbench.sh init \
  <owner> <outcome> <completion-boundary>
```

The response includes the new revision. Run `next` and then its printed
`continue` command.

## Mutate through typed requests

Write one JSON request and apply it:

```bash
sh <installed-skill-dir>/scripts/agent-workbench.sh apply request.json
```

Every request contains a unique operation identity, the exact expected
revision, and one supported command. An exact retry returns the same receipt.
Reusing an operation identity with changed content is rejected.

Blocked work uses the ordinary suspension request with a reason, return point,
assumptions, and resume conditions. A completed record is immutable. To reopen
its outcome, create a `register-follow-up` work unit whose predecessor is the
exact terminal work.

When a completion boundary names repository details, use
`record-repository-evidence` to retain the repository, snapshot, commit, and
unique changed-file list. KPT observations use `record-kpt`; observations alone
and any learning candidate remain non-authoritative context events and do not
change completion or evidence freshness. A later user decision to adopt the
learning uses the separate correction and authority transition path.

## Export

Export one private class at a time:

```bash
sh <installed-skill-dir>/scripts/agent-workbench.sh export \
  <purpose> <ledger|evidence|review|correction|backup|design> <output>
```

The command never exports another class implicitly.

## Complete

Before completion, record the accepted design and decomposition, required
reviews and caller decisions, positive validation evidence, repository state,
and completion plan. Complete ordered phases only after their dependencies,
tasks, and phase reviews are ready.

After completing the work, start a fresh process and run `status` and `next`
again. The recovered terminal state is the final check.
