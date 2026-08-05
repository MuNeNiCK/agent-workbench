# Installation

This page is for a project user installing Agent Workbench into a repository. The normal route does
not require a global Workbench CLI, Elan, Lean, Docker, or QEMU.

## Install into one repository

From the repository root:

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.7 \
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
Native `init` then uses the bundled official Elan executable to acquire `leanprover/lean4:v4.30.0`
below `.agent-workbench/toolchains`.

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
| `.agent-workbench/mutation-lock.db` | Process-safe mutation serialization | Never |

Workbench does not modify `.gitignore`, Git configuration, or the index. The project decides whether
`.agents` and `.agent-workbench` are tracked, ignored, or provisioned another way. In this repository
both are intentionally ignored.

## Source verification for maintainers

The repository pins its compiler with `lean-toolchain`. A maintainer can build and run the native
test targets with:

```bash
lake build agent-workbench agent-workbench-tests agent-workbench-proof-tests
.lake/build/bin/agent-workbench-tests
.lake/build/bin/agent-workbench-proof-tests
```

This is a source-maintenance route, not an installation prerequisite for project users.
