# Reviews and caller authority

A review claim is advisory. It can report `clean` or `findings`, but it cannot
change product authority by itself.

The caller separately records a decision with a non-empty reason. Evaluate each
observation:

- accept a concrete defect when its invariant, affected surface, and failure
  path are demonstrated;
- reject a speculative proposal that is not required by the accepted outcome;
- rescope only when the accepted outcome actually changes;
- request evidence when the claim is not yet decidable.

Do not promote an unresolved consideration into a requirement merely because
it appears in a formal design document. Do not add a test asserting that a
rejected or removed implementation approach is absent. Tests should exercise
required positive behavior.

When a blocking finding is fixed, verify the exact remediation artifact with a
reviewer independent of the original reviewer. The caller still decides
whether to accept that verification.
