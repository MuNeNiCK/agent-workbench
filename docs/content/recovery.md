# Recovery and interruption

## Normal recovery

Run:

```bash
agent-workbench --state <state-path> status
agent-workbench --state <state-path> next
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

## Uncertain external effects

Do not blindly retry a release or another external operation after a timeout.
First inspect the remote system, then record the observed identity and artifact
digest. Continue only with a transition consistent with that observation.

## Unsupported state

An unreadable or unsupported state file fails safely. Keep the original file
and restore from a known exact backup or use the supported migration path for
that release. Never repair it with direct SQL.
