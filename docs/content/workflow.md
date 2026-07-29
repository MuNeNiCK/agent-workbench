# Workflow

## Start and recover

At the start of a session, the Skill runs:

```text
status
next
```

Both use project language. Initialize new project memory with one outcome and
its first concrete Task:

```text
init <outcome> <first-task>
```

Small fixes can finish that Task directly without creating design, Phase,
formal, or review records.

## Capture accepted design

Record caller-stated design, review that exact proposal, then record the
caller's explicit acceptance:

```text
record-design <key> <role> <assurance> <statement> [dependency-key...]
request-design-review <review> <key>
record-clean-review <review> <reviewer>
accept-design <key> <reason>
add-task-for-design <task> <design-key>...
```

Use `propose-design ordinary` or `propose-design complexity` for agent
suggestions. A complexity proposal cannot use ordinary acceptance; the caller
records necessity, why the simpler option is insufficient, bounded scope, and
maintenance cost with `accept-complex-design`. Use `record-instruction` for a
binding repository or agent rule. Questions and rejected proposals remain
context and do not select implementation or tests.

If one caller statement contains several effects, use
`record-source-effects` once. It records the design clause, operating
instruction, unresolved question, and optional new Work atomically with one
source.

## Validate proportionally

For an external observation, use `add-evidence`, perform the stated method, and
record the actual result with `record-evidence`.

For formal behavior:

1. write project-domain contract and proof modules;
2. run `preview-formal` before acceptance with a project-domain oracle that
   prints concrete examples; a product surface, adapter, and input cases are
   needed only for external implementation conformance;
3. review the exact proposed design and preview result;
4. let the caller accept or reject it with a reason;
5. add the implementation Task; the preview result remains selected for that
   exact accepted design;
6. when the unchanged product disagrees with a corrected oracle, retain that
   counterexample, correct the product, and rerun
   `formal-check <assurance-key>`; and
7. rerun the same check after any later declared product-surface change.

The check reads the selected target from project memory. Input-only cases go to
both the Lean oracle and an adapter over the ordinary product boundary; their
JSON observations are compared structurally.
An observed mismatch does not invalidate the checked design meaning, but it
does block Work completion until product conformance passes.

For a change outside the declared product surfaces, request a bounded `reuse`
Review over the changed artifact. Confirm that the surface declaration remains
complete instead of rerunning unrelated proofs. A clean decision preserves the
existing formal and review identities.

## Review, interrupt, and complete

Request a bounded review, record each observation, and record the caller's
reasoned disposition. A reviewer proposal that adds complexity requires an
explicit necessity, simpler-alternative analysis, bounded scope, and
maintenance cost before adoption.

`interrupt` atomically saves the current Work and Task before starting urgent
work. `return` restores that point when its assumptions remain current.
When they changed, `replan-return` requires the caller to select the current
outcome and record a reason.
For independent work without automatic return, use `start-work`; use
`switch-work` to return by outcome description.
`correct-review` moves no historical result; it removes the mistaken
completion selection and requests a new review for the intended outcome,
current Task and Design scope, and intended artifact.

Run `complete`. It reports completion only when every current positive boundary
member is satisfied; otherwise `next` names the missing project result.
