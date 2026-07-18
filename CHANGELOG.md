# Release history

## Unreleased

- v0.1.20 was withdrawn. It is not a supported release and has no tag or GitHub
  Release because its reduced schema removed established Agent Workbench use
  cases.
- Restore the complete work, Phase, Design Package, review, evidence,
  correction/KPT, repository, and record workflows after the withdrawn change.
- Add explicit `update inspect`, staged `update apply`, and reversible `update
  restore` commands. Ordinary status and lifecycle commands no longer apply
  schema migrations as a side effect.
- Make required Phase review recovery executable from both `phase close-ready`
  and the owner resolver, with an exact reasoned `review plan waive` command and
  no separate authority-event requirement.
- Let trace-aware resume retain the authority references captured at suspend
  while loading newer user directions, instead of blocking on every addition.
- Keep historical review plans, validation gates, and superseded Design Package
  approvals out of the current applicable-rule set.

## v0.1.19 — 2026-07-18

- Restore simple review claims and separate owner decisions without signatures,
  trust stores, grants, capabilities, or external administrator setup.
- Add transactional schema 13 migration while preserving schema 12 signed
  decisions as inert audit history.
- Restore the v0.1.9-style review workflow and simplify the installed skill.

## v0.1.18 — 2026-07-18

- Normalize supported legacy ledger migrations.

## v0.1.17 — 2026-07-18

- Preserve legacy pre-attempt verification history during migration.

## v0.1.16 — 2026-07-17

- Separate reviewer claims from owner decisions and add legacy normalization.

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
