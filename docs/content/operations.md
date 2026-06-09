# Operations

This page covers the operational details maintainers should know before adopting
Agent Workbench in a repository.

## Project-local data

Agent Workbench creates `.agent-workbench/` in the project.

Important paths:

| Path | Purpose |
| --- | --- |
| `.agent-workbench/ledger.sqlite` | Structured project ledger. |
| `.agent-workbench/designs/` | Design Package drafts and imported sources. |
| `.agent-workbench/logs/` | Validation and command logs. |
| `.agent-workbench/exports/` | Human-readable exports. |

The ledger is operational state. Decide per project whether `.agent-workbench/`
should be kept local, archived, or committed.

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
- `agent-workbench-<tag>-checksums.txt`

The wrapper verifies the archive before executing the cached CLI.

Publishing a new version means creating a release that includes both the
archive and checksum file. Published release assets are immutable; use a new
tag instead of replacing an existing release.

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

The ledger can contain:

- task titles
- user corrections
- command names and logs
- design summaries
- Git commit and file references
- review findings

Treat `.agent-workbench/` as project operational data and apply the same privacy
rules you use for logs and design documents.
