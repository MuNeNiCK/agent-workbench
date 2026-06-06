# Repository And Validation Evidence

Use this reference when close or resume decisions depend on command results,
validation gates, commits, files, or repository state.

## Validation Runs

Record reusable commands first when they are stable:

```sh
agent-workbench command fixed add --name tests --type test --scope project --command "cargo test" --expected-result pass
```

Record observed command runs:

```sh
agent-workbench command usage add --command "cargo test" --result pass --work-unit <id> --snapshot <snapshot-id>
```

Record selected validation gate outcomes:

```sh
agent-workbench gate record --gate <gate-id> --result pass --usage <usage-id> --snapshot <snapshot-id>
```

List prior runs:

```sh
agent-workbench gate run list --gate <gate-id>
```

## Repository State

Record repositories and snapshots:

```sh
agent-workbench repository add main --path . --head <sha> --status clean
agent-workbench repository snapshot add --repository main --activation <activation-id> --head <sha> --branch <branch> --status clean --clean
```

For dirty snapshots, record dirty entries and classify them:

```sh
agent-workbench repository dirty add --snapshot <snapshot-id> --path <path> --type modified
agent-workbench repository classify add --snapshot <snapshot-id> --dirty-entry <dirty-id> --classification expected --reason "implementation edit"
```

For repo-aware resume, record a current snapshot and compare it with the
suspend snapshot:

```sh
agent-workbench repository compare add --base <base-snapshot-id> --current <current-snapshot-id> --type resume --result same
```

Use `changed_classified` only after the changed state has been classified.
`changed_unclassified` should keep `gate resume-ready --maturity repo-aware
--dry-run` blocked.
