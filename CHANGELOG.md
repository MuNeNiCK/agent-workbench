# Release history

## v0.1.27 — 2026-07-24

- Preserve exact multi-target review findings and keep legacy single-target
  callers idempotent.
- Freeze the reviewed work boundary and release targets until an explicit
  reconciliation or supersession invalidates the affected review.
- Scope release assembly and revalidation readiness to the selected work unit,
  so unresolved review state in separate work does not block its release.
- Make review-target invalidation transactional and expose public recovery
  paths for completed work.
- Audit generation 26 migrations and retained trigger bodies so upgrades
  cannot silently omit release-integrity guards.

## v0.1.26 — 2026-07-24

- Add an audited public path to rebind a completed task derivation to the
  correct checklist condition during scoped finding remediation.
- Let a typed source correction reconcile a generated checklist across design
  versions through its same-session alias.
- Keep remediation selection identical across status, next actions, mutation
  guards, closure operations, and database constraints, including mixed typed
  corrections, plan disposition, and terminal versus nonterminal acceptances.

## v0.1.25 — 2026-07-23

- Let an ordinary structurally ready Decomposition Plan publish a reviewed
  replacement revision without treating its incomplete predecessor as an
  applied reconciliation source.

## v0.1.24 — 2026-07-23

- v0.1.24 is withdrawn. Its Decomposition Plan fix distinguished ordinary
  ready revisions from reconciliation successors but did not permit the
  ordinary ready revision to publish its corrected successor. Use v0.1.25.
- Keep ordinary incomplete-to-ready Decomposition Plan revisions distinct from
  reconciliation successors, so their accepted ready plan can be applied.
- Require Phase-targeted review plans to derive clean state from exact Phase
  review contexts, and recover existing projects that recorded only an
  aggregate clean run before Phase closure.

## v0.1.23 — 2026-07-22

- Make recovery selection preflight the complete staged transition so it never
  offers a checkpoint that cannot finish the update.
- Recover from the newest verified checkpoint in the formal update/restore
  lineage while preserving explicit owner selection for externally supplied
  ambiguous backups.
- Preserve and rebuild every dependent trigger when replacing a retained table
  during migration; existing generation 24 projects now reach generation 25
  without dropping review, finding, continuation, or owner-decision guards.
- Keep every verified backup discoverable in normal and recovery inspection,
  while running the complete staged update preflight only when a recovery
  source must be selected.
- Follow completed apply/restore direction and sequence when selecting a
  managed recovery checkpoint, reject symlink-substituted or sidecar-dependent
  restore sources, and preserve explicit owner selection for sequence-less or
  external history.
- Validate recovery transitions in memory so inspection never stages a private
  project ledger in shared operating-system temporary storage.
- Close every active or suspended activation epoch owned by a completed work
  unit, and keep release-asset verification hermetic under an external Cargo
  target directory.

## v0.1.22 — 2026-07-22

- Make public-contract verification treat the project-local installed Skill
  copy as optional in clean source checkouts while still requiring exact
  equality when that copy exists.

## v0.1.21 — 2026-07-22

- v0.1.20 was withdrawn. It is not a supported release and has no tag or GitHub
  Release because it removed established Agent Workbench workflows.
- Restore the complete work, Phase, Design Package, review, evidence,
  correction/KPT, repository, and record workflows after the withdrawn change.
- Add explicit `update inspect`, `update apply`, and reversible `update restore`
  commands. Ordinary status and lifecycle commands no longer update projects as
  a side effect.
- Make required Phase review recovery executable from both `phase close-ready`
  and the owner resolver, with an exact reasoned `review plan waive` command and
  no separate authority-event requirement.
- Let trace-aware resume retain the authority references captured at suspend
  while loading newer user directions, instead of blocking on every addition.
- Keep historical review plans, validation gates, and superseded Design Package
  approvals out of the current applicable-rule set.
- Add first-class Decomposition Plans that can be inspected, reviewed, applied,
  and reconciled while preserving checklists, Phases, and task membership. The
  existing `decompose design` workflow remains available as the automatic-plan
  path.
- Replace release-specific update handling with one stable `update inspect` /
  `update apply` / `update restore` flow that carries every supported project
  forward.
- Add the main binary's `operator release` lifecycle for candidate assembly,
  local inspection, annotated source publication, exact asset publication,
  downloaded remote verification, interruption reconciliation, idempotent retry,
  non-destructive withdrawal, and authority-backed supersession.
- Make release recovery detect partial publication, stale handles, and changed
  requests while preserving already-published assets.
- Retire the remaining signing-era owner and reviewer authority fields through
  the registered storage generation 25 transition. Exact accepted legacy
  results remain neutral migration provenance and no longer participate in the
  current review trust model.
- Validate every public command leaf at its parser and command-owner boundary,
  validate documented invocations against the live command tree, and repeat the
  registry checks against the extracted binary, Skill, and documentation
  release artifacts.

## v0.1.19 — 2026-07-18

- Restore simple review claims and separate owner decisions without signatures,
  trust stores, grants, capabilities, or external administrator setup.
- Preserve existing signed decisions as read-only audit history while returning
  to the simple review workflow.
- Restore the v0.1.9-style review workflow and simplify the installed skill.

## v0.1.18 — 2026-07-18

- Make supported existing projects update to the current workflow consistently.

## v0.1.17 — 2026-07-18

- Preserve pre-attempt verification history when updating existing projects.

## v0.1.16 — 2026-07-17

- Separate reviewer claims from owner decisions while preserving existing
  project history.

## v0.1.15 — 2026-07-15

- Require Markdown sources for Design Packages.

## v0.1.14 — 2026-07-14

- Restore maintainable workflow boundaries.

## v0.1.13 — 2026-07-13

- Preserve closed-phase completion during reconciliation.

## v0.1.12 — 2026-07-12

- Recover legacy design decomposition state.

## v0.1.11 — 2026-07-12

- Preserve stable tasks across design refreshes.

## v0.1.10 — 2026-07-10

- Complete finding-remediation lifecycle paths.

## v0.1.9 — 2026-07-10

- Add validation-link recovery diagnostics and repair.
