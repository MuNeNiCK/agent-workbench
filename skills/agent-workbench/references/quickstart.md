# Quickstart

Use this when starting a project that has no Agent Workbench ledger yet.

Run these commands through the skill wrapper described in `SKILL.md`. The
examples use `agent-workbench ...` as the command spelling; in an installed
skill, execute the same arguments with:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh ...
```

`<installed-skill-dir>` is the directory containing the loaded `SKILL.md`.

## First Run

```sh
agent-workbench init
agent-workbench status
agent-workbench next
agent-workbench rules applicable --scope current
agent-workbench correction list
agent-workbench command list
```

If `status` or `next` reports `phase_blocked: true` or `blocked phase`, follow
the printed blocker-resolution command before asking the agent to implement or
validate code.

If the project already has design material, convert it into the workbench design
package shape under `.agent-workbench/designs/<design-id>` before importing it.
Do not import arbitrary external prose directly as authority.

```sh
agent-workbench design init <design-id> --title "<title>"
agent-workbench design import .agent-workbench/designs/<design-id> --status draft
agent-workbench requirement list --design <design-version-id>
agent-workbench gate-template list --design <design-version-id>
agent-workbench design-decision list --design <design-version-id>
```

## Design To Implementation

1. Add a required design review plan.
2. Build `review-context design-review`.
3. Launch a fresh design review agent with the printed `context_ref`.
4. Record a clean design review run or findings and closures.
5. Run `gate design-ready --dry-run`.
6. Decompose the design.
7. Add a required design task decomposition review plan.
8. Build `review-context design-task-decomposition`.
9. Launch a fresh decomposition review agent with the printed `context_ref`.
10. Run `gate implementation-ready --dry-run`.
11. Run `next` and implement through the same work unit that owns the
    decomposed tasks, checklists, validation gates, and review plans.
12. If `next` reports a blocked phase, resolve the printed finding, review,
    gate, or work-unit blocker first.
13. If `next` reports an open inactive work unit, run the exact printed
    `work activate <work-unit-id> --design-version <design-version-id>`
    command. If it reports suspended work, run the printed resume-check and
    resume commands.
14. If the CLI cannot continue, activate, or resume that same work unit, report
    the workflow blocker. Do not create an unrelated work unit with
    `work start`, and do not inspect the ledger directly.

```sh
agent-workbench review plan add --work-unit <work-unit-id> --type design_review --stage design-ready --design-version <design-version-id> --required
agent-workbench review-context design-review --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean
agent-workbench gate design-ready --design-version <design-version-id> --dry-run
agent-workbench decompose design <design-version-id> --work-unit <work-unit-id>
agent-workbench checklist list
agent-workbench stale list
agent-workbench review plan add --work-unit <work-unit-id> --type design_task_decomposition --stage implementation-ready --design-version <design-version-id> --required
agent-workbench review-context design-task-decomposition --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean
agent-workbench gate implementation-ready --design-version <design-version-id> --dry-run
agent-workbench next
agent-workbench work activate <work-unit-id> --design-version <design-version-id>
```

## While Implementing

Before choosing commands, use fixed or preferred command profiles.

```sh
agent-workbench command list
agent-workbench command usage add --profile <profile-name> --result pass --work-unit <work-unit-id>
agent-workbench gate record --gate <gate-id> --result pass --usage <usage-id>
```

For design-derived tasks, record both implementation evidence and coverage.

```sh
agent-workbench evidence add --task <task-id> --design <design-version-id> --requirement <requirement-key> --type file --file <path> --note "<evidence>"
agent-workbench coverage add --design <design-version-id> --requirement <requirement-key> --task <task-id> --status covered --requirement-text "<summary>" --runtime "<runtime evidence>" --tests-or-gates "<validation evidence>"
```

## Close

Use close reviews before closing design-derived work.

```sh
agent-workbench review plan add --work-unit <work-unit-id> --type design_implementation_diff --stage close-ready --design-version <design-version-id> --required
agent-workbench review plan add --work-unit <work-unit-id> --type implementation_review --stage close-ready --design-version <design-version-id> --required
agent-workbench review-context design-implementation-diff --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review-context implementation-review --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench gate close-ready --dry-run
agent-workbench work close --summary "<summary>"
```
