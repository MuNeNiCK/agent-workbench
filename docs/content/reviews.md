# Reviews and caller authority

Reviewers produce observations, not product authority. Each review is bound to
one outcome, the exact design versions and Task it examined, its purpose, and
its artifact. Later bookkeeping revisions of the same outcome do not invalidate
it; changing an examined design or Task does. Its result cannot complete
another outcome.
Reusing a Review key in another outcome starts a separate Work-scoped lineage;
recording and disposition always resolve the exact ReviewRef selected by the
focused completion boundary.

A resumed reviewer retains context only to continue the same review lineage.
A fresh review uses a different reviewer execution without inherited
implementation or prior-review context. A resumed remediation confirmation is
useful evidence, but it does not satisfy a selected fresh-review boundary.
Workbench persists exact scope and reviewer identity; the Skill owns correct
external reviewer orchestration because product state cannot inspect hidden
agent context.

The caller records one disposition and a reason for each observation:
`accepted`, `rejected`, `rescoped`, `deferred`, or `needs-evidence`.
`deferred` and `needs-evidence` may later receive a final disposition.

Adopting a reviewer proposal that adds a concept, abstraction, mechanism, or
mode also records:

- why it is necessary for the current accepted requirement;
- why the simpler alternative is insufficient;
- the bounded scope being added; and
- expected maintenance cost.

The caller adopts an ordinary reviewed proposal with
`adopt-review-proposal`. Complexity-adding proposals use
`adopt-complex-review-proposal`; the reviewer never owns adoption.

Rejecting a proposal ends that proposal's authority. It does not create an
absence test or a permanent prohibition.

If a review was requested for the wrong Work, `correct-review` preserves the
old review as history, removes it from the mistaken completion boundary, and
creates a fresh review from the intended Work's current Task, Design scope, and
caller-visible artifact.
