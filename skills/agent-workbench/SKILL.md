---
name: agent-workbench
description: Use when managing long-running coding-agent work with structured project memory, task tracking, design decisions, review loops, work records, and classified Markdown exports across Agent Skills-compatible coding-agent workflows.
license: MIT
---

# Agent Workbench

Use this skill when a project needs durable agent operating state: checklists,
design decisions, review findings, work records, or compatibility notes that
should survive across sessions without forcing the agent to grep old Markdown
logs.

## CLI Entry

This skill uses the Agent Workbench CLI through the Linux x86_64 wrapper bundled
with the installed skill.

Resolve the wrapper path relative to this `SKILL.md` file. Do not assume the
current repository has a top-level `scripts/` directory. If this skill is
installed at `<installed-skill-dir>/SKILL.md`, run:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh <agent-workbench-args>
```

The wrapper uses the bundled `CLI_VERSION` file by default, downloads the
matching Linux x86_64 release asset, verifies its SHA256 checksum, caches it
under the user's cache directory, and then executes it.

When the skill lives inside an Agent Workbench source checkout, the wrapper
first uses the checkout's already-built `target/debug/agent-workbench` or
`target/release/agent-workbench` binary when present. This keeps project-scope
development installs aligned with the source tree under review. A normal
installed skill without a source checkout continues to use `CLI_VERSION`.

When references show commands as `agent-workbench ...`, run the same arguments
through the wrapper. For example, `agent-workbench status` means:

```sh
sh <installed-skill-dir>/scripts/agent-workbench.sh status
```

## Command Scope

The wrapped CLI is the command authority. Use `agent-workbench help` and
`agent-workbench <command> --help` through the wrapper when exact syntax matters.
Supported command leaves are enumerated below. Reference recipes may show
additional operands, but must resolve to one of these leaves:

- `agent-workbench init`
- `agent-workbench update inspect`
- `agent-workbench update authority-record`
- `agent-workbench update apply --expected-current <identity>`
- `agent-workbench update decide <inspection> --choice <choice> --authority <event> --reason <reason> --expected-current <identity>`
- `agent-workbench update apply <inspection> --expected-current <identity> --idempotency-key <key>`
- `agent-workbench update restore --backup <handle> --expected-current <identity>`
- `agent-workbench update restore --backup <handle> --expected-current <identity> --idempotency-key <key>`
- `agent-workbench operator release candidate assemble --work <work-unit-id> --version <semver> --commit <reviewed-commit> --expected-current absent --idempotency-key <key>`
- `agent-workbench operator release candidate inspect`
- `agent-workbench operator release publish-source`
- `agent-workbench operator release publish-assets`
- `agent-workbench operator release verify-remote`
- `agent-workbench operator release reconcile`
- `agent-workbench operator release retry`
- `agent-workbench operator release withdraw`
- `agent-workbench operator release supersede`
- `agent-workbench status`
- `agent-workbench next`
- `agent-workbench doctor validation-links`
- `agent-workbench doctor validation-links --repair`
- `agent-workbench doctor validation-links --audit`
- `agent-workbench doctor validation-links repair`
- `agent-workbench doctor validation-links retire`
- `agent-workbench migration task-history plan`
- `agent-workbench migration task-history ambiguity-list`
- `agent-workbench migration task-history authority-record`
- `agent-workbench migration task-history ambiguity-decide`
- `agent-workbench migration task-history apply`
- `agent-workbench migration task-history audit`
- `agent-workbench migration reviewer bind`
- `agent-workbench rules applicable --scope current`
- `agent-workbench authority list`
- `agent-workbench authority add`
- `agent-workbench review run add`
- `agent-workbench review adjudicate`
- `agent-workbench finding decide`
- `agent-workbench verification adjudicate`
- `agent-workbench design init`
- `agent-workbench design import`
- `agent-workbench design refresh`
- `agent-workbench design approve`
- `agent-workbench design inspect`
- `agent-workbench requirement list`
- `agent-workbench design-decision list`
- `agent-workbench gate-template list`
- `agent-workbench acceptance add`
- `agent-workbench authority event add`
- `agent-workbench correction add`
- `agent-workbench correction list`
- `agent-workbench decision add`
- `agent-workbench decision list`
- `agent-workbench decision search`
- `agent-workbench decision adjudicate`
- `agent-workbench decision continuation show`
- `agent-workbench decision continuation apply`
- `agent-workbench command fixed add`
- `agent-workbench command prefer`
- `agent-workbench command list`
- `agent-workbench command usage add`
- `agent-workbench command usage list`
- `agent-workbench command usage promote`
- `agent-workbench command deviation add`
- `agent-workbench command deprecate`
- `agent-workbench task add`
- `agent-workbench task list`
- `agent-workbench task close`
- `agent-workbench task accept-out-of-scope`
- `agent-workbench phase create`
- `agent-workbench phase list`
- `agent-workbench phase show`
- `agent-workbench phase assign`
- `agent-workbench phase dependency add`
- `agent-workbench phase dependency list`
- `agent-workbench phase dependency satisfy`
- `agent-workbench phase dependency accept`
- `agent-workbench phase trace list`
- `agent-workbench phase trace decide`
- `agent-workbench phase inventory`
- `agent-workbench phase rescope`
- `agent-workbench phase split`
- `agent-workbench phase close-ready`
- `agent-workbench phase close`
- `agent-workbench phase accept-out-of-scope`
- `agent-workbench decompose design`
- `agent-workbench decomposition show`
- `agent-workbench decomposition import`
- `agent-workbench decomposition validate`
- `agent-workbench decomposition revise`
- `agent-workbench decomposition apply`
- `agent-workbench decomposition reconcile`
- `agent-workbench trace derive-task`
- `agent-workbench trace derivation list`
- `agent-workbench trace derivation rebind`
- `agent-workbench checklist list`
- `agent-workbench checklist item list`
- `agent-workbench checklist item close`
- `agent-workbench checklist close`
- `agent-workbench stale list`
- `agent-workbench stale accept`
- `agent-workbench stale close`
- `agent-workbench gate design-ready --dry-run`
- `agent-workbench gate implementation-ready --dry-run`
- `agent-workbench gate close-ready --dry-run`
- `agent-workbench gate resume-ready --maturity basic|trace-aware|repo-aware --dry-run`
- `agent-workbench gate select`
- `agent-workbench gate record`
- `agent-workbench gate run list`
- `agent-workbench review scope start`
- `agent-workbench review scope list`
- `agent-workbench review policy add`
- `agent-workbench review policy list`
- `agent-workbench review plan add`
- `agent-workbench review plan list`
- `agent-workbench review plan context`
- `agent-workbench review plan waive`
- `agent-workbench review plan supersede`
- `agent-workbench review plan target add`
- `agent-workbench review correction add`
- `agent-workbench review provenance issue`
- `agent-workbench review invocation request`
- `agent-workbench review invocation start`
- `agent-workbench review invocation complete`
- `agent-workbench review invocation fail`
- `agent-workbench review invocation cancel`
- `agent-workbench review result stage`
- `agent-workbench review result finding-add`
- `agent-workbench review result complete`
- `agent-workbench review result cancel`
- `agent-workbench review run add`
- `agent-workbench review run list`
- `agent-workbench review adjudicate`
- `agent-workbench finding add`
- `agent-workbench finding classify`
- `agent-workbench finding decide`
- `agent-workbench finding reopen`
- `agent-workbench finding recover`
- `agent-workbench finding list`
- `agent-workbench finding verify`
- `agent-workbench verification adjudicate`
- `agent-workbench finding accept-out-of-scope`
- `agent-workbench closure add`
- `agent-workbench closure correction-begin`
- `agent-workbench closure transition apply`
- `agent-workbench closure ready`
- `agent-workbench closure supersede`
- `agent-workbench review-context`
- `agent-workbench evidence add`
- `agent-workbench evidence list`
- `agent-workbench coverage add`
- `agent-workbench coverage list`
- `agent-workbench export design`
- `agent-workbench export plan`
- `agent-workbench repository add`
- `agent-workbench repository list`
- `agent-workbench repository snapshot add`
- `agent-workbench repository snapshot list`
- `agent-workbench repository dirty add`
- `agent-workbench repository classify add`
- `agent-workbench repository compare add`
- `agent-workbench repository commit add`
- `agent-workbench repository file add`
- `agent-workbench git commit add`
- `agent-workbench git files add`
- `agent-workbench work start`
- `agent-workbench work activate`
- `agent-workbench work remediate --finding <finding-id>`
- `agent-workbench work suspend`
- `agent-workbench work interrupt`
- `agent-workbench work resume`
- `agent-workbench work reopen`
- `agent-workbench work follow-up`
- `agent-workbench work close`
- `agent-workbench work block`
- `agent-workbench work unblock`
- `agent-workbench work abandon`
- `agent-workbench work fork`
- `agent-workbench resume-check --maturity basic|trace-aware|repo-aware`
- `agent-workbench record create`
- `agent-workbench record list`
- `agent-workbench record command add`
- `agent-workbench record commit add`
- `agent-workbench record file add`
- `agent-workbench record link command`
- `agent-workbench record link commit`
- `agent-workbench record link file`
- `agent-workbench record export <record-id> --output <path>`
- `agent-workbench kpt start --from corrections`
- `agent-workbench kpt list`
- `agent-workbench kpt close`
- `agent-workbench kpt item add`
- `agent-workbench kpt item list`
- `agent-workbench kpt item convert`
- `agent-workbench kpt item dismiss`

Load only the reference needed for the current operation:

- `references/quickstart.md` for first project setup and the normal
  design-to-implementation path.
- `references/cli-workflow.md` for the compact command sequence during normal
  coding-agent work.
- `references/review-recipes.md` for review role selection, fresh/resume review
  rules, finding lifecycle, and completion review prompt requirements.
- `references/interruption-recovery.md` for suspend, interrupt, resume, reopen,
  follow-up, and fork flows.
- `references/repository-validation.md` for validation, repository, Git, and
  work-record evidence commands.
- `references/close-ready-troubleshooting.md` when `close-ready` is blocked.
- `references/state-recovery.md` when normal commands report invalid
  validation-run project links or when historical phase-bearing task data needs
  the explicit task-history migration.

## References

Use the local design references in this repository when changing the skill or
CLI. Do not treat old Markdown exports as standing policy unless they are linked
from current managed project state.

## Rules

- Prefer structured project memory over broad Markdown history searches.
- Use `agent-workbench status` and `agent-workbench next` before long-running
  work, executed through the wrapper path resolved relative to this skill.
- If `status` or `next` reports `finding_remediation: true` or
  `finding remediation`, inspect `finding_remediation_count`, then run
  `agent-workbench finding list --status open` to explicitly retrieve the
  project-internal closure contracts. Implementation is allowed only in the
  printed owning work unit and only to satisfy those contracts. Keep the findings open. After implementing and
  testing each fix, run its exact printed `closure ready` command; do not launch
  resume review before that boundary. Stale design-derived state always takes
  precedence over this exception.
- If `status` or `next` reports `phase_blocked: true` or `blocked phase`, do
  not start implementation, edit code, record implementation evidence, or run
  implementation validation as the next action. Follow the printed blocker
  command for finding classification, closure, verification, gate repair, or
  work unblock first.
- A printed `work remediate --finding <id>` is the sole activation path for an
  eligible registered implementation finding, including an inactive owner.
  Run that exact command before editing. It creates or reuses only the audited
  scoped activation selected by the resolver; do not substitute `work activate`,
  `work reopen`, or changes outside the supported CLI.
- In `finding_remediation`, edit only the listed closure contracts. `work
  suspend`, `work block`, and `work abandon` are the only work lifecycle
  alternates; they apply only to the active bound owner. Finish a selected
  finding with `closure ready`, then run the exact finding-fix resume review and
  matching `finding verify` before relying on a later fresh review.
- In `source_correction`, edit only the printed typed Markdown surfaces after
  `closure correction-begin`. Apply declared `transition:` tokens in ordinal
  order with `closure transition apply`; do not run the underlying task,
  phase, dependency, or stale mutation directly. A
  `decomposition-plan-reconcile` token is the one exception: run the printed
  `decomposition show`, then its exact `decomposition reconcile --dry-run` and
  printed execute action; the reconciliation atomically consumes that token.
  Run `closure ready` only after every file postcondition and transition token
  is complete.
- For legacy partial or duplicate current decomposition, use only a declared
  `design-reconcile:<design>/<work>/<canonical-checklist>` transition. Apply
  stale tokens first, reconcile before using generated aliases, replace phase
  memberships before predecessor or rejected numeric task disposition, and
  keep normal `design-decompose` strict. Reconciliation may replace a uniquely
  matched completed predecessor membership inside a closed phase with the
  canonical task while preserving the phase's closed state; never create an
  open replacement phase for completed work. If the registered closure lacks these
  tokens, use authority-backed `closure supersede`; do not improvise commands.
- After `closure ready`, generate `review-context finding-fix`, record the
  independent verification claim with `review run add`, and record the owner's
  separate decision with `verification adjudicate`. A resume review never replaces
  the later fresh unbiased completion review.
- Use `agent-workbench rules applicable --scope current` before acting on a
  resumed or interrupted work unit.
- Before planning, editing, or reviewing, run `agent-workbench correction list`
  and apply active user corrections for the current scope.
- Treat Design Package authority as Markdown-only. Every manifest path under
  `arc42`, `requirements`, and `validation` must name a regular `.md` file.
  Keep JSON, test vectors, fixtures, generated output, binaries, and other data
  outside the Design Package; specify their required behavior in Markdown and
  validate them through repository evidence.
- Before choosing validation or test commands, run `agent-workbench command list`
  and prefer applicable fixed or preferred command profiles.
- Treat managed project state as a private implementation detail. Do not use
  unsupported tools, internal queries, or implementation names during
  normal agent workflow. Use Agent Workbench CLI commands and `review-context`.
  If a required state question has no CLI or review-context answer, report the
  missing product surface as a blocker instead of bypassing the CLI.
- If normal commands report invalid validation project links, use
  `agent-workbench doctor validation-links` for read-only diagnosis.
  Run `agent-workbench doctor validation-links --repair` only when the complete
  plan is repairable. It preserves a recovery point, repairs known
  parent/dependent links atomically, records an immutable audit, and
  commits only after migration and integrity validation pass. Use `--audit` to
  list prior repairs. An unrepairable result is a blocker, not permission to
  inspect or alter private managed state outside the CLI.
- Historical task migration is an explicit operator action. Normal task, phase,
  refresh, reconciliation, status, and resume commands must never apply it.
  Follow `references/state-recovery.md`, bind every mutating step to the opaque
  handles printed by the read-only plan, and stop on a stale handle or unresolved
  ambiguity.
- When repeated corrections, command drift, recurring findings, or recurring
  close/resume failures appear, propose or run `agent-workbench kpt start` and
  inspect items with `agent-workbench kpt item list`.
- Use read-only gates before state-changing steps: `design-ready`,
  `implementation-ready`, `close-ready`, and `resume-ready`.
- For design-derived implementation work, create the required review plans and
  clean runs before relying on gates: `design_review` for `design-ready`,
  `design_task_decomposition` for `implementation-ready`, and both
  `design_implementation_diff` plus `implementation_review` for `close-ready`.
- After a fresh design review claim, record the owner's separate
  `review adjudicate` decision, run `gate design-ready --dry-run`, and explicitly
  run `design approve <design-version-id>`. Do not import, generate, or apply a
  Decomposition Plan for a merely imported Design Version.
- For the canonical branch of an approved Design Version, run `decomposition
  show`, follow its exact pathless or authored import/validate/revise actions,
  obtain and owner-adjudicate the fresh `design_task_decomposition` review, then
  run the exact `decomposition apply` action.
- Installed `decompose design` is the sole compatibility branch after approval.
  It intentionally preserves the atomic generated graph and matching applied
  compatibility Plan before the review; do not apply it again. Obtain and
  owner-adjudicate the same fresh exact-Plan review afterward. Neither branch
  may rely on implementation-ready before that accepted review.
- Design-derived implementation work must use explicit implementation
  activation for the work unit owned by the applied Decomposition Plan:
  `work activate --implementation --design-version <design-version-id>
  <work-unit-id>`. `work start --implementation --design-version ...` and
  plain reserved aliases such as `work start implementation` are not valid
  implementation paths.
- Prefer the explicit Decomposition Plan lifecycle for normal design-to-plan
  conversion. Use `decompose design` only as the installed compatibility entry,
  and `trace derive-task` for explicit manual links or corrections; neither
  bypasses approval, exact-Plan review, owner adjudication, or implementation readiness.
- When a valid close-ready finding identifies only an incorrect checklist
  boundary on an existing completed task derivation, first obtain the numeric
  requirement ID from `requirement list --design <design-version-id>` and add
  the finding with the exact `--design-requirement <id> --task <task-id>`
  target pair. Enter the printed implementation remediation and use `trace derivation rebind --design
  <design> --requirement <key> --task <task> --checklist-item <item> --closure
  <closure> --reason "<reason>"`. Do not reopen the task, create a duplicate
  derivation, use a targetless finding, or use the completion-text revision
  option for this case.
- A source-correction closure may carry a generated checklist across an
  approved successor design with an earlier `design-decompose:<design>/<work>`
  transition followed by
  `design-reconcile:<design>/<work>/@checklist`. The alias is resolved only
  from the same correction session and only after the producing transition.
- When an aggregate work unit needs internal scheduling or smaller reviewable
  chunks, use work phases instead of inventing task order from intuition. Create
  ordered phases with `phase create`, assign tasks with `phase assign`, inspect
  trace bundles with `phase inventory`, and run `phase rescope --dry-run` or
  `phase split --dry-run` before any cross-work-unit move. Cross-phase
  dependencies outrank simple phase order and must be satisfied or
  authority-accepted with phase dependency commands.
- For grouped phase reviews that remain inside the aggregate work unit, add a
  phase review target with
  `review plan target add --plan <id> --type phase --phase <phase-id>` and use
  `review-context <kind> --design-version <id> --work-unit <id> --phase <phase-id>`.
  Split phase work units use normal work-unit-scoped review contexts.
- Run `phase close-ready <phase-id> --dry-run` before `phase close`. Closing a
  phase does not close the aggregate work unit; aggregate close still requires
  `gate close-ready --dry-run`.
- Use `stale list` to inspect stale design-derived records. Use
  `stale accept <record-type> <id> --reason "<reason>"` when a stale record
  should remain auditable without changing its lifecycle status. Use
  `stale close <record-type> <id> --reason "<reason>"` for stale
  `task_derivation`, `checklist`, or `validation_gate` records that should be
  closed. Do not use `task accept-out-of-scope` for stale validation gates;
  that command is only for task scope exceptions.
- Record implementation evidence and coverage items for design-derived tasks,
  then close completed checklist items with `checklist item close <item-id>`
  and close their parent checklist with `checklist close <checklist-id>`.
  Use `checklist item list --checklist <checklist-id>` to inspect items. Do
  not use stale disposition commands for non-stale checklist completion.
- Use `review-context` when launching a review agent so the prompt is focused
  on the relevant design version, work unit, or review kind. Pass the printed
  target value to `review run add --target`. The command
  records a completed review; it does not launch or perform the review.
  Design-derived gates require clean fresh runs tied to that context with
  trusted provenance such as `--provenance external_agent --external-agent-id
  <agent-id> --provenance-ref <review-output-ref>` or
  `--provenance human_review --provenance-ref <review-output-ref>`.
  Self-recorded clean runs must not be used to satisfy readiness gates.
- If a required review plan was created for the wrong scope, is no longer
  required, or is intentionally superseded, do not inspect or alter private
  managed state. Run `agent-workbench review plan waive <review-plan-id>
  --reason "<reason>"`. The reasoned user action is the audit record.
- For final completion checks, use a fresh unbiased review unless the user
  explicitly asks only for resume verification of known findings. Do not use a
  resume review as the final completion signal by default.
- After close-ready, use only the main binary's `operator release` lifecycle to
  publish a release. `candidate assemble` builds and binds the reviewed commit;
  `candidate inspect` rechecks the exact local files; source, asset, and remote
  steps then use the opaque revision and idempotency key printed by the
  resolver. After an interrupted effect, run the printed `reconcile` command;
  run `retry` only when reconciliation selected it. Never delete or overwrite a
  remote tag or asset to escape a conflict. Build scripts and observer CI do
  not establish release completion.
- Run the entire release lifecycle in the same managed project root and ledger
  that recorded the implementation, findings, verification, and close-ready
  boundary. Never initialize or reconstruct replacement Workbench state in a
  clone, worktree, or temporary directory to qualify or publish a release.
- When asking a review agent whether the project is complete, include the skill
  package itself in the review scope. A project is not complete if the CLI and
  tests match the design but the installed skill cannot guide a fresh coding
  agent through setup, reviews, implementation, interruption recovery,
  close-ready troubleshooting, and evidence recording.
- Use `resume-check` only when a durable result should be recorded for an actual
  resume operation.
- Record reusable validation commands with `command fixed add` and command runs
  with `command usage add` so work records can link to stable evidence.
- Record selected validation gate results with `gate record`; list prior runs
  with `gate run list`.
- Register every Git boundary that can affect the task. For repo-aware resume,
  every registered repository needs suspend and current snapshots plus classified
  comparisons.
- Record repository snapshots, dirty entries, classifications, resume
  comparisons, close comparisons, commits, and file changes when repository
  state matters to close or resume decisions.
- Create work records with `record create` and link commands, commits, and files
  when they are available. Manual commit/path links can be recorded first; later
  `repository commit add` and `repository file add` entries backfill structured
  Git identities when the match is unambiguous.
- Use `work suspend`, `work interrupt`, `work reopen`, and `work follow-up` to
  preserve the activation stack before switching tasks.
- Treat accepted design decisions as durable constraints until the user changes them.
