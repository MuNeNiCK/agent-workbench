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
```

Run an actual independent review first. `review run add` records the result; it
does not launch or perform the review.

For a clean design-derived gate review, copy the printed `context_ref` into the
run and include trusted provenance for the reviewer output:

```sh
agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --target <context-ref> --clean --provenance external_agent --external-agent-id <agent-id> --provenance-ref <review-output-ref>
```

Use `--provenance human_review --provenance-ref <review-output-ref>` for a
human reviewer. Do not use a self-recorded clean run to satisfy readiness gates.

## Finding Lifecycle

When a review finds a problem:

```sh
agent-workbench finding add --run <review-run-id> --type <finding-type> --severity <severity> --description "<description>"
agent-workbench finding classify <finding-id> --classification valid
agent-workbench closure add --finding <finding-id> --invariant "<what must now hold>"
agent-workbench finding verify --run <review-run-id> --finding <finding-id> --closure <closure-id> --result verified
```

Record a new clean run only after valid findings have closures and verification.

## Completion Review Prompt Contract

When asking a review agent whether the project is complete, the prompt must
make the skill itself part of the product. A valid `COMPLETE` answer must check:

- the CLI and database implement the design workflow
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
