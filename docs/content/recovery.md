# Recovery and interruption

## Normal recovery

Run:

```bash
sh <installed-skill-dir>/scripts/agent-workbench.sh status
sh <installed-skill-dir>/scripts/agent-workbench.sh next
```

Possible actions include:

- `continue` for active work;
- `resume` for a suspended activation whose recorded readiness basis is
  current;
- `repair` when a cached projection does not match the authoritative ledger;
- a concrete blocker when user input or evidence is required.

Use the exact revision and identities printed by `next`. A stale command is
rejected without changing state.

## Projection repair

Projection repair rebuilds derived state from the durable ledger. It does not
invent missing events or modify the managed project.

Run `doctor` for read-only integrity diagnosis. Run `update inspect` to obtain a
dry-run schema plan. `update apply` accepts only that exact plan, writes a
content-addressed backup, verifies the staged conversion, and prints the exact
`update restore` arguments. Normal work commands never update storage.

## Uncertain external effects

Do not blindly retry a release or another external operation after a timeout.
First inspect the exact remote target, then record its identity and artifact
digest. Continue only with a transition consistent with the prepared target,
artifact, and remote precondition. The Skill invokes the relevant external
tool; the core validates and persists the sequence rather than embedding a
generic network transport.

## Unsupported state

An unreadable or unsupported state file fails safely. Keep the original file
and restore from a known exact backup or use the supported migration path for
that release. Never repair it with direct SQL.
