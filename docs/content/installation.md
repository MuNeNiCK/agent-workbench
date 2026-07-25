# Installation

## Install the native CLI

Download the pinned release, verify it through the published checksum file,
and install the executable:

```bash
version=v0.2.0
gh release download "$version" \
  --repo MuNeNiCK/agent-workbench \
  --pattern "agent-workbench-$version-linux-x86_64.tar.gz" \
  --pattern "agent-workbench-$version-checksums.txt"
grep "agent-workbench-$version-linux-x86_64.tar.gz" \
  "agent-workbench-$version-checksums.txt" | sha256sum -c -
tar -xzf "agent-workbench-$version-linux-x86_64.tar.gz"
install -Dm755 agent-workbench "$HOME/.local/bin/agent-workbench"
agent-workbench --version
```

The expected output is:

```text
agent-workbench 0.2.0
```

## Install the Agent Skill

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.0 \
  --scope user --agent <target-agent>
```

The Skill contains operating guidance, not another copy of the runtime. The
agent invokes the native executable installed above.

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
