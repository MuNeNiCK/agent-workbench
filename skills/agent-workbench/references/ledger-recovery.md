# Ledger Recovery

Use this recovery only when normal CLI commands report
`validation_runs contains invalid project links` and print the doctor guidance.
The doctor remains available because it opens the ledger without first running
normal migration.

Start with read-only diagnosis:

```sh
agent-workbench doctor validation-links --dry-run
```

The output lists stable validation-run IDs, violated relationships, proposed
field changes, and whole-plan repairability. The command does not mutate the
ledger. If every row is repairable, run:

```sh
agent-workbench doctor validation-links --repair
```

Repair creates and prints a consistent project-local backup, updates the
gate-authoritative validation links and deterministic dependent evidence in one
transaction, writes immutable field-level audit rows, runs normal migration and
final integrity validation, and commits only if all steps succeed. Then run:

```sh
agent-workbench status
agent-workbench doctor validation-links --audit
```

Running `--repair` again should report `no_changes`. If diagnosis reports an
unrepairable missing gate, authority conflict, or unknown dependent relation,
stop and report the product-level reason. Do not inspect the schema, run SQL,
detach evidence manually, move authority events between projects, or delete
validation history.
