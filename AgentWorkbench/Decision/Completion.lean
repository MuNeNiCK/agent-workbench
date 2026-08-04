import AgentWorkbench.Decision.ProofReuse

namespace AgentWorkbench

structure TargetObservation where
  target : String
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def currentSnapshot? (observations : List TargetObservation) (target : String) : Option String :=
  uniqueBy? observations (·.target) target |>.map (·.snapshot)

def requiredTasksClosed (projection : CurrentProjection) : Bool :=
  projection.entries.all (fun entry =>
    match entry.payload with
    | .task task => !task.required || task.closed
    | _ => true)

def designSourcesCurrent
    (projection : CurrentProjection) (observations : List TargetObservation) : Bool :=
  projection.design.sourceDocuments.all (fun source =>
    currentSnapshot? observations source.target == some source.snapshot)

def evidenceEntryCurrent
    (projection : CurrentProjection) (observations : List TargetObservation)
    (entry : LedgerEntry) : Bool :=
  entry.workId == some projection.work.id &&
  entry.designRevision == some projection.design.id &&
  match entry.payload with
  | .artifactObservation evidence =>
      evidence.successful &&
      currentSnapshot? observations evidence.target == some evidence.snapshot
  | .commandExecution evidence =>
      match evidence.target, evidence.snapshot with
      | some target, some snapshot =>
          evidence.successful &&
          currentSnapshot? observations target == some snapshot &&
          projection.entries.any (fun candidate =>
            candidate.id == evidence.profileEntryId &&
            match candidate.payload with | .commandProfile _ => true | _ => false)
      | _, _ => false
  | _ => false

def criterionHasEvidence
    (projection : CurrentProjection) (observations : List TargetObservation)
    (criterion : AcceptanceCriterion) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .artifactObservation evidence =>
        criterion.evidenceKind == "artifact" &&
        evidenceEntryCurrent projection observations entry &&
        evidence.criterionId == criterion.id &&
        evidence.target == criterion.target &&
        !evidence.operation.isEmpty && !evidence.result.isEmpty
    | .commandExecution evidence =>
        criterion.evidenceKind == "command" &&
        evidenceEntryCurrent projection observations entry &&
        evidence.criterionId == some criterion.id &&
        evidence.target == some criterion.target &&
        currentSnapshot? observations criterion.target == evidence.snapshot
    | _ => false)

def claimHasReceipt
    (projection : CurrentProjection) (digests : List CurrentClaimDigest)
    (claim : LeanClaim) : Bool :=
  match uniqueBy? digests (·.claimId) claim.id with
  | none => false
  | some current => projection.entries.any (fun entry =>
      match entry.payload with
      | .leanProofReceipt receipt =>
          entry.workId == some projection.work.id &&
          entry.designRevision == some projection.design.id &&
          canReuseReceipt claim current receipt
      | _ => false)

def acceptedFindingResolved
    (projection : CurrentProjection) (observations : List TargetObservation)
    (findingEntry : LedgerEntry) (finding : FindingRecord) : Bool :=
  let accepted := projection.entries.any (fun entry =>
    match entry.payload with
    | .reviewDisposition disposition =>
        disposition.findingEntryId == findingEntry.id && disposition.decision == .accepted
    | _ => false)
  if !accepted then true else
  projection.entries.any (fun entry =>
    match entry.payload with
    | .reviewVerification verification =>
        verification.findingEntryId == findingEntry.id &&
        verification.reviewId == finding.reviewId && verification.resolved &&
        currentSnapshot? observations verification.target == some verification.snapshot &&
        projection.entries.any (fun evidenceEntry =>
          evidenceEntry.id == verification.evidenceEntryId &&
          evidenceEntryCurrent projection observations evidenceEntry &&
          match evidenceEntry.payload with
          | .artifactObservation evidence =>
              evidence.target == verification.target &&
              evidence.snapshot == verification.snapshot
          | .commandExecution evidence =>
              evidence.target == some verification.target &&
              evidence.snapshot == some verification.snapshot
          | _ => false)
    | _ => false)

def noBlockingEntries
    (projection : CurrentProjection) (observations : List TargetObservation) : Bool :=
  projection.entries.all (fun entry =>
    match entry.payload with
    | .finding finding => acceptedFindingResolved projection observations entry finding
    | .userCorrection correction =>
        correction.resolvedByEntryId.isSome ||
          correction.incorporatedIn == some projection.design.id
    | _ => true)

def completionReady
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection =>
      designSourcesCurrent projection observations &&
      requiredTasksClosed projection &&
      projection.design.acceptanceCriteria.all
        (criterionHasEvidence projection observations) &&
      projection.design.leanClaims.all (claimHasReceipt projection digests) &&
      noBlockingEntries projection observations

end AgentWorkbench
