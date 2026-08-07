# Reviews

Use review when an independent examination of a design or implementation materially improves the
evidence for the outcome. Workbench is evidence-driven, not review-driven; review is not mandatory
for every change unless the accepted design selects it.

## The two review purposes

- **Design Review** examines an immutable DesignRevision before or independently of implementation.
- **Implementation Review** examines a fixed Design/Plan/Task/evidence manifest together with the
  current snapshots of its planned output targets.

The Design, current Plan, immutable Work identity, and their producer provenance are derived from
the authoritative project state when the target is captured. A caller cannot substitute a
superseded Plan, rewrite the Work identity, or choose different structural producers while keeping
an otherwise self-consistent manifest.

`fresh` and `resume` describe reviewer context. They are not additional review types.

## Fresh review

Fresh review starts a new lineage for a target that has no prior review/remediation context in that
lineage. The reviewer run must differ from the run that produced the fixed target. The reviewer
receives only the isolated review input returned for that Review entry, not implementation chat,
earlier findings, or remediation conversation.

The same reviewer identity may examine another target when it did not produce that target.
Independence is relative to the target producer; it is not a rule that one reviewer identity can be
used only once forever.

## Resume review

Resume continues the same review identifier, immutable target snapshot and manifest, reviewer,
findings, and remediation lineage. Remediation evidence is a separate causal input; it never
replaces the root target.
Checking whether an existing finding was fixed is resumed review, not another fresh review.

The resumed reviewer cannot verify a finding using remediation evidence produced by that same
reviewer run, even if a Work handoff later made that run responsible for implementation.

## Findings do not decide product authority

A Design Finding quotes an exact current statement, criterion, or assumption. An Implementation
Finding identifies an exact component ID and snapshot already present in the immutable target
manifest. Both are grounded by the fresh root Review. `review finding` becomes applicable when that
root Review is current.
The request cannot select separate evidence, target, snapshot, or producer provenance. If it is
inapplicable, inspect the current Work/Design binding and Review entry; do not create unrelated
evidence or a replacement Review to force the operation.

A Finding is advisory. The responsible work agent records an allowed disposition with a reason:

- `accepted` means the mismatch becomes a causal obligation;
- `rejected` means it is not adopted under the accepted design;
- `replaced` means a later disposition supersedes it.

A reviewer cannot silently add a Task, criterion, mechanism, or product requirement. For a Design
Review, candidate amendment must name the accepted Finding as a change basis; the old Review remains
immutable and cannot authorize the new candidate. For an Implementation Review, an accepted
output-changing Finding must be included in a replacement Plan before materialization and is resolved
through current evidence in the same resumed Review lineage. Only an accepted unresolved Finding in
current implementation authority blocks completion.

## Diagnosing review state

Use `review context` with a Review entry ID. Fresh context has an empty lineage. Resumed context
contains only that same Review lineage. If the target, producer, reviewer, or lineage is unexpected,
do not work around it by creating an unrelated fresh review; inspect the fixed target source and the
relevant Review entries first.
