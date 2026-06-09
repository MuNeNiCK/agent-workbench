# agent-workbench

Agent Workbench is an Agent Skill for structured long-running coding-agent
work.

It gives agents a project-local SQLite ledger for work units, design packages,
tasks, traceability, validation gates, repository evidence, reusable commands,
review loops, KPT checks, and work records.

## Install

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench --scope user --agent <target-agent>
```

The installed skill uses its bundled wrapper to fetch and run the Linux x86_64
`agent-workbench` CLI from GitHub Releases.
