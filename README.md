# agent-workbench

Agent Workbench is a Codex/Claude Code skill scaffold for structured
long-running agent work.

The intended direction is a skill plus a SQLite-backed memory CLI for tasks,
decisions, reviews, commands, and handoffs. This repository currently contains
the minimal distributable skill structure and an initial Rust CLI.

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
```

## Local Preview

```bash
gh skill preview . skills/agent-workbench
```

## Local Install

```bash
gh skill install . agent-workbench --from-local --agent codex --scope user --force
```
