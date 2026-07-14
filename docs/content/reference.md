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
| `AGENT_WORKBENCH_BIN` | Execute an already-built local CLI instead of downloading a release asset. Used for local development and explicit release inspection. |
| `GITHUB_TOKEN` | Optional token for GitHub API and release download requests. |
| `XDG_CACHE_HOME` | Override the cache root. |

When the skill is inside an Agent Workbench source checkout, the wrapper uses
the checkout's already-built `target/debug/agent-workbench` or
`target/release/agent-workbench` before falling back to `CLI_VERSION`.

## Command groups

| Group | Purpose |
| --- | --- |
| `init`, `status`, `next` | Project setup and current work query. |
| `doctor validation-links` | Migration-independent diagnosis, atomic repair, and immutable repair audit for legacy validation links. |
| `work` | Work-unit lifecycle and activation stack operations. |
| `resume-check`, `gate resume-ready` | Recorded and read-only resume evaluation. |
| `rules`, `correction`, `authority` | Project and work-scope operating rules. |
| `command` | Fixed/preferred command profiles, usage, promotion, and deviations. |
| `record` | Structured work records and evidence links. |
| `repository`, `git` | Repository snapshots, Git commits, file changes, and comparisons. |
| `design`, `requirement`, `design-decision`, `gate-template` | Design package import and inspection. |
| `trace`, `decompose`, `checklist`, `stale` | Design-to-task traceability, checklist completion, and explicit stale record disposition. |
| `phase` | Ordered work-phase grouping, phase dependencies, trace decisions, dry-run rescope/split, and phase close. |
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

When `close-ready` reports shadowed rule conflicts, agents should inspect
`rules applicable --scope current`. Lines with `shadowed_by=<id>` identify the
shadowed rule. If the override is intentional, record approval with
`acceptance add --target rule:<shadowed-rule-id> --type explicit_exception`.

## Review modes

| Mode | Use |
| --- | --- |
| `fresh` / `new_unbiased_review` | New unbiased review and completion checks. |
| `resume` / `finding_fix_verification` | Verify known finding closures. |

`review run add` records a review result; it does not execute a review.
Design-derived readiness gates only accept clean context-targeted review runs
with trusted provenance. Use `--provenance external_agent --external-agent-id
<agent-id> --provenance-ref <review-output-ref>` for a separate agent review,
or `--provenance human_review --provenance-ref <review-output-ref>` for a human
review. Self-recorded clean runs do not satisfy gate review evidence.

Finding-fix resume runs target the verification context emitted by
`closure ready` and require `--finding-result
verified|not_fixed|needs_evidence`. They carry exactly one finding. `verified`
is clean; failure outcomes are non-clean. The later `finding verify --result`
must match exactly.

| Closure command | Purpose |
| --- | --- |
| `closure add` | Register a contract while keeping the finding open. |
| `closure ready` | Record fix evidence and create an immutable attempt. |
| `closure supersede` | Authority-replace a registered or incomplete contract. |
| `finding accept-out-of-scope` | Authority-dispose without claiming verification. |

## Readiness gates

| Gate | Purpose |
| --- | --- |
| `design-ready` | Design can move to decomposition. |
| `implementation-ready` | Design-derived tasks and gate selections are ready for implementation. |
| `close-ready` | Active work has required evidence, coverage, closed checklists, reviews, command usage, repository state, and records. |
| `resume-ready` | Suspended work can resume without stale or unresolved assumptions. |

Design-derived implementation should use explicit activation for the work unit
produced by `decompose design`:
`agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>`.
`agent-workbench work start --implementation --design-version ...` is rejected
so agents do not create unbound implementation work.

If a required review plan was created for the wrong scope or is intentionally
not required, record a user, policy, or design authority event and run
`agent-workbench review plan waive <review-plan-id> --reason "<reason>" --authority <authority-event-id>`.
This records an approved exception that readiness gates understand without
private-state changes outside the CLI.

Work phases group tasks inside an aggregate work unit without changing
ownership by default. Use `phase create`, `phase assign`, and `phase inventory`
to define and inspect the grouping. Use `phase rescope --dry-run` or
`phase split --dry-run` before moving a phase to another work unit; dry-run
prints trace records, shared-record decisions, dependency blockers, and exact
next commands. Phase-scoped reviews use
`review plan target add --type phase --phase <phase-id>` and
`review-context ... --phase <phase-id>`. Run
`phase close-ready <phase-id> --dry-run` before `phase close`; aggregate work
still closes through `gate close-ready`.

For carried requirements whose key and content are unchanged across design
versions, close-ready treats the current design's selected validation gate as
covering the older equivalent requirement record for the same task. Agents
should not repair that case by selecting gates against obsolete design records.

When `close-ready` is blocked, use its classified diagnostic output and the
exact follow-up commands it prints. Do not infer private record relationships
or inspect managed state outside the CLI.

## Agent-facing state

Agents should use `status`, `next`, classified inspection commands, and
readiness gates to decide what to do. If the CLI cannot answer a workflow
question, that is a product gap to fix, not permission to inspect private
managed state by another route.

When normal migration reports invalid validation-run links, use `doctor
validation-links` (or explicit `--dry-run`), then `--repair`. Use `--audit` to
list prior repairs. An unrepairable result requires resolving the reported gate
or authority conflict.

`status` and `next` can report a phase blocker. In that state the agent-facing
next action is the printed blocker-resolution command, such as finding
classification, closure, verification, or work unblock. Implementation should
wait until the blocker is gone.
