# Request reference

Every mutation request begins with:

```json
{
  "operation": "unique-operation-id",
  "expectedRevision": 1,
  "command": "command-name"
}
```

The CLI accepts commands for:

- work registration, suspension, readiness, resume, terminal-predecessor
  follow-up, and completion;
- design import, approval, decomposition, and authority transitions;
- review plans, claims, caller adjudications, findings, closure, and
  verification;
- aggregate completion plans, phases, tasks, checklists, rescope, and split;
- evidence obligations and observations;
- repository classification, exact commit and changed-file evidence, and work
  records;
- KPT context and explicitly proposed learning;
- external operation preparation, dispatch, uncertainty, reconciliation, and
  completion.

Use the exact field shapes in the installed Skill reference
`references/request-format.md`. Required lists must be present; omission is not
interpreted as an empty list.

Supported review purposes are `design`, `decomposition`,
`design-conformance`, and `implementation-quality`. Review claims are `clean`
or `findings`; caller decisions are `accepted` or `rejected`.
