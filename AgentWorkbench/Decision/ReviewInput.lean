import AgentWorkbench.Decision.Context
import AgentWorkbench.Domain.ContentDigest

namespace AgentWorkbench

structure ReviewInput where
  designId : String
  workId : Option String
  review : LedgerEntry
  remediationPlanIds : List String
  remediation : List EntryReference
  lineage : List EntryReference
  lineageTruncated : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewInspection where
  designId : String
  workId : Option String
  review : LedgerEntry
  lineage : List LedgerEntry
  currentTargetSnapshot : Option String := none
  targetCurrent : Bool := false
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private structure ReviewWorkIdentity where
  id : String
  outcome : String
  scope : String
  baselineDesignRevision : Option String
  deriving Repr, DecidableEq, Lean.ToJson

def reviewWorkIdentitySnapshot (work : Work) : String :=
  ContentDigest.string (Lean.toJson ({
    id := work.id
    outcome := work.outcome
    scope := work.scope
    baselineDesignRevision := work.baselineDesignRevision
  } : ReviewWorkIdentity)).compress

private def handoffRecordsFor
    (state : ProjectState) (workId : String) : List (Nat × WorkHandoffRecord) :=
  state.ledgerEntries.filterMap fun entry =>
    if entry.workId != some workId then none else
    match entry.payload with
    | .workHandoff value => some (entry.order, value)
    | _ => none

def responsibleWorkAgentRunAt
    (state : ProjectState) (work : Work) (coverageOrder : Nat) : String :=
  let handoffs : List (Nat × WorkHandoffRecord) := handoffRecordsFor state work.id
    |>.mergeSort (fun (left right : Nat × WorkHandoffRecord) => left.1 < right.1)
  let before : List (Nat × WorkHandoffRecord) :=
    handoffs.filter (fun value => value.1 < coverageOrder)
  match before.foldl (fun (_ : Option String) value => some value.2.successorRun) none with
  | some value => value
  | none =>
      match (handoffs.find? fun value => value.1 >= coverageOrder) with
      | some value => value.2.predecessorRun
      | none => work.responsibleAgentRun

def reviewWorkProducerRunsAt
    (state : ProjectState) (work : Work) (coverageOrder : Nat) : List String :=
  let handoffs : List (Nat × WorkHandoffRecord) := handoffRecordsFor state work.id
    |>.mergeSort (fun (left right : Nat × WorkHandoffRecord) => left.1 < right.1)
  let before : List (Nat × WorkHandoffRecord) :=
    handoffs.filter (fun value => value.1 < coverageOrder)
  let responsibleAt := responsibleWorkAgentRunAt state work coverageOrder
  (before.flatMap (fun value => [value.2.predecessorRun, value.2.successorRun]) ++
    [responsibleAt]).foldl (fun (found : List String) (value : String) =>
      if value.isEmpty || found.contains value then found else found ++ [value]) []

def acceptedImplementationFindingIdsIn
    (entries : List LedgerEntry) (workId : String) : List String :=
  entries.filterMap fun entry =>
    match entry.payload with
    | .finding _ =>
        if (findingDispositionIn? entries entry.id workId).any fun dispositionEntry =>
          match dispositionEntry.payload with
          | .reviewDisposition value => value.decision == .accepted
          | _ => false
        then some entry.id else none
    | _ => none

/-- Version-1 manifests froze the complete applicable execution/history projection. Keep this
definition only for exact validation of already-immutable v1 Review records. -/
def implementationReviewLedgerEntriesV1
    (entries : List LedgerEntry) (plan : ImplementationPlan) (workId : String) : List LedgerEntry :=
  let acceptedFindings := acceptedImplementationFindingIdsIn entries workId
  entries.filter fun entry => match entry.payload with
    | .task value => value.planId == some plan.id && !value.retired
    | .commandExecution _ | .artifactObservation _ | .leanProofReceipt _ => true
    | .userCorrection _ => true
    | .finding _ => acceptedFindings.contains entry.id
    | .reviewDisposition value => acceptedFindings.contains value.findingEntryId &&
        ((findingDispositionIn? entries value.findingEntryId workId).any (·.id == entry.id))
    | _ => false

def implementationReviewHistoryEntriesV1
    (entries : List LedgerEntry) (workId : String) : List LedgerEntry :=
  entries.filter fun entry =>
    entry.workId == some workId && match entry.payload with
    | .workHandoff _ | .workDesignAdoption _ | .workWithdrawal _ | .workResume _ => true
    | _ => false

private def latestEntryFor? (entries : List LedgerEntry) (predicate : LedgerEntry → Bool) : Option LedgerEntry :=
  entries.foldl (fun latest entry =>
    if !predicate entry then latest else
    match latest with
    | some prior => if prior.order < entry.order then some entry else latest
    | none => some entry) none

/-- Version-2 manifests selected Task-declared evidence and the latest accepted receipt without
external currency inputs. Keep this definition only for validation of already-immutable v2 Review
records. -/
def implementationReviewLedgerEntries
    (entries : List LedgerEntry) (design : DesignRevision)
    (plan : ImplementationPlan) (workId : String) : List LedgerEntry :=
  let acceptedFindings := acceptedImplementationFindingIdsIn entries workId
  let tasks := entries.filter fun entry => match entry.payload with
    | .task value => value.planId == some plan.id && !value.retired
    | _ => false
  let selectedEvidenceIds := tasks.flatMap fun entry => match entry.payload with
    | .task value => value.verificationEvidenceEntryIds
    | _ => []
  let selectedReceiptIds := design.leanClaims.filterMap fun claim =>
    latestEntryFor? entries (fun entry => match entry.payload with
      | .leanProofReceipt receipt => receipt.claimId == claim.id && receipt.kernelAccepted
      | _ => false) |>.map (·.id)
  let selectedDispositionIds := acceptedFindings.filterMap fun findingId =>
    (findingDispositionIn? entries findingId workId).map (·.id)
  let selectedVerificationIds := acceptedFindings.filterMap fun findingId =>
    latestEntryFor? entries (fun entry => match entry.payload with
      | .reviewVerification value => value.findingEntryId == findingId && value.resolved
      | _ => false) |>.map (·.id)
  entries.filter fun entry =>
    tasks.any (·.id == entry.id) || selectedEvidenceIds.contains entry.id ||
    selectedReceiptIds.contains entry.id || selectedDispositionIds.contains entry.id ||
    selectedVerificationIds.contains entry.id || acceptedFindings.contains entry.id ||
    match entry.payload with
    | .userCorrection value => value.resolvedByEntryId.isNone && value.incorporatedIn.isNone
    | _ => false

/-- The current implementation projection contains only identities selected by the same artifact,
environment, private-input, and Claim-digest observations used by completion. Stale Task closing
evidence and stale Lean receipts remain history but are not frozen into a new Review target. -/
def currentImplementationReviewLedgerEntries
    (projection : CurrentProjection) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) (plan : ImplementationPlan) : List LedgerEntry :=
  let entries := projection.entries
  let acceptedFindings := acceptedImplementationFindingIdsIn entries projection.work.id
  let tasks := entries.filter fun entry => match entry.payload with
    | .task value => value.planId == some plan.id && !value.retired
    | _ => false
  let selectedEvidenceIds := tasks.flatMap fun entry => match entry.payload with
    | .task task =>
        task.verificationEvidenceEntryIds.filter fun evidenceId =>
          entries.any fun evidenceEntry =>
            evidenceEntry.id == evidenceId &&
              evidenceEntryCurrent projection observations evidenceEntry
    | _ => []
  let selectedReceiptIds := projection.design.leanClaims.filterMap fun claim => do
    let current ← uniqueBy? digests (·.claimId) claim.id
    latestEntryFor? entries (fun entry => match entry.payload with
      | .leanProofReceipt receipt => canReuseReceipt claim current receipt
      | _ => false) |>.map (·.id)
  let selectedDispositionIds := acceptedFindings.filterMap fun findingId =>
    (findingDispositionIn? entries findingId projection.work.id).map (·.id)
  let selectedVerificationIds := acceptedFindings.filterMap fun findingId =>
    latestEntryFor? entries (fun entry => match entry.payload with
      | .reviewVerification value => value.findingEntryId == findingId && value.resolved
      | _ => false) |>.map (·.id)
  entries.filter fun entry =>
    tasks.any (·.id == entry.id) || selectedEvidenceIds.contains entry.id ||
    selectedReceiptIds.contains entry.id || selectedDispositionIds.contains entry.id ||
    selectedVerificationIds.contains entry.id || acceptedFindings.contains entry.id ||
    match entry.payload with
    | .userCorrection value => value.resolvedByEntryId.isNone && value.incorporatedIn.isNone
    | _ => false

def reviewEntryProducerRunsAt
    (state : ProjectState) (work : Work) (implicitAuthorOrder : Nat)
    (entry : LedgerEntry) : List String :=
  match entry.payload with
  | .commandExecution value => [value.producerAgentRun]
  | .artifactObservation value => [value.producerAgentRun]
  | .reviewDisposition value => [value.decidedByRun]
  | .workHandoff value => [value.predecessorRun, value.successorRun]
  | .workWithdrawal value => [value.withdrawnByRun]
  | .workResume value => [value.resumedByRun]
  | .workCompletion value => [value.completedByRun]
  | .designRejection value => [value.rejectedByRun]
  | .workDesignAdoption value => [value.adoptedByRun]
  | .task _ | .commandProfile _ | .userCorrection _ | .kpt _ | .leanProofReceipt _ =>
      [responsibleWorkAgentRunAt state work implicitAuthorOrder]
  | .review value => value.producerAgentRuns
  | .finding value => value.producerAgentRuns
  | .reviewVerification value => [value.verifiedByRun]
  | .reviewHandoff value => [value.predecessorReviewerRun, value.successorReviewerRun]
  | .reviewConclusion value => [value.reviewerAgentRun]

def reviewLedgerComponent
    (state : ProjectState) (work : Work) (entry : LedgerEntry) : ReviewTargetComponent :=
  { kind := entry.payload.tag
    id := entry.id
    snapshot := ContentDigest.string (Lean.toJson entry).compress
    producerAgentRuns := (reviewEntryProducerRunsAt state work entry.order entry).eraseDups }

def reviewLedgerComponentAt
    (state : ProjectState) (work : Work) (implicitAuthorOrder : Nat)
    (entry : LedgerEntry) : ReviewTargetComponent :=
  { kind := entry.payload.tag
    id := entry.id
    snapshot := ContentDigest.string (Lean.toJson entry).compress
    producerAgentRuns :=
      (reviewEntryProducerRunsAt state work implicitAuthorOrder entry).eraseDups }

def normalizeReviewTargetComponents
    (components : List ReviewTargetComponent) : List ReviewTargetComponent :=
  components.mergeSort (fun left right =>
    if left.kind == right.kind then left.id < right.id else left.kind < right.kind)

def deduplicateReviewTargetComponents
    (components : List ReviewTargetComponent) : List ReviewTargetComponent :=
  components.foldl (fun found component =>
    if found.contains component then found else found ++ [component]) []

def implementationReviewOutputTargetIds
    (entries : List LedgerEntry) (plan : ImplementationPlan) : List String :=
  entries.foldl (fun found entry => match entry.payload with
    | .task task =>
        if task.planId == some plan.id && !task.retired then
          task.outputScopes.foldl (fun targets target =>
            if targets.contains target then targets else targets ++ [target]) found
        else found
    | _ => found) []

private def belongsToReview (reviewId : String) (findingIds : List String)
    (entry : LedgerEntry) : Bool :=
  match entry.payload with
  | .review value => value.reviewId == reviewId
  | .finding value => value.reviewId == reviewId
  | .reviewDisposition value => findingIds.contains value.findingEntryId
  | .reviewVerification value => value.reviewId == reviewId
  | .reviewHandoff value => value.reviewId == reviewId
  | .reviewConclusion value => value.reviewId == reviewId
  | _ => false

def reviewInput? (state : ProjectState) (reviewEntryId : String) : Option ReviewInput := do
  let reviewEntry ← state.entry? reviewEntryId
  let review ← match reviewEntry.payload with
    | .review value => some value
    | _ => none
  let designId ← reviewEntry.designRevision
  let findingIds := state.ledgerEntries.filterMap (fun (entry : LedgerEntry) =>
    match entry.payload with
    | .finding value => if value.reviewId == review.reviewId then some entry.id else none
    | _ => none)
  let remediationPlanIds :=
    match review.context with
    | .fresh => []
    | .resume =>
        state.implementationPlans.filterMap fun plan =>
          if plan.workId == reviewEntry.workId.getD "" &&
              plan.designRevision == designId &&
              plan.steps.any (fun step =>
                step.acceptedFindingEntryIds.any findingIds.contains) then
            some plan.id
          else none
  let remediationTaskEntries : List LedgerEntry := state.ledgerEntries.filter fun entry =>
    match entry.payload with
    | .task task => task.planId.any remediationPlanIds.contains
    | _ => false
  let remediationEvidenceIds : List String := remediationTaskEntries.flatMap fun (entry : LedgerEntry) =>
    match entry.payload with
    | .task task => task.verificationEvidenceEntryIds
    | _ => []
  let remediation := state.ledgerEntries.filter (fun entry =>
    entry.order < reviewEntry.order && entry.scope == reviewEntry.scope &&
    entry.workId == reviewEntry.workId && entry.designRevision == reviewEntry.designRevision &&
    (remediationTaskEntries.any (·.id == entry.id) || remediationEvidenceIds.contains entry.id))
    |>.mergeSort (fun left right => left.order > right.order)
  let lineage :=
    match review.context with
    | .fresh => []
    | .resume =>
        state.ledgerEntries.filter (fun (entry : LedgerEntry) =>
          entry.order < reviewEntry.order && entry.scope == reviewEntry.scope &&
          entry.workId == reviewEntry.workId && entry.designRevision == reviewEntry.designRevision &&
          belongsToReview review.reviewId findingIds entry)
          |>.mergeSort (fun left right => left.order > right.order)
  pure {
    designId
    workId := reviewEntry.workId
    review := reviewEntry
    remediationPlanIds
    remediation := remediation.map (fun (entry : LedgerEntry) => entry.reference)
    lineage := (lineage.take currentContextLimit).map (fun (entry : LedgerEntry) => entry.reference)
    lineageTruncated := lineage.length > currentContextLimit }

def reviewInspection? (state : ProjectState) (reviewEntryId : String) : Option ReviewInspection := do
  let reviewEntry ← state.entry? reviewEntryId
  let review ← match reviewEntry.payload with
    | .review value => some value
    | _ => none
  let designId ← reviewEntry.designRevision
  let findingIds := state.ledgerEntries.filterMap (fun entry =>
    match entry.payload with
    | .finding value => if value.reviewId == review.reviewId then some entry.id else none
    | _ => none)
  let lineage := state.ledgerEntries.filter fun entry =>
    entry.order != reviewEntry.order && entry.scope == reviewEntry.scope &&
      entry.workId == reviewEntry.workId && belongsToReview review.reviewId findingIds entry
  pure { designId, workId := reviewEntry.workId, review := reviewEntry, lineage }

end AgentWorkbench
