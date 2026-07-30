# Agent Workbench v0.2.3

`v0.2.3` replaces the prior workflow with a Lean-assisted project workflow.

- Capture caller-approved design separately from agent and reviewer proposals.
- Require explicit necessity, simpler-alternative analysis, bounded scope, and
  maintenance cost before adopting an AI-proposed complexity increase.
- Select formal, external-evidence, mixed, or no assurance per accepted design
  statement.
- Check project-domain Lean contracts with a pinned portable tool acquired only
  when formal assurance is used.
- Preview an unaccepted formal design, review its exact meaning, and only then
  let the caller adopt it.
- Show the concrete Lean-oracle observations, contract, tool, and exact
  artifacts that make up that review scope.
- Compare a Lean oracle with the real product boundary through input-only cases
  without putting Workbench concepts into product code.
- Preserve a checked counterexample when a corrected design disagrees with the
  unchanged product, review that meaning first, and require restored
  conformance only for Work completion.
- Reject unfinished Lean proofs, and distinguish a valid product
  counterexample from timeout, process, malformed-output, and output-limit
  execution failures.
- Reuse unaffected assurance after a local design correction and invalidate
  only the exact formal-result binding whose declared product surface changed,
  without blocking or clearing a same-key assurance for another Design.
- Let only the latest current result for an exact formal spec govern
  conformance and completion; a newer counterexample or execution failure
  cannot be masked by an older pass.
- Reconcile the selected exact stale binding inside the successful
  `formal-check` operation, so rebuilding an identical generated artifact
  reports restored completion in that same response.
- Select a same-key formal successor by its latest unaccepted DesignRef during
  preview while an accepted completion command remains bound to its exact
  accepted DesignRef, and disambiguate same-key Evidence obligations with an
  optional Design key while retaining basis-exact currentness.
- Bound oracle, adapter, and aggregate preview output while processes run, and
  transfer formal result files through the private runtime boundary instead of
  a potentially oversized command-line argument.
- Derive formal-result retry identity from bounded semantic file content rather
  than temporary paths, and transfer stale-currentness identities through a
  streamed private file instead of one environment value. Do not impose an
  aggregate stale-set cap below valid durable state; fail closed if the
  producer, inspection, sort, or finalization stage fails; and validate
  projection metadata before mutation so rendering cannot fail after commit
  and discard exact retry state.
- Replace completion members by semantic Design lineage across later Tasks:
  superseded versions are removed while unrelated same-key bases are retained.
  Render the exact Design selector required by `formal-check`, `add-evidence`,
  and `record-evidence`.
- Let `finish-task` follow the exact first missing Task member for any number of
  pending Tasks, and scope same-key Review and Evidence resolution to the
  focused Work and exact completion member.
- Encode retry intentions injectively: the Skill hashes NUL-delimited argument
  vectors and the runtime stores canonical JSON arrays, so control characters
  in free text cannot merge distinct operations.
- Distinguish resumed reviewer continuity from a fresh, context-independent
  reviewer execution in the installed Skill; a resumed remediation check
  cannot be reported as the required fresh Review.
- Keep Phase as optional display metadata and derive completion only from the
  current positive work boundary.
- Correct a mistaken review target without relinking history, and preserve the
  exact return point and caller decision across urgent interruptions. An
  unfinished interrupting Work cannot be abandoned through implicit `return`;
  only a reasoned caller `replan-return` can replace the saved plan.
- Retire accepted design as a caller correction, show its declared affected
  dependency paths, and preserve unrelated work.
- Install as one Agent Skill with a static Linux x86_64 runtime and no
  host-provided glibc.
