# Quickstart

Use this when starting a project that has not initialized Agent Workbench yet.

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

If it reports `finding_remediation: true` or `finding remediation`, inspect
`finding_remediation_count`, run `agent-workbench finding list --status open`
to retrieve the classified closure contracts, and implement only those contracts in
their owning work unit, then run each printed `closure ready` command. After
that boundary, use the exact finding-fix contexts, independent typed resume
reviews, and matching `finding verify` results. Stale design state blocks this
permission.

If the project already has design material, convert it into the workbench design
package shape at a project-local location printed by the command before importing it.
Do not import arbitrary external prose directly as authority.
All paths declared under `arc42`, `requirements`, and `validation` must be
regular Markdown files ending in `.md`; keep JSON, fixtures, test vectors, and
generated output outside the Design Package.

```sh
agent-workbench design init <design-id> --title "<title>"
agent-workbench design import <package-path> --status draft
agent-workbench requirement list --design <design-version-id>
agent-workbench gate-template list --design <design-version-id>
agent-workbench design-decision list --design <design-version-id>
```

## Design To Implementation

1. Add a required design review plan.
2. Build `review-context design-review`.
3. Launch a fresh design review agent with the printed review target.
4. Record a clean design review run or findings and closures.
5. Run `gate design-ready --dry-run`.
6. Decompose the design.
7. Add a required design task decomposition review plan.
8. Build `review-context design-task-decomposition`.
9. Launch a fresh decomposition review agent with the printed review target.
10. Run `gate implementation-ready --dry-run`.
11. Run `next` and implement through the same work unit that owns the
    decomposed tasks, checklists, validation gates, and review plans.
12. If `next` reports a blocked phase, resolve the printed finding, review,
    gate, or work-unit blocker first.
13. If `next` reports an open inactive work unit, run the exact printed
    `work activate --implementation --design-version <design-version-id> <work-unit-id>`
    command. If it reports suspended work, run the printed resume-check and
    resume commands.
14. If the CLI cannot continue, activate, or resume that same work unit, report
    the workflow blocker. Do not create an unrelated work unit with
    `work start`, and do not inspect private managed state outside the CLI.

```sh
agent-workbench review plan add --work-unit <work-unit-id> --type design_review --stage design-ready --design-version <design-version-id> --required
agent-workbench review-context design-review --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean --provenance external_agent --external-agent-id <agent-id> --provenance-ref <review-output-ref>
agent-workbench gate design-ready --design-version <design-version-id> --dry-run
agent-workbench decompose design <design-version-id> --work-unit <work-unit-id>
agent-workbench checklist list
agent-workbench stale list
agent-workbench review plan add --work-unit <work-unit-id> --type design_task_decomposition --stage implementation-ready --design-version <design-version-id> --required
agent-workbench review-context design-task-decomposition --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean --provenance external_agent --external-agent-id <agent-id> --provenance-ref <review-output-ref>
agent-workbench gate implementation-ready --design-version <design-version-id> --dry-run
agent-workbench next
agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>
```

## Work Phases

For large aggregate work units, group tasks before implementation instead of
using `task list` as a scheduler.

```sh
agent-workbench phase create --work-unit <work-unit-id> --key <key> --title "<title>" --kind <kind> --order <n>
agent-workbench phase assign <phase-id> --task <task-id>
agent-workbench phase inventory <phase-id>
agent-workbench phase rescope --phase <phase-id> --to-work-unit <work-unit-id> --shared-record-policy require-decisions --dry-run
agent-workbench review plan target add --plan <review-plan-id> --type phase --phase <phase-id>
agent-workbench review-context implementation-review --design-version <design-version-id> --work-unit <work-unit-id> --phase <phase-id>
agent-workbench phase close-ready <phase-id> --dry-run
```

Use `phase dependency satisfy` or `phase dependency accept` when a dry-run
reports cross-phase dependency blockers. Use `phase trace decide` when a
dry-run reports shared trace records that must be split, carried, or accepted.

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
agent-workbench checklist item list --checklist <checklist-id>
agent-workbench checklist item close <checklist-item-id>
agent-workbench checklist close <checklist-id>
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
