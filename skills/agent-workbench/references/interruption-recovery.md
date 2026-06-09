# Interruption And Recovery

Use this when the active work changes because of an interrupt, invalid closure,
or redo.

## Before Switching Work

Always preserve the active frame before starting unrelated work.

```sh
agent-workbench status
agent-workbench next
agent-workbench rules applicable --scope current
agent-workbench work suspend --reason "<why switching>" --next "<what to resume>"
```

For an interrupting child task, use `work interrupt` instead of manually
starting a second work unit.

```sh
agent-workbench work interrupt "<child title>" --reason "<why this blocks parent>"
```

## Returning To Suspended Work

Run a dry gate first. Record `resume-check` only when the resume decision should
be persisted.

```sh
agent-workbench gate resume-ready --maturity trace-aware --dry-run
agent-workbench resume-check --maturity trace-aware
agent-workbench work resume --check <resume-check-id>
```

Use `repo-aware` when repository state, nested repositories, dirty files,
snapshots, or comparisons can affect correctness.

```sh
agent-workbench repository snapshot add --repository <name> --activation <activation-id> --head <sha> --branch <branch> --status clean --clean
agent-workbench repository compare add --base <suspend-snapshot-id> --current <current-snapshot-id> --type resume --result same
agent-workbench gate resume-ready --maturity repo-aware --dry-run
```

## Follow-Up Versus Reopen

Use follow-up when the old work was valid at close time and later work found a
new related task.

```sh
agent-workbench work follow-up <closed-work-unit-id> "<title>" --reason "<new related work>"
```

Use reopen only when the closed work unit's own closure, evidence, assumption,
or authority became invalid. Reopen needs authority or an approved acceptance.

```sh
agent-workbench authority event add --type user_instruction --summary "<why closure is invalid>"
agent-workbench work reopen <work-unit-id> --reason "<reason>" --reason-type closure_invalid|closure_incomplete|authority_superseded --authority <authority-event-id>
```

## Forking Work

Use fork when the old attempt should remain auditable but a new attempt should
start from a known record, activation, repository snapshot, or commit.

```sh
agent-workbench work fork "<redo title>" --from-record <work-record-id> --reason agent_drift
agent-workbench work fork "<redo title>" --from-activation <activation-id> --reason design_change
agent-workbench work fork "<redo title>" --from-snapshot <repository-snapshot-id> --reason invalid_assumption
agent-workbench work fork "<redo title>" --from-commit <sha> --reason failed_validation
```

After returning to parent work, run resume readiness again. A child task can
invalidate parent assumptions, design reviews, selected validation gates,
repository state, or implementation review state.
