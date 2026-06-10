# Close-Ready Troubleshooting

Use this when `agent-workbench gate close-ready --dry-run` reports blocked
items.

| Blocked item | Typical action |
| --- | --- |
| active tasks remain open | `agent-workbench task close <task-id>` or `agent-workbench task accept-out-of-scope <task-id> --reason "<reason>"` |
| missing implementation evidence | `agent-workbench evidence add --task <task-id> --design <design-version-id> --requirement <key> --type file --file <path> --note "<evidence>"` |
| missing coverage | `agent-workbench coverage add --design <design-version-id> --requirement <key> --task <task-id> --status covered --requirement-text "<summary>" --runtime "<runtime evidence>" --tests-or-gates "<validation evidence>"` |
| open checklist items or active checklists | inspect with `agent-workbench checklist item list --checklist <checklist-id>`, close completed items with `agent-workbench checklist item close <item-id>`, then close the active checklist with `agent-workbench checklist close <checklist-id>` |
| selected gate has no run | `agent-workbench command usage add ...` then `agent-workbench gate record --gate <gate-id> --result pass --usage <usage-id>` |
| selected gate is stale | update the selected gate from the current design, or run `agent-workbench stale close validation_gate <gate-id> --reason "<reason>"`; do not use `task accept-out-of-scope` |
| validation failure is unresolved | classify the failure, fix it, rerun, or record user-approved acceptance |
| fixed command was not used | run the fixed command and record usage, or add a command deviation and acceptance |
| shadowed rule conflicts remain | run `agent-workbench rules applicable --scope current`, inspect lines with `shadowed_by=<id>`, then fix the conflicting rule or record `agent-workbench acceptance add --target rule:<rule-id> --type explicit_exception ...` for the shadowed rule |
| repeated user corrections are active | start a KPT review or record explicit deferral through user authority and acceptance |
| required close review is missing | add `design_implementation_diff` and `implementation_review` plans, build review contexts, run fresh reviews |
| review finding is open | classify, fix, add closure invariant, verify, then record a new clean run |
| repository state is missing | add repositories and snapshots for every relevant working tree |
| repository changed during work | add close comparison and classify the changed state |
| work record evidence is missing | create a work record and link commands, commits, or files |
| child activation is still active or suspended | close, abandon with reason, or resume and finish the child first |

## Acceptance Pattern

Do not invent exceptions. Record user, policy, or design authority first.

```sh
agent-workbench authority event add --type user_instruction --summary "<approval>"
agent-workbench acceptance add --target <kind:id> --type <acceptance-type> --reason "<reason>" --authority <authority-event-id>
```

For repeated corrections intentionally deferred by the user:

```sh
agent-workbench acceptance add --target stale:user_correction:<correction-id> --type stale_accepted --reason "<why deferred>" --authority <authority-event-id>
```

For an intentional rule precedence override:

```sh
agent-workbench rules applicable --scope current
agent-workbench acceptance add --target rule:<shadowed-rule-id> --type explicit_exception --reason "<why the higher-precedence rule should win>" --authority <authority-event-id>
```

For stale design-derived records:

```sh
agent-workbench stale accept <record-type> <id> --reason "<why accepted>"
agent-workbench stale close validation_gate <gate-id> --reason "<why closed>"
```

For command deviations:

```sh
agent-workbench command deviation add --profile <profile-name> --usage <usage-id> --reason "<why command differed>"
agent-workbench acceptance add --target command-deviation:<deviation-id> --type explicit_exception --reason "<approval reason>" --authority <authority-event-id>
```

## Work Record Evidence

```sh
agent-workbench record create --work-unit <work-unit-id> --topic "<topic>" --work-performed "<summary>"
agent-workbench record command add <record-id> --usage <usage-id>
agent-workbench record commit add <record-id> --sha <sha> --role created
agent-workbench record file add <record-id> --path <path> --role changed
```

Close only after the gate passes:

```sh
agent-workbench gate close-ready --dry-run
agent-workbench work close --summary "<summary>"
```
