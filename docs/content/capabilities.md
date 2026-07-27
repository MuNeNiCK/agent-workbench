# Capability coverage

The Lean product preserves user outcomes rather than historical command names
or database tables. The following map is the release acceptance boundary.

| Capability outcome | Lean product route | Acceptance evidence |
| --- | --- | --- |
| Project-local ledger and focused exports | Project-relative state plus `export <purpose> <class> <output>` | REQ-001, REQ-015; GATE-001, GATE-015 |
| Work, activation, block, resume, reopen, follow-up | Blocking uses durable suspension; readiness controls resume; `register-follow-up` continues a terminal predecessor without rewriting it | REQ-003; GATE-003 |
| Design versions, requirements, decisions, gates | `import-design`, independent review, caller adjudication, `approve-design` | REQ-004, REQ-005; GATE-004, GATE-005 |
| Decomposition, tasks, checklists, coverage | `record-decomposition`, `plan-completion`, focused completion operations | REQ-004, REQ-008; GATE-004, GATE-008 |
| Grouped phases, dependencies, phase review, rescope, split | Phase specifications and `record-scope-change` inside aggregate work | REQ-016; GATE-016 and CLI aggregate lifecycle |
| Command profiles, invocations, validation runs | `record-obligation`, `record-evidence`, `pass-validation` | REQ-006; GATE-006 |
| Review plans, frozen targets, claims, findings, closure, verification | Typed review requests and caller-owned dispositions | REQ-005, REQ-019; GATE-005, GATE-019 |
| Repository snapshot, commit, changed files, implementation evidence | `classify-repository`, `record-repository-evidence`, exact evidence records | REQ-006; GATE-006 |
| Corrections, rules, KPT learning | Corrections and authority transitions; `record-kpt` keeps observations as context unless a learning is explicitly adopted | REQ-009, REQ-020; GATE-009, GATE-020 |
| Deterministic status, next, readiness, completion | Read-only `status` and `next`, revision-bound actions, explicit `complete-work` | REQ-007, REQ-008; GATE-007, GATE-008 |
| Update, diagnosis, backup, integrity, repair, restore | `doctor`, `repair`, `update inspect`, exact-plan `update apply`, printed `update restore` receipt | REQ-010, REQ-011; GATE-010, GATE-011 |
| Release, transport, publication evidence | Prepared intent and dispatched state precede the Skill's external call; uncertainty requires remote reconciliation before retry or success | REQ-018; GATE-018 |
| Legacy ledger and historical command compatibility | Intentionally retired for the first Lean release; no implicit migration in normal commands | DEC-012, REQ-010; GATE-010, GATE-016 |
| Cryptographic principals, keys, external administrators | Intentionally not adopted; principals provide local provenance and role separation, not authentication | REQ-005, REQ-016; GATE-005, GATE-016 |

The external-operation boundary is deliberate: Agent Workbench does not embed a
generic network client. The Agent Skill invokes the appropriate project tool
after the prepared and dispatched facts are durable, then records the exact
observation. A timeout or lost response is recorded as uncertain and cannot
satisfy completion.
