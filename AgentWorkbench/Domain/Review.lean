import AgentWorkbench.Domain.Work

namespace AgentWorkbench

inductive ReviewPurpose where
  | design
  | implementation
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive ReviewContext where
  | fresh
  | resume
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewTargetComponent where
  kind : String
  id : String
  snapshot : String
  producerAgentRuns : List String := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewRecord where
  reviewId : String
  purpose : ReviewPurpose
  context : ReviewContext
  continuesEntryId : Option String := none
  targetSourceId : String
  target : String
  targetSnapshot : String
  targetManifestVersion : Nat := 0
  targetManifest : List ReviewTargetComponent := []
  producerAgentRuns : List String := []
  reviewerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson

private structure PersistedReviewRecord where
  reviewId : String
  purpose : ReviewPurpose
  context : ReviewContext
  continuesEntryId : Option String := none
  targetSourceId : String
  target : String
  targetSnapshot : String
  targetManifestVersion : Nat := 0
  targetManifest : List ReviewTargetComponent
  producerAgentRuns : List String
  reviewerAgentRun : String
  deriving Lean.FromJson

private structure PersistedReviewRecordV0 where
  reviewId : String
  purpose : ReviewPurpose
  context : ReviewContext
  continuesEntryId : Option String := none
  targetSourceId : String
  target : String
  targetSnapshot : String
  targetManifest : List ReviewTargetComponent
  producerAgentRuns : List String
  reviewerAgentRun : String
  deriving Lean.FromJson

private structure LegacyReviewRecord where
  reviewId : String
  purpose : ReviewPurpose
  context : ReviewContext
  continuesEntryId : Option String := none
  targetSourceId : String
  target : String
  targetSnapshot : String
  producerAgentRun : String
  reviewerAgentRun : String
  deriving Lean.FromJson

instance : Lean.FromJson ReviewRecord where
  fromJson? json :=
    match (Lean.fromJson? json : Except String PersistedReviewRecord) with
    | .ok value => pure {
        reviewId := value.reviewId, purpose := value.purpose, context := value.context
        continuesEntryId := value.continuesEntryId, targetSourceId := value.targetSourceId
        target := value.target, targetSnapshot := value.targetSnapshot
        targetManifestVersion := value.targetManifestVersion
        targetManifest := value.targetManifest, producerAgentRuns := value.producerAgentRuns
        reviewerAgentRun := value.reviewerAgentRun }
    | .error currentError =>
        match (Lean.fromJson? json : Except String PersistedReviewRecordV0) with
        | .ok value => pure {
            reviewId := value.reviewId, purpose := value.purpose, context := value.context
            continuesEntryId := value.continuesEntryId, targetSourceId := value.targetSourceId
            target := value.target, targetSnapshot := value.targetSnapshot
            targetManifest := value.targetManifest, producerAgentRuns := value.producerAgentRuns
            reviewerAgentRun := value.reviewerAgentRun }
        | .error persistedV0Error =>
            match (Lean.fromJson? json : Except String LegacyReviewRecord) with
            | .ok value => pure {
                reviewId := value.reviewId, purpose := value.purpose, context := value.context
                continuesEntryId := value.continuesEntryId, targetSourceId := value.targetSourceId
                target := value.target, targetSnapshot := value.targetSnapshot
                producerAgentRuns := [value.producerAgentRun]
                reviewerAgentRun := value.reviewerAgentRun }
            | .error legacyError =>
                throw (s!"invalid Review: {currentError}; persisted v0: {persistedV0Error}; " ++
                  s!"legacy: {legacyError}" : String)

inductive FindingSubjectKind where
  | statement
  | criterion
  | assumption
  | implementationComponent
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure FindingSubject where
  kind : FindingSubjectKind
  id : String
  exactQuote : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure FindingRecord where
  reviewId : String
  subject : FindingSubject
  targetSourceId : String
  target : String
  targetSnapshot : String
  producerAgentRuns : List String := []
  summary : String
  deriving Repr, DecidableEq, Lean.ToJson

private structure PersistedFindingRecord where
  reviewId : String
  subject : FindingSubject
  targetSourceId : String
  target : String
  targetSnapshot : String
  producerAgentRuns : List String
  summary : String
  deriving Lean.FromJson

private structure LegacyFindingRecord where
  reviewId : String
  subject : FindingSubject
  targetSourceId : String
  target : String
  targetSnapshot : String
  producerAgentRun : String
  summary : String
  deriving Lean.FromJson

instance : Lean.FromJson FindingRecord where
  fromJson? json :=
    match (Lean.fromJson? json : Except String PersistedFindingRecord) with
    | .ok value => pure {
        reviewId := value.reviewId, subject := value.subject
        targetSourceId := value.targetSourceId, target := value.target
        targetSnapshot := value.targetSnapshot, producerAgentRuns := value.producerAgentRuns
        summary := value.summary }
    | .error currentError =>
        match (Lean.fromJson? json : Except String LegacyFindingRecord) with
        | .ok value => pure {
            reviewId := value.reviewId, subject := value.subject
            targetSourceId := value.targetSourceId, target := value.target
            targetSnapshot := value.targetSnapshot, producerAgentRuns := [value.producerAgentRun]
            summary := value.summary }
        | .error legacyError =>
            throw s!"invalid Finding: {currentError}; legacy: {legacyError}"

structure ReviewDispositionRecord where
  findingEntryId : String
  decision : DispositionDecision
  reason : String
  decidedByRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewVerificationRecord where
  reviewId : String
  findingEntryId : String
  reviewEntryId : String
  evidenceEntryId : String
  target : String
  snapshot : String
  verifiedByRun : String
  resolved : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewHandoffRecord where
  reviewId : String
  reviewEntryId : String
  predecessorReviewerRun : String
  successorReviewerRun : String
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewConclusionRecord where
  reviewId : String
  reviewEntryId : String
  reviewerAgentRun : String
  clean : Bool
  summary : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
