# Review Recipes

Use this reference when launching or recording review agents.

## Role Selection

| Review type | Use for | Must check |
| --- | --- | --- |
| `design_review` | Before design-ready | Internal contradictions, missing requirements, invalid decisions, unclear validation expectations |
| `design_task_decomposition` | Before implementation-ready | Every active requirement has a task, completion condition, checklist coverage, and selected validation gate |
| `design_implementation_diff` | Before close-ready | Implementation evidence and coverage match the design requirements with no missing or extra behavior |
| `implementation_review` | Before close-ready | Language idioms, maintainability, security, error handling, tests, integration quality |

Design-diff review is not a general implementation quality review.
Implementation review is not allowed to waive design requirements.

## Fresh Versus Resume

Use `fresh` for unbiased review:

- final completion checks
- design-ready and implementation-ready clean runs
- close-ready clean runs unless the user explicitly asks only to verify known
  fixes
- any time the current thread may have biased the reviewer

Use `resume` only to verify whether known findings were fixed:

- pass the prior finding and closure context
- do not allow new unrelated findings unless the review policy allows them
- do not use resume review as the final unbiased completion check unless the
  user explicitly authorizes it

## Required Context

Every clean run for a design-derived gate must target a generated review
context.

```sh
agent-workbench review-context design-review --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review-context design-task-decomposition --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review-context design-implementation-diff --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review-context implementation-review --design-version <design-version-id> --work-unit <work-unit-id>
agent-workbench review-context implementation-review --design-version <design-version-id> --work-unit <work-unit-id> --phase <phase-id>
```

Use `--phase <phase-id>` only for grouped phases that remain inside the
aggregate work unit. Split phase work units use the normal work-unit-scoped
context for the child work unit.

Issue a project-local provenance claim, request the invocation, and publish the
independent claim. Reviewer output remains advisory until the owner records a
separate decision:

```sh
agent-workbench review provenance issue --reviewer <reviewer-id> --plan <review-plan-id> --target <context-ref> --kind external_agent --purpose new_unbiased_review --reference <review-output-ref> --idempotency-key <key>
agent-workbench review invocation request --plan <review-plan-id> --target <context-ref> --reviewer <reviewer-id> --provenance <provenance-handle> --purpose new_unbiased_review --idempotency-key <key> --expected-plan-current open
agent-workbench review invocation complete <invocation-id> --claim clean --summary "<summary>" --expected-current requested --idempotency-key <key>
agent-workbench review adjudicate <review-run-id> --decision accepted --reason "<reason>" --expected-current pending
```

The claim and owner decision remain separate records. No key, signature, grant,
principal resolution, or capability bootstrap is required.

For a review with findings, create a result stage, add every finding, and
publish the stage atomically:

```sh
agent-workbench review result stage <invocation-id> --expected-current requested --idempotency-key <key>
agent-workbench review result finding-add <stage-handle> --type <finding-type> --severity <severity> --description "<description>" --expected-current <version-handle> --idempotency-key <key>
agent-workbench review result complete <stage-handle> --expected-findings <count> --summary "<summary>" --expected-current <version-handle> --invocation-current requested --idempotency-key <key>
```

Use `review invocation fail|cancel` and `review result cancel` for explicit
terminal outcomes. Exact retry with the same idempotency key returns the same
recorded result; changed payloads are rejected.

## Finding Lifecycle

When a review finds a problem:

```sh
agent-workbench finding add --run <review-run-id> --type <finding-type> --severity <severity> --description "<description>"
agent-workbench finding decide <finding-id> --decision accepted --reason "<reason>" --expected-current pending
agent-workbench closure add --finding <finding-id> --invariant "<what must now hold>" --surfaces "<affected surfaces>" --fix-plan "<fix>" --tests "<tests>" --verification "<resume review>"
```

If the remediation will mutate an existing design-derived record, register its
exact typed target when adding the finding. Obtain the numeric requirement ID
from `requirement list --design <design-version-id>`, then add each intended
requirement/task pair with `--design-requirement <id> --task <task-id>`.
Targetless findings do not authorize derivation rebinds.

For an eligible close-ready implementation finding, closure registration opens
scoped remediation while the finding remains open:

```sh
agent-workbench work remediate --finding <finding-id>
# implement and test only the printed remediation scope
agent-workbench closure ready <closure-id> --evidence "<fix evidence>" --tests "<test evidence>" --commit <sha>
agent-workbench review-context finding-fix --finding <finding-id> --closure <closure-id> --attempt <attempt-id>
agent-workbench review run add --plan <plan-id> --type resume --purpose finding_fix_verification --target <context-ref> --status completed --clean --new-findings 0 --carried-findings 1 --summary "<summary>" --finding-result verified --provenance external_agent --external-agent-id <reviewer-id> --provenance-ref <review-output-ref>
agent-workbench verification adjudicate --run <run-id> --finding <finding-id> --closure <closure-id> --attempt <attempt-id> --decision accepted --reason "<reason>" --expected-current pending
```

`closure ready` evidence belongs to the immutable numbered attempt and does
not rewrite the registered closure contract. Use
`review run list --plan <plan-id>` to recover the persisted `finding_result`;
`next` then prints the concrete matching `finding verify` command.

Use a non-clean resume run and matching `--finding-result not_fixed` or
`needs_evidence` when verification fails. That returns the closure to
remediation and requires a new `closure ready` attempt. Use
`finding accept-out-of-scope <id> --reason <reason> --authority <id>` for an
authority disposition; `out_of_scope` is not a verification result.

Design/decomposition findings use a typed source-correction session and never
grant implementation permission:

```sh
agent-workbench closure correction-begin <closure-id>
# edit only the declared design:/plan:/docs:/workflow: Markdown surfaces
agent-workbench closure transition apply <closure-id> --token <ordinal>
agent-workbench closure ready <closure-id> --evidence "<evidence>" --tests "<tests>"
```

Apply only declared transition tokens and never substitute their underlying
task, phase, dependency, or stale command. For a declared
`decomposition-plan-reconcile` token, follow the printed `decomposition show`
action instead of `closure transition apply`; run its exact reconciliation
preview and then the preview's printed execute action. That public
reconciliation consumes the token atomically. Then use the same exact-context
resume/verify path. Record a later fresh clean run after all findings are
verified or disposed; resume review is not final completion proof.

## Completion Review Prompt Contract

When asking a review agent whether the project is complete, the prompt must
make the skill itself part of the product. A valid `COMPLETE` answer must check:

- the CLI behavior implements the design workflow
- tests cover the implemented design surfaces
- README examples match executable CLI behavior
- `skills/agent-workbench/SKILL.md` tells agents when to load references and
  which gates or memory checks block progress
- skill references contain enough operational guidance for a fresh coding agent
  to run setup, design review, decomposition review, implementation, close
  review, interruption recovery, KPT, fixed commands, acceptance, repository
  evidence, and close-ready troubleshooting without reading private design docs

If the implementation is correct but the skill does not teach the workflow
well enough, the correct review result is `NOT COMPLETE`.
