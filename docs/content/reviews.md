# Reviews

Use review when an independent examination of a design or implementation materially improves the
evidence for the outcome. Workbench is evidence-driven, not review-driven; review is not mandatory
for every change unless the accepted design selects it.

## The two review purposes

- **Design Review** examines an immutable DesignRevision only after its Assurance Contracts and
  selected Claim receipts are current.
- **Implementation Review** starts only after independent completion readiness, then examines a fixed Design/Plan/Task/evidence manifest together with the
  current snapshots of its planned output targets.

The implementation manifest contains the current Task graph and the evidence selected by those
Tasks, one current receipt per selected Claim, current Corrections, and accepted-Finding
dispositions. Re-running a command does not append unselected execution history to every later
Review. This keeps the only fresh-review input bounded by current Design and Plan cardinality.

The Design, current Plan, immutable Work identity, and their producer provenance are derived from
the authoritative project state when the target is captured. A caller cannot substitute a
superseded Plan, rewrite the Work identity, or choose different structural producers while keeping
an otherwise self-consistent manifest.

A later Work handoff or successor-Design adoption does not rewrite or invalidate an older fixed
Review. Historical authorship is checked at the entry's capture order; the new responsible run and
new Design apply only to later operations.

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

A Finding is advisory. The responsible work agent records a decision, impact class, and reason. The
impact is `implementationDefect`, `assuranceOmission`, or `noChange`; `noChange` cannot accompany an
accepted decision:

- `accepted` means the mismatch becomes a causal obligation;
- `rejected` means it is not adopted under the accepted design;
- `replaced` means a later disposition supersedes it.

A reviewer cannot silently add a Task, criterion, mechanism, or product requirement. An accepted
implementation defect must already be covered by a current Assurance Contract. For a Design
Review, candidate amendment must name the accepted Finding as a change basis; the old Review remains
immutable and cannot authorize the new candidate. For an Implementation Review, an accepted
output-changing Finding must be included in a replacement Plan before materialization and is resolved
through current evidence in the same resumed Review lineage. Only an accepted unresolved Finding in
current implementation authority blocks completion.

An accepted assurance omission is different: it closes Plan materialization and every productive
Task, Profile, Command, and completion route for that Design. Output work becomes reachable only
through a strict successor Design with a fresh closed matrix, adoption, and materialized Plan.

A recorded verification remains valid history after a later Plan replacement, but it stops
resolving the Finding when its remediation Task or evidence is no longer current. The replacement
Plan must carry the Finding again and obtain new evidence before another resumed verification.

If an implementation defect is accepted after Work completion, Workbench preserves the original
completion, Review, Finding, disposition, and evidence as incident history and leaves that Work
terminally `completed`. Remediation starts a distinct Work and `work bind-remediation` records the
exact origin Work and accepted Finding as its causal basis. That Work receives its own Plan, Tasks,
evidence, fixed-target Review, and completion; the origin completion never becomes remediation
evidence or productive authority.

## Diagnosing review state

Use `review context` with a Review entry ID. Fresh context has an empty lineage. Resumed context
contains only that same Review lineage plus separate IDs of Finding-bound remediation Plans and the
exact Task/evidence entries selected by those Plans. Remediation entries are not discarded by the
lineage limit. The bounded Review lineage is newest-first, so recent
remediation is retained when older Review history is truncated. If the target, producer, reviewer, or lineage is unexpected,
do not work around it by creating an unrelated fresh review; inspect the fixed target source and the
relevant Review entries first.
