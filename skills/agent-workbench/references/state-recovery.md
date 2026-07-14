# State Recovery

This reference covers two explicit recovery surfaces. They are independent:
validation-link repair does not normalize historical task identity, and the
task-history migration does not repair validation links.

## Validation links

Use this section only when normal CLI commands report invalid validation
project links and print doctor guidance. Start with read-only diagnosis:

```sh
agent-workbench doctor validation-links --dry-run
```

If the complete plan is reported as repairable, run:

```sh
agent-workbench doctor validation-links --repair
agent-workbench status
agent-workbench doctor validation-links --audit
```

Repair is atomic, preserves a recovery point, records an immutable audit, and
validates the result before committing. A repeated repair should report
`no_changes`. If diagnosis reports an unrepairable conflict, stop and report
the product-level reason. Do not inspect or alter private managed state outside
the supported CLI.

## Historical task identity

Use this migration only when the CLI reports that historical tasks and phase
memberships need explicit identity normalization. Ordinary commands never apply
it implicitly.

Start with the read-only owner index, then request the exact owner plan using
the printed opaque handle:

```sh
agent-workbench migration task-history plan
agent-workbench migration task-history plan --owner <owner-handle>
```

If the plan has no ambiguities, apply that exact plan and inspect its audit:

```sh
agent-workbench migration task-history apply --owner <owner-handle> --plan <plan-handle>
agent-workbench migration task-history audit --owner <owner-handle>
```

For an ambiguity, use the opaque choices printed by `ambiguity-list`. Record
user authority before recording the matching decision:

```sh
agent-workbench migration task-history ambiguity-list --owner <owner-handle> --plan <plan-handle>
agent-workbench migration task-history authority-record --owner <owner-handle> --plan <plan-handle> --ambiguity <ambiguity-handle> --resolution <resolution-handle> --statement "<user instruction>" --provenance user_instruction --provenance-ref <reference>
agent-workbench migration task-history ambiguity-decide --owner <owner-handle> --plan <plan-handle> --ambiguity <ambiguity-handle> --resolution <resolution-handle> --authority <authority-handle>
```

Use `--retire` instead of `--resolution` only when the user explicitly directs
retirement. Apply the resolved plan handle printed by `ambiguity-decide`.
Planning is read-only. Stop on source drift, an unknown or stale handle, an
unsupported historical version, or an unresolved ambiguity. Do not inspect or
alter private managed state outside the supported CLI.
