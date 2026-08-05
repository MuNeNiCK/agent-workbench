# Reviews

Use review when an independent examination of a design or implementation materially improves the
evidence for the outcome. Workbench is evidence-driven, not review-driven; review is not mandatory
for every change unless the accepted design selects it.

## The two review purposes

- **Design Review** examines an immutable DesignRevision before or independently of implementation.
- **Implementation Review** examines a fixed implementation snapshot represented by recorded
  evidence.

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

Resume continues the same review identifier, target, reviewer, findings, and remediation lineage.
Checking whether an existing finding was fixed is resumed review, not another fresh review.

The resumed reviewer cannot verify a finding using remediation evidence produced by that same
reviewer run, even if a Work handoff later made that run responsible for implementation.

## Findings do not decide product authority

A Finding quotes an exact current design subject and is grounded by the immutable target already
fixed by its fresh root Review. `review finding` becomes applicable when that root Review is current.
The request cannot select separate evidence, target, snapshot, or producer provenance. If it is
inapplicable, inspect the current Work/Design binding and Review entry; do not create unrelated
evidence or a replacement Review to force the operation.

A Finding is advisory. The responsible work agent records an allowed disposition with a reason:

- `accepted` means the mismatch must be resolved and verified;
- `rejected` means it is not adopted under the accepted design;
- `replaced` means a later disposition supersedes it.

A reviewer cannot silently add a Task, criterion, mechanism, or product requirement. Only an accepted
finding without current resumed-review verification blocks completion.

## Diagnosing review state

Use `review context` with a Review entry ID. Fresh context has an empty lineage. Resumed context
contains only that same Review lineage. If the target, producer, reviewer, or lineage is unexpected,
do not work around it by creating an unrelated fresh review; inspect the fixed target source and the
relevant Review entries first.
