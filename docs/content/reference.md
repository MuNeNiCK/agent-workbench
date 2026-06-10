# Reference

## Installed skill wrapper

The installed skill includes `scripts/agent-workbench.sh`.

People normally ask their agent to use `$agent-workbench`. The agent follows the
skill instructions and uses the wrapper internally.

## Wrapper environment variables

| Variable | Description |
| --- | --- |
| `AGENT_WORKBENCH_REPO` | Override the GitHub repository used for release downloads. |
| `AGENT_WORKBENCH_VERSION` | Pin the release tag used by the wrapper. |
| `GITHUB_TOKEN` | Optional token for GitHub API and release download requests. |
| `XDG_CACHE_HOME` | Override the cache root. |

## Command groups

| Group | Purpose |
| --- | --- |
| `init`, `status`, `next` | Project setup and current work query. |
| `work` | Work-unit lifecycle and activation stack operations. |
| `resume-check`, `gate resume-ready` | Recorded and read-only resume evaluation. |
| `rules`, `correction`, `authority` | Project and work-scope operating rules. |
| `command` | Fixed/preferred command profiles, usage, promotion, and deviations. |
| `record` | Structured work records and evidence links. |
| `repository`, `git` | Repository snapshots, Git commits, file changes, and comparisons. |
| `design`, `requirement`, `design-decision`, `gate-template` | Design package import and inspection. |
| `trace`, `decompose`, `checklist`, `stale` | Design-to-task traceability and explicit stale record disposition. |
| `review`, `finding`, `closure`, `review-context` | Review planning, runs, findings, closures, and focused context. |
| `evidence`, `coverage`, `gate` | Implementation evidence, coverage, validation gates, and readiness checks. |
| `kpt` | Process review over corrections, command drift, findings, and outcomes. |

## Rule precedence

```text
latest user instruction
  > active work-unit rule
  > project or repository rule
  > skill default
  > historical export or work record
```

## Review modes

| Mode | Use |
| --- | --- |
| `fresh` / `new_unbiased_review` | New unbiased review and completion checks. |
| `resume` / `finding_fix_verification` | Verify known finding closures. |

## Readiness gates

| Gate | Purpose |
| --- | --- |
| `design-ready` | Design can move to decomposition. |
| `implementation-ready` | Design-derived tasks and gate selections are ready for implementation. |
| `close-ready` | Active work has required evidence, reviews, command usage, repository state, and records. |
| `resume-ready` | Suspended work can resume without stale or unresolved assumptions. |
