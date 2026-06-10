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

This keeps the agent from implementing stale or unreviewed design material.
When stale records remain intentionally, record the disposition with
`stale accept` or, for closeable stale records such as selected validation
gates, `stale close`.

## Review-driven close

Before closing work, the agent should run close readiness.

Close readiness can require:

- open tasks to be closed or accepted out of scope
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
