import AgentWorkbench.Adapter.Codec

namespace AgentWorkbench.Adapter.LegacyV5

open AgentWorkbench.Domain
open SQLite.Blob

structure Requirement where
  key : String
  active : Bool
deriving DecidableEq, Repr

structure DesignVersion where
  id : DesignId
  revision : Revision
  predecessor : Option DesignId := none
  owner : String
  contentDigest : String
  requirements : List Requirement
  decisions : List String
  validationGates : List String
deriving DecidableEq, Repr

structure ReviewPlan where
  id : ReviewPlanId
  owner : String
  reviewer : String
  adjudicator : String
  scope : Domain.Review.FrozenScope
deriving DecidableEq, Repr

structure AuthorityException where
  key : String
  plan : ReviewPlanId
  scope : Domain.Review.FrozenScope
  owner : String
  reviewer : String
  adjudicator : String
  authorizedBy : String
  reason : String
deriving DecidableEq, Repr

structure ObservationDisposition where
  observation : String
  decision : Domain.Review.ObservationDecision
  reason : String
  changesAuthority : Bool := false
  successorDesign : Option DesignId := none
deriving DecidableEq, Repr

structure Adjudication where
  review : ReviewId
  decision : OwnerDecision
  adjudicator : String := ""
  reason : String
  observations : List ObservationDisposition := []
deriving DecidableEq, Repr

structure Obligation where
  work : WorkId
  key : String
  revision : Revision
  commandProfile : String
  invocation : String
  repository : String
  snapshot : String
  artifactDigest : String
  current : Bool
  kind : EvidenceKind := .test
  requirements : List String := []
  expectedProducer : String := ""
  expectedObservation : String := ""
  design : DesignId := ⟨0⟩
  designRevision : Revision := ⟨0⟩
  negative : Bool := false
  reintroductionHarm : String := ""
  positiveBoundaryInsufficient : String := ""
deriving DecidableEq, Repr

structure Attempt where
  operation : OperationId
  work : Option WorkId := none
  kind : Domain.ExternalOperation.OperationKind := .publication
  artifactDigest : String
  state : Domain.ExternalOperation.AttemptState
  observation : Option Domain.ExternalOperation.RemoteObservation := none
  disposition : Option String := none
deriving DecidableEq, Repr, BEq

structure CompletionPlan where
  work : WorkId
  relatedWork : List Domain.Lifecycle.RelatedWorkRequirement
  phases : List Domain.Lifecycle.PhaseSpec
  tasks : List String
  checklists : List String
  reviews : List ReviewPlanId
  findings : List String
  validations : List String
  repositories : List String
  corrections : List String
  workRecords : List String
deriving DecidableEq, Repr

structure CompletionState where
  plan : CompletionPlan
  epoch : CompletionEpoch
  phases : List Domain.Lifecycle.PhaseRecord
  tasks : List Domain.Lifecycle.TaskRecord
  checklists : List Domain.Lifecycle.ChecklistRecord
  findings : List Domain.Lifecycle.FindingRecord
  validations : List Domain.Lifecycle.ValidationRecord
  repositories : List Domain.Lifecycle.RepositoryRecord
  corrections : List Domain.Lifecycle.CorrectionRecord
  workRecords : List Domain.Lifecycle.WorkRecordLink
  scopeChanges : List Domain.Lifecycle.ScopeChange
deriving DecidableEq, Repr

structure State where
  revision : Revision
  work : List Domain.Work.WorkUnit
  activations : List Domain.Work.Activation
  designs : List DesignVersion
  designApprovals : List Domain.Design.Approval
  decompositions : List Domain.Design.Decomposition
  reviewPlans : List ReviewPlan
  authorityExceptions : List AuthorityException
  claims : List Domain.Review.Claim
  adjudications : List Adjudication
  reviewFindings : List Domain.Review.Finding
  findingVerifications : List Domain.Review.Verification
  corrections : List Domain.Design.Correction
  authorityTransitions : List Domain.Design.AuthorityTransition
  evidence : List Domain.Evidence.Evidence
  externalOperations : List Attempt
  obligations : List Obligation
  lifecycle : List CompletionState
  returnTarget : Option ActivationId
deriving DecidableEq, Repr

inductive Event
  | workInitialized (work : Domain.Work.WorkUnit) (activation : Domain.Work.Activation)
  | workRegistered (work : Domain.Work.WorkUnit)
  | suspendedActivationRegistered (activation : Domain.Work.Activation)
  | workSuspended (work : WorkId) (activation : ActivationId)
      (context : Domain.Work.SuspensionContext)
  | resumeReadinessConfirmed (work : WorkId) (activation : ActivationId)
      (basis : Domain.Work.ReadinessBasis)
  | suspensionRevised (work : WorkId) (activation : ActivationId)
      (context : Domain.Work.SuspensionContext)
  | workResumed (work : WorkId) (activation : ActivationId)
  | designImported (version : DesignVersion)
  | designApproved (approval : Domain.Design.Approval)
  | decompositionRecorded (decomposition : Domain.Design.Decomposition)
  | authorityExceptionRecorded (exception : AuthorityException)
  | reviewPlanRecorded (plan : ReviewPlan)
  | completionPlanned (plan : CompletionPlan)
  | relatedWorkTerminalAcknowledged (owner related : WorkId)
  | scopeChangeRecorded (change : Domain.Lifecycle.ScopeChange)
  | phaseCompleted (work : WorkId) (key : String)
  | taskCompleted (work : WorkId) (key : String)
  | checklistCompleted (work : WorkId) (key : String)
  | findingResolved (work : WorkId) (key : String)
  | validationPassed (work : WorkId) (key artifactDigest : String)
  | repositoryClassified (work : WorkId) (key snapshotDigest : String)
  | correctionResolved (work : WorkId) (key : String)
  | workRecordLinked (work : WorkId) (key reference : String)
  | reviewClaimed (claim : Domain.Review.Claim)
  | reviewAdjudicated (decision : Adjudication)
  | reviewFindingRecorded (finding : Domain.Review.Finding)
  | reviewFindingAdjudicated (key principal reason : String) (accepted : Bool)
  | reviewFindingClosureAttempted (key : String)
      (attempt : Domain.Review.ClosureAttempt)
  | findingVerified (verification : Domain.Review.Verification)
  | findingVerificationAdjudicated (finding : String) (attempt : Nat)
      (adjudicator : String)
  | correctionRecorded (correction : Domain.Design.Correction)
  | userCorrectionResolved (key reason : String) (rejected : Bool)
  | authorityTransitionRecorded (transition : Domain.Design.AuthorityTransition)
  | evidenceRecorded (item : Domain.Evidence.Evidence)
  | externalOperationRecorded (attempt : Attempt)
  | externalOperationAdvanced (attempt : Attempt)
  | obligationRecorded (obligation : Obligation)
  | workCompleted (work : WorkId) (activation : ActivationId)
deriving DecidableEq, Repr

inductive Command
  | initializeWork (expectedRevision : Revision)
      (work : Domain.Work.WorkUnit) (activation : Domain.Work.Activation)
  | registerWork (expectedRevision : Revision) (work : Domain.Work.WorkUnit)
  | registerSuspendedActivation (expectedRevision : Revision)
      (activation : Domain.Work.Activation)
  | suspendWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId) (context : Domain.Work.SuspensionContext)
  | confirmResumeReadiness (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId) (basis : Domain.Work.ReadinessBasis)
  | reviseSuspension (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId) (context : Domain.Work.SuspensionContext)
  | resumeWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId)
  | importDesign (expectedRevision : Revision) (version : DesignVersion)
  | approveDesign (expectedRevision : Revision) (design : DesignId)
  | recordDecomposition (expectedRevision : Revision)
      (decomposition : Domain.Design.Decomposition)
  | recordAuthorityException (expectedRevision : Revision)
      (exception : AuthorityException)
  | recordReviewPlan (expectedRevision : Revision) (plan : ReviewPlan)
  | planCompletion (expectedRevision : Revision) (plan : CompletionPlan)
  | acknowledgeRelatedWorkTerminal (expectedRevision : Revision) (owner related : WorkId)
  | recordScopeChange (expectedRevision : Revision)
      (change : Domain.Lifecycle.ScopeChange)
  | completePhase (expectedRevision : Revision) (work : WorkId) (key : String)
  | completeTask (expectedRevision : Revision) (work : WorkId) (key : String)
  | completeChecklist (expectedRevision : Revision) (work : WorkId) (key : String)
  | resolveFinding (expectedRevision : Revision) (work : WorkId) (key : String)
  | passValidation (expectedRevision : Revision) (work : WorkId)
      (key artifactDigest : String)
  | classifyRepository (expectedRevision : Revision) (work : WorkId)
      (key snapshotDigest : String)
  | resolveCorrection (expectedRevision : Revision) (work : WorkId) (key : String)
  | linkWorkRecord (expectedRevision : Revision) (work : WorkId)
      (key reference : String)
  | recordReviewClaim (expectedRevision : Revision) (claim : Domain.Review.Claim)
  | recordReviewAdjudication (expectedRevision : Revision)
      (adjudication : Adjudication)
  | recordReviewFinding (expectedRevision : Revision) (finding : Domain.Review.Finding)
  | adjudicateReviewFinding (expectedRevision : Revision)
      (key principal reason : String) (accepted : Bool)
  | closeReviewFinding (expectedRevision : Revision) (key : String)
      (attempt : Domain.Review.ClosureAttempt)
  | verifyReviewFinding (expectedRevision : Revision)
      (verification : Domain.Review.Verification)
  | adjudicateFindingVerification (expectedRevision : Revision)
      (finding : String) (attempt : Nat) (adjudicator : String)
  | recordUserCorrection (expectedRevision : Revision)
      (correction : Domain.Design.Correction)
  | resolveUserCorrection (expectedRevision : Revision) (key reason : String)
  | rejectUserProposal (expectedRevision : Revision) (key reason : String)
  | recordAuthorityTransition (expectedRevision : Revision)
      (transition : Domain.Design.AuthorityTransition)
  | recordEvidence (expectedRevision : Revision) (evidence : Domain.Evidence.Evidence)
  | recordExternalOperation (expectedRevision : Revision) (attempt : Attempt)
  | advanceExternalOperation (expectedRevision : Revision) (attempt : Attempt)
  | recordObligation (expectedRevision : Revision) (obligation : Obligation)
  | completeWork (expectedRevision : Revision) (target : WorkId)
deriving DecidableEq, Repr

structure CanonicalRequest where
  command : Command
  artifacts : List (String × Nat)
deriving DecidableEq, Repr

inductive ProjectionPayload
  | decoded (state : State)
  | decodeFailed (fault : Domain.Projection.DecodeFault)
deriving DecidableEq, Repr

structure ProjectionObservation where
  fingerprint : Domain.Projection.ProjectionFingerprint
  reference : Domain.Projection.ProjectionRef
  payload : ProjectionPayload
deriving DecidableEq, Repr

deriving instance ToBinary, FromBinary for Requirement
deriving instance ToBinary, FromBinary for DesignVersion
deriving instance ToBinary, FromBinary for ReviewPlan
deriving instance ToBinary, FromBinary for AuthorityException
deriving instance ToBinary, FromBinary for ObservationDisposition
deriving instance ToBinary, FromBinary for Adjudication
deriving instance ToBinary, FromBinary for Obligation
deriving instance ToBinary, FromBinary for Attempt
deriving instance ToBinary, FromBinary for CompletionPlan
deriving instance ToBinary, FromBinary for CompletionState
deriving instance ToBinary, FromBinary for State
deriving instance ToBinary, FromBinary for Event
deriving instance ToBinary, FromBinary for Command
deriving instance ToBinary, FromBinary for CanonicalRequest
deriving instance ToBinary, FromBinary for ProjectionPayload
deriving instance ToBinary, FromBinary for ProjectionObservation

def Requirement.toCurrent (requirement : Requirement) : Domain.Design.Requirement :=
  { key := requirement.key
    active := requirement.active
    negativeValidationAuthority := false }

def DesignVersion.toCurrent (version : DesignVersion) : Domain.Design.DesignVersion :=
  { id := version.id
    revision := version.revision
    predecessor := version.predecessor
    owner := version.owner
    contentDigest := version.contentDigest
    requirements := version.requirements.map Requirement.toCurrent
    decisions := version.decisions
    validationGates := version.validationGates }

def ReviewPlan.toCurrent (plan : ReviewPlan) : Domain.Review.Plan :=
  { id := plan.id
    owner := plan.owner
    reviewer := plan.reviewer
    adjudicator := plan.adjudicator
    caller := plan.adjudicator
    scope := plan.scope }

def AuthorityException.toCurrent
    (exception : AuthorityException) : Domain.Review.AuthorityException :=
  { key := exception.key
    plan := exception.plan
    scope := exception.scope
    owner := exception.owner
    reviewer := exception.reviewer
    adjudicator := exception.adjudicator
    caller := exception.adjudicator
    authorizedBy := exception.authorizedBy
    reason := exception.reason }

private def migrationRationale (disposition : ObservationDisposition) :
    Domain.Review.AdoptionRationale := {
  necessity :=
    s!"Preserve accepted schema-v5 observation {disposition.observation} during explicit update."
  simplerAlternativesInsufficient :=
    "Dropping or changing the accepted decision would alter recovered authoritative state."
  boundedScope :=
    s!"Only the already-adopted schema-v5 observation {disposition.observation}."
  complexityCost :=
    "The compatibility metadata is limited to the v5-to-v6 update boundary." }

def ObservationDisposition.toCurrent (proposalKeys : List String)
    (disposition : ObservationDisposition) : Domain.Review.ObservationDisposition :=
  { observation := disposition.observation
    decision := disposition.decision
    reason := disposition.reason
    changesAuthority := disposition.changesAuthority
    successorDesign := disposition.successorDesign
    adoptionRationale :=
      if disposition.decision == .accepted &&
          proposalKeys.contains disposition.observation then
        some (migrationRationale disposition)
      else none }

def Adjudication.toCurrent (claims : List Domain.Review.Claim)
    (adjudication : Adjudication) : Domain.Review.Adjudication :=
  let proposalKeys :=
    (claims.find? (·.id == adjudication.review)).map
      (fun claim =>
        (claim.observations.filter (·.kind == .proposal)).map (·.key)) |>.getD []
  { review := adjudication.review
    decision := adjudication.decision
    adjudicator := adjudication.adjudicator
    reason := adjudication.reason
    observations :=
      adjudication.observations.map (ObservationDisposition.toCurrent proposalKeys) }

def Obligation.toCurrent (obligation : Obligation) : Domain.Evidence.Obligation :=
  { work := obligation.work
    key := obligation.key
    revision := obligation.revision
    commandProfile := obligation.commandProfile
    invocation := obligation.invocation
    repository := obligation.repository
    snapshot := obligation.snapshot
    artifactDigest := obligation.artifactDigest
    current := obligation.current
    kind := obligation.kind
    requirements := obligation.requirements
    expectedProducer := obligation.expectedProducer
    expectedObservation := obligation.expectedObservation
    design := obligation.design
    designRevision := obligation.designRevision
    negative := obligation.negative }

def Attempt.toCurrent (attempt : Attempt) : Domain.ExternalOperation.Attempt :=
  let failedWithoutObservation :=
    attempt.state == .failed && attempt.observation.isNone
  { operation := attempt.operation
    work := attempt.work
    kind := attempt.kind
    target := .unresolved
    artifactDigest := attempt.artifactDigest
    remotePrecondition := {}
    state := attempt.state
    observation := attempt.observation
    disposition :=
      if failedWithoutObservation then
        some "Imported terminal schema-v5 failure without a remote observation."
      else attempt.disposition }

def CompletionPlan.toCurrent (decompositions : List Domain.Design.Decomposition)
    (obligations : List Obligation)
    (plan : CompletionPlan) : Domain.Lifecycle.CompletionPlan :=
  { work := plan.work
    decomposition :=
      (decompositions.find? (·.work == plan.work)).map (·.key)
    relatedWork := plan.relatedWork
    phases := plan.phases
    tasks := plan.tasks
    checklists := plan.checklists
    reviews := plan.reviews
    obligations :=
      (obligations.filter fun obligation =>
        obligation.work == plan.work && obligation.current).map (·.key)
    findings := plan.findings
    validations := plan.validations
    repositories := plan.repositories
    corrections := plan.corrections
    workRecords := plan.workRecords }

def Event.toCurrent : Event → AgentWorkbench.Kernel.Replay.Event
  | .workInitialized work activation => .workInitialized work activation
  | .workRegistered work => .workRegistered work
  | .suspendedActivationRegistered activation => .suspendedActivationRegistered activation
  | .workSuspended work activation context => .workSuspended work activation context
  | .resumeReadinessConfirmed work activation basis =>
      .resumeReadinessConfirmed work activation basis
  | .suspensionRevised work activation context => .suspensionRevised work activation context
  | .workResumed work activation => .workResumed work activation
  | .designImported version => .designImported version.toCurrent
  | .designApproved approval => .designApproved approval
  | .decompositionRecorded decomposition => .decompositionRecorded decomposition
  | .authorityExceptionRecorded exception =>
      .authorityExceptionRecorded exception.toCurrent
  | .reviewPlanRecorded plan => .reviewPlanRecorded plan.toCurrent
  | .completionPlanned plan =>
      .completionPlanned (plan.toCurrent [] [])
  | .relatedWorkTerminalAcknowledged owner related =>
      .relatedWorkTerminalAcknowledged owner related
  | .scopeChangeRecorded change => .scopeChangeRecorded change
  | .phaseCompleted work key => .phaseCompleted work key
  | .taskCompleted work key => .taskCompleted work key
  | .checklistCompleted work key => .checklistCompleted work key
  | .findingResolved work key => .findingResolved work key
  | .validationPassed work key artifactDigest => .validationPassed work key artifactDigest
  | .repositoryClassified work key snapshotDigest =>
      .repositoryClassified work key snapshotDigest
  | .correctionResolved work key => .correctionResolved work key
  | .workRecordLinked work key reference => .workRecordLinked work key reference
  | .reviewClaimed claim => .reviewClaimed claim
  | .reviewAdjudicated decision => .reviewAdjudicated (decision.toCurrent [])
  | .reviewFindingRecorded finding => .reviewFindingRecorded finding
  | .reviewFindingAdjudicated key principal reason accepted =>
      .reviewFindingAdjudicated key principal reason accepted
  | .reviewFindingClosureAttempted key attempt =>
      .reviewFindingClosureAttempted key attempt
  | .findingVerified verification => .findingVerified verification
  | .findingVerificationAdjudicated finding attempt adjudicator =>
      .findingVerificationAdjudicated finding attempt adjudicator
  | .correctionRecorded correction => .correctionRecorded correction
  | .userCorrectionResolved key reason rejected =>
      .userCorrectionResolved key reason rejected
  | .authorityTransitionRecorded transition => .authorityTransitionRecorded transition
  | .evidenceRecorded item => .evidenceRecorded item
  | .externalOperationRecorded attempt => .externalOperationRecorded attempt.toCurrent
  | .externalOperationAdvanced attempt => .externalOperationAdvanced attempt.toCurrent
  | .obligationRecorded obligation => .obligationRecorded obligation.toCurrent
  | .workCompleted work activation => .workCompleted work activation

private def CompletionPlan.toCurrentAt
    (state : AgentWorkbench.Kernel.Replay.State) (plan : CompletionPlan) :
    Domain.Lifecycle.CompletionPlan :=
  { plan.toCurrent [] [] with
    decomposition := (state.decompositions.find? (·.work == plan.work)).map (·.key)
    obligations :=
      (state.obligations.filter fun obligation =>
        obligation.work == plan.work && obligation.current).map (·.key) }

def Event.toCurrentAt (state : AgentWorkbench.Kernel.Replay.State) :
    Event → AgentWorkbench.Kernel.Replay.Event
  | .completionPlanned plan => .completionPlanned (plan.toCurrentAt state)
  | .reviewAdjudicated decision =>
      .reviewAdjudicated (decision.toCurrent state.claims)
  | event => event.toCurrent

def Command.toCurrent : Command → AgentWorkbench.Kernel.Decide.Command
  | .initializeWork revision work activation => .initializeWork revision work activation
  | .registerWork revision work => .registerWork revision work
  | .registerSuspendedActivation revision activation =>
      .registerSuspendedActivation revision activation
  | .suspendWork revision work activation context =>
      .suspendWork revision work activation context
  | .confirmResumeReadiness revision work activation basis =>
      .confirmResumeReadiness revision work activation basis
  | .reviseSuspension revision work activation context =>
      .reviseSuspension revision work activation context
  | .resumeWork revision work activation => .resumeWork revision work activation
  | .importDesign revision version => .importDesign revision version.toCurrent
  | .approveDesign revision design => .approveDesign revision design
  | .recordDecomposition revision decomposition =>
      .recordDecomposition revision decomposition
  | .recordAuthorityException revision exception =>
      .recordAuthorityException revision exception.toCurrent
  | .recordReviewPlan revision plan => .recordReviewPlan revision plan.toCurrent
  | .planCompletion revision plan => .planCompletion revision (plan.toCurrent [] [])
  | .acknowledgeRelatedWorkTerminal revision owner related =>
      .acknowledgeRelatedWorkTerminal revision owner related
  | .recordScopeChange revision change => .recordScopeChange revision change
  | .completePhase revision work key => .completePhase revision work key
  | .completeTask revision work key => .completeTask revision work key
  | .completeChecklist revision work key => .completeChecklist revision work key
  | .resolveFinding revision work key => .resolveFinding revision work key
  | .passValidation revision work key artifactDigest =>
      .passValidation revision work key artifactDigest
  | .classifyRepository revision work key snapshotDigest =>
      .classifyRepository revision work key snapshotDigest
  | .resolveCorrection revision work key => .resolveCorrection revision work key
  | .linkWorkRecord revision work key reference =>
      .linkWorkRecord revision work key reference
  | .recordReviewClaim revision claim => .recordReviewClaim revision claim
  | .recordReviewAdjudication revision adjudication =>
      .recordReviewAdjudication revision (adjudication.toCurrent [])
  | .recordReviewFinding revision finding => .recordReviewFinding revision finding
  | .adjudicateReviewFinding revision key principal reason accepted =>
      .adjudicateReviewFinding revision key principal reason accepted
  | .closeReviewFinding revision key attempt => .closeReviewFinding revision key attempt
  | .verifyReviewFinding revision verification =>
      .verifyReviewFinding revision verification
  | .adjudicateFindingVerification revision finding attempt adjudicator =>
      .adjudicateFindingVerification revision finding attempt adjudicator
  | .recordUserCorrection revision correction =>
      .recordUserCorrection revision correction
  | .resolveUserCorrection revision key reason => .resolveUserCorrection revision key reason
  | .rejectUserProposal revision key reason => .rejectUserProposal revision key reason
  | .recordAuthorityTransition revision transition =>
      .recordAuthorityTransition revision transition
  | .recordEvidence revision evidence => .recordEvidence revision evidence
  | .recordExternalOperation revision attempt =>
      .recordExternalOperation revision attempt.toCurrent
  | .advanceExternalOperation revision attempt =>
      .advanceExternalOperation revision attempt.toCurrent
  | .recordObligation revision obligation =>
      .recordObligation revision obligation.toCurrent
  | .completeWork revision target => .completeWork revision target

def Command.toCurrentAt (state : AgentWorkbench.Kernel.Replay.State) :
    Command → AgentWorkbench.Kernel.Decide.Command
  | .planCompletion revision plan =>
      .planCompletion revision (plan.toCurrentAt state)
  | .recordReviewAdjudication revision adjudication =>
      .recordReviewAdjudication revision (adjudication.toCurrent state.claims)
  | command => command.toCurrent

private def Requirement.fromCurrent
    (requirement : Domain.Design.Requirement) : Requirement :=
  { key := requirement.key, active := requirement.active }

private def DesignVersion.fromCurrent
    (version : Domain.Design.DesignVersion) : DesignVersion :=
  { id := version.id
    revision := version.revision
    predecessor := version.predecessor
    owner := version.owner
    contentDigest := version.contentDigest
    requirements := version.requirements.map Requirement.fromCurrent
    decisions := version.decisions
    validationGates := version.validationGates }

private def ReviewPlan.fromCurrent (plan : Domain.Review.Plan) : ReviewPlan :=
  { id := plan.id
    owner := plan.owner
    reviewer := plan.reviewer
    adjudicator := plan.adjudicator
    scope := plan.scope }

private def AuthorityException.fromCurrent
    (exception : Domain.Review.AuthorityException) : AuthorityException :=
  { key := exception.key
    plan := exception.plan
    scope := exception.scope
    owner := exception.owner
    reviewer := exception.reviewer
    adjudicator := exception.adjudicator
    authorizedBy := exception.authorizedBy
    reason := exception.reason }

private def ObservationDisposition.fromCurrent
    (disposition : Domain.Review.ObservationDisposition) : ObservationDisposition :=
  { observation := disposition.observation
    decision := disposition.decision
    reason := disposition.reason
    changesAuthority := disposition.changesAuthority
    successorDesign := disposition.successorDesign }

private def Adjudication.fromCurrent
    (adjudication : Domain.Review.Adjudication) : Adjudication :=
  { review := adjudication.review
    decision := adjudication.decision
    adjudicator := adjudication.adjudicator
    reason := adjudication.reason
    observations := adjudication.observations.map ObservationDisposition.fromCurrent }

private def CompletionPlan.fromCurrent
    (plan : Domain.Lifecycle.CompletionPlan) : CompletionPlan :=
  { work := plan.work
    relatedWork := plan.relatedWork
    phases := plan.phases
    tasks := plan.tasks
    checklists := plan.checklists
    reviews := plan.reviews
    findings := plan.findings
    validations := plan.validations
    repositories := plan.repositories
    corrections := plan.corrections
    workRecords := plan.workRecords }

private def CompletionState.fromCurrent
    (state : Domain.Lifecycle.CompletionState) : CompletionState :=
  { plan := CompletionPlan.fromCurrent state.plan
    epoch := state.epoch
    phases := state.phases
    tasks := state.tasks
    checklists := state.checklists
    findings := state.findings
    validations := state.validations
    repositories := state.repositories
    corrections := state.corrections
    workRecords := state.workRecords
    scopeChanges := state.scopeChanges }

private def latestLegacyAttempt (events : List Event)
    (operation : OperationId) : Option Attempt :=
  events.foldl (init := none) fun latest event =>
    match event with
    | .externalOperationRecorded attempt
    | .externalOperationAdvanced attempt =>
        if attempt.operation == operation then some attempt else latest
    | _ => latest

private def latestLegacyObligation (events : List Event)
    (work : WorkId) (key : String) : Option Obligation :=
  events.foldl (init := none) fun latest event =>
    match event with
    | .obligationRecorded obligation =>
        if obligation.work == work && obligation.key == key then
          some obligation
        else latest
    | _ => latest

def State.fromCurrent (events : List Event)
    (state : AgentWorkbench.Kernel.Replay.State) : Except String State := do
  let mut externalOperations := []
  for attempt in state.externalOperations do
    let legacy ← match latestLegacyAttempt events attempt.operation with
      | some legacy => .ok legacy
      | none => .error (s!"legacy v5 attempt {attempt.operation.value} is absent from its event prefix")
    externalOperations := externalOperations ++ [legacy]
  let mut obligations := []
  for obligation in state.obligations do
    let legacy ← match latestLegacyObligation events obligation.work obligation.key with
      | some legacy => .ok legacy
      | none => .error (s!"legacy v5 obligation {obligation.key} is absent from its event prefix")
    obligations := obligations ++ [{ legacy with current := obligation.current }]
  return {
    revision := state.revision
    work := state.work
    activations := state.activations
    designs := state.designs.map DesignVersion.fromCurrent
    designApprovals := state.designApprovals
    decompositions := state.decompositions
    reviewPlans := state.reviewPlans.map ReviewPlan.fromCurrent
    authorityExceptions := state.authorityExceptions.map AuthorityException.fromCurrent
    claims := state.claims
    adjudications := state.adjudications.map Adjudication.fromCurrent
    reviewFindings := state.reviewFindings
    findingVerifications := state.findingVerifications
    corrections := state.corrections
    authorityTransitions := state.authorityTransitions
    evidence := state.evidence
    externalOperations
    obligations
    lifecycle := state.lifecycle.map CompletionState.fromCurrent
    returnTarget := state.returnTarget }

def eventDigest (events : List Event) : String :=
  s!"{repr events}"

def stateDigest (state : State) : String :=
  s!"{repr state}"

end AgentWorkbench.Adapter.LegacyV5
