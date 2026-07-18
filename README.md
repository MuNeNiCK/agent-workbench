# agent-workbench

Agent Workbench is an Agent Skill for structured long-running coding-agent
work.

It gives agents a project-local SQLite ledger for work units, design packages,
tasks, traceability, validation gates, repository evidence, reusable commands,
review loops, KPT checks, and work records.

## Install

Requirements: Linux x86_64 with GitHub CLI `gh skill`, `curl`, `sed`, `tar`,
`sha256sum`, network access to GitHub Releases, and a writable user cache.

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench --scope user --agent <target-agent>
```

The installed skill uses its bundled wrapper to fetch and run the Linux x86_64
`agent-workbench` CLI from GitHub Releases.

After installation, ask your coding agent:

```text
Use $agent-workbench for this project and initialize the ledger.
```

Docs: https://munenick.github.io/agent-workbench/
