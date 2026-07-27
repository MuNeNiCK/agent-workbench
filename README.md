# Agent Workbench

Agent Workbench is a Lean-verified state machine and native CLI for durable
coding-agent work. It keeps work, design, review, evidence, interruption,
recovery, and completion state outside the managed project's implementation.

The Lean implementation is the default product from `v0.2.0`.

## Install

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.1 \
  --scope user --agent <target-agent>
```

Then ask the agent:

```text
Use $agent-workbench for this project and initialize its state.
```

That is the complete product installation. The Skill downloads the pinned,
statically linked Linux x86_64 runtime, verifies its published checksum, caches
it internally, and stores project state under `.agent-workbench`. Users do not
install Lean or a separate CLI.

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
