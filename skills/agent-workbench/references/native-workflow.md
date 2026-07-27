# Native workflow

Run every command through the Skill wrapper. It selects the pinned runtime and
the project-relative state path internally.

1. Run `init` with the work owner, outcome, and completion boundary.
2. Run `next`, then run its exact revision-bound command.
3. Import one immutable design.
4. Record an independent design review plan, its reviewer claim, and the
   caller's separate reasoned adjudication.
5. Approve the reviewed design.
6. Record an independent decomposition review plan, claim, and adjudication.
7. Record the reviewed decomposition.
8. Record design-conformance and implementation-quality review plans over the
   same repository snapshot and artifact.
9. Record the completion plan.
10. For aggregate work, assign ordered grouped phases, explicit dependencies,
    tasks, and any phase-scoped review plans. Record rescope or split only when
    the outcome, owner, or independent lifecycle actually changes, with
    reasoned dispositions for the reported shared records and dependencies.
11. Complete each phase only after its dependencies, assigned tasks, and
    phase-scoped reviews are ready.
    A confirmed implementation defect found by a phase-scoped review uses the
    same bounded finding closure and independent verification lifecycle as a
    work-level implementation defect. Do not translate it into a Markdown
    source correction merely because the phase is not yet close-ready.
12. Record the two fresh clean work-level review claims and the caller's separate
    adjudications.
13. Record current validation obligations and their exact evidence.
14. For a release operation, first record the prepared intent, then record the
    dispatched state before the Skill invokes the external tool. Record the
    returned observation. A timeout or lost response becomes `uncertain`;
    inspect the exact remote target and record the reconciled observation before
    retrying or reporting success. The core records and validates this sequence;
    it does not provide a generic remote-service transport.
15. Complete the active work.
16. Start a fresh process and run `status` and `next` to verify recovery and
    the terminal state.

Use one `apply` request per mutation. After every accepted request, use the
returned revision as the next request's `expectedRevision`.

Reviewers provide evidence; they do not decide product authority. If a review
contains observations, the caller must decide each observation separately and
record a nonempty reason. Only an accepted authority-changing proposal tied to
a real successor design may change the design.

The final conformance and quality plans must freeze the same repository
snapshot and artifact. Validation obligation and evidence identities must
match that frozen pair exactly.

Use `suspend-work` to represent blocked work. A terminal work record is
immutable; `register-follow-up` creates a successor work unit with an exact
terminal predecessor when the outcome must be continued or reopened.

KPT observations use `record-kpt` and remain a non-authoritative work record,
including any `learningCandidate`. If the user later adopts that candidate,
record the separate user correction and caller-owned authority transition
before it can change design, implementation, tests, or completion.
