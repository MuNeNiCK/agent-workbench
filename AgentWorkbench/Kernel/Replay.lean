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
  claims : List Review.Claim
  adjudications : List Review.Adjudication
  evidence : List Evidence.Evidence
  externalOperations : List ExternalOperation.Attempt
  obligations : List Evidence.Obligation
  lifecycle : List Lifecycle.CompletionState
deriving DecidableEq, Repr

def ReviewClaimsReferencePlans (states : List Lifecycle.CompletionState)
    (claims : List Review.Claim) : Prop :=
  (claims.all fun claim => states.any fun state =>
    state.plan.work == claim.work && state.plan.reviews.contains claim.plan &&
      decide (claim.epoch.value ≤ state.epoch.value)) = true

def ValidState (state : State) : Prop :=
  Work.ValidWorkState state.work state.activations ∧
  Review.ValidReviewState state.claims state.adjudications ∧
  ReviewClaimsReferencePlans state.lifecycle state.claims ∧
  Evidence.UniqueEvidenceIds state.evidence ∧
  Evidence.EvidenceWellFormed state.evidence ∧
  Evidence.EvidenceCurrentAt state.revision state.evidence ∧
  Evidence.EvidenceReferencesObligations state.evidence state.obligations ∧
  ExternalOperation.UniqueOperations state.externalOperations ∧
  ExternalOperation.AttemptsWellFormed state.externalOperations ∧
  Lifecycle.ValidLifecycleState (state.work.map (·.id)) state.lifecycle ∧
  Evidence.UniqueObligations state.obligations ∧
  Evidence.ObligationsWellFormed state.obligations ∧
  Evidence.ObligationsReferenceWork (state.work.map (·.id)) state.obligations ∧
  Evidence.CurrentObligationsReferenceOpenWork
    ((state.work.filter (·.status == .open)).map (·.id)) state.obligations ∧
  Evidence.ObligationsCurrentAt state.revision state.obligations

instance (state : State) : Decidable (ValidState state) := by
  unfold ValidState Work.ValidWorkState Work.UniqueWorkIds
    Work.UniqueActivationIds Work.AtMostOneActive Work.ActiveReferencesOpenWork
    Work.ActivationsReferenceWork Work.NonterminalActivationsReferenceOpenWork
    Review.ValidReviewState Review.UniqueClaimIds Review.UniqueAdjudications
    Review.AdjudicationsReferenceClaims
    Evidence.UniqueEvidenceIds Evidence.EvidenceWellFormed
    Evidence.EvidenceCurrentAt
    Evidence.EvidenceReferencesObligations
    ExternalOperation.UniqueOperations ExternalOperation.AttemptsWellFormed
    ReviewClaimsReferencePlans
    Lifecycle.ValidLifecycleState Lifecycle.ValidPlan Lifecycle.MatchesPlan
    Lifecycle.RecordsWellFormed
    Lifecycle.nonemptyKeys
    Evidence.UniqueObligations Evidence.ObligationsWellFormed
    Evidence.ObligationsReferenceWork Evidence.CurrentObligationsReferenceOpenWork
    Evidence.ObligationsCurrentAt
  infer_instance

structure VerifiedState where
  state : State
  valid : ValidState state

inductive Event
  | workInitialized (work : Work.WorkUnit) (activation : Work.Activation)
  | workRegistered (work : Work.WorkUnit)
  | suspendedActivationRegistered (activation : Work.Activation)
  | workResumed (work : WorkId) (activation : ActivationId)
  | completionPlanned (plan : Lifecycle.CompletionPlan)
  | relatedWorkTerminated (owner related : WorkId)
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
  | evidenceRecorded (item : Evidence.Evidence)
  | externalOperationRecorded (attempt : ExternalOperation.Attempt)
  | obligationRecorded (obligation : Evidence.Obligation)
  | workCompleted (work : WorkId) (activation : ActivationId)
deriving DecidableEq, Repr

private def applyUnchecked (event : Event) (state : State) : State :=
  let revised := state.revision.next
  let invalidated : State := {
    state with
    revision := revised
    evidence := Evidence.invalidateEvidence state.evidence
    obligations := state.obligations.map fun obligation =>
      if obligation.current then { obligation with revision := revised } else obligation }
  match event with
  | .workInitialized work activation =>
      { invalidated with work := [work], activations := [activation] }
  | .workRegistered work =>
      { invalidated with work := state.work ++ [work] }
  | .suspendedActivationRegistered activation =>
      { invalidated with activations := state.activations ++ [activation] }
  | .workResumed _ activation =>
      match Work.resume state.activations activation with
      | some activations => { invalidated with activations }
      | none => invalidated
  | .completionPlanned plan =>
      { invalidated with lifecycle := state.lifecycle ++ [Lifecycle.initializeState plan] }
  | .relatedWorkTerminated owner related =>
      { invalidated with
        work := Work.closeWork state.work related
        lifecycle := state.lifecycle.map fun completion =>
          if completion.plan.work == owner then Lifecycle.advance completion else completion }
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
  | .evidenceRecorded item =>
      let obligations := state.obligations.map fun obligation =>
        if obligation.current then { obligation with revision := revised } else obligation
      let evidence := state.evidence.map fun existing =>
        if existing.current then { existing with revision := revised } else existing
      { invalidated with
        evidence := evidence ++
          [{ item with revision := revised, current := true }]
        obligations }
  | .externalOperationRecorded attempt =>
      { invalidated with externalOperations := state.externalOperations ++ [attempt] }
  | .obligationRecorded obligation =>
      let retained := invalidated.obligations.filter fun existing =>
        existing.work != obligation.work || existing.key != obligation.key
      { invalidated with obligations := retained ++ [{ obligation with revision := revised, current := true }] }
  | .workCompleted work activation =>
      { invalidated with
        work := Work.closeWork state.work work
        activations := Work.closeActivation state.activations activation
        evidence := Evidence.invalidateEvidence state.evidence
        obligations := Evidence.invalidate state.obligations }

def completionRelatedWorkTerminal (work : List Work.WorkUnit)
    (requirements : List Lifecycle.RelatedWorkRequirement) : Bool :=
  requirements.all fun requirement =>
    work.any fun unit => unit.id == requirement.work &&
      (unit.status == .closed || unit.status == .abandoned)

def latestAcceptedCompletionReview (plan : ReviewPlanId) (work : WorkId)
    (epoch : CompletionEpoch) (claims : List Review.Claim)
    (adjudications : List Review.Adjudication) : Option Review.Claim :=
  claims.foldl (init := none) fun latest claim =>
    if claim.plan == plan && claim.work == work && claim.epoch == epoch &&
        adjudications.any (fun decision =>
          decision.review == claim.id && decision.decision == .accepted) then
      some claim
    else
      latest

def completionReviewsReady (state : Lifecycle.CompletionState)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication) : Bool :=
  state.plan.reviews.all fun plan =>
    match latestAcceptedCompletionReview plan state.plan.work state.epoch
        claims adjudications with
    | some claim => claim.claim == .clean
    | none => false

def completionObligationSatisfied (evidence : List Evidence.Evidence)
    (obligation : Evidence.Obligation) : Bool :=
  obligation.current && evidence.any fun item =>
    item.work == obligation.work && item.obligation == obligation.key &&
      item.current && item.revision == obligation.revision

def completionObligationsReady (target : WorkId)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation) : Bool :=
  let owned := Evidence.forWork obligations target
  !owned.isEmpty && owned.all (completionObligationSatisfied evidence)

def completionApplicable (target : WorkId) (state : State) : Bool :=
  completionObligationsReady target state.evidence state.obligations &&
  (Work.activeFor state.activations target).isSome &&
  Work.workIsOpen state.work target &&
  match Lifecycle.forWork state.lifecycle target with
  | none => false
  | some completion =>
      completionRelatedWorkTerminal state.work completion.plan.relatedWork &&
      Lifecycle.recordsReady completion &&
      completionReviewsReady completion state.claims state.adjudications

def eventApplicable (event : Event) (state : State) : Bool :=
  match event with
  | .workInitialized work activation =>
      state.work.isEmpty && state.activations.isEmpty &&
      work.status == .open && activation.status == .active &&
      !activation.readyToResume && activation.work == work.id
  | .workRegistered work =>
      work.status == .open && !state.work.any (·.id == work.id)
  | .suspendedActivationRegistered activation =>
      activation.status == .suspended && activation.readyToResume &&
      state.work.any (fun work => work.id == activation.work && work.status == .open) &&
      !state.activations.any (·.id == activation.id)
  | .workResumed work activation =>
      Work.workIsOpen state.work work &&
      state.activations.any (fun candidate =>
        candidate.id == activation && candidate.work == work) &&
      Work.resumable state.activations activation
  | .completionPlanned plan =>
      !state.lifecycle.any (fun completion => completion.plan.work == plan.work) &&
      decide (Lifecycle.ValidPlan (state.work.map (·.id)) plan)
  | .relatedWorkTerminated owner related =>
      match Lifecycle.forWork state.lifecycle owner with
      | none => false
      | some completion =>
          completion.plan.relatedWork.any (·.work == related) &&
          state.work.any (fun work => work.id == related && work.status == .open) &&
          (Work.activeFor state.activations related).isNone
  | .phaseCompleted work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion => completion.phases.any (fun record =>
          record.key == key && record.status == .pending)
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
      match Lifecycle.forWork state.lifecycle claim.work with
      | some completion => completion.plan.reviews.contains claim.plan &&
          claim.epoch == completion.epoch && !state.claims.any (·.id == claim.id)
      | none => false
  | .reviewAdjudicated decision =>
      state.claims.any (·.id == decision.review) &&
      !state.adjudications.any (·.review == decision.review)
  | .evidenceRecorded item =>
      !item.obligation.isEmpty && !item.artifactDigest.isEmpty &&
      !state.evidence.any (·.id == item.id) &&
      state.obligations.any fun obligation =>
        obligation.work == item.work && obligation.key == item.obligation
  | .externalOperationRecorded attempt =>
      attempt.state == .prepared && !attempt.operation.value.isEmpty &&
      !attempt.artifactDigest.isEmpty &&
      !state.externalOperations.any (·.operation == attempt.operation)
  | .obligationRecorded obligation =>
      !obligation.key.isEmpty &&
      state.work.any (fun work => work.id == obligation.work && work.status == .open)
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
    claims := []
    adjudications := []
    evidence := []
    externalOperations := []
    obligations := []
    lifecycle := [] }

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
    Work.UniqueActivationIds, Work.AtMostOneActive,
    Work.ActiveReferencesOpenWork, Work.ActivationsReferenceWork,
    Work.NonterminalActivationsReferenceOpenWork,
    Review.ValidReviewState,
    Review.UniqueClaimIds, Review.UniqueAdjudications,
    Review.AdjudicationsReferenceClaims, Evidence.UniqueEvidenceIds,
    Evidence.EvidenceWellFormed, Evidence.EvidenceCurrentAt,
    Evidence.EvidenceReferencesObligations,
    ExternalOperation.UniqueOperations,
    ExternalOperation.AttemptsWellFormed,
    ReviewClaimsReferencePlans,
    Lifecycle.ValidLifecycleState, Lifecycle.ValidPlan, Lifecycle.MatchesPlan,
    Lifecycle.RecordsWellFormed,
    Lifecycle.nonemptyKeys,
    Evidence.UniqueObligations, Evidence.ObligationsWellFormed,
    Evidence.ObligationsReferenceWork, Evidence.CurrentObligationsReferenceOpenWork,
    Evidence.ObligationsCurrentAt,
    Work.activeActivations, emptyState]

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
