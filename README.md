# agent-workbench

Agent Workbench is a Codex/Claude Code skill and Rust CLI for structured
long-running coding-agent work.

It provides project-local SQLite-backed memory for work stacks, design packages,
tasks, traceability, validation gates, repository state, Git evidence, reusable
commands, review loops, KPT checks, and work records.

## Layout

```text
skills/agent-workbench/
  SKILL.md
  agents/openai.yaml
  scripts/
  references/
    quickstart.md
    cli-workflow.md
    review-recipes.md
    interruption-recovery.md
    repository-validation.md
    close-ready-troubleshooting.md
  assets/templates/
src/
  main.rs
  lib.rs
```

## CLI

The `agent-workbench` CLI stores project-local state in
`.agent-workbench/ledger.sqlite`.

```bash
cargo run -- init
cargo run -- status
cargo run -- next
cargo run -- work start "current task"
cargo run -- work suspend --reason "interrupting issue" --next "resume current task"
cargo run -- resume-check --maturity basic
cargo run -- gate resume-ready --dry-run
cargo run -- authority event add --type user_instruction --scope project --summary "closure evidence was invalid"
cargo run -- work reopen 1 --reason "closure evidence was invalid" --reason-type closure_invalid --authority 1
cargo run -- work follow-up 1 "related follow-up" --reason "later work found a related issue"
cargo run -- correction add --scope project --type process --pattern "old behavior" --correction "new rule"
cargo run -- command fixed add --name tests --type test --scope project --command "cargo test"
cargo run -- command usage add --profile tests --result pass --log .agent-workbench/logs/tests.log
cargo run -- command usage list --profile tests
cargo run -- command deviation add --profile tests --usage 1 --reason "platform-specific path"
cargo run -- rules applicable --scope project
cargo run -- record create --topic "current work" --work-performed "implemented feature" --next-actions "run review"
cargo run -- record command add 1 --command "cargo test" --result pass
cargo run -- record command add 1 --usage 1
cargo run -- record commit add 1 --sha "$(git rev-parse HEAD)" --role created
cargo run -- record file add 1 --path src/lib.rs --role changed
cargo run -- record export 1
cargo run -- work fork "redo from record" --from-record 1 --reason agent_drift
cargo run -- repository add main --path . --head "$(git rev-parse HEAD)" --status clean
cargo run -- repository snapshot add --repository main --head "$(git rev-parse HEAD)" --branch main --status clean --clean
cargo run -- repository commit add --repository main --sha "$(git rev-parse HEAD)" --short "$(git rev-parse --short HEAD)" --subject "current change"
cargo run -- repository file add --commit 1 --path src/lib.rs --type modified
cargo run -- repository compare add --base 1 --current 2 --type resume --result same
cargo run -- task add "write parser tests" --priority high --source design
cargo run -- task list --status open
cargo run -- task accept-out-of-scope 1 --reason "not required for current scope"
cargo run -- decision add --topic database --decision "use one sqlite ledger per project"
cargo run -- design init storage-lifecycle --title "Storage Lifecycle"
cargo run -- design import .agent-workbench/designs/storage-lifecycle --status draft
cargo run -- requirement list --design 1
cargo run -- design-decision list --design 1
cargo run -- gate-template list --design 1
cargo run -- authority event add --type user_instruction --scope project --summary "REQ-001 is out of scope for this work"
cargo run -- acceptance add --design 1 --target requirement:REQ-001 --type accepted_out_of_scope --reason "not needed for current scope" --authority 2
cargo run -- design approve 1 --summary "design passed document checks"
cargo run -- review plan add --work-unit 1 --type design_review --stage design-ready --design-version 1 --required
cargo run -- review-context design-review --design-version 1 --work-unit 1
cargo run -- review run add --plan 1 --type fresh --purpose new_unbiased_review --target "<context-ref>" --clean
cargo run -- gate design-ready --design-version 1 --dry-run
cargo run -- decompose design 1 --work-unit 1
cargo run -- review plan add --work-unit 1 --type design_task_decomposition --stage implementation-ready --design-version 1 --required
cargo run -- review-context design-task-decomposition --design-version 1 --work-unit 1
cargo run -- review run add --plan 2 --type fresh --purpose new_unbiased_review --target "<context-ref>" --clean
cargo run -- gate implementation-ready --design-version 1 --dry-run
cargo run -- work start "implement approved design" --design-version 1
cargo run -- gate close-ready --dry-run
cargo run -- authority event add --type user_instruction --scope project --summary "prefer local design notes"
cargo run -- kpt start --scope project --from corrections --period 30d --summary "process review"
cargo run -- kpt item add --type try --title "stabilize validation command"
cargo run -- kpt list --status open
cargo run -- kpt item list --review 1
cargo run -- kpt item convert --item 1 --to task --priority high
```

## Local Preview

```bash
gh skill preview . skills/agent-workbench
```

## Local Install

```bash
gh skill install . agent-workbench --from-local --agent codex --scope user --force
```
