# Project action reference

All actions run through the installed Skill wrapper. The agent supplies
caller-visible descriptions; operation tokens, revisions, links, storage
fields, and retry receipts are private.

The public action groups are:

- recovery: `status`, `next`, `complete`;
- work: `init`, `start-work`, `switch-work`, `add-task`,
  `add-task-for-design`, `finish-task`;
- design authority: `record-design`, `propose-design`,
  `request-design-review`, `accept-design`,
  `accept-complex-design`, `retire-design`,
  `record-instruction`, `record-question`, `reject-proposal`,
  `record-source-effects`;
- assurance: `add-evidence`, `record-evidence`, `preview-formal`,
  `formal-check`;
- review: `request-review`, `record-review`, `record-clean-review`,
  `resolve-review`, `adopt-review-proposal`,
  `adopt-complex-review-proposal`, `correct-review`;
- interruption: `interrupt`, `return`, `replan-return`; and
- optional presentation: `assign-phase`, `rename-phase`, `order-phase`.

Roles are `goal`, `functional`, `non-functional`, `constraint`, `decision`,
`structure`, `fact`, and `boundary`. Assurance choices are `formal`,
`evidence`, `mixed`, and `none`.

See the installed Skill's `references/request-format.md` for exact positional
signatures.

Command Profiles record exact argv/cwd guidance and can be selected only by an
EvidenceSpec; they do not execute commands. KPT records durable project
learning without changing completion. The installed reference includes the
profile, deviation, KPT, adoption, and atomic-conclusion actions.
Command Profile argv is rendered as a lossless JSON vector. KPT actions carry
a stable author name separate from per-action source provenance, and
`kpt-history` exposes immutable succession and relations in project language.
Evidence profile selection records a caller reason and cannot silently remove
a required same-lineage binding. KPT adoption names the proposal author.
