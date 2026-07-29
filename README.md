# Agent Workbench

Agent Workbench is one Agent Skill for carrying accepted project design through
implementation, assurance, review, interruption, and exact completion without
depending on chat memory.

Lean is used where it changes the user's development workflow: selected
project-domain rules become executable contracts and proofs. After a local
design correction, unchanged accepted assurance remains reusable while the
affected design, implementation surface, and review are revisited.

## Install

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.3 \
  --scope user --agent <target-agent>
```

Then ask the agent to use `$agent-workbench` in the project. The Skill acquires
and verifies its Linux x86_64 runtime itself. It downloads the portable pinned
Lean tool only when the project selects formal assurance. Users do not install
or operate a separate CLI, Lean toolchain, or glibc runtime.

Project memory is stored under `.agent-workbench`. The using repository—not
Agent Workbench—decides whether that directory is tracked, ignored, or shared.

## Build from source

Use the toolchain pinned by `lean-toolchain`:

```bash
lake build
lake test
scripts/test-skill.sh .lake/build/bin/agent-workbench
```

Release validation additionally exercises the packaged formal tool through the
installed Skill route.

Documentation is published at <https://munenick.github.io/agent-workbench/>.

## License

MIT
