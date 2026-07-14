# Operations

This page covers the operational details maintainers should know before adopting
Agent Workbench in a repository.

## Project-local data

Agent Workbench manages structured state inside the project. Treat that state as
private operational data and access it through the CLI. Human-readable exports
are created only by an explicit export command with a caller-selected
destination. Decide per project how managed state and chosen exports are
retained.

## Skill installation scope

`gh skill install --scope user` installs the skill for the user.

`gh skill install --scope project` installs it for the current project.

Project-scope installs create an agent-specific installed skill directory, such
as `.agents/skills/`. That directory is an installed environment artifact. Do
not treat it as the source skill package.

The source skill package in this repository is `skills/agent-workbench/`.

## Public release asset

The installed skill wrapper fetches the pinned Linux x86_64 CLI from GitHub
Releases. Release binaries are built on `ubuntu-latest` and target a
glibc-compatible Linux runtime.

The release contains:

- `agent-workbench-<tag>-linux-x86_64.tar.gz`
- `agent-workbench-<tag>-skill.tar.gz`
- `agent-workbench-<tag>-docs.tar.gz`
- `agent-workbench-<tag>-release-metadata.txt`
- `agent-workbench-<tag>-checksums.txt`

Each archive is built from a finite file list and its exact member list is
verified before publication. The binary archive remains binary-only for wrapper
compatibility. The skill and documentation archives include the project
`LICENSE`.

The wrapper verifies the archive before executing the cached CLI.

For project-scope development installs that live inside an Agent Workbench
source checkout, the wrapper first uses an already-built checkout binary from
`target/debug/agent-workbench` or `target/release/agent-workbench`. Normal
installed skills without a source checkout use the pinned release path.

Publishing a new version means creating a release that includes every listed
asset and the checksum file. Published release assets are immutable; use a new
tag instead of replacing an existing release.

## Validation-link recovery

If normal commands stop with `validation_runs contains invalid project links`,
do not inspect or alter private managed state. Diagnosis deliberately bypasses normal
migration so it remains available while `status`, `next`, rules, corrections,
or command lists are blocked:

```bash
agent-workbench doctor validation-links --dry-run
agent-workbench doctor validation-links --repair
agent-workbench doctor validation-links --audit
```

The first command is read-only and prints every affected validation-run ID,
reason, proposed field change, and whether the complete plan is repairable.
Repair refuses the whole plan when a gate is missing, authority provenance
would cross projects, or an unknown dependent relation has no deterministic
rule. Otherwise it performs the supported repair atomically, preserves a
recovery point, records an immutable audit, validates the result, and commits
only if every step passes.

A second repair is a no-op. After a successful repair, rerun `status`; no
special flag is needed for subsequent commands.

## Public documentation

The public documentation is built from `docs/mkdocs.yml` and deployed to GitHub
Pages by `.github/workflows/docs.yml`.

The docs workflow runs when public docs or the docs workflow change on `main`.
It builds with MkDocs strict mode, uploads the generated `site/` artifact, and
deploys it through GitHub Pages.

## Cache

The wrapper caches the CLI under the user's cache directory, using
`XDG_CACHE_HOME` when set.

To force a fresh download, remove the cached release directory.

## Version pinning

Set `AGENT_WORKBENCH_VERSION` when the agent should use a specific release tag.

Set `AGENT_WORKBENCH_REPO` only when testing a fork or alternate release source.

Most users should not set either variable.

## Privacy

Managed project state is private operational data. Access it only through the
CLI, and publish only an explicitly requested classified export whose
destination has been chosen by the caller.
