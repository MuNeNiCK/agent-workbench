import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.ExternalOperation

-- Projection wire types and verified projection operations are part of the
-- normative Kernel.Replay module; their namespaces remain stable for callers.
namespace AgentWorkbench.Domain.Projection

open AgentWorkbench.Domain

structure LedgerPoint where
  ledger : LedgerId
  revision : Revision
  historyDigest : Digest
deriving DecidableEq, Repr

structure ProjectionFingerprint where
  id : ProjectionId
  rawDigest : Digest
deriving DecidableEq, Repr

structure ProjectionRef where
  fingerprint : ProjectionFingerprint
  ledger : LedgerId
  revision : Revision
  historyDigest : Digest
  stateDigest : Digest
deriving DecidableEq, Repr

inductive DecodeFault
  | unreadable
  | unsupportedSchema
deriving DecidableEq, Repr

inductive LedgerFault
  | replayRejected (error : DomainError)
  | headRevisionMismatch (replayed stored : Revision)
  | historyDigestMismatch (replayed stored : Digest)
deriving DecidableEq, Repr

inductive ProjectionFault
  | undecodable (fault : DecodeFault)
  | wrongLedger (observed expected : LedgerId)
  | aheadOfLedger (observed expected : Revision)
  | historyDigestMismatch
  | stateDigestMismatch
  | replayMismatch
deriving DecidableEq, Repr

structure RepairBinding where
  head : LedgerPoint
  observed : Option ProjectionFingerprint
deriving DecidableEq, Repr

structure RepairCommand where
  binding : RepairBinding
deriving DecidableEq, Repr

end AgentWorkbench.Domain.Projection

namespace AgentWorkbench.Kernel.Replay

open AgentWorkbench.Domain

structure State where
  revision : Revision
  work : List Work.WorkUnit
  activations : List Work.Activation
  designs : List Design.DesignVersion
  designApprovals : List Design.Approval
  decompositions : List Design.Decomposition
  reviewPlans : List Review.Plan
  authorityExceptions : List Review.AuthorityException
  claims : List Review.Claim
  adjudications : List Review.Adjudication
  reviewFindings : List Review.Finding
  findingVerifications : List Review.Verification
  corrections : List Design.Correction
  authorityTransitions : List Design.AuthorityTransition
  evidence : List Evidence.Evidence
  externalOperations : List ExternalOperation.Attempt
  obligations : List Evidence.Obligation
  lifecycle : List Lifecycle.CompletionState
  returnTarget : Option ActivationId
deriving DecidableEq, Repr

def ReviewClaimsReferencePlans (_states : List Lifecycle.CompletionState)
    (plans : List Review.Plan) (claims : List Review.Claim) : Prop :=
  (claims.all fun claim =>
    plans.any (fun plan => Review.scopeExact plan claim)) = true

def approvedDesignCurrent (state : State) (version : Design.DesignVersion) : Bool :=
  state.designApprovals.any (·.design == version.id) &&
  !state.designs.any (fun successor =>
    successor.predecessor == some version.id &&
    state.designApprovals.any (·.design == successor.id))

def designApprovalLineageReady (state : State)
    (version : Design.DesignVersion) : Bool :=
  version.predecessor.all fun predecessor =>
    state.designs.any fun current =>
      current.id == predecessor && approvedDesignCurrent state current

def reviewAuthorityCurrent (authority : String) (scope : Review.FrozenScope)
    (state : State) : Bool :=
  (scope.design.bind fun design =>
    state.designs.find? fun version =>
      version.id == design &&
      (scope.purpose == .design || approvedDesignCurrent state version)).any
      (fun version =>
        version.requirements.any (fun requirement =>
          requirement.key == authority && requirement.active) ||
        version.decisions.contains authority) ||
  Design.authorityCurrentFor authority scope.work scope.design
    state.authorityTransitions

def proposalSuccessorsExact (claim : Review.Claim)
    (decision : Review.Adjudication) (state : State) : Bool :=
  decision.observations.all fun disposition =>
    disposition.successorDesign.all fun successor =>
      claim.scope.bind (·.design) |>.any fun reviewed =>
        state.designs.any fun version =>
          version.id == successor && version.predecessor == some reviewed

def reviewPlanOwnerCurrent (plan : Review.Plan) (state : State) : Bool :=
  state.work.any fun work =>
    work.id == plan.scope.work && work.status == .open &&
    work.owner == plan.owner

def reviewAdjudicationApplicable (decision : Review.Adjudication)
    (state : State) : Bool :=
  state.claims.any (fun claim =>
    Review.adjudicationExact claim decision &&
    proposalSuccessorsExact claim decision state &&
    state.reviewPlans.any (fun plan =>
      plan.id == claim.plan &&
      Review.scopeExact plan claim &&
      reviewPlanOwnerCurrent plan state &&
      decision.adjudicator == plan.adjudicator &&
      decision.adjudicator != claim.reviewer)) &&
  !state.adjudications.any (·.review == decision.review)

def returnTargetValid (state : State) : Bool :=
  match state.returnTarget with
  | none => true
  | some target =>
      state.activations.any (fun activation =>
        activation.id == target && activation.status == .suspended &&
        Work.workIsOpen state.work activation.work) &&
      state.activations.any (fun activation =>
        activation.status == .closed && activation.parent == some target)

def ObligationsReferenceDesigns (designs : List Design.DesignVersion)
    (obligations : List Evidence.Obligation) : Prop :=
  (obligations.all fun obligation =>
    designs.any fun design =>
      design.id == obligation.design &&
      design.revision == obligation.designRevision) = true

def ValidState (state : State) : Prop :=
  Work.ValidWorkState state.work state.activations ∧
  (state.designs.map (·.id)).Nodup ∧
  (state.designs.all Design.versionWellFormed) = true ∧
  (state.decompositions.map (·.key)).Nodup ∧
  (state.designApprovals.map (·.design)).Nodup ∧
  (state.decompositions.all Design.decompositionWellFormed) = true ∧
  (state.reviewPlans.map (·.id)).Nodup ∧
  (state.authorityExceptions.map (·.key)).Nodup ∧
  (state.reviewPlans.all Review.planWellFormed) = true ∧
  (state.claims.all Review.claimWellFormed) = true ∧
  Review.ValidReviewState state.claims state.adjudications ∧
  (state.reviewFindings.map (·.key)).Nodup ∧
  (state.reviewFindings.all Review.findingWellFormed) = true ∧
  (state.corrections.map (·.key)).Nodup ∧
  (state.corrections.all Design.correctionWellFormed) = true ∧
  (state.authorityTransitions.map (·.key)).Nodup ∧
  (state.authorityTransitions.all Design.authorityTransitionWellFormed) = true ∧
  ReviewClaimsReferencePlans state.lifecycle state.reviewPlans state.claims ∧
  Evidence.UniqueEvidenceIds state.evidence ∧
  Evidence.EvidenceWellFormed state.evidence ∧
  Evidence.EvidenceReferencesObligations state.evidence state.obligations ∧
  ExternalOperation.UniqueOperations state.externalOperations ∧
  ExternalOperation.AttemptsWellFormed state.externalOperations ∧
  Lifecycle.ValidLifecycleState (state.work.map (·.id)) state.lifecycle ∧
  Evidence.UniqueObligations state.obligations ∧
  Evidence.ObligationsWellFormed state.obligations ∧
  Evidence.ObligationsReferenceWork (state.work.map (·.id)) state.obligations ∧
  ObligationsReferenceDesigns state.designs state.obligations ∧
  Evidence.CurrentObligationsReferenceOpenWork
    ((state.work.filter (·.status == .open)).map (·.id)) state.obligations ∧
  returnTargetValid state = true ∧
  True

instance (state : State) : Decidable (ValidState state) := by
  unfold ValidState Work.ValidWorkState Work.UniqueWorkIds
    Work.UniqueActivationIds Work.OwnersPresent Work.AtMostOneActive
    Work.ActiveReferencesOpenWork
    Work.ActivationsReferenceWork Work.NonterminalActivationsReferenceOpenWork
    Design.versionWellFormed Design.decompositionWellFormed
    Design.correctionWellFormed Design.authorityTransitionWellFormed
    Review.planWellFormed Review.claimWellFormed Review.findingWellFormed
    Review.ValidReviewState Review.UniqueClaimIds Review.UniqueAdjudications
    Review.AdjudicationsReferenceClaims
    Evidence.UniqueEvidenceIds Evidence.EvidenceWellFormed
    Evidence.EvidenceReferencesObligations
    ExternalOperation.UniqueOperations ExternalOperation.AttemptsWellFormed
    ReviewClaimsReferencePlans
    Lifecycle.ValidLifecycleState Lifecycle.ValidPlan Lifecycle.MatchesPlan
    Lifecycle.RecordsWellFormed
    Lifecycle.nonemptyKeys
    Evidence.UniqueObligations Evidence.ObligationsWellFormed
    Evidence.ObligationsReferenceWork Evidence.CurrentObligationsReferenceOpenWork
    ObligationsReferenceDesigns returnTargetValid
  infer_instance

structure VerifiedState where
  state : State
  valid : ValidState state

inductive Event
  | workInitialized (work : Work.WorkUnit) (activation : Work.Activation)
  | workRegistered (work : Work.WorkUnit)
  | suspendedActivationRegistered (activation : Work.Activation)
  | workSuspended (work : WorkId) (activation : ActivationId)
      (context : Work.SuspensionContext)
  | resumeReadinessConfirmed (work : WorkId) (activation : ActivationId)
      (basis : Work.ReadinessBasis)
  | suspensionRevised (work : WorkId) (activation : ActivationId)
      (context : Work.SuspensionContext)
  | workResumed (work : WorkId) (activation : ActivationId)
  | designImported (version : Design.DesignVersion)
  | designApproved (approval : Design.Approval)
  | decompositionRecorded (decomposition : Design.Decomposition)
  | authorityExceptionRecorded (exception : Review.AuthorityException)
  | reviewPlanRecorded (plan : Review.Plan)
  | completionPlanned (plan : Lifecycle.CompletionPlan)
  | relatedWorkTerminalAcknowledged (owner related : WorkId)
  | scopeChangeRecorded (change : Lifecycle.ScopeChange)
  | phaseCompleted (work : WorkId) (key : String)
  | taskCompleted (work : WorkId) (key : String)
  | checklistCompleted (work : WorkId) (key : String)
  | findingResolved (work : WorkId) (key : String)
  | validationPassed (work : WorkId) (key artifactDigest : String)
  | repositoryClassified (work : WorkId) (key snapshotDigest : String)
  | correctionResolved (work : WorkId) (key : String)
  | workRecordLinked (work : WorkId) (key reference : String)
  | reviewClaimed (claim : Review.Claim)
  | reviewAdjudicated (decision : Review.Adjudication)
  | reviewFindingRecorded (finding : Review.Finding)
  | reviewFindingAdjudicated (key principal reason : String) (accepted : Bool)
  | reviewFindingClosureAttempted (key : String)
      (attempt : Review.ClosureAttempt)
  | findingVerified (verification : Review.Verification)
  | findingVerificationAdjudicated (finding : String) (attempt : Nat)
      (adjudicator : String)
  | correctionRecorded (correction : Design.Correction)
  | userCorrectionResolved (key reason : String) (rejected : Bool)
  | authorityTransitionRecorded (transition : Design.AuthorityTransition)
  | evidenceRecorded (item : Evidence.Evidence)
  | externalOperationRecorded (attempt : ExternalOperation.Attempt)
  | externalOperationAdvanced (attempt : ExternalOperation.Attempt)
  | obligationRecorded (obligation : Evidence.Obligation)
  | workCompleted (work : WorkId) (activation : ActivationId)
deriving DecidableEq, Repr

private def applyUnchecked (event : Event) (state : State) : State :=
  let revised := state.revision.next
  let invalidated : State := {
    state with
    revision := revised }
  match event with
  | .workInitialized work activation =>
      { invalidated with work := [work], activations := [activation] }
  | .workRegistered work =>
      { invalidated with work := state.work ++ [work] }
  | .suspendedActivationRegistered activation =>
      { invalidated with activations := state.activations ++ [activation] }
  | .workSuspended _ activation context =>
      match Work.suspend state.activations activation context with
      | some activations => { invalidated with activations }
      | none => invalidated
  | .resumeReadinessConfirmed _ activation basis =>
      match Work.markResumeReady state.activations activation basis with
      | some activations => { invalidated with activations }
      | none => invalidated
  | .suspensionRevised _ activation context =>
      match Work.reviseSuspension state.activations activation context with
      | some activations => { invalidated with activations }
      | none => invalidated
  | .workResumed _ activation =>
      match Work.resume state.activations activation with
      | some activations =>
          { { invalidated with activations } with returnTarget := none }
      | none => invalidated
  | .designImported version =>
      { invalidated with designs := state.designs ++ [version] }
  | .designApproved approval =>
      let predecessor :=
        (state.designs.find? (·.id == approval.design)).bind (·.predecessor)
      let obligations := state.obligations.map fun obligation =>
        if predecessor == some obligation.design then
          { obligation with current := false }
        else obligation
      let evidence := state.evidence.map fun item =>
        if predecessor == some item.design then
          { item with current := false }
        else item
      { invalidated with
        designApprovals := state.designApprovals ++ [approval]
        obligations
        evidence }
  | .decompositionRecorded decomposition =>
      { invalidated with decompositions := state.decompositions ++ [decomposition] }
  | .authorityExceptionRecorded exception =>
      { invalidated with
        authorityExceptions := state.authorityExceptions ++ [exception] }
  | .reviewPlanRecorded plan =>
      { invalidated with reviewPlans := state.reviewPlans ++ [plan] }
  | .completionPlanned plan =>
      { invalidated with lifecycle := state.lifecycle ++ [Lifecycle.initializeState plan] }
  | .relatedWorkTerminalAcknowledged owner _ =>
      { invalidated with
        lifecycle := state.lifecycle.map fun completion =>
          if completion.plan.work == owner then Lifecycle.advance completion else completion }
  | .scopeChangeRecorded change =>
      let work := match change.kind with
        | .rescope =>
            match change.resultingScopes with
            | [scope] =>
                state.work.map fun unit =>
                  if unit.id == change.work then
                    { unit with
                      owner := scope.owner
                      outcome := scope.outcome
                      completionBoundary := scope.completionBoundary }
                  else unit
            | _ => state.work
        | .split =>
            state.work ++ change.resultingScopes.map (·.toWorkUnit)
      { invalidated with
        work
        lifecycle := state.lifecycle.map fun completion =>
          if completion.plan.work == change.work then
            Lifecycle.recordScopeChange completion change
          else completion }
  | .phaseCompleted work key =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then Lifecycle.completePhase completion key else completion }
  | .taskCompleted work key =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then Lifecycle.completeTask completion key else completion }
  | .checklistCompleted work key =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then Lifecycle.completeChecklist completion key else completion }
  | .findingResolved work key =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then Lifecycle.resolveFinding completion key else completion }
  | .validationPassed work key artifactDigest =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then
          Lifecycle.passValidation completion key artifactDigest else completion }
  | .repositoryClassified work key snapshotDigest =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then
          Lifecycle.classifyRepository completion key snapshotDigest else completion }
  | .correctionResolved work key =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then Lifecycle.resolveCorrection completion key else completion }
  | .workRecordLinked work key reference =>
      { invalidated with lifecycle := state.lifecycle.map fun completion =>
        if completion.plan.work == work then
          Lifecycle.linkWorkRecord completion key reference else completion }
  | .reviewClaimed claim => { invalidated with claims := state.claims ++ [claim] }
  | .reviewAdjudicated decision =>
      { invalidated with adjudications := state.adjudications ++ [decision] }
  | .reviewFindingRecorded finding =>
      { invalidated with reviewFindings := state.reviewFindings ++ [finding] }
  | .reviewFindingAdjudicated key _ reason accepted =>
      { invalidated with reviewFindings := state.reviewFindings.map fun finding =>
          if finding.key == key then
            { finding with accepted, adjudicated := true, decisionReason := reason }
          else finding }
  | .reviewFindingClosureAttempted key attempt =>
      { invalidated with reviewFindings := state.reviewFindings.map fun finding =>
          if finding.key == key then
            { finding with
              closureAttempts := finding.closureAttempts ++ [attempt] }
          else finding }
  | .findingVerified verification =>
      { invalidated with
        findingVerifications := state.findingVerifications ++ [verification] }
  | .findingVerificationAdjudicated finding attempt adjudicator =>
      { invalidated with
        findingVerifications := state.findingVerifications.map fun verification =>
          if verification.finding == finding &&
              verification.attempt == attempt then
            { verification with adjudicated := true, accepted := true, adjudicator := adjudicator }
          else verification }
  | .correctionRecorded correction =>
      { invalidated with
        corrections := state.corrections ++ [correction]
        evidence := state.evidence.map fun item =>
          if Design.correctionApplies correction item.work (some item.design) then
            { item with current := false }
          else item
        obligations := state.obligations.map fun obligation =>
          if Design.correctionApplies correction obligation.work
              (some obligation.design) then
            { obligation with current := false }
          else obligation
        activations := state.activations.map fun activation =>
          if Design.correctionApplies correction activation.work
              (activation.confirmedBasis.map (·.design)) then
            { activation with readyToResume := false, confirmedBasis := none }
          else activation }
  | .userCorrectionResolved key reason rejected =>
      { invalidated with corrections := state.corrections.map fun correction =>
          if correction.key == key then
            { correction with
              resolved := true
              resolutionReason := some reason
              rejected }
          else correction }
  | .authorityTransitionRecorded transition =>
      { invalidated with
        corrections := state.corrections.map fun correction =>
          if correction.key == transition.correction then
            { correction with
              resolved := true
              resolutionReason := some transition.reason
              authorityTransition := some transition.key }
          else correction
        authorityTransitions := state.authorityTransitions ++ [transition] }
  | .evidenceRecorded item =>
      { invalidated with
        evidence := state.evidence ++ [{ item with current := true }] }
  | .externalOperationRecorded attempt =>
      { invalidated with externalOperations := state.externalOperations ++ [attempt] }
  | .externalOperationAdvanced attempt =>
      { invalidated with
        externalOperations := state.externalOperations.map fun current =>
          if current.operation == attempt.operation then attempt else current }
  | .obligationRecorded obligation =>
      let obligations := invalidated.obligations.map fun existing =>
        if existing.work == obligation.work && existing.key == obligation.key then
          { existing with current := false }
        else
          existing
      let evidence := invalidated.evidence.map fun item =>
        if item.work == obligation.work && item.obligation == obligation.key then
          { item with current := false }
        else
          item
      { invalidated with
        obligations := obligations ++ [{ obligation with current := true }]
        evidence }
  | .workCompleted work activation =>
      let returnTarget := (state.activations.find? (·.id == activation)).bind (·.parent)
      { invalidated with
        work := Work.closeWork state.work work
        activations := Work.closeActivation state.activations activation
        evidence := Evidence.invalidateEvidence state.evidence
        obligations := Evidence.invalidate state.obligations
        returnTarget }

def completionRelatedWorkTerminal (work : List Work.WorkUnit)
    (requirements : List Lifecycle.RelatedWorkRequirement) : Bool :=
  requirements.all fun requirement =>
    work.any fun unit => unit.id == requirement.work &&
      (unit.status == .closed || unit.status == .abandoned)

def latestCompletionReview (plan : ReviewPlanId) (work : WorkId)
    (epoch : CompletionEpoch) (claims : List Review.Claim) : Option Review.Claim :=
  claims.foldl (init := none) fun latest claim =>
    if claim.plan == plan && claim.work == work && claim.epoch == epoch then
      some claim
    else
      latest

def completionReviewsReady (state : Lifecycle.CompletionState)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication) : Bool :=
  state.plan.reviews.all fun plan =>
    match latestCompletionReview plan state.plan.work state.epoch claims with
    | some claim =>
        claim.claim == .clean && adjudications.any (fun decision =>
          decision.review == claim.id && decision.decision == .accepted)
    | none => false

def completionObligationSatisfied (evidence : List Evidence.Evidence)
    (obligation : Evidence.Obligation) : Bool :=
  obligation.current && evidence.any (Evidence.exactFor · obligation)

def completionObligationsReady (target : WorkId)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion)
    (decompositions : List Design.Decomposition) : Bool :=
  let owned := Evidence.forWork obligations target
  match decompositions.reverse.find? (·.work == target) with
  | none => false
  | some decomposition =>
      match designs.find? fun design =>
          design.id == decomposition.design &&
          design.revision == decomposition.designRevision with
      | none => false
      | some design =>
          let active := (design.requirements.filter (·.active)).map (·.key)
          !owned.isEmpty &&
          owned.all (fun obligation =>
            obligation.design == design.id &&
            obligation.designRevision == design.revision &&
            completionObligationSatisfied evidence obligation) &&
          active.all fun requirement =>
            owned.any (fun obligation =>
              obligation.requirements.contains requirement)

def reviewScopeReady (planId : ReviewPlanId) (state : State) : Bool :=
  state.reviewPlans.any fun plan =>
    plan.id == planId && reviewPlanOwnerCurrent plan state &&
      Review.isLatestPlan plan state.reviewPlans &&
      Review.scopeReady plan state.claims state.adjudications
        state.reviewFindings state.findingVerifications

def phaseReviewsReady (completion : Lifecycle.CompletionState)
    (phase : Lifecycle.PhaseRecord) (state : State) : Bool :=
  phase.reviews.all fun review =>
    (state.reviewPlans.find? (·.id == review)).any fun assigned =>
      assigned.scope.work == completion.plan.work &&
      assigned.scope.phase == some phase.key &&
      (Review.latestPlanForContext? assigned.scope state.reviewPlans).any fun plan =>
        reviewPlanOwnerCurrent plan state &&
        reviewScopeReady plan.id state &&
        (Review.latestClaimFor plan state.claims).any fun claim =>
          claim.epoch == completion.epoch && claim.claim == .clean &&
          state.adjudications.any fun decision =>
            decision.review == claim.id && decision.decision == .accepted &&
            Review.adjudicationExact claim decision

def phaseCompletionReady (completion : Lifecycle.CompletionState)
    (key : String) (state : State) : Bool :=
  completion.phases.any fun phase =>
    phase.key == key && phase.status == .pending &&
    Lifecycle.phaseDependenciesReady completion phase &&
    Lifecycle.phaseTasksReady completion phase &&
    phaseReviewsReady completion phase state

def scopeChangeApplicable (change : Lifecycle.ScopeChange)
    (state : State) : Bool :=
  Lifecycle.scopeChangeWellFormed change &&
  state.work.any (fun work =>
    work.id == change.work && work.status == .open &&
    work.owner == change.principal &&
    match change.kind, change.cause, change.resultingScopes with
    | .rescope, .outcome, [scope] =>
        scope.work == work.id && scope.owner == work.owner &&
        scope.outcome != work.outcome
    | .rescope, .owner, [scope] =>
        scope.work == work.id && scope.owner != work.owner &&
        scope.outcome == work.outcome &&
        scope.completionBoundary == work.completionBoundary
    | .split, .independentLifecycle, scopes =>
        scopes.all (fun scope =>
          scope.work != work.id &&
          !state.work.any (·.id == scope.work))
    | _, _, _ => false) &&
  (Work.activeFor state.activations change.work).isSome &&
  !state.lifecycle.any (fun completion =>
    completion.scopeChanges.any (·.key == change.key)) &&
  (Lifecycle.forWork state.lifecycle change.work).any fun completion =>
    change.sharedRecords == Lifecycle.sharedRecords completion &&
    change.dependencies == Lifecycle.phaseDependencies completion

def decompositionRecordable (decomposition : Design.Decomposition)
    (state : State) : Bool :=
  Design.decompositionWellFormed decomposition &&
  !state.decompositions.any (·.key == decomposition.key) &&
  state.designs.any (fun version =>
    version.id == decomposition.design &&
    version.revision == decomposition.designRevision &&
    state.designApprovals.any fun approval =>
      approval.design == version.id &&
      Design.decompositionCovers version approval decomposition) &&
  state.reviewPlans.any (fun plan =>
    plan.scope.design == some decomposition.design &&
    plan.scope.work == decomposition.work &&
    plan.scope.purpose == .decomposition &&
    plan.scope.artifactDigest == decomposition.contentDigest &&
    plan.reviewer == decomposition.reviewer &&
    plan.adjudicator == decomposition.adjudicator &&
    Review.isLatestPlan plan state.reviewPlans &&
    reviewScopeReady plan.id state)

def traceReadyFor (design : DesignId) (work : WorkId)
    (key digest : String) (state : State) : Bool :=
  match state.decompositions.reverse.find? (·.work == work) with
  | none => false
  | some decomposition =>
      decomposition.design == design && decomposition.key == key &&
      decomposition.contentDigest == digest &&
      state.designs.any fun version =>
        version.id == design && state.designApprovals.any fun approval =>
          approval.design == design &&
          Design.decompositionCovers version approval decomposition

def evidenceReadyFor (basis : Work.ReadinessBasis) (work : WorkId)
    (state : State) : Bool :=
  let current := state.obligations.filter fun obligation =>
    obligation.work == work && obligation.current
  current.map (·.key) == basis.obligationKeys &&
  !current.isEmpty && current.all fun obligation =>
    obligation.revision == basis.evidenceRevision &&
    obligation.design == basis.design &&
    obligation.designRevision == basis.designRevision &&
    obligation.snapshot == basis.repositorySnapshot &&
    state.evidence.any fun item =>
      Evidence.exactFor item obligation && item.revision == basis.evidenceRevision

def implementationReviewReadyFor (basis : Work.ReadinessBasis) (work : WorkId)
    (state : State) : Bool :=
  match Review.latestPlanFor? (some basis.design) work .designConformance
      state.reviewPlans with
  | none => false
  | some plan =>
      plan.id == basis.reviewPlan &&
      plan.scope.repositorySnapshot == basis.repositorySnapshot &&
      reviewScopeReady plan.id state

def completionPurposeReviewReady (target : WorkId) (design : Option DesignId)
    (purpose : Review.Purpose) (state : State) : Bool :=
  match Review.latestPlanFor? design target purpose state.reviewPlans with
  | none => false
  | some plan =>
      Review.scopeReady plan state.claims state.adjudications
        state.reviewFindings state.findingVerifications

def completionRequiredReviewPurposes : List Review.Purpose :=
  [.designConformance, .implementationQuality]

def completionRequiredReviewsReady (target : WorkId) (state : State) : Bool :=
  let design :=
    (state.decompositions.reverse.find? (·.work == target)).map (·.design)
  match Review.latestPlanFor? design target .designConformance
      state.reviewPlans with
  | none => false
  | some conformance =>
      match Review.latestPlanFor? design target .implementationQuality
          state.reviewPlans with
      | none => false
      | some quality =>
          state.work.any (fun unit =>
            unit.id == target && unit.status == .open &&
            conformance.owner == unit.owner && quality.owner == unit.owner) &&
          Review.sameArtifactScope conformance.scope quality.scope &&
          completionRequiredReviewPurposes.all fun purpose =>
            completionPurposeReviewReady target design purpose state

def completionBinding? (target : WorkId) (state : State) :
    Option (String × String) :=
  let design :=
    (state.decompositions.reverse.find? (·.work == target)).map (·.design)
  match Review.latestPlanFor? design target .designConformance state.reviewPlans,
      Review.latestPlanFor? design target .implementationQuality
        state.reviewPlans with
  | some conformance, some quality =>
      if Review.sameArtifactScope conformance.scope quality.scope then
        some (conformance.scope.repositorySnapshot,
          conformance.scope.artifactDigest)
      else none
  | _, _ => none

def completionBindingReady (target : WorkId) (binding : String × String)
    (state : State) : Bool :=
  match Lifecycle.forWork state.lifecycle target with
  | none => false
  | some completion =>
      (completion.repositories.all fun record =>
        record.status == .classified && record.snapshotDigest == binding.1) &&
      (completion.validations.all fun record =>
        record.status == .passed && record.artifactDigest == binding.2) &&
      let current := state.obligations.filter fun obligation =>
        obligation.work == target && obligation.current
      !current.isEmpty &&
      current.all fun obligation =>
        obligation.snapshot == binding.1 &&
        obligation.artifactDigest == binding.2 &&
        state.evidence.any fun item =>
          Evidence.exactFor item obligation &&
          item.snapshot == binding.1 && item.artifactDigest == binding.2

def correctionsCurrentFor (state : State) (work : WorkId)
    (design : Option DesignId) : Bool :=
  !state.corrections.any fun correction =>
    !correction.resolved && Design.correctionApplies correction work design

def currentDesignFor (state : State) (work : WorkId) : Option DesignId :=
  (state.decompositions.reverse.find? (·.work == work)).map (·.design)

def workCorrectionsCurrent (state : State) (work : WorkId) : Bool :=
  correctionsCurrentFor state work (currentDesignFor state work)

def readinessCurrent (work : WorkId) (activation : ActivationId)
    (basis : Work.ReadinessBasis) (state : State) : Bool :=
  Work.noActive state.activations &&
  state.activations.any (fun current =>
    current.id == activation && current.work == work &&
    current.status == .suspended &&
    current.suspension.any (fun context =>
      context.readinessWellFormed && context.basis == some basis) &&
    (match current.parent with
    | none => true
    | some parent =>
        state.activations.any (fun candidate =>
          candidate.id == parent && candidate.status == .suspended &&
          Work.workIsOpen state.work candidate.work))) &&
  traceReadyFor basis.design work basis.decompositionKey
    basis.decompositionDigest state &&
  state.designs.any (fun version =>
    version.id == basis.design && version.revision == basis.designRevision) &&
  implementationReviewReadyFor basis work state &&
  evidenceReadyFor basis work state &&
  correctionsCurrentFor state work (some basis.design)

def resumeCurrent (work : WorkId) (activation : ActivationId) (state : State) : Bool :=
  match state.activations.find? (·.id == activation) with
  | none => false
  | some current =>
      (state.returnTarget.isNone || state.returnTarget == some activation) &&
      current.readyToResume && current.confirmedBasis.any fun basis =>
        readinessCurrent work activation basis state

def completionApplicable (target : WorkId) (state : State) : Bool :=
  completionObligationsReady target state.evidence state.obligations
    state.designs state.decompositions &&
  (Work.activeFor state.activations target).isSome &&
  Work.workIsOpen state.work target &&
  (match Lifecycle.forWork state.lifecycle target with
  | none => false
  | some completion =>
      completionRelatedWorkTerminal state.work completion.plan.relatedWork &&
      Lifecycle.recordsReady completion &&
      completionReviewsReady completion state.claims state.adjudications) &&
  (match state.decompositions.reverse.find? (·.work == target) with
  | none => false
  | some decomposition =>
      state.designs.any fun design =>
        design.id == decomposition.design &&
        design.revision == decomposition.designRevision &&
        state.designApprovals.any fun approval =>
          approval.design == design.id &&
          Design.decompositionCovers design approval decomposition) &&
  completionRequiredReviewsReady target state &&
  !state.corrections.any (fun correction =>
    !correction.resolved &&
    Design.correctionApplies correction target
      ((state.decompositions.reverse.find? (·.work == target)).map (·.design))) &&
  (completionBinding? target state).any fun binding =>
    completionBindingReady target binding state

def eventApplicable (event : Event) (state : State) : Bool :=
  match event with
  | .workInitialized work activation =>
      state.work.isEmpty && state.activations.isEmpty &&
      work.status == .open && work.wellFormed &&
      activation.status == .active &&
      !activation.readyToResume && activation.work == work.id
  | .workRegistered work =>
      work.status == .open && work.wellFormed &&
      !state.work.any (·.id == work.id)
  | .suspendedActivationRegistered activation =>
      activation.status == .suspended &&
      !activation.readyToResume && activation.confirmedBasis.isNone &&
      activation.suspension.any (fun context =>
        context.wellFormed &&
        context.readinessWellFormed) &&
      state.work.any (fun work => work.id == activation.work && work.status == .open) &&
      !state.activations.any (·.id == activation.id) &&
      (activation.suspension.any fun context =>
        context.basis.any fun basis =>
          traceReadyFor basis.design activation.work
            basis.decompositionKey basis.decompositionDigest state) &&
      (activation.parent.isNone || activation.parent.any fun parent =>
        state.activations.any (fun current =>
          current.id == parent && current.status == .active))
  | .workSuspended work activation context =>
      context.wellFormed && state.activations.any (fun current =>
        current.id == activation && current.work == work &&
          current.status == .active)
  | .resumeReadinessConfirmed work activation basis =>
      readinessCurrent work activation basis state
  | .suspensionRevised work activation context =>
      context.readinessWellFormed && state.activations.any (fun current =>
        current.id == activation && current.work == work &&
        current.status == .suspended)
  | .workResumed work activation =>
      Work.workIsOpen state.work work &&
      state.activations.any (fun candidate =>
        candidate.id == activation && candidate.work == work) &&
      Work.resumable state.activations activation &&
      resumeCurrent work activation state
  | .designImported version =>
      Design.versionWellFormed version &&
      (match version.predecessor with
      | none => true
      | some predecessor =>
          state.designs.any fun current =>
            current.id == predecessor &&
              Design.versionCurrent state.designs current) &&
      !state.designs.any (·.id == version.id)
  | .designApproved approval =>
      !state.designApprovals.any (·.design == approval.design) &&
      state.designs.any (fun version => version.id == approval.design &&
      designApprovalLineageReady state version &&
      state.reviewPlans.any (fun plan =>
        plan.scope.design == some approval.design &&
        plan.scope.purpose == .design &&
        plan.scope.artifactDigest == version.contentDigest &&
        plan.owner == version.owner &&
        Review.isLatestPlan plan state.reviewPlans &&
        state.claims.any (fun claim =>
          claim.id == approval.review &&
          Review.scopeExact plan claim && claim.claim == .clean &&
          state.adjudications.any (fun decision =>
            decision.review == claim.id && decision.decision == .accepted) &&
          reviewScopeReady plan.id state))) &&
      !state.corrections.any (fun correction =>
        !correction.resolved &&
        (correction.design == some approval.design ||
          (correction.design.isNone && correction.work.isNone)))
  | .decompositionRecorded decomposition =>
      decompositionRecordable decomposition state
  | .authorityExceptionRecorded exception =>
      !exception.key.isEmpty && !exception.reason.isEmpty &&
      exception.authorizedBy == "user" &&
      !state.authorityExceptions.any (·.key == exception.key)
  | .reviewPlanRecorded plan =>
      Review.planWellFormed plan && !state.reviewPlans.any (·.id == plan.id) &&
      state.work.any (·.id == plan.scope.work) &&
      plan.scope.phase.all (fun phase =>
        (Lifecycle.forWork state.lifecycle plan.scope.work).any fun completion =>
          completion.phases.any (·.key == phase)) &&
      (Review.independent plan ||
        state.authorityExceptions.any (Review.exceptionExact plan)) &&
      (match plan.scope.design with
      | some design => state.designs.any (fun version =>
          version.id == design && version.owner == plan.owner) &&
          state.work.any (fun work =>
            work.id == plan.scope.work && work.owner == plan.owner)
      | none => state.work.any (fun work =>
          work.id == plan.scope.work && work.owner == plan.owner))
  | .completionPlanned plan =>
      !state.lifecycle.any (fun completion => completion.plan.work == plan.work) &&
      decide (Lifecycle.ValidPlan (state.work.map (·.id)) plan) &&
      plan.reviews.all (fun review =>
        state.reviewPlans.any (fun existing =>
          existing.id == review && existing.scope.work == plan.work))
  | .relatedWorkTerminalAcknowledged owner related =>
      match Lifecycle.forWork state.lifecycle owner with
      | none => false
      | some completion =>
          completion.plan.relatedWork.any (·.work == related) &&
          state.work.any (fun work => work.id == related &&
            (work.status == .closed || work.status == .abandoned))
  | .scopeChangeRecorded change => scopeChangeApplicable change state
  | .phaseCompleted work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => phaseCompletionReady completion key state
      | none => false
  | .taskCompleted work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => completion.tasks.any (fun record =>
          record.key == key && record.status == .pending)
      | none => false
  | .checklistCompleted work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => completion.checklists.any (fun record =>
          record.key == key && record.status == .pending)
      | none => false
  | .findingResolved work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => completion.findings.any (fun record =>
          record.key == key && record.status == .open)
      | none => false
  | .validationPassed work key artifactDigest =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => !artifactDigest.isEmpty && completion.validations.any (·.key == key)
      | none => false
  | .repositoryClassified work key snapshotDigest =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => !snapshotDigest.isEmpty && completion.repositories.any (·.key == key)
      | none => false
  | .correctionResolved work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => completion.corrections.any (fun record =>
          record.key == key && record.status == .open)
      | none => false
  | .workRecordLinked work key reference =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => !reference.isEmpty && completion.workRecords.any (fun record =>
          record.key == key && record.status == .unlinked)
      | none => false
  | .reviewClaimed claim =>
      Review.claimWellFormed claim &&
      !state.claims.any (·.id == claim.id) &&
      state.reviewPlans.any (fun plan =>
        Review.scopeExact plan claim && reviewPlanOwnerCurrent plan state)
  | .reviewAdjudicated decision =>
      reviewAdjudicationApplicable decision state
  | .reviewFindingRecorded finding =>
      Review.findingWellFormed finding &&
      !finding.adjudicated && finding.closureAttempts.isEmpty &&
      state.claims.any (fun claim => claim.id == finding.review &&
        Review.claimAcceptsFindings claim state.reviewPlans state.claims &&
        state.reviewPlans.any (fun plan =>
          plan.id == claim.plan && reviewPlanOwnerCurrent plan state) &&
        claim.scope.any (reviewAuthorityCurrent finding.authority · state)) &&
      !state.reviewFindings.any (·.key == finding.key)
  | .reviewFindingAdjudicated key principal reason _ =>
      state.reviewFindings.any (fun finding =>
        finding.key == key && !finding.adjudicated && !reason.isEmpty &&
        finding.closureAttempts.isEmpty &&
        state.claims.any (fun claim =>
          claim.id == finding.review &&
          principal != claim.reviewer &&
          state.work.any (fun work =>
            work.id == claim.work && work.status == .open &&
            work.owner == principal)))
  | .reviewFindingClosureAttempted key attempt =>
      Review.attemptWellFormed attempt &&
      state.reviewFindings.any (fun finding =>
        finding.key == key && finding.accepted &&
        attempt.attempt == finding.closureAttempts.length + 1 &&
        Review.mayStartAttempt finding state.findingVerifications &&
        state.claims.any (fun claim =>
          claim.id == finding.review && claim.scope.any (fun scope =>
            scope.repositorySnapshot != attempt.repositorySnapshot)))
  | .findingVerified verification =>
      state.reviewFindings.any (fun finding =>
        finding.key == verification.finding &&
        finding.closureAttempts.getLast?.any (fun attempt =>
          attempt.attempt == verification.attempt &&
          attempt.evidenceDigest == verification.evidenceDigest &&
          attempt.repositorySnapshot ==
            verification.scope.repositorySnapshot) &&
        state.claims.any (fun claim =>
          claim.id == finding.review &&
          !verification.adjudicated && !verification.accepted &&
          verification.finding == finding.key &&
          claim.scope.any (Review.sameContext verification.scope) &&
          verification.verifier != claim.reviewer)) &&
      !state.findingVerifications.any (fun existing =>
        existing.finding == verification.finding &&
        existing.attempt == verification.attempt)
  | .findingVerificationAdjudicated finding attempt adjudicator =>
      state.findingVerifications.any (fun verification =>
        verification.finding == finding && verification.attempt == attempt &&
        !verification.adjudicated && !verification.accepted &&
        state.reviewFindings.any (fun record =>
          record.key == finding &&
          state.claims.any (fun claim =>
            claim.id == record.review &&
            adjudicator != verification.verifier &&
            state.work.any (fun work =>
              work.id == claim.work && work.status == .open &&
              work.owner == adjudicator))))
  | .correctionRecorded correction =>
      Design.correctionWellFormed correction && !correction.resolved &&
      !state.corrections.any (·.key == correction.key)
  | .userCorrectionResolved key reason rejected =>
      state.corrections.any (fun correction =>
        correction.key == key && !correction.resolved) &&
      !reason.isEmpty &&
      (!rejected || !state.authorityTransitions.any
        (·.correction == key))
  | .authorityTransitionRecorded transition =>
      Design.authorityTransitionWellFormed transition &&
      state.corrections.any (fun correction =>
        correction.key == transition.correction && !correction.resolved &&
        correction.scope == transition.scope &&
        correction.work == transition.work &&
        correction.design == transition.design) &&
      !state.authorityTransitions.any (·.key == transition.key) &&
      (match Design.latestAuthorityFor? transition.target transition.scope
          transition.work transition.design state.authorityTransitions with
      | none => transition.operation == .create
      | some current =>
          transition.operation != .create &&
          current.operation != .retire &&
          current.scope == transition.scope &&
          current.kind == transition.kind &&
          current.lifetime == transition.lifetime)
  | .evidenceRecorded item =>
      !item.obligation.isEmpty && !item.artifactDigest.isEmpty &&
      Evidence.traceable item &&
      !state.evidence.any (·.id == item.id) &&
      state.obligations.any (fun obligation =>
        obligation.work == item.work && obligation.key == item.obligation &&
        Evidence.exactFor item obligation)
  | .externalOperationRecorded attempt =>
      attempt.state == .prepared && attempt.wellFormed &&
      attempt.work.all (fun work =>
        state.work.any fun unit => unit.id == work && unit.status == .open) &&
      !state.externalOperations.any (·.operation == attempt.operation)
  | .externalOperationAdvanced attempt =>
      state.externalOperations.any fun current =>
        current.operation == attempt.operation &&
          ExternalOperation.transitionAllowed current attempt
  | .obligationRecorded obligation =>
      !obligation.key.isEmpty && !obligation.commandProfile.isEmpty &&
      !obligation.invocation.isEmpty && !obligation.repository.isEmpty &&
      !obligation.snapshot.isEmpty && !obligation.artifactDigest.isEmpty &&
      !obligation.requirements.isEmpty && !obligation.expectedProducer.isEmpty &&
      !obligation.expectedObservation.isEmpty &&
      Evidence.negativeBoundaryAdmissible obligation &&
      obligation.revision == state.revision &&
      state.work.any (fun work => work.id == obligation.work && work.status == .open) &&
      (state.designs.any (fun version =>
        version.id == obligation.design &&
        version.revision == obligation.designRevision &&
        approvedDesignCurrent state version &&
        Design.requirementsActive version obligation.requirements))
  | .workCompleted work activation =>
      match Work.activeFor state.activations work with
      | some current => current.id == activation &&
          completionApplicable work state
      | none => false

def verifyState (state : State) : Except DomainError VerifiedState :=
  if valid : ValidState state then
    .ok ⟨state, valid⟩
  else
    .error (.invariantViolation "state invariant violation")

def applyEvent (event : Event) (verified : VerifiedState) : Except DomainError VerifiedState :=
  if eventApplicable event verified.state then
    verifyState (applyUnchecked event verified.state)
  else
    .error (.invalidTransition "event is not applicable to authoritative state")

def replayFrom : List Event → VerifiedState → Except DomainError VerifiedState
  | [], state => .ok state
  | event :: rest, state => do
      replayFrom rest (← applyEvent event state)

def replay (events : List Event) (initial : State) : Except DomainError VerifiedState := do
  replayFrom events (← verifyState initial)

def eventDigest (events : List Event) : Digest :=
  ⟨s!"{repr events}"⟩

def stateDigest (state : State) : Digest :=
  ⟨s!"{repr state}"⟩

def emptyState : State :=
  { revision := ⟨0⟩
    work := []
    activations := []
    designs := []
    designApprovals := []
    decompositions := []
    reviewPlans := []
    authorityExceptions := []
    claims := []
    adjudications := []
    reviewFindings := []
    findingVerifications := []
    corrections := []
    authorityTransitions := []
    evidence := []
    externalOperations := []
    obligations := []
    lifecycle := []
    returnTarget := none }

structure LedgerImage where
  id : LedgerId
  events : List Event
  storedHead : Revision
  storedHistoryDigest : Digest
deriving DecidableEq, Repr

structure VerifiedLedger where
  image : LedgerImage
  head : VerifiedState
  replayed : replay image.events emptyState = .ok head
  revisionExact : head.state.revision = image.storedHead
  digestExact : eventDigest image.events = image.storedHistoryDigest

def verifyLedger (image : LedgerImage) : Except Projection.LedgerFault VerifiedLedger :=
  match replayed : replay image.events emptyState with
  | .error error => .error (.replayRejected error)
  | .ok head =>
      if revisionExact : head.state.revision = image.storedHead then
        if digestExact : eventDigest image.events = image.storedHistoryDigest then
          .ok ⟨image, head, replayed, revisionExact, digestExact⟩
        else
          .error (.historyDigestMismatch (eventDigest image.events) image.storedHistoryDigest)
      else
        .error (.headRevisionMismatch head.state.revision image.storedHead)

def VerifiedLedger.point (ledger : VerifiedLedger) : Projection.LedgerPoint :=
  { ledger := ledger.image.id
    revision := ledger.head.state.revision
    historyDigest := eventDigest ledger.image.events }

def replayAt (ledger : VerifiedLedger) (revision : Revision) :
    Except Projection.LedgerFault VerifiedState :=
  match replay (ledger.image.events.take revision.value) emptyState with
  | .error error => .error (.replayRejected error)
  | .ok state =>
      if state.state.revision = revision then .ok state
      else .error (.headRevisionMismatch state.state.revision revision)

theorem verified_ledger_head_is_replay (ledger : VerifiedLedger) :
    replay ledger.image.events emptyState = .ok ledger.head :=
  ledger.replayed

theorem replay_deterministic (events : List Event) (initial : State)
    {left right : VerifiedState}
    (leftResult : replay events initial = .ok left)
    (rightResult : replay events initial = .ok right) :
    left.state = right.state := by
  rw [leftResult] at rightResult
  simp only [Except.ok.injEq] at rightResult
  exact congrArg VerifiedState.state rightResult

theorem replay_preserves_valid (events : List Event) (initial : State)
    {result : VerifiedState} (_accepted : replay events initial = .ok result) :
    ValidState result.state :=
  result.valid

theorem work_completed_event_exact (verified : VerifiedState)
    (work : WorkId) (activation : ActivationId) {completed : VerifiedState}
    (accepted : applyEvent (.workCompleted work activation) verified = .ok completed) :
    completed.state.work = Work.closeWork verified.state.work work ∧
    completed.state.activations = Work.closeActivation verified.state.activations activation ∧
    completed.state.revision = verified.state.revision.next := by
  unfold applyEvent at accepted
  split at accepted
  · unfold verifyState at accepted
    split at accepted
    · cases accepted
      simp [applyUnchecked]
    · contradiction
  · contradiction

theorem emptyState_valid : ValidState emptyState := by
  simp [ValidState, Work.ValidWorkState, Work.UniqueWorkIds,
    Work.UniqueActivationIds, Work.OwnersPresent, Work.AtMostOneActive,
    Work.ActiveReferencesOpenWork, Work.ActivationsReferenceWork,
    Work.NonterminalActivationsReferenceOpenWork,
    Review.ValidReviewState,
    Review.UniqueClaimIds, Review.UniqueAdjudications,
    Review.AdjudicationsReferenceClaims, Evidence.UniqueEvidenceIds,
    Evidence.EvidenceWellFormed,
    Evidence.EvidenceReferencesObligations,
    ExternalOperation.UniqueOperations,
    ExternalOperation.AttemptsWellFormed,
    ReviewClaimsReferencePlans,
    Lifecycle.ValidLifecycleState, Lifecycle.ValidPlan, Lifecycle.MatchesPlan,
    Lifecycle.RecordsWellFormed,
    Lifecycle.nonemptyKeys,
    Evidence.UniqueObligations, Evidence.ObligationsWellFormed,
    Evidence.ObligationsReferenceWork, ObligationsReferenceDesigns,
    Evidence.CurrentObligationsReferenceOpenWork,
    returnTargetValid, Work.activeActivations, emptyState]

end AgentWorkbench.Kernel.Replay

namespace AgentWorkbench.Kernel.Projection

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive ProjectionPayload
  | decoded (state : State)
  | decodeFailed (fault : Domain.Projection.DecodeFault)
deriving DecidableEq, Repr, BEq

structure ProjectionObservation where
  fingerprint : Domain.Projection.ProjectionFingerprint
  reference : Domain.Projection.ProjectionRef
  payload : ProjectionPayload
deriving DecidableEq, Repr

structure StagedProjection where
  id : StageId
  binding : Domain.Projection.RepairBinding
  candidate : ProjectionObservation
deriving DecidableEq, Repr

structure RepairReceipt where
  stage : StageId
  before : Option Domain.Projection.ProjectionFingerprint
  adopted : Domain.Projection.ProjectionFingerprint
  head : Domain.Projection.LedgerPoint
deriving DecidableEq, Repr

structure Store where
  ledger : LedgerImage
  active : Option ProjectionObservation
  staged : List StagedProjection
  receipts : List RepairReceipt
  nextStage : StageId
deriving DecidableEq, Repr

inductive Inspection
  | ledgerCorrupt (fault : Domain.Projection.LedgerFault)
  | fresh (ledger : VerifiedLedger) (projection : ProjectionObservation)
  | missing (ledger : VerifiedLedger) (repair : Domain.Projection.RepairCommand)
  | stale (ledger : VerifiedLedger) (projection : ProjectionObservation)
      (repair : Domain.Projection.RepairCommand)
  | corrupt (ledger : VerifiedLedger) (projection : Option ProjectionObservation)
      (fault : Domain.Projection.ProjectionFault)
      (repair : Domain.Projection.RepairCommand)

def observedFingerprint (store : Store) :
    Option Domain.Projection.ProjectionFingerprint :=
  store.active.map (·.fingerprint)

def repairCommand (ledger : VerifiedLedger) (store : Store) :
    Domain.Projection.RepairCommand :=
  { binding := { head := ledger.point, observed := observedFingerprint store } }

def projectionMatchesHead (ledger : VerifiedLedger)
    (projection : ProjectionObservation) : Bool :=
  projection.reference.fingerprint == projection.fingerprint &&
  projection.reference.ledger == ledger.image.id &&
  projection.reference.revision == ledger.head.state.revision &&
  projection.reference.historyDigest == eventDigest ledger.image.events &&
  projection.reference.stateDigest == stateDigest ledger.head.state &&
  projection.fingerprint.rawDigest == stateDigest ledger.head.state &&
  projection.payload == .decoded ledger.head.state

def classifyProjection (ledger : VerifiedLedger) (store : Store) : Inspection :=
  let repair := repairCommand ledger store
  match store.active with
  | none => .missing ledger repair
  | some projection =>
      match projection.payload with
      | .decodeFailed fault => .corrupt ledger (some projection) (.undecodable fault) repair
      | .decoded state =>
          if projection.reference.ledger != ledger.image.id then
            .corrupt ledger (some projection)
              (.wrongLedger projection.reference.ledger ledger.image.id) repair
          else if ledger.head.state.revision.value < projection.reference.revision.value then
            .corrupt ledger (some projection)
              (.aheadOfLedger projection.reference.revision ledger.head.state.revision) repair
          else if projection.reference.fingerprint != projection.fingerprint then
            .corrupt ledger (some projection) .stateDigestMismatch repair
          else if projection.reference.revision = ledger.head.state.revision then
            if projectionMatchesHead ledger projection then
              .fresh ledger projection
            else
              .corrupt ledger (some projection) .replayMismatch repair
          else
            match replayAt ledger projection.reference.revision with
            | .error _ => .corrupt ledger (some projection) .replayMismatch repair
            | .ok prefixState =>
                if projection.reference.historyDigest ==
                    eventDigest (ledger.image.events.take projection.reference.revision.value) &&
                    projection.reference.stateDigest == stateDigest prefixState.state &&
                    projection.fingerprint.rawDigest == stateDigest prefixState.state &&
                    state == prefixState.state then
                  .stale ledger projection repair
                else
                  .corrupt ledger (some projection) .replayMismatch repair

def inspect (store : Store) : Inspection :=
  match verifyLedger store.ledger with
  | .error fault => .ledgerCorrupt fault
  | .ok ledger => classifyProjection ledger store

def Inspection.repairCommand? : Inspection → Option Domain.Projection.RepairCommand
  | .missing _ repair | .stale _ _ repair | .corrupt _ _ _ repair => some repair
  | .fresh _ _ | .ledgerCorrupt _ => none

def Inspection.currentState? : Inspection → Option State
  | .fresh _ projection =>
      match projection.payload with
      | .decoded state => some state
      | .decodeFailed _ => none
  | _ => none

def Inspection.ledgerPoint? : Inspection → Option Domain.Projection.LedgerPoint
  | .fresh ledger _ | .missing ledger _ | .stale ledger _ _ | .corrupt ledger _ _ _ =>
      some ledger.point
  | .ledgerCorrupt _ => none

def Inspection.describe : Inspection → String
  | .ledgerCorrupt fault => s!"ledger-corrupt {repr fault}"
  | .fresh ledger _ => s!"fresh {repr ledger.point}"
  | .missing ledger repair => s!"missing {repr ledger.point} repair={repr repair}"
  | .stale ledger projection repair =>
      s!"stale projected={repr projection.reference.revision} head={repr ledger.point} repair={repr repair}"
  | .corrupt ledger _ fault repair =>
      s!"projection-corrupt head={repr ledger.point} fault={repr fault} repair={repr repair}"

inductive RepairError
  | ledgerCorrupt (fault : Domain.Projection.LedgerFault)
  | commandMismatch
  | stageMissing (stage : StageId)
  | candidateMismatch
  | candidateNotVerified
deriving DecidableEq, Repr

def candidateObservation (ledger : VerifiedLedger) (stage : StageId) :
    ProjectionObservation :=
  let fingerprint : Domain.Projection.ProjectionFingerprint :=
    { id := ⟨s!"repair-{stage.value}"⟩, rawDigest := stateDigest ledger.head.state }
  { fingerprint
    reference := {
      fingerprint
      ledger := ledger.image.id
      revision := ledger.head.state.revision
      historyDigest := eventDigest ledger.image.events
      stateDigest := stateDigest ledger.head.state }
    payload := .decoded ledger.head.state }

structure StageTransaction where
  stage : StagedProjection
  result : Store

def stageRepair (command : Domain.Projection.RepairCommand) (store : Store) :
    Except RepairError StageTransaction :=
  match inspect store with
  | .ledgerCorrupt fault => .error (.ledgerCorrupt fault)
  | .fresh _ _ => .error .commandMismatch
  | .missing ledger expected | .stale ledger _ expected | .corrupt ledger _ _ expected =>
      if command = expected then
        let staged : StagedProjection := {
          id := store.nextStage
          binding := command.binding
          candidate := candidateObservation ledger store.nextStage }
        .ok {
          stage := staged
          result := { store with
            staged := store.staged ++ [staged]
            nextStage := ⟨store.nextStage.value + 1⟩ } }
      else
        .error .commandMismatch

structure VerifiedStage where
  stage : StagedProjection
  ledger : VerifiedLedger
  candidateState : State
  candidateExact : stage.candidate.payload = .decoded candidateState
  replayExact : candidateState = ledger.head.state
  candidateMatches : projectionMatchesHead ledger stage.candidate = true

def verifyStage (stageId : StageId) (store : Store) : Except RepairError VerifiedStage := do
  let stage ← match store.staged.find? (·.id == stageId) with
    | some stage => .ok stage
    | none => .error (.stageMissing stageId)
  let ledger ← match verifyLedger store.ledger with
    | .ok ledger => .ok ledger
    | .error fault => .error (.ledgerCorrupt fault)
  unless stage.binding.head = ledger.point &&
      stage.binding.observed = observedFingerprint store do
    throw .commandMismatch
  match candidateState : stage.candidate.payload with
  | .decodeFailed _ => .error .candidateMismatch
  | .decoded state =>
      if replayExact : state = ledger.head.state then
        if candidateMatches : projectionMatchesHead ledger stage.candidate then
          .ok ⟨stage, ledger, state, candidateState, replayExact, candidateMatches⟩
        else
          .error .candidateMismatch
      else
        .error .candidateMismatch

structure AdoptionTransaction where
  receipt : RepairReceipt
  candidate : ProjectionObservation
  sourceLedger : LedgerImage
  result : Store
  ledgerUnchanged : result.ledger = sourceLedger
  activeAdopted : result.active = some candidate

def adoptVerified (verified : VerifiedStage) (store : Store) :
    Except RepairError AdoptionTransaction := do
  let current ← match store.staged.find? (·.id == verified.stage.id) with
    | some stage => .ok stage
    | none => .error (.stageMissing verified.stage.id)
  unless current = verified.stage do throw .candidateMismatch
  let ledger ← match verifyLedger store.ledger with
    | .ok ledger => .ok ledger
    | .error fault => .error (.ledgerCorrupt fault)
  unless verified.stage.binding.head = ledger.point &&
      verified.stage.binding.observed = observedFingerprint store &&
      projectionMatchesHead ledger verified.stage.candidate do
    throw .commandMismatch
  let receipt : RepairReceipt := {
    stage := verified.stage.id
    before := observedFingerprint store
    adopted := verified.stage.candidate.fingerprint
    head := ledger.point }
  return {
    receipt
    candidate := verified.stage.candidate
    sourceLedger := store.ledger
    result := { store with
      active := some verified.stage.candidate
      staged := store.staged.filter (·.id != verified.stage.id)
      receipts := store.receipts ++ [receipt] }
    ledgerUnchanged := rfl
    activeAdopted := rfl }

structure RepairTransaction where
  staged : StageTransaction
  verified : VerifiedStage
  adopted : AdoptionTransaction

def repair (command : Domain.Projection.RepairCommand) (store : Store) :
    Except RepairError RepairTransaction := do
  let staged ← stageRepair command store
  let verified ← verifyStage staged.stage.id staged.result
  let adopted ← adoptVerified verified staged.result
  return { staged, verified, adopted }

def status (store : Store) : Store × Inspection :=
  (store, inspect store)

theorem status_is_read_only (store : Store) :
    (status store).1 = store :=
  rfl

theorem stage_preserves_ledger_and_active (command : Domain.Projection.RepairCommand)
    (store : Store) {transaction : StageTransaction}
    (accepted : stageRepair command store = .ok transaction) :
    transaction.result.ledger = store.ledger ∧
    transaction.result.active = store.active := by
  unfold stageRepair at accepted
  split at accepted <;> try contradiction
  all_goals
    split at accepted
    · cases accepted
      exact ⟨rfl, rfl⟩
    · contradiction

theorem verified_stage_matches_replay (verified : VerifiedStage) :
    verified.candidateState = verified.ledger.head.state ∧
    projectionMatchesHead verified.ledger verified.stage.candidate = true :=
  ⟨verified.replayExact, verified.candidateMatches⟩

theorem adoption_is_atomic (transaction : AdoptionTransaction) :
    transaction.result.ledger = transaction.sourceLedger ∧
    transaction.result.active = some transaction.candidate :=
  ⟨transaction.ledgerUnchanged, transaction.activeAdopted⟩

def initialLedger : LedgerImage :=
  { id := ⟨"agent-workbench"⟩
    events := []
    storedHead := emptyState.revision
    storedHistoryDigest := eventDigest [] }

def initialProjection : ProjectionObservation :=
  let fingerprint : Domain.Projection.ProjectionFingerprint :=
    { id := ⟨"projection-0"⟩, rawDigest := stateDigest emptyState }
  { fingerprint
    reference := {
      fingerprint
      ledger := initialLedger.id
      revision := emptyState.revision
      historyDigest := eventDigest []
      stateDigest := stateDigest emptyState }
    payload := .decoded emptyState }

def initialStore : Store :=
  { ledger := initialLedger
    active := some initialProjection
    staged := []
    receipts := []
    nextStage := ⟨1⟩ }

end AgentWorkbench.Kernel.Projection
