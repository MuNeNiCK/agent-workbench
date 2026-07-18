---
name: agent-workbench
description: Use when managing long-running coding-agent work with durable project-local tasks, phases, reviews, corrections, evidence, KPT, and interruption recovery.
license: MIT
---

# Agent Workbench

Use Agent Workbench when work must remain understandable and resumable across
agent sessions. The CLI owns workflow state; do not edit its private storage.

## CLI

Resolve this skill directory from `SKILL.md`, then run:

```sh
sh <skill-directory>/scripts/agent-workbench.sh <arguments>
```

Use `--help` on the root command and relevant command group for exact syntax.
The wrapper verifies and runs the binary pinned by `CLI_VERSION`.

## Start and continue work

1. Run `status`, then `next`.
2. If the project is uninitialized, run `init`.
3. If an older supported ledger is reported, first run `update --dry-run`.
   Reset only with an explicit reason using `update --reset --reason <reason>`.
   Keep the printed backup handle. Use `update restore --backup <handle>
   --expected-current <identity>` when the reset must be reversed.
4. Run the exact selected action printed by `next`. Do not substitute a
   different lifecycle mutation.
5. Record work with tasks and phases. Use explicit dependency commands instead
   of relying on ordering implied by prose.

## Reviews and findings

- A review run is a claim, not a workflow decision. Record the owner decision
  separately with `review decide`.
- A required blocked review plan is exited with the exact reasoned `review plan
  waive` command printed by `next` and phase close-ready. It needs no signature,
  trust store, capability, or administrator setup.
- Verification is recorded with `finding verify`; only a separate owner
  `finding decide` accepting a `verified` claim closes the finding.
- Keep closure contracts and remediation links explicit. Do not treat a clean
  sibling plan, KPT item, or reviewer label as an owner decision.

## Corrections and KPT

- Read active corrections before planning or editing.
- A critical correction is complete only after its approved requirement link,
  current fixed-command validation usage, and explicit `correction resolve`.
- KPT items must be converted or dismissed before closing the KPT review. KPT
  never resolves a linked correction by itself.

## Evidence and interruption

- Record commands, finalized repository snapshots, comparisons, and typed work
  record links. Draft repository state is not closure evidence.
- Suspend with the next intended action. Run the read-only `resume-check`; if it
  reports a semantic delta, follow its mapped exit instead of bypassing it.
- Run phase and work close-ready gates before closing. A blocker must include an
  executable action or an exact user decision that is still required.

## Rules

- Use project-local state only. No machine-global configuration is required.
- Normal commands never migrate schema or repair storage. `doctor integrity` is
  read-only; schema replacement is owned only by `update`.
- Reviewer claims are evidence. Only ordinary, reasoned owner commands change
  lifecycle state.
- Prefer the smallest direct command and retain auditable reasons for waivers,
  exceptions, dependency acceptance, and correction exceptions.
