# agent-workbench

Agent Workbench is a Codex/Claude Code skill scaffold for structured
long-running agent work.

The intended direction is a skill plus a SQLite-backed memory CLI for tasks,
decisions, reviews, commands, and handoffs. This repository currently contains
the minimal distributable skill structure.

## Layout

```text
skills/agent-workbench/
  SKILL.md
  agents/openai.yaml
  scripts/
  references/
  assets/templates/
```

## Local Preview

```bash
gh skill preview . skills/agent-workbench
```

## Local Install

```bash
gh skill install . agent-workbench --from-local --agent codex --scope user --force
```
