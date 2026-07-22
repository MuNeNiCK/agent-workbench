import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.ExternalOperation
import AgentWorkbench.Domain.Projection
import AgentWorkbench.Domain.Lifecycle
import AgentWorkbench.Policy.Completion

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

def ValidState (state : State) : Prop :=
  Work.ValidWorkState state.work state.activations ∧
  Review.ValidReviewState state.claims state.adjudications ∧
  Lifecycle.ReviewClaimsReferencePlans state.lifecycle state.claims ∧
  Evidence.UniqueEvidenceIds state.evidence ∧
  Evidence.EvidenceWellFormed state.evidence ∧
  Evidence.EvidenceCurrentAt state.revision state.evidence ∧
  Evidence.EvidenceReferencesObligations state.evidence state.obligations ∧
  ExternalOperation.UniqueOperations state.externalOperations ∧
  ExternalOperation.AttemptsWellFormed state.externalOperations ∧
  Lifecycle.ValidLifecycleState state.work state.lifecycle ∧
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
    Lifecycle.ReviewClaimsReferencePlans
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

def applyUnchecked (event : Event) (state : State) : State :=
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
  | .completionPlanned plan =>
      !state.lifecycle.any (fun completion => completion.plan.work == plan.work) &&
      decide (Lifecycle.ValidPlan state.work plan)
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
          Policy.Completion.closeable work state.work state.activations
            state.claims state.adjudications state.lifecycle state.evidence state.obligations
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

theorem work_completed_event_exact (state : State) (work : WorkId) (activation : ActivationId) :
    let completed := applyUnchecked (.workCompleted work activation) state
    completed.work = Work.closeWork state.work work ∧
    completed.activations = Work.closeActivation state.activations activation ∧
    completed.revision = state.revision.next := by
  simp [applyUnchecked]

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
    Lifecycle.ReviewClaimsReferencePlans,
    Lifecycle.ValidLifecycleState, Lifecycle.ValidPlan, Lifecycle.MatchesPlan,
    Lifecycle.RecordsWellFormed,
    Lifecycle.nonemptyKeys,
    Evidence.UniqueObligations, Evidence.ObligationsWellFormed,
    Evidence.ObligationsReferenceWork, Evidence.CurrentObligationsReferenceOpenWork,
    Evidence.ObligationsCurrentAt,
    Work.activeActivations, emptyState]

end AgentWorkbench.Kernel.Replay
