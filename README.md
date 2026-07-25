# Agent Workbench

Agent Workbench is a Lean-verified state machine and native CLI for durable
coding-agent work. It keeps work, design, review, evidence, interruption,
recovery, and completion state outside the managed project's implementation.

The Lean implementation is the default product from `v0.2.0`.

## Install

The published binary supports Linux x86_64 without requiring a host-provided
glibc or other dynamic C runtime.

```bash
version=v0.2.1
gh release download "$version" \
  --repo MuNeNiCK/agent-workbench \
  --pattern "agent-workbench-$version-linux-x86_64-static.tar.gz"
tar -xzf "agent-workbench-$version-linux-x86_64-static.tar.gz"
install -Dm755 agent-workbench "$HOME/.local/bin/agent-workbench"
agent-workbench --version
```

Install the Agent Skill separately:

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.1 \
  --scope user --agent <target-agent>
```

Then ask the agent:

```text
Use $agent-workbench for this project and initialize its state.
```

## Build from source

Requirements: Git, a C toolchain, and the Lean toolchain selected by
`lean-toolchain`.

```bash
lake build
.lake/build/bin/kernel-laws
.lake/build/bin/storage-laws
.lake/build/bin/workflow-laws
.lake/build/bin/cli-laws
```

## Documentation

User and operator documentation is published at
<https://munenick.github.io/agent-workbench/>.

## License

MIT
