# Installation

## Install the Agent Skill

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.2 \
  --scope user --agent <target-agent>
```

Then ask the coding agent to use `$agent-workbench` for the project. The Skill
acquires the pinned static Linux x86_64 runtime, verifies the release checksum,
caches it, and invokes it. There is no separate CLI or Lean installation.

Workbench state is created under the managed project's `.agent-workbench`
directory. The repository's own policy decides whether that directory is
ignored, tracked, copied, or shared.

## Build from source

Install Git, a C toolchain, and `elan`. Clone the default `lean` branch, then:

```bash
lake build
.lake/build/bin/agent-workbench --version
```

Run all executable laws before using a source build:

```bash
.lake/build/bin/kernel-laws
.lake/build/bin/storage-laws
.lake/build/bin/workflow-laws
.lake/build/bin/cli-laws
```
