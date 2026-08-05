# Everyday workflow

Workbench does not require the user to follow a fixed phase sequence. The user states outcomes and
corrections; the coding agent owns ordinary design, implementation, state transitions, and evidence
collection.

## Start an outcome

```text
Use $agent-workbench for this repository. I want <outcome>.
Research the project, explain material design choices, implement the result, and verify completion.
```

The agent should recover current context before acting. For a new project it constructs and accepts
an appropriate design before starting Work. It may add required Tasks when they clarify the outcome,
but should not create ceremony merely to fill a workflow.

## Continue after a session boundary

```text
Use $agent-workbench. Continue the focused work from current project state.
```

The response should identify the outcome and remaining result, not merely print internal IDs. Old
conversation text is not used as current authority when Workbench state says it was replaced.

## Correct a misunderstanding

```text
The accepted behavior is <correct behavior>, not <previous interpretation>.
Record the correction and update the design before continuing.
```

The agent records the correction immediately. If it changes design, it suspends Work, constructs and
accepts a strict successor, records the impact, adopts that successor into the same Work, and resumes.
The user does not manually choose database transitions.

## Interrupt and resume

To pause one outcome, ask the agent to preserve it with a concrete return condition. Workbench keeps
the same outcome, design binding, tasks, evidence, and findings. Resuming returns to that Work only
when its accepted design remains current; otherwise the agent must inspect and adopt the successor
first.

## Hand work to another agent run

A handoff changes responsibility without replacing Work. Ask the current agent to record why and to
transfer the same focused Work to the successor run. Evidence and review boundaries remain attached
to the original Work.

## Verify project results

The agent should use an applicable Command Profile rather than guess a build or test command. It first
shows the resolved argv and then executes that same profile. Artifact criteria are observed against
their current target. Selected Lean claims are checked through Workbench's pinned project-local
toolchain.

If a target, profile, design, or proof input changes, earlier evidence becomes stale and the agent
must obtain evidence for the new current state.

## Use review when it adds evidence

Review is not a mandatory loop around every task. Use Design Review for an immutable design and
Implementation Review for a fixed implementation snapshot when the accepted design or risk warrants
it. Findings remain advisory until disposition. See [Reviews](reviews.md).

## Complete

The agent asks Workbench for derived readiness. Readiness requires current design sources, closed
required Tasks, evidence for every criterion, receipts for selected Lean claims, no effective user
correction, and no unresolved accepted finding.

Only then can the focused Work become completed. A clean Git tree, commit, review message, KPT entry,
or agent statement cannot substitute for missing current evidence.
