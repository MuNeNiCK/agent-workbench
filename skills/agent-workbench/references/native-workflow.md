# Native workflow

1. Run `status` and `next`.
2. Initialize one outcome and first task when no state exists.
3. Classify caller design, instructions, proposals, questions, and rejections
   as distinct effects. Use one `record-source-effects` action when a caller
   source contains several effects.
4. For a formal candidate, run its selected Lean modules with `preview-formal`.
5. Request a fresh review of the exact proposed design meaning using a
   different reviewer execution with no inherited implementation or
   prior-review context.
6. Let the caller adopt or reject the reviewed design with a reason.
7. Add tasks scoped to the accepted design they implement.
8. Select only the assurance needed by each accepted design statement.
9. Run formal checking or record positive external evidence as selected.
10. Request bounded implementation review. Resume a reviewer only within the
    same Review lineage; use a context-independent reviewer execution whenever
    the Review is fresh. Record observations; let the caller accept, reject,
    rescope, defer, or request evidence with a reason.
11. Finish the task and run `complete`.
12. Start a fresh process and run `status`, `next`, and `complete` again.

For an interruption, `interrupt` atomically preserves the current return point
and starts the urgent outcome. `return` restores it. If its assumptions changed,
the caller uses `replan-return` to choose the current outcome and record why,
rather than silently retargeting work. The same explicit caller decision may
replace the pending plan before interrupting Work is complete; ordinary
`return` cannot abandon unfinished Work.

For independent work, `start-work` keeps the prior outcome intact without an
automatic return plan. `switch-work` selects either outcome by its
project-language description.

For formal external conformance, the adapter is test-side only, calls the same
product boundary as an ordinary caller, and emits actual JSON observations. It
contains no expected result or copied policy. Product code contains no Agent
Workbench vocabulary or dependency.

After a change outside the declared product surfaces, use a bounded
`request-review <key> reuse <changed-artifact>` review to confirm that the
surface declaration remains complete. Do not rerun an unrelated proof solely
because another repository file changed. A resolved clean reuse review keeps
the existing proof and review identities.
