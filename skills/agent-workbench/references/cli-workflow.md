# Agent Workbench CLI Workflow

Use this reference when deciding which `agent-workbench` command to run during
normal coding-agent work.

## Start Or Resume

1. Run `agent-workbench status`.
2. Run `agent-workbench next`.
3. Run `agent-workbench rules applicable --scope current`.
4. If resuming suspended work, run
   `agent-workbench gate resume-ready --maturity trace-aware --dry-run`.
5. Use `agent-workbench resume-check --maturity trace-aware` only when the
   resume decision should be recorded.

Use `--maturity repo-aware` when repository snapshots or dirty state affect the
resume decision. Register every relevant repository first; repo-aware resume
expects every registered repository to have comparable suspend and current
snapshots.

## Design To Implementation

1. Create or convert design material into a workbench design package with
   `agent-workbench design init`.
2. Import the package with `agent-workbench design import`.
3. Add the required design document review plan with
   `agent-workbench review plan add --work-unit <work-unit-id> --type design_review --stage design-ready --design-version <design-version-id> --required`.
4. Record a clean design review run with
   `agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --clean`.
5. Check design readiness with
   `agent-workbench gate design-ready --dry-run`.
6. Decompose the design with
   `agent-workbench decompose design <design-version-id> --work-unit <work-unit-id>`.
7. Inspect generated planning state with
   `agent-workbench checklist list`, `agent-workbench requirement list`, and
   `agent-workbench stale list`.
8. Select validation gates with `agent-workbench gate select` when the
   decomposition did not already select the required gate.
9. Add the required decomposition review plan with
   `agent-workbench review plan add --work-unit <work-unit-id> --type design_task_decomposition --stage implementation-ready --design-version <design-version-id> --required`.
10. Record a clean decomposition review run with
    `agent-workbench review run add --plan <review-plan-id> --type fresh --purpose new_unbiased_review --clean`.
11. Check implementation readiness with
   `agent-workbench gate implementation-ready --dry-run`.

## Close Work

1. Record implementation evidence with `agent-workbench evidence add`.
2. Record coverage with `agent-workbench coverage add`.
3. Close or accept out-of-scope all tasks derived from the design.
4. Record command usage, validation runs, repository state, Git evidence, and
   work record evidence.
5. Add required close review plans:
   `design_implementation_diff` and `implementation_review`, both at
   `--stage close-ready`, with `--work-unit <work-unit-id>`.
6. Use `agent-workbench review-context design-implementation-diff` or
   `agent-workbench review-context implementation-review` to launch focused
   review agents.
7. Record clean close review runs or record findings, closures, and
   verifications until the configured review policy is satisfied. Every
   `review run add` command must include `--plan <review-plan-id>`.
8. Run `agent-workbench gate close-ready --dry-run`.
9. If blocked, perform the blocking action printed by the gate before closing.
10. Create or export work records when the user expects human-readable output.
11. Close the work unit only after `close-ready` passes.
