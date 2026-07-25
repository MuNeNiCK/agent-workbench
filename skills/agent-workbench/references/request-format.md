# Typed request format

Every request is a JSON object with:

```json
{
  "operation": "unique-operation-id",
  "expectedRevision": 1,
  "command": "command-name"
}
```

The supported commands and additional fields are:

- `register-work`: `work`, `owner`, `outcome`, `completionBoundary`.
- `register-suspended-activation`: `activation`, `work`, nullable `parent`,
  `reason`, `returnPoint`, `assumptions`, `resumeConditions`.
- `suspend-work`: `work`, `activation`, `reason`, `returnPoint`,
  `assumptions`, `resumeConditions`.
- `revise-suspension`: the `suspend-work` fields plus `design`,
  `designRevision`, `decompositionKey`, `decompositionDigest`,
  `repositorySnapshot`, `obligationKeys`, `evidenceRevision`, `reviewPlan`.
- `confirm-resume-readiness`: `work`, `activation`, and the same readiness
  basis fields used by `revise-suspension`.
- `acknowledge-related-work-terminal`: `work`, `relatedWork`.
- `import-design`: `design`, `designRevision`, nullable `predecessor`, `owner`,
  `contentDigest`, `requirements`, `decisions`, `validationGates`.
- `record-review-plan`: `plan`, `owner`, `reviewer`, `adjudicator`, nullable
  `design`, `work`, optional nullable `phase`, `repositorySnapshot`,
  `artifactDigest`, `purpose`.
- `record-review-claim`: `review`, `plan`, `work`, `epoch`, `claim`,
  `reviewer`, nullable `design`, optional nullable `phase`,
  `repositorySnapshot`, `artifactDigest`, `purpose`, and optional
  `observations`.
- `record-review-adjudication`: `review`, `decision`, `adjudicator`, `reason`,
  and optional observation dispositions.
- `record-review-finding`: `key`, `review`, `blocking`, `authority`,
  `failureAccount`, `invariant`, `remediationSurfaces`.
- `adjudicate-review-finding`: `key`, `adjudicator`, `reason`, `accepted`.
- `close-review-finding`: `key`, `attempt`, `evidenceDigest`,
  `repositorySnapshot`.
- `verify-review-finding`: `finding`, `attempt`, `verifier`, nullable `design`,
  `work`, `repositorySnapshot`, `artifactDigest`, `purpose`,
  `evidenceDigest`, `result`.
- `adjudicate-finding-verification`: `finding`, `attempt`, `adjudicator`.
- `record-user-correction`: `key`, `scope`, `statement`, nullable `work`,
  nullable `design`.
- `resolve-user-correction` or `reject-user-proposal`: `key`, `reason`.
- `record-authority-transition`: `key`, `correction`, `target`,
  `authorityOperation`, `authorityKind`, `scope`, nullable `work`, nullable
  `design`, `lifetime`, `statement`, `reason`.
- `approve-design`: `design`.
- `record-decomposition`: `key`, `design`, `work`, `designRevision`,
  `contentDigest`, `requirements`, `implementationWork`, `tasks`,
  `completionChecks`, `checklists`, `validationGates`, `reviewer`,
  `adjudicator`.
- `plan-completion`: `work`, `relatedWork`, `phases`, `tasks`, `checklists`,
  numeric `reviews`, `findings`, `validations`, `repositories`, `corrections`,
  `workRecords`. Each phase contains `key`, `group`, positive numeric `order`,
  string `dependencies`, string `tasks`, and numeric `reviews`.
- `record-scope-change`: `key`, `work`, `kind`, `cause`, `principal`, `reason`,
  `sharedRecords`, `dependencies`, `dispositions`, `resultingScopes`. Kinds
  are `rescope` and `split`; causes are `outcome`, `owner`, and
  `independent-lifecycle`. Each resulting scope contains `key`, numeric `work`,
  `owner`, `outcome`, and `completionBoundary`.
- `complete-phase`, `complete-task`, `complete-checklist`, `resolve-finding`,
  or `resolve-correction`: `work`, `key`.
- `pass-validation`: `work`, `key`, `artifactDigest`.
- `classify-repository`: `work`, `key`, `snapshotDigest`.
- `link-work-record`: `work`, `key`, `reference`.
- `record-obligation`: `work`, `key`, `commandProfile`, `invocation`,
  `repository`, `snapshot`, `artifactDigest`, `kind`, `requirements`,
  `expectedProducer`, `expectedObservation`, `design`, `designRevision`.
- `record-evidence`: `evidence`, `work`, `obligation`, `observedRevision`,
  `commandProfile`, `invocation`, `exitCode`, `repository`, `snapshot`,
  `artifactDigest`, `kind`, `requirements`, `producer`, `observedAt`,
  `design`, `designRevision`.
- `record-external-operation`: `externalOperation`, optional nullable `work`,
  `kind`, `artifactDigest`.
- `advance-external-operation`: `externalOperation`, optional nullable `work`,
  `kind`, `artifactDigest`, `state`, nullable `observationIdentity`, nullable
  `observedArtifactDigest`, nullable `disposition`.
- `complete-work`: `work`.

Purposes are `design`, `decomposition`, `design-conformance`, and
`implementation-quality`. Claims are `clean` or `findings`. Decisions are
`accepted` or `rejected`. Evidence kinds are `build`, `test`, `review`, and
`remediation`.

Review observations contain `key`, `kind`, `summary`, and `evidence`.
Observation kinds are `risk` and `proposal`. Dispositions contain
`observation`, `decision`, `reason`, optional `changesAuthority`, and nullable
`successorDesign`. Disposition decisions are `accepted`, `rejected`,
`rescoped`, `deferred`, and `needs-evidence`.

Each `relatedWork` entry contains numeric `work` and `kind`, where `kind` is
`child` or `dependency`.

Phase dependencies must name lower-ordered phases. A phase can complete only
after its assigned tasks, dependencies, and phase-scoped reviews are current.
A rescope has one resulting scope and changes the active work's outcome or
owner. A split has at least two non-conflicting resulting work identities and
creates their independent open lifecycles. Scope changes are accepted only
from the current work owner and must report the aggregate work's exact shared
records and phase dependencies.

Lists are JSON arrays. Required lists must be present; a missing required list
is not interpreted as an empty list. The optional review `observations` and
adjudication dispositions default to `[]` when omitted.

Each accepted request prints its resulting revision. An exact retry with the
same operation and payload returns the canonical receipt without advancing
state. Reusing an operation with changed content is a conflict.
