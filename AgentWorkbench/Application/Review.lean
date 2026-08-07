import AgentWorkbench.Application.Ledger
import AgentWorkbench.Adapter.ReviewTarget
import AgentWorkbench.Decision.Finding

namespace AgentWorkbench

structure ReviewStartRequest where
  entryId : String
  reviewId : String
  purpose : ReviewPurpose
  targetDesignRevision : Option String := none
  reviewerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewResumeRequest where
  entryId : String
  continuesEntryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure FindingRecordRequest where
  entryId : String
  reviewEntryId : String
  subject : FindingSubject
  summary : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DispositionRecordRequest where
  entryId : String
  findingEntryId : String
  decision : DispositionDecision
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure VerificationRecordRequest where
  entryId : String
  findingEntryId : String
  reviewEntryId : String
  evidenceEntryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewHandoffRequest where
  entryId : String
  reviewEntryId : String
  successorReviewerRun : String
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewConclusionRequest where
  entryId : String
  reviewEntryId : String
  clean : Bool
  summary : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def appendIo (result : Except String ProjectState) : IO ProjectState :=
  match result with
  | .ok value => pure value
  | .error message => throw (IO.userError message)

private def activeReviewerRun (state : ProjectState) (review : ReviewRecord) : String :=
  (state.ledgerEntries.filterMap (fun entry =>
    match entry.payload with
    | .reviewHandoff value =>
        if value.reviewId == review.reviewId then some (entry.order, value.successorReviewerRun)
        else none
    | _ => none) |>.mergeSort (fun left right => left.1 > right.1) |>.head?).map (·.2)
    |>.getD review.reviewerAgentRun

def startReview
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : ReviewStartRequest) : IO ProjectState := do
  let _ := projectRoot
  let fixed ← match ← ReviewTarget.freeze projectRoot state request.purpose request.targetDesignRevision with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  let (designId, work) ← match request.purpose with
    | .design =>
        let design ← match state.design? fixed.sourceId with
          | some value => pure value
          | none => throw (IO.userError s!"no DesignRevision {fixed.sourceId}")
        let workId ← match design.workId with
          | some value => pure value
          | none => throw (IO.userError "Design Review target is not Work-bound")
        let work ← match state.work? workId with
          | some value => pure value
          | none => throw (IO.userError s!"no Work {workId}")
        pure (design.id, work)
    | .implementation =>
        let projection ← match currentProjection? state with
          | some value => pure value
          | none => throw (IO.userError "Implementation Review requires a current Work")
        pure (projection.design.id, projection.work)
  appendIo (appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := work.scope
    workId := some work.id, designRevision := some designId
    payload := .review {
      reviewId := request.reviewId, purpose := request.purpose, context := .fresh
      targetSourceId := fixed.sourceId, target := fixed.target, targetSnapshot := fixed.snapshot
      targetManifest := fixed.manifest, producerAgentRuns := fixed.producerAgentRuns
      reviewerAgentRun := request.reviewerAgentRun } })

def resumeReview
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : ReviewResumeRequest) : IO ProjectState := do
  let priorEntry ← match state.entry? request.continuesEntryId with
    | some value => pure value
    | none => throw (IO.userError s!"no Review entry {request.continuesEntryId}")
  let prior ← match priorEntry.payload with
    | .review value => pure value
    | _ => throw (IO.userError s!"entry {request.continuesEntryId} is not a Review")
  let fixed ← match ← ReviewTarget.refreeze projectRoot state prior with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  let reviewerRun := activeReviewerRun state prior
  appendIo (appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := priorEntry.scope
    workId := priorEntry.workId, designRevision := priorEntry.designRevision
    payload := .review {
      reviewId := prior.reviewId, purpose := prior.purpose, context := .resume
      continuesEntryId := some priorEntry.id, targetSourceId := prior.targetSourceId
      target := prior.target, targetSnapshot := fixed.snapshot, targetManifest := fixed.manifest
      producerAgentRuns := fixed.producerAgentRuns
      reviewerAgentRun := reviewerRun } })

def recordFinding
    (state : ProjectState) (request : FindingRecordRequest) : Except String ProjectState := do
  let reviewEntry ← match state.entry? request.reviewEntryId with
    | some value => pure value
    | none => throw s!"no Review entry {request.reviewEntryId}"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"entry {request.reviewEntryId} is not a Review"
  if review.context != .fresh then throw "Finding must be recorded against the fresh Review root"
  let designId ← match reviewEntry.designRevision with
    | some value => pure value
    | none => throw "Review is not Design-bound"
  let design ← match state.design? designId with
    | some value => pure value
    | none => throw s!"no DesignRevision {designId}"
  if !findingSubjectCurrent design request.subject then
    throw "Finding subject is not an exact subject in the fixed Review Design"
  appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := reviewEntry.scope
    workId := reviewEntry.workId, designRevision := reviewEntry.designRevision
    payload := .finding {
      reviewId := review.reviewId, subject := request.subject
      targetSourceId := review.targetSourceId, target := review.target
      targetSnapshot := review.targetSnapshot, producerAgentRuns := review.producerAgentRuns
      summary := request.summary } }

def recordDisposition
    (state : ProjectState) (request : DispositionRecordRequest) : Except String ProjectState := do
  let finding ← match state.entry? request.findingEntryId with
    | some value => pure value
    | none => throw s!"no Finding entry {request.findingEntryId}"
  let workId ← match finding.workId with
    | some value => pure value
    | none => throw "Finding is not Work-bound"
  let work ← match state.work? workId with
    | some value => pure value
    | none => throw s!"no Work {workId}"
  appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := finding.scope
    workId := finding.workId, designRevision := finding.designRevision
    payload := .reviewDisposition {
      findingEntryId := request.findingEntryId, decision := request.decision
      reason := request.reason, decidedByRun := work.responsibleAgentRun } }

def recordVerification
    (state : ProjectState) (request : VerificationRecordRequest) : Except String ProjectState := do
  let reviewEntry ← match state.entry? request.reviewEntryId with
    | some value => pure value
    | none => throw s!"no Review entry {request.reviewEntryId}"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"entry {request.reviewEntryId} is not a Review"
  if review.context != .resume then throw "Review verification requires a resumed Review"
  if review.purpose != .implementation then
    throw "evidence-entry verification applies only to Implementation Review"
  let findingEntry ← match state.entry? request.findingEntryId with
    | some value => pure value
    | none => throw s!"no Finding entry {request.findingEntryId}"
  let finding ← match findingEntry.payload with
    | .finding value => pure value
    | _ => throw s!"entry {request.findingEntryId} is not a Finding"
  if finding.reviewId != review.reviewId || findingEntry.order >= reviewEntry.order then
    throw "Finding does not precede the resumed Review in the same lineage"
  let evidenceEntry ← match state.entry? request.evidenceEntryId with
    | some value => pure value
    | none => throw s!"no evidence entry {request.evidenceEntryId}"
  if evidenceEntry.order <= findingEntry.order || evidenceEntry.order >= reviewEntry.order ||
      evidenceEntry.scope != reviewEntry.scope || evidenceEntry.workId != reviewEntry.workId ||
      evidenceEntry.designRevision != reviewEntry.designRevision then
    throw "remediation evidence is not causally bound between the Finding and resumed Review"
  let (target, snapshot, producer) ← match evidenceEntry.payload with
    | .artifactObservation value =>
        if value.successful then pure (value.target, value.snapshot, value.producerAgentRun)
        else throw "remediation evidence is unsuccessful"
    | .commandExecution value =>
        if value.successful then match value.target, value.snapshot with
          | some target, some snapshot => pure (target, snapshot, value.producerAgentRun)
          | _, _ => throw "remediation command evidence has no target snapshot"
        else throw "remediation evidence is unsuccessful"
    | _ => throw "Review verification requires command or artifact evidence"
  if activeReviewerRun state review == producer then
    throw "remediation evidence producer cannot verify its own output"
  if !(review.targetManifest.any fun component =>
      component.kind == "implementation_target" && component.id == target &&
        component.snapshot == snapshot) then
    throw "remediation target is absent from the resumed Review manifest"
  appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := reviewEntry.scope
    workId := reviewEntry.workId, designRevision := reviewEntry.designRevision
    payload := .reviewVerification {
    reviewId := review.reviewId, findingEntryId := request.findingEntryId
    reviewEntryId := reviewEntry.id, evidenceEntryId := request.evidenceEntryId
    target, snapshot, verifiedByRun := activeReviewerRun state review, resolved := true } }

def handoffReview
    (state : ProjectState) (request : ReviewHandoffRequest) : Except String ProjectState := do
  let reviewEntry ← match state.entry? request.reviewEntryId with
    | some value => pure value
    | none => throw s!"no Review entry {request.reviewEntryId}"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"entry {request.reviewEntryId} is not a Review"
  let predecessor := activeReviewerRun state review
  if request.successorReviewerRun.isEmpty || request.reason.isEmpty then
    throw "Review handoff requires a successor reviewer and reason"
  if predecessor == request.successorReviewerRun then
    throw "Review handoff successor is already active"
  if review.producerAgentRuns.contains request.successorReviewerRun then
    throw "Review handoff successor produced part of the fixed target"
  appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := reviewEntry.scope
    workId := reviewEntry.workId, designRevision := reviewEntry.designRevision
    payload := .reviewHandoff {
      reviewId := review.reviewId, reviewEntryId := reviewEntry.id
      predecessorReviewerRun := predecessor
      successorReviewerRun := request.successorReviewerRun, reason := request.reason } }

def concludeReview
    (state : ProjectState) (request : ReviewConclusionRequest) : Except String ProjectState := do
  let reviewEntry ← match state.entry? request.reviewEntryId with
    | some value => pure value
    | none => throw s!"no Review entry {request.reviewEntryId}"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"entry {request.reviewEntryId} is not a Review"
  if request.summary.isEmpty then throw "Review conclusion requires a summary"
  let hasFinding := state.ledgerEntries.any fun entry =>
    entry.order > reviewEntry.order && match entry.payload with
    | .finding value => value.reviewId == review.reviewId
    | _ => false
  if request.clean == hasFinding then
    throw "clean conclusion requires no Findings; a non-clean conclusion requires a Finding"
  appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := reviewEntry.scope
    workId := reviewEntry.workId, designRevision := reviewEntry.designRevision
    payload := .reviewConclusion {
      reviewId := review.reviewId, reviewEntryId := reviewEntry.id
      reviewerAgentRun := activeReviewerRun state review
      clean := request.clean, summary := request.summary } }

end AgentWorkbench
