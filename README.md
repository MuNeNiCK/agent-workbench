# agent-workbench

Agent Workbench is a Codex/Claude Code skill scaffold for structured
long-running agent work.

The intended direction is a skill plus a SQLite-backed memory CLI for tasks,
decisions, reviews, commands, and work records. This repository currently
contains the minimal distributable skill structure and an initial Rust CLI.

## Layout

```text
skills/agent-workbench/
  SKILL.md
  agents/openai.yaml
  scripts/
  references/
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
cargo run -- work reopen 1 --reason "closure evidence was invalid"
cargo run -- work follow-up 1 "related follow-up" --reason "later work found a related issue"
cargo run -- correction add --scope project --type process --pattern "old behavior" --correction "new rule"
cargo run -- command fixed add --name tests --type test --scope project --command "cargo test"
cargo run -- command usage add --profile tests --result pass --log local/logs/tests.log
cargo run -- command usage list --profile tests
cargo run -- command deviation add --profile tests --usage 1 --reason "platform-specific path"
cargo run -- rules applicable --scope project
cargo run -- record create --topic "current work" --work-performed "implemented feature" --next-actions "run review"
cargo run -- record command add 1 --command "cargo test" --result pass
cargo run -- record commit add 1 --sha "$(git rev-parse --short HEAD)" --role created
cargo run -- record file add 1 --path src/lib.rs --role changed
cargo run -- record export 1
cargo run -- work fork "redo from record" --from-record 1 --reason agent_drift
cargo run -- task add "write parser tests" --priority high --source design
cargo run -- task list --status open
cargo run -- decision add --topic database --decision "use one sqlite ledger per project"
cargo run -- authority event add --type user_instruction --scope project --summary "prefer local design notes"
cargo run -- kpt start --scope project --summary "process review"
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
