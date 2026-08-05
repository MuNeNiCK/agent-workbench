import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

inductive EvidenceGapKind where
  | missingEvidence
  | missingObservation
  | staleEvidence
  | missingInputDigest
  | staleReceipt
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CriterionGap where
  criterionId : String
  target : String
  kind : EvidenceGapKind
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ClaimGap where
  claimId : String
  kind : EvidenceGapKind
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignReference where
  id : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkReference where
  id : String
  outcome : String
  status : WorkStatus
  resumeCondition : Option String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure EntryReference where
  id : String
  order : Nat
  kind : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CurrentContext where
  design : DesignReference
  work : WorkReference
  designSourceGaps : List String
  unfinishedRequiredTasks : List EntryReference
  commandProfiles : List EntryReference
  effectiveUserCorrections : List EntryReference
  relevantKpt : List EntryReference
  unresolvedAcceptedFindings : List EntryReference
  criterionGaps : List CriterionGap
  claimGaps : List ClaimGap
  truncated : List String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def currentContextLimit : Nat := 32

def LedgerEntry.reference (entry : LedgerEntry) : EntryReference :=
  { id := entry.id, order := entry.order, kind := entry.payload.tag }

private def boundedReferences (entries : List LedgerEntry) : List EntryReference :=
  (entries.take currentContextLimit).map (·.reference)

private def criterionGap?
    (projection : CurrentProjection) (observations : List TargetObservation)
    (criterion : AcceptanceCriterion) : Option CriterionGap :=
  if criterionHasEvidence projection observations criterion then none
  else if !criterionEvidenceRecorded projection criterion then
    some { criterionId := criterion.id, target := criterion.target, kind := .missingEvidence }
  else if (currentSnapshot? observations criterion.target).isNone then
    some { criterionId := criterion.id, target := criterion.target, kind := .missingObservation }
  else
    some { criterionId := criterion.id, target := criterion.target, kind := .staleEvidence }

private def claimGap?
    (projection : CurrentProjection) (digests : List CurrentClaimDigest)
    (claim : LeanClaim) : Option ClaimGap :=
  if claimHasReceipt projection digests claim then none
  else if !claimReceiptRecorded projection claim then
    some { claimId := claim.id, kind := .missingEvidence }
  else if (uniqueBy? digests (·.claimId) claim.id).isNone then
    some { claimId := claim.id, kind := .missingInputDigest }
  else
    some { claimId := claim.id, kind := .staleReceipt }

private def isAcceptedFinding
    (projection : CurrentProjection) (findingEntry : LedgerEntry) : Bool :=
  projection.entries.any (fun entry =>
    match entry.payload with
    | .reviewDisposition disposition =>
        disposition.findingEntryId == findingEntry.id && disposition.decision == .accepted
    | _ => false)

def currentContext?
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Option CurrentContext := do
  let projection ← currentProjection? state
  let unfinishedRequiredTasks := projection.entries.filter (fun entry =>
    match entry.payload with
      | .task task => task.required && !task.closed
      | _ => false)
  let designSourceGaps := projection.design.sourceDocuments.filterMap (fun source =>
    if currentSnapshot? observations source.target == some source.snapshot then none
    else some source.target)
  let commandProfiles := projection.entries.filter (fun entry =>
    match entry.payload with | .commandProfile _ => true | _ => false)
  let effectiveUserCorrections := projection.entries.filter (fun entry =>
    match entry.payload with
    | .userCorrection correction =>
        correction.resolvedByEntryId.isNone &&
          correction.incorporatedIn != some projection.design.id
    | _ => false)
  let relevantKpt := projection.entries.filter (fun entry =>
    match entry.payload with | .kpt _ => true | _ => false)
  let unresolvedAcceptedFindings := projection.entries.filter (fun entry =>
    match entry.payload with
    | .finding finding =>
        isAcceptedFinding projection entry &&
        !acceptedFindingResolved projection observations entry finding
    | _ => false)
  let criterionGaps := projection.design.acceptanceCriteria.filterMap
    (criterionGap? projection observations)
  let claimGaps := projection.design.leanClaims.filterMap (claimGap? projection digests)
  let truncated :=
    [("designSourceGaps", designSourceGaps.length),
      ("unfinishedRequiredTasks", unfinishedRequiredTasks.length),
      ("commandProfiles", commandProfiles.length),
      ("effectiveUserCorrections", effectiveUserCorrections.length),
      ("relevantKpt", relevantKpt.length),
      ("unresolvedAcceptedFindings", unresolvedAcceptedFindings.length),
      ("criterionGaps", criterionGaps.length), ("claimGaps", claimGaps.length)]
    |>.filterMap (fun (name, count) => if count > currentContextLimit then some name else none)
  pure {
    design := { id := projection.design.id }
    work := {
      id := projection.work.id
      outcome := projection.work.outcome
      status := projection.work.status
      resumeCondition := projection.work.resumeCondition }
    designSourceGaps := designSourceGaps.take currentContextLimit
    unfinishedRequiredTasks := boundedReferences unfinishedRequiredTasks
    commandProfiles := boundedReferences commandProfiles
    effectiveUserCorrections := boundedReferences effectiveUserCorrections
    relevantKpt := boundedReferences relevantKpt
    unresolvedAcceptedFindings := boundedReferences unresolvedAcceptedFindings
    criterionGaps := criterionGaps.take currentContextLimit
    claimGaps := claimGaps.take currentContextLimit
    truncated }

end AgentWorkbench
