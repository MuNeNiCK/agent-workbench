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
- Reuse unaffected assurance after a local design correction and invalidate
  results whose declared product surface changed.
- Keep Phase as optional display metadata and derive completion only from the
  current positive work boundary.
- Correct a mistaken review target without relinking history, and preserve the
  exact return point across urgent interruptions.
- Retire accepted design as a caller correction, show its declared affected
  dependency paths, and preserve unrelated work.
- Install as one Agent Skill with a static Linux x86_64 runtime and no
  host-provided glibc.
