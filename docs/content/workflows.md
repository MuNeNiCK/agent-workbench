# Workflows

Agent Workbench is useful when the agent's work has state that should survive
the current chat. These workflows describe what the human should ask for and
what the agent should record.

## Starting work

For non-trivial work, ask the agent to start a work unit:

```text
Use $agent-workbench and start work on expanding the docs.
```

The work unit gives the agent a place to record decisions, command runs,
reviews, evidence, and close readiness.

## Resuming work

When returning to a repository, ask for status before planning:

```text
Use $agent-workbench and report the current status, next action, active work,
rules, corrections, and command profiles.
```

The agent should not rely only on chat history. It should query the ledger and
explain what is active or blocked.

If status or next reports a blocked phase, the agent should resolve the printed
finding, review, gate, or work-unit blocker before implementation. It should
not edit code or record implementation evidence while that phase blocker is
present.

An eligible valid finding from a required close-ready implementation review has
one explicit exception. After a complete closure contract is registered,
`status` prints `finding_remediation: true`, a
`finding_remediation_count`, and every eligible contract; `next` prints the
same deterministic list. The findings stay open while their owning work unit
implements only those scoped fixes. `closure ready` records fix evidence, ends
remediation for that contract, and restores a verification blocker. Stale
design-derived state suppresses all remediation permission until resolved.

Generate the printed `review-context finding-fix`, run an independent resume
review, and record its typed `--finding-result` with one carried finding and
trusted provenance. `finding verify --result` must match that outcome. Failed
verification returns to remediation with a new attempt. Verified or
authority-disposed findings still require a later fresh unbiased clean review.
`closure ready` stores evidence, tests, and commit on the immutable numbered
attempt without rewriting the registered closure contract. After an
interruption, `review run list` exposes `finding_result`, and `next` prints the
concrete matching `finding verify` command.

## Handling interruptions

If the agent finds a blocking issue, it should not silently switch tasks.

Instead:

1. Suspend the current activation with a reason and next action.
2. Open the interrupting work.
3. Finish or abandon the interrupting work.
4. Run a resume check before returning.

This preserves why the original work stopped and what must still be true before
it resumes.

## Reopening work

Reopen earlier work only when its own closure was invalid or incomplete.

Use follow-up work when the earlier work was valid at the time but later work
found a new related issue.

This distinction matters because reopening changes the meaning of the earlier
closure, while a follow-up preserves the original closure and records a new
relationship.

## Design-driven work

When work is based on design requirements:

1. Convert design notes into a Design Package.
2. Import the package into the ledger.
3. Run design review.
4. Decompose requirements into tasks and checklists.
5. Run task decomposition review.
6. Start implementation only after implementation readiness passes.
7. Continue, resume, or activate the same work unit that owns the decomposed
   records. The agent should follow the exact next action printed by
   `agent-workbench next`; it should not open an unrelated new work unit after
   decomposition.
8. Use explicit implementation activation for design-derived implementation:
   `work activate --implementation --design-version <design-version-id>
   <work-unit-id>`. Do not start an unrelated implementation work unit after
   decomposition.

This keeps the agent from implementing stale or unreviewed design material.
When stale records remain intentionally, record the disposition with
`stale accept` or, for closeable stale records such as selected validation
gates, `stale close`.

For large aggregate work units, create ordered work phases and assign tasks
before implementation. `next` reports the next unblocked phase when phases
exist. Cross-phase dependencies override simple phase order and must be
satisfied or authority-accepted before split or rescope. Use phase dry-runs to
inspect the trace bundle and blockers:

```sh
agent-workbench phase inventory <phase-id>
agent-workbench phase rescope --phase <phase-id> --to-work-unit <work-unit-id> --shared-record-policy require-decisions --dry-run
agent-workbench phase split <phase-id> --title "<title>" --reason "<reason>" --shared-record-policy require-decisions --dry-run
```

Grouped phases can be reviewed without splitting by adding a phase target and
using phase-scoped review context. A phase closes with `phase close-ready` and
`phase close`; the aggregate work unit still closes with the normal
`gate close-ready` and `work close` flow.

## Review-driven close

Before closing work, the agent should run close readiness.

Close readiness can require:

- open tasks to be closed or accepted out of scope
- checklist items and active checklists to be closed
- required reviews to be clean
- findings to be classified and closed
- validation commands to be recorded
- repository state to be clean or classified
- work records to link material evidence

If close readiness blocks, the agent should report the blockers instead of
claiming completion.

## Human review

When an agent reports completion, ask for:

- the active work unit that was closed
- the relevant decisions and requirements
- validation commands and outcomes
- review findings and how they were closed
- Git commits or changed files used as evidence

This gives maintainers a compact audit trail without reading the whole chat.

## Repeated corrections

When a user corrects the agent, Agent Workbench can store the correction for the
project or work scope. Future agents should query corrections before planning,
reviewing, or choosing commands.

This prevents the same feedback from being repeated across sessions.
