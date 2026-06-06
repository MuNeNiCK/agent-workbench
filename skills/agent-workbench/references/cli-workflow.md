# Agent Workbench CLI Workflow

Use this reference when deciding which `agent-workbench` command to run during
normal coding-agent work.

## Start Or Resume

1. Run `agent-workbench status`.
2. Run `agent-workbench next`.
3. Run `agent-workbench rules applicable --scope current`.
4. If resuming suspended work, run
   `agent-workbench gate resume-ready --maturity trace-aware --dry-run`.
5. Use `agent-workbench resume-check --maturity trace-aware` only when the
   resume decision should be recorded.

Use `--maturity repo-aware` when repository snapshots or dirty state affect the
resume decision. Register every relevant repository first; repo-aware resume
expects every registered repository to have comparable suspend and current
snapshots.

## Design To Implementation

1. Create or convert design material into a workbench design package with
   `agent-workbench design init`.
2. Import the package with `agent-workbench design import`.
3. Check design readiness with
   `agent-workbench gate design-ready --dry-run`.
4. Derive implementation tasks with `agent-workbench trace derive-task`.
5. Select validation gates with `agent-workbench gate select`.
6. Check implementation readiness with
   `agent-workbench gate implementation-ready --dry-run`.

## Close Work

1. Record command usage, validation runs, repository state, Git evidence, and
   work record evidence.
2. Run `agent-workbench gate close-ready --dry-run`.
3. If blocked, perform the blocking action printed by the gate before closing.
4. Close tasks before closing the work unit.
5. Create or export work records when the user expects human-readable output.
