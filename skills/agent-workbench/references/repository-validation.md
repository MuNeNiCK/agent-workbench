# Repository And Validation Evidence

Use this reference when close or resume decisions depend on command results,
validation gates, commits, files, or repository state.

## Validation Runs

Record reusable commands first when they are stable:

```sh
agent-workbench command fixed add --name tests --type test --scope project --command "<project-test-command>" --expected-result pass
```

Record observed command runs:

```sh
agent-workbench command usage add --command "<project-test-command>" --result pass --work-unit <id> --snapshot <snapshot-id>
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

Record every repository boundary that can affect the task, including nested
repositories that are separate Git working trees:

```sh
agent-workbench repository add main --path . --head <sha> --status clean
agent-workbench repository add nested --path vendor/lib --head <sha> --status clean
agent-workbench repository list
```

Record snapshots:

```sh
agent-workbench repository snapshot add --repository main --activation <activation-id> --head <sha> --branch <branch> --status clean --clean
agent-workbench repository snapshot list --repository main
```

For dirty snapshots, record dirty entries and classify them:

```sh
agent-workbench repository dirty add --snapshot <snapshot-id> --path <path> --type modified
agent-workbench repository classify add --snapshot <snapshot-id> --dirty-entry <dirty-id> --classification expected --reason "implementation edit"
```

For repo-aware resume, record a current snapshot for every registered repository
that had a suspend snapshot, and compare it with the suspend snapshot:

```sh
agent-workbench repository compare add --base <base-snapshot-id> --current <current-snapshot-id> --type resume --result same
```

Use `changed_classified` only after the changed state has been classified.
`changed_unclassified` should keep `gate resume-ready --maturity repo-aware
--dry-run` blocked.

For close readiness, compare the prior and active snapshots with `--type close`
when a repository changed during the work:

```sh
agent-workbench repository compare add --base <prior-snapshot-id> --current <current-snapshot-id> --type close --result changed_classified --head-changed
```

## Git Evidence

Record Git identities when commits and file-level changes are known:

```sh
agent-workbench repository commit add --repository main --sha <sha> --short <short-sha> --subject "change summary"
agent-workbench repository file add --commit <git-commit-id> --path src/lib.rs --type modified
```

Work records may be linked before Git evidence is imported:

```sh
agent-workbench record commit add <work-record-id> --sha <sha> --role created
agent-workbench record file add <work-record-id> --path src/lib.rs --role changed
```

When `repository commit add` or `repository file add` is run later, manual work
record links are backfilled with structured Git IDs if the commit SHA or file
path match is unambiguous. If multiple repositories have the same path, include
the repository-specific Git file change when linking the work record.
