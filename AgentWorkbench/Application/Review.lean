import AgentWorkbench.Application.Ledger
import AgentWorkbench.Adapter.ReviewTarget
import AgentWorkbench.Decision.Finding

namespace AgentWorkbench

structure ReviewStartRequest where
  entryId : String
  reviewId : String
  purpose : ReviewPurpose
  targetSourceId : String
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

private def appendIo (result : Except String ProjectState) : IO ProjectState :=
  match result with
  | .ok value => pure value
  | .error message => throw (IO.userError message)

def startReview
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : ReviewStartRequest) : IO ProjectState := do
  let _ := projectRoot
  let fixed ← match ReviewTarget.fromReference state request.purpose request.targetSourceId with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  appendIo (appendCurrentEntry state request.entryId (.review {
    reviewId := request.reviewId, purpose := request.purpose, context := .fresh
    targetSourceId := fixed.sourceId, target := fixed.target, targetSnapshot := fixed.snapshot
    producerAgentRun := fixed.producerAgentRun
    reviewerAgentRun := request.reviewerAgentRun }))

def resumeReview
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : ReviewResumeRequest) : IO ProjectState := do
  let priorEntry ← match state.entry? request.continuesEntryId with
    | some value => pure value
    | none => throw (IO.userError s!"no Review entry {request.continuesEntryId}")
  let prior ← match priorEntry.payload with
    | .review value => pure value
    | _ => throw (IO.userError s!"entry {request.continuesEntryId} is not a Review")
  let snapshot ← ReviewTarget.currentSnapshot projectRoot state prior.purpose prior.target
  appendIo (appendCurrentEntry state request.entryId (.review {
    reviewId := prior.reviewId, purpose := prior.purpose, context := .resume
    continuesEntryId := some priorEntry.id, targetSourceId := prior.targetSourceId
    target := prior.target, targetSnapshot := snapshot, producerAgentRun := prior.producerAgentRun
    reviewerAgentRun := prior.reviewerAgentRun }))

def recordFinding
    (state : ProjectState) (request : FindingRecordRequest) : Except String ProjectState := do
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw "review finding requires a current Work and accepted Design"
  if !findingInputsEligible projection request.reviewEntryId request.subject then
    throw "review finding requires a compatible current root Review and exact subject"
  let reviewEntry ← match state.entry? request.reviewEntryId with
    | some value => pure value
    | none => throw s!"no Review entry {request.reviewEntryId}"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"entry {request.reviewEntryId} is not a Review"
  appendCurrentEntry state request.entryId (.finding {
    reviewId := review.reviewId, subject := request.subject
    targetSourceId := review.targetSourceId, target := review.target
    targetSnapshot := review.targetSnapshot, producerAgentRun := review.producerAgentRun
    summary := request.summary })

def recordDisposition
    (state : ProjectState) (request : DispositionRecordRequest) : Except String ProjectState := do
  let (_, work) ← currentBinding state
  if !work.delegatedReviewDecisions.contains request.decision then
    throw "the responsible Work does not delegate this Review disposition decision"
  appendCurrentEntry state request.entryId (.reviewDisposition {
    findingEntryId := request.findingEntryId, decision := request.decision
    reason := request.reason, decidedByRun := work.responsibleAgentRun })

def recordVerification
    (state : ProjectState) (request : VerificationRecordRequest) : Except String ProjectState := do
  let reviewEntry ← match state.entry? request.reviewEntryId with
    | some value => pure value
    | none => throw s!"no Review entry {request.reviewEntryId}"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"entry {request.reviewEntryId} is not a Review"
  if review.context != .resume then throw "Review verification requires a resumed Review"
  appendCurrentEntry state request.entryId (.reviewVerification {
    reviewId := review.reviewId, findingEntryId := request.findingEntryId
    reviewEntryId := reviewEntry.id, evidenceEntryId := request.evidenceEntryId
    target := review.target, snapshot := review.targetSnapshot
    verifiedByRun := review.reviewerAgentRun, resolved := true })

end AgentWorkbench
