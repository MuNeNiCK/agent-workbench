# Agent Workbench

Coding agents can lose track of which design is current, reuse an old verification result after the
code changes, guess a project command, or report completion without the evidence the project
requires. These failures become more likely when work spans several sessions or agents.

Agent Workbench is one Agent Skill that keeps the current design, requested outcome, remaining work,
user corrections, project learning, review results, and verification evidence with the repository.
It gives the coding agent a current starting point without treating the whole conversation history as
authority.

## What changes for you

- You describe the outcome in ordinary language; the coding agent constructs and maintains the
  project design.
- A later correction replaces the affected current requirement instead of being lost among old chat
  messages.
- Interrupted work can resume with the same outcome, accepted Design, materialized Plan/Tasks, and
  verification gaps.
- The agent uses project-recorded commands instead of guessing how to build or verify the result.
- Verification results stop counting when the design, checked files, or verification method changes.
- Completion requires the current evidence selected by the project; a verbal “done” is not enough.
- KPT learning remains available to later sessions without silently becoming a new requirement.

For projects that benefit from theorem proving, selected design properties can be checked with Lean.
Lean is acquired and operated by Workbench; the user does not need to install or drive it directly.

## Install

Install the Skill into the repository:

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.10 \
  --agent codex --scope project
```

Replace `codex` with the coding-agent host used by the project. Project scope is recommended so the
installed Skill belongs to this repository rather than changing every project on the machine.

## First use

Ask the coding agent to use the Skill and state the outcome you want. For example:

```text
Use $agent-workbench for this repository. I want <outcome>.
Research the existing project, construct the design, implement it, and verify completion.
```

For an existing Workbench project:

```text
Use $agent-workbench. Read the current project context and continue the focused work.
Tell me if a user decision is actually required.
```

The Skill installs its verified project-local runtime on first use. Workbench state and the private
Design/Plan editing workspace are stored under `.agent-workbench`; a project-installed Skill is
stored under `.agents/skills/agent-workbench`. SQLite is the sole authority after a proposal; later
draft edits do not rewrite Design history. Workbench is not a Git-policy manager, so the repository
must keep private Workbench paths out of version control when that is its policy.

Supported releases are available for Linux x86_64/aarch64, macOS x86_64/aarch64, and Windows x86_64.

## Documentation

- [Start using Agent Workbench](docs/content/getting-started.md)
- [Installation and repository policy](docs/content/installation.md)
- [Everyday workflow](docs/content/workflow.md)
- [Concepts and terminology](docs/content/concepts.md)
- [State and transition reference](docs/content/state-reference.md)
- [Native operation reference](docs/content/operation-reference.md)
- [Lean assurance and its limits](docs/content/assurance.md)
- [Failure and recovery](docs/content/recovery.md)

## License

MIT
