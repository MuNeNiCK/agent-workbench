# Installation

This page is for a project user installing Agent Workbench into a repository. The normal route does
not require a global Workbench CLI, Elan, Lean, Docker, or QEMU.

## Install into one repository

From the repository root:

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.11 \
  --agent codex --scope project
```

Replace `codex` with the host used by the project's coding agent. Project scope is recommended: the
Skill then belongs to this repository instead of changing all projects for the current user.

After installation, ask the agent:

```text
Use $agent-workbench for this repository. Read the current project context, or initialize it if this
is the first use, and work toward <outcome>.
```

## What first use downloads

The installed Skill selects the archive for the current platform from that exact Skill release. It
verifies the archive's GitHub build-provenance attestation for the repository, release workflow, and
tag, then verifies the published SHA-256 checksum before extracting below `.agent-workbench/bin`.
Native `init` then uses the bundled official Elan executable to acquire `leanprover/lean4:v4.32.2`
below `.agent-workbench/toolchains`.

Every project entry runs setup as a cheap version check. It compares the installed Skill's release
marker with the marker embedded in the private runtime bundle. A missing or different marker causes
the exact pinned archive to replace the private runtime. Setup extracts into a fresh sibling,
requires the complete release bundle and matching marker, and then performs a recoverable directory
swap. The old bundle remains available while the replacement performs the required native
`context` load or `init`. Failure before native activation restores the exact old bundle, removes
the failed candidate, and allows the pinned archive to be retried. After `context` or `init`
succeeds, setup persists a separate activation-commit marker before removing the old bundle. An
interruption after that point retains the new runtime, including when `init` migrated the database;
the next setup finishes cleanup instead of exposing the migrated state to the old runtime. Files
that belonged only to a successfully replaced old bundle cannot survive the replacement. For a
matching complete runtime, the acquisition and version-check phase performs no download,
extraction, or runtime-bundle write. This prevents a newly installed Skill from silently continuing
with an older, partially replaced, or non-activating runtime.

When setup finds a v0.2.7 database, it first attempts a read-only context load. Only the explicit
schema-revision mismatch is handed to native `init` for migration; other read failures remain
failures. Migration preserves the recorded Designs, Works, and ledger history, marks unavailable
historical source archives as unavailable, and advances project state once. Later setup runs are
read-only and idempotent.

A legacy `blocked` Work is migrated to `suspended` with a persisted diagnostic explaining the
translation and requiring its recorded resume condition to be verified before resume. If the
legacy row had no condition, migration records an explicit recovery condition requiring the reason
for the old block to be inspected; it does not create an unresumable suspended Work. The status
change is therefore visible through ordinary Work inspection rather than hidden in migration code.

The POSIX setup entry point is invoked through `sh`, so installed script executable mode is not a
requirement. Once setup finishes, the Skill calls the native Workbench executable directly; shell is
not the application workflow.

## Supported platforms

| Operating system | Architecture |
|---|---|
| Linux | x86_64, aarch64 |
| macOS | x86_64, aarch64 |
| Windows | x86_64 |

An unsupported OS/architecture pair is rejected before installation is treated as successful.

## Files added to the project

| Path | Purpose | Edit manually? |
|---|---|---|
| `.agents/skills/agent-workbench` | Project-installed Skill guidance and setup entry point | Update through the Skill installer, not ad hoc copying |
| `.agent-workbench/bin` | Native runtime, bundled Elan, and redistribution licenses | No |
| `.agent-workbench/toolchains` | Project-local pinned Lean toolchain | No |
| `.agent-workbench/state.db` | Transactional project state | Never |
| `.agent-workbench/mutation.lock` | Process-safe mutation serialization; file existence alone is not a held lock | Never |
| `.agent-workbench/design/product` | Private editable project requirements and constraints | Coding agent |
| `.agent-workbench/design/implementation` | Private editable architecture and technology decisions | Coding agent |
| `.agent-workbench/design/plans/<work-id>` | Private editable implementation-plan sources | Coding agent |
| `.agent-workbench/design/proofs` | Project Lean sources selected by Design Claims | Coding agent |

Workbench does not modify `.gitignore`, Git configuration, or the index. The project decides whether
`.agents` and `.agent-workbench` are tracked, ignored, or provisioned another way. In this repository
both are intentionally ignored.

Workbench does not add or alter the project's product source, build inputs, runtime state, or
shipped artifacts. It may operate as a separate development or release gate without becoming a
product dependency. A verification helper used only to record Workbench evidence belongs below
`.agent-workbench`. Removing Workbench leaves the project's product implementation unchanged.

## Source verification for maintainers

The repository pins its compiler with `lean-toolchain`. A maintainer can build and run the native
test targets with:

```bash
lake build agent-workbench agent-workbench-tests agent-workbench-proof-tests
.lake/build/bin/agent-workbench-tests
.lake/build/bin/agent-workbench-proof-tests
```

This is a source-maintenance route, not an installation prerequisite for project users.
