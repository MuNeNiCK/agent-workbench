# Agent Workbench CLI Workflow

Use this reference when deciding which `agent-workbench` command to run during
normal coding-agent work.

## Start Or Resume

1. Run `agent-workbench status`.
2. Run `agent-workbench next`.
3. If either command reports `finding_remediation: true` or
   `finding remediation`, inspect `finding_remediation_count`, run
   `agent-workbench finding list --status open` to retrieve the classified
   contracts, implement only those contracts in their owning work unit, then run each
   printed `closure ready` command. Stale design-derived state takes precedence.
   If either
   command reports `phase_blocked: true` or `blocked phase`, perform
   the printed blocker-resolution command before planning implementation. Do
   not edit code, start implementation work, or record implementation evidence
   until the blocker is resolved and `next` stops reporting the blocked phase.
4. Run `agent-workbench rules applicable --scope current`.
5. If resuming suspended work, run
   `agent-workbench gate resume-ready --maturity trace-aware --dry-run`.
6. Use `agent-workbench resume-check --maturity trace-aware` only when the
   resume decision should be recorded.

Use `--maturity repo-aware` when repository snapshots or dirty state affect the
resume decision. Register every relevant repository first; repo-aware resume
expects every registered repository to have comparable suspend and current
snapshots.

Use `agent-workbench work follow-up` for ordinary additional work after closure.
Use `agent-workbench work reopen <work-unit-id> --reason "<reason>" --reason-type closure_invalid|closure_incomplete|authority_superseded --authority <authority-event-id>`
only when a closed unit itself is invalidated by user, policy, or design
authority.

## Design To Implementation

1. Create or convert design material into a workbench design package with
   `agent-workbench design init <design-id> --title "<title>"`.
   Every manifest-declared `arc42`, `requirements`, and `validation` file must
   be a regular Markdown file ending in `.md`; arbitrary data, test vectors,
   fixtures, and generated output remain outside the Design Package.
2. Import the package with
   `agent-workbench design import <package-path> --status draft`, using the path
   printed by `design init`.
3. Add the required design document review plan with
   `agent-workbench review plan add --work-unit <work-unit-id> --type design_review --stage design-ready --design-version <design-version-id> --required`.
4. Build the design review context with
   `agent-workbench review-context design-review --design-version <design-version-id> --work-unit <work-unit-id>`,
   then run an independent review agent and record its clean result with
   `agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean --provenance external_agent --external-agent-id <agent-id> --provenance-ref <review-output-ref>`.
5. Check design readiness with
   `agent-workbench gate design-ready --dry-run`.
6. Decompose the design with
   `agent-workbench decompose design <design-version-id> --work-unit <work-unit-id>`.
7. Inspect generated planning state with
   `agent-workbench checklist list`,
   `agent-workbench requirement list --design <design-version-id>`, and
   `agent-workbench stale list`.
   If stale records appear, resolve the design mismatch, or explicitly record a
   disposition with
   `agent-workbench stale accept <record-type> <id> --reason "<reason>"` or
   `agent-workbench stale close <record-type> <id> --reason "<reason>"`.
   Use `stale close` only for `task_derivation`, `checklist`, and
   `validation_gate`; use `stale accept` for stale `coverage_item` and
   `review_plan` records.
8. Select validation gates with
   `agent-workbench gate select --design <design-version-id> --template <gate-key> --requirement <requirement-key> --task <task-id>`
   when the decomposition did not already select the required gate. Add
   `--command-profile <name>` and `--timeout <duration>` when a fixed command
   profile should drive the gate.
9. Add the required decomposition review plan with
   `agent-workbench review plan add --work-unit <work-unit-id> --type design_task_decomposition --stage implementation-ready --design-version <design-version-id> --required`.
10. Build the decomposition review context with
    `agent-workbench review-context design-task-decomposition --design-version <design-version-id> --work-unit <work-unit-id>`,
    then run an independent review agent and record its clean result with
    `agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean --provenance external_agent --external-agent-id <agent-id> --provenance-ref <review-output-ref>`.
11. Check implementation readiness with
   `agent-workbench gate implementation-ready --design-version <design-version-id> --dry-run`.
12. Run `agent-workbench next` and implement through the same work unit passed
   to `decompose design` and the required review plans. If `next` reports a
   blocked phase, resolve the printed blocker first. If `next` reports an open
   inactive work unit, run the exact printed
   `agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>`
   command. If `next` reports suspended work, run the printed resume-check and
   resume commands. Do not start an unrelated new work unit after decomposition.
   If `next` cannot identify the correct continue, activate, or resume command,
   report the workflow blocker instead of inspecting private managed state.
13. Treat `task list` as an inventory. Follow explicit implementation plan,
   dependency, wave, checklist, or requirement order. Do not invent a different
   task order from implementation intuition.

## Work Phases

Use work phases when an aggregate work unit needs feature, milestone, timebox,
release-slice, or implementation-wave grouping.

```sh
agent-workbench phase create --work-unit <work-unit-id> --key <key> --title "<title>" --kind <kind> --order <n>
agent-workbench phase assign <phase-id> --task <task-id>
agent-workbench phase dependency add --from <phase-id> --to <phase-id> --type blocks|requires --reason "<reason>"
agent-workbench phase inventory <phase-id>
agent-workbench phase rescope --phase <phase-id> --to-work-unit <work-unit-id> --shared-record-policy require-decisions --dry-run
agent-workbench phase split <phase-id> --title "<title>" --reason "<reason>" --shared-record-policy require-decisions --dry-run
```

If dry-run reports shared trace blockers, resolve them explicitly:

```sh
agent-workbench phase trace list <phase-id>
agent-workbench phase trace decide --phase <phase-id> --record <type:id> --decision split|carry|accept --reason "<reason>" --authority <authority-event-id>
```

Cross-phase dependencies outrank simple phase order. Satisfy or accept them
before split/rescope:

```sh
agent-workbench phase dependency satisfy <dependency-id> --reason "<reason>" --evidence <ref>
agent-workbench phase dependency accept <dependency-id> --reason "<reason>" --authority <authority-event-id>
```

For grouped phase reviews inside the aggregate work unit, target the phase and
use phase-scoped context:

```sh
agent-workbench review plan target add --plan <review-plan-id> --type phase --phase <phase-id>
agent-workbench review-context implementation-review --design-version <design-version-id> --work-unit <work-unit-id> --phase <phase-id>
```

Close phases independently from the aggregate work unit:

```sh
agent-workbench phase close-ready <phase-id> --dry-run
agent-workbench phase close <phase-id> --summary "<summary>"
agent-workbench phase accept-out-of-scope <phase-id> --reason "<reason>" --authority <authority-event-id>
```

Agents must treat managed project state as private during this workflow. Use
Agent Workbench CLI commands and classified inspection output; do not bypass
the supported CLI.

## Close Work

1. Record implementation evidence with
   `agent-workbench evidence add --task <task-id> --design <design-version-id> --requirement <requirement-key> --type file --file <path> --note "<evidence>"`.
2. Record coverage with
   `agent-workbench coverage add --design <design-version-id> --requirement <requirement-key> --task <task-id> --status covered --requirement-text "<requirement summary>" --runtime "<runtime evidence>" --tests-or-gates "<validation evidence>"`.
3. Close completed checklist items and their parent checklist:
   `agent-workbench checklist item list --checklist <checklist-id>`,
   `agent-workbench checklist item close <checklist-item-id>`, then
   `agent-workbench checklist close <checklist-id>`.
   Use `stale close checklist <id>` only for stale checklist disposition, not
   normal completion.
4. Close or accept out-of-scope all tasks derived from the design.
5. Record command usage, validation runs, repository state, Git evidence, and
   work record evidence.
6. Add required close review plans:
   `design_implementation_diff` and `implementation_review`, both at
   `--stage close-ready`, with `--work-unit <work-unit-id>` and
   `--design-version <design-version-id>`.
7. Use `agent-workbench review-context design-implementation-diff` or
   `agent-workbench review-context implementation-review` to launch focused
   review agents. Include the relevant `--design-version` and `--work-unit`
   flags, then pass the printed review target to `review run add --target`.
8. Record clean close review runs or record findings, closures, and
   verifications until the configured review policy is satisfied. Every
   `review run add` command must include `--plan <review-plan-id>` and trusted
   provenance, for example
   `--provenance external_agent --external-agent-id <agent-id> --provenance-ref <review-output-ref>`.
9. Run `agent-workbench gate close-ready --dry-run`.
10. If blocked, perform the blocking action printed by the gate before closing.
   For stale validation gates, do not use `task accept-out-of-scope`; run
   `agent-workbench stale close validation_gate <gate-id> --reason "<reason>"`
   when the selected stale gate should be closed, or
   `agent-workbench stale accept validation_gate <gate-id> --reason "<reason>"`
   when the stale state is intentionally accepted without changing gate status.
   To accept an exception, first record the user's approval with
   `agent-workbench authority event add --type user_instruction --summary "<approval>"`,
   then run
   `agent-workbench acceptance add --target <kind:id> --type <acceptance-type> --reason "<reason>" --authority <authority-event-id>`.
   To intentionally defer a repeated user correction, use
   `agent-workbench acceptance add --target stale:user_correction:<correction-id> --type stale_accepted --reason "<reason>" --authority <authority-event-id>`.
   To intentionally accept a shadowed rule, run
   `agent-workbench rules applicable --scope current` and then
   `agent-workbench acceptance add --target rule:<shadowed-rule-id> --type explicit_exception --reason "<reason>" --authority <authority-event-id>`.
   If a required review plan was created for the wrong scope or is intentionally
   not required, run
   `agent-workbench review plan waive <review-plan-id> --reason "<reason>"`
   after recording the authority event.
   For design-package exceptions, include either `--design <design-version-id>`
   or `--package <design-id>` in that `acceptance add` command.
   When converting KPT items into fixed command profiles, use the same authority
   pattern and pass `--authority <authority-event-id>` with
   `agent-workbench kpt item convert --to command-profile --command-status fixed`.
11. Create or export work records when the user expects human-readable output.
12. Close the work unit only after `close-ready` passes.
