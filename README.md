# agent-workbench

Agent Workbench is an Agent Skill for structured long-running coding-agent
work.

It gives agents a project-local SQLite ledger for work units, design packages,
tasks, traceability, validation gates, repository evidence, reusable commands,
review loops, KPT checks, and work records.

Valid close-review findings support a scoped remediation interval: register the
closure contract, implement the fix while the finding stays open, then require
an exact-context resume review and verification before a later fresh completion
review.

If the owning work unit is inactive, `status` and `next` select
`agent-workbench work remediate --finding <id>`. That command creates an audited
scoped activation without reviving suspended history, binds the exact current
closure set, and preserves the single-active invariant. Blocked or terminal
owners route through explicit unblock or authority-backed reopen first.

Design, plan, documentation, and workflow findings use a distinct typed source
correction session. Run the printed `closure correction-begin`, change only the
declared Markdown surfaces, apply any declared product transitions with
`closure transition apply`, and then run `closure ready`. Direct underlying
task, phase, decomposition, or stale commands cannot bypass an active session.
If an older release already created partial or duplicate current derivations,
supersede the correction contract with the printed audited tokens and use
`transition:design-reconcile:<design>/<work>/<canonical-checklist>`. Apply stale
dispositions first, reconcile the canonical checklist, replace phase membership
with current `@task/<requirement>` aliases, and authority-disposition rejected
numeric task IDs last. Normal `design-decompose` continues to reject partial
state.

Legacy ledgers that fail migration with invalid validation-run links have an
official recovery path. Run `agent-workbench doctor validation-links` for a
read-only plan, then `agent-workbench doctor validation-links --repair`. Repair
creates a project-local backup, records immutable field-level audit entries,
and commits only after normal migration and integrity validation pass. Direct
SQLite edits are neither required nor recommended.

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
