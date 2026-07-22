import AgentWorkbench.Kernel.Replay
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority
import AgentWorkbench.Policy.Completion
import AgentWorkbench.Policy.Update

namespace AgentWorkbench.Kernel.Decide

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Command
  | initializeWork (expectedRevision : Revision)
      (work : Work.WorkUnit) (activation : Work.Activation)
  | registerWork (expectedRevision : Revision) (work : Work.WorkUnit)
  | registerSuspendedActivation (expectedRevision : Revision)
      (activation : Work.Activation)
  | resumeWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId)
  | planCompletion (expectedRevision : Revision) (plan : Lifecycle.CompletionPlan)
  | terminateRelatedWork (expectedRevision : Revision) (owner related : WorkId)
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
  | recordReviewClaim (expectedRevision : Revision) (claim : Review.Claim)
  | recordReviewAdjudication (expectedRevision : Revision)
      (adjudication : Review.Adjudication)
  | recordEvidence (expectedRevision : Revision) (evidence : Evidence.Evidence)
  | recordExternalOperation (expectedRevision : Revision)
      (attempt : ExternalOperation.Attempt)
  | recordObligation (expectedRevision : Revision) (obligation : Evidence.Obligation)
  | completeWork (expectedRevision : Revision) (target : WorkId)
deriving DecidableEq, Repr

def Command.expectedRevision : Command → Revision
  | .initializeWork revision _ _
  | .registerWork revision _
  | .registerSuspendedActivation revision _
  | .resumeWork revision _ _
  | .planCompletion revision _
  | .terminateRelatedWork revision _ _
  | .completePhase revision _ _
  | .completeTask revision _ _
  | .completeChecklist revision _ _
  | .resolveFinding revision _ _
  | .passValidation revision _ _ _
  | .classifyRepository revision _ _ _
  | .resolveCorrection revision _ _
  | .linkWorkRecord revision _ _ _
  | .recordReviewClaim revision _
  | .recordReviewAdjudication revision _
  | .recordEvidence revision _
  | .recordExternalOperation revision _
  | .recordObligation revision _
  | .completeWork revision _ => revision

structure DerivedEvents where
  events : List Event
  eventsNonempty : events ≠ []

def deriveEvents (command : Command) (state : State) : Except DomainError DerivedEvents :=
  match command with
  | .initializeWork _ work activation =>
      if state.work.isEmpty && state.activations.isEmpty &&
          work.status == .open && activation.status == .active &&
          !activation.readyToResume && activation.work == work.id then
        .ok ⟨[.workInitialized work activation], by simp⟩
      else
        .error (.invalidTransition "work initialization requires an empty state and one matching open active frame")
  | .registerWork _ work =>
      if work.status == .open && !state.work.any (·.id == work.id) then
        .ok ⟨[.workRegistered work], by simp⟩
      else
        .error (.invalidTransition "registered work must be a new open work unit")
  | .registerSuspendedActivation _ activation =>
      if activation.status == .suspended && activation.readyToResume &&
          state.work.any (fun work => work.id == activation.work && work.status == .open) &&
          !state.activations.any (·.id == activation.id) then
        .ok ⟨[.suspendedActivationRegistered activation], by simp⟩
      else
        .error (.invalidTransition "registered parent activation must be new, suspended, ready, and reference open work")
  | .resumeWork _ work activation =>
      if Work.workIsOpen state.work work &&
          state.activations.any (fun candidate =>
            candidate.id == activation && candidate.work == work) &&
          Work.resumable state.activations activation then
        .ok ⟨[.workResumed work activation], by simp⟩
      else
        .error (.invalidTransition "resumed activation must be ready, suspended, inactive, and reference the exact open work")
  | .planCompletion _ plan =>
      if state.lifecycle.any (fun completion => completion.plan.work == plan.work) then
        .error (.invalidTransition "completion plan already exists")
      else if Lifecycle.ValidPlan (state.work.map (·.id)) plan then
        .ok ⟨[.completionPlanned plan], by simp⟩
      else
        .error (.invalidTransition "completion plan is not authoritative or well formed")
  | .terminateRelatedWork _ owner related =>
      match Lifecycle.forWork state.lifecycle owner with
      | none => .error (.invalidTransition "completion plan is missing")
      | some completion =>
          if completion.plan.relatedWork.any (·.work == related) &&
              state.work.any (fun work => work.id == related && work.status == .open) &&
              (Work.activeFor state.activations related).isNone then
            .ok ⟨[.relatedWorkTerminated owner related], by simp⟩
          else
            .error (.invalidTransition "related work is not an open inactive requirement")
  | .completePhase _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.phases.any (fun record =>
              record.key == key && record.status == .pending) then
            .ok ⟨[.phaseCompleted work key], by simp⟩
          else .error (.invalidTransition "phase is not pending")
      | none => .error (.invalidTransition "completion plan is missing")
  | .completeTask _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.tasks.any (fun record =>
              record.key == key && record.status == .pending) then
            .ok ⟨[.taskCompleted work key], by simp⟩
          else .error (.invalidTransition "task is not pending")
      | none => .error (.invalidTransition "completion plan is missing")
  | .completeChecklist _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.checklists.any (fun record =>
              record.key == key && record.status == .pending) then
            .ok ⟨[.checklistCompleted work key], by simp⟩
          else .error (.invalidTransition "checklist is not pending")
      | none => .error (.invalidTransition "completion plan is missing")
  | .resolveFinding _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.findings.any (fun record =>
              record.key == key && record.status == .open) then
            .ok ⟨[.findingResolved work key], by simp⟩
          else .error (.invalidTransition "finding is not open")
      | none => .error (.invalidTransition "completion plan is missing")
  | .passValidation _ work key artifactDigest =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if !artifactDigest.isEmpty && completion.validations.any (·.key == key) then
            .ok ⟨[.validationPassed work key artifactDigest], by simp⟩
          else .error (.invalidTransition "validation observation is not a required nonempty artifact")
      | none => .error (.invalidTransition "completion plan is missing")
  | .classifyRepository _ work key snapshotDigest =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if !snapshotDigest.isEmpty && completion.repositories.any (·.key == key) then
            .ok ⟨[.repositoryClassified work key snapshotDigest], by simp⟩
          else .error (.invalidTransition "repository classification is not bound to a required snapshot")
      | none => .error (.invalidTransition "completion plan is missing")
  | .resolveCorrection _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.corrections.any (fun record =>
              record.key == key && record.status == .open) then
            .ok ⟨[.correctionResolved work key], by simp⟩
          else .error (.invalidTransition "correction is not open")
      | none => .error (.invalidTransition "completion plan is missing")
  | .linkWorkRecord _ work key reference =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if !reference.isEmpty && completion.workRecords.any (fun record =>
              record.key == key && record.status == .unlinked) then
            .ok ⟨[.workRecordLinked work key reference], by simp⟩
          else .error (.invalidTransition "work record link is not a required unlinked record")
      | none => .error (.invalidTransition "completion plan is missing")
  | .recordReviewClaim _ claim =>
      match Lifecycle.forWork state.lifecycle claim.work with
      | some completion =>
          if completion.plan.reviews.contains claim.plan &&
              claim.epoch == completion.epoch &&
              !state.claims.any (·.id == claim.id) then
            .ok ⟨[.reviewClaimed claim], by simp⟩
          else .error (.invalidTransition "review claim is not bound to the current required scope")
      | none => .error (.invalidTransition "completion plan is missing")
  | .recordReviewAdjudication _ adjudication =>
      .ok ⟨[.reviewAdjudicated adjudication], by simp⟩
  | .recordEvidence _ evidence =>
      .ok ⟨[.evidenceRecorded evidence], by simp⟩
  | .recordExternalOperation _ attempt =>
      if attempt.state == .prepared then
        .ok ⟨[.externalOperationRecorded attempt], by simp⟩
      else
        .error (.invalidTransition "external operation must begin prepared")
  | .recordObligation _ obligation =>
      .ok ⟨[.obligationRecorded obligation], by simp⟩
  | .completeWork _ target =>
      match Work.activeFor state.activations target with
      | none => .error (.invalidTransition "target work is not active")
      | some activation =>
          if Policy.Completion.closeable target state.work state.activations
              state.claims state.adjudications state.lifecycle
              state.evidence state.obligations then
            .ok ⟨[.workCompleted target activation.id], by simp⟩
          else
            .error (.invalidTransition "completion obligations remain")

structure AcceptedTransaction where
  command : Command
  events : List Event
  eventsNonempty : events ≠ []
  result : VerifiedState

def decide (command : Command) (state : State) : Except DomainError AcceptedTransaction :=
  if command.expectedRevision = state.revision then
    match deriveEvents command state with
    | .error error => .error error
    | .ok derived =>
        match replay derived.events state with
        | .ok result => .ok ⟨command, derived.events, derived.eventsNonempty, result⟩
        | .error error => .error error
  else
    .error .staleRevision

theorem decide_preserves_valid (command : Command) (state : State)
    {transaction : AcceptedTransaction}
    (_accepted : decide command state = .ok transaction) :
    ValidState transaction.result.state :=
  transaction.result.valid

theorem decide_emits_only_derived_events (command : Command) (state : State)
    {transaction : AcceptedTransaction}
    (accepted : decide command state = .ok transaction) :
    ∃ derived, deriveEvents command state = .ok derived ∧
      transaction.events = derived.events := by
  unfold decide at accepted
  split at accepted
  · split at accepted
    · contradiction
    · split at accepted
      · cases accepted
        exact ⟨_, by assumption, rfl⟩
      · contradiction
  · contradiction

def committedEvents (result : Except DomainError AcceptedTransaction) : List Event :=
  match result with
  | .ok transaction => transaction.events
  | .error _ => []

def committedState (result : Except DomainError AcceptedTransaction) (original : State) : State :=
  match result with
  | .ok transaction => transaction.result.state
  | .error _ => original

inductive Commits (command : Command) (initial : State) : List Event → State → Prop
  | accepted (transaction : AcceptedTransaction)
      (accepted : decide command initial = .ok transaction) :
      Commits command initial transaction.events transaction.result.state

theorem decide_rejection_has_no_effect (command : Command) (state : State)
    (error : DomainError) (rejected : decide command state = .error error) :
    (¬ ∃ events result, Commits command state events result) ∧
    committedEvents (decide command state) = [] ∧
    committedState (decide command state) state = state ∧
    (committedState (decide command state) state).revision = state.revision := by
  constructor
  · rintro ⟨_, _, commit⟩
    cases commit with
    | accepted transaction accepted =>
        rw [rejected] at accepted
        contradiction
  · simp [committedEvents, committedState, rejected]

structure CompletionTransaction extends AcceptedTransaction where
  target : WorkId
  activation : ActivationId

def closeWork (target : WorkId) (state : State) : Except DomainError CompletionTransaction :=
  match Work.activeFor state.activations target with
  | none => .error (.invalidTransition "target work is not active")
  | some activation =>
      match decide (.completeWork state.revision target) state with
      | .error error => .error error
      | .ok transaction => .ok { transaction with target, activation := activation.id }

theorem close_work_preserves_valid (target : WorkId) (state : State)
    {transaction : CompletionTransaction}
    (_accepted : closeWork target state = .ok transaction) :
    ValidState transaction.result.state :=
  transaction.result.valid

theorem decide_complete_emits (target : WorkId) (state : State)
    (activation : Work.Activation) (transaction : AcceptedTransaction)
    (active : Work.activeFor state.activations target = some activation)
    (accepted : decide (.completeWork state.revision target) state = .ok transaction) :
    transaction.events = [.workCompleted target activation.id] := by
  unfold decide at accepted
  simp only [Command.expectedRevision] at accepted
  simp only [if_true] at accepted
  by_cases ready : Policy.Completion.closeable target state.work state.activations
      state.claims state.adjudications state.lifecycle state.evidence state.obligations = true
  · simp only [deriveEvents, active, ready, if_true] at accepted
    split at accepted
    · cases accepted
      rfl
    · contradiction
  · have notReady : Policy.Completion.closeable target state.work state.activations
        state.claims state.adjudications state.lifecycle state.evidence state.obligations = false := by
      cases result : Policy.Completion.closeable target state.work state.activations
          state.claims state.adjudications state.lifecycle state.evidence state.obligations with
      | false => rfl
      | true => exact (ready result).elim
    simp [deriveEvents, active, notReady] at accepted

theorem decide_complete_requires_closeable (target : WorkId) (state : State)
    {transaction : AcceptedTransaction}
    (accepted : decide (.completeWork state.revision target) state = .ok transaction) :
    Policy.Completion.closeable target state.work state.activations
      state.claims state.adjudications state.lifecycle state.evidence state.obligations = true := by
  obtain ⟨derived, derivedBy, _⟩ :=
    decide_emits_only_derived_events (.completeWork state.revision target) state accepted
  simp only [deriveEvents] at derivedBy
  split at derivedBy
  · contradiction
  · split at derivedBy
    · assumption
    · contradiction

theorem replay_completion_applicability_matches_policy (target : WorkId)
    (state : State) :
    completionApplicable target state =
      Policy.Completion.closeable target state.work state.activations
        state.claims state.adjudications state.lifecycle
        state.evidence state.obligations := by
  unfold completionApplicable completionObligationsReady
    completionObligationSatisfied completionRelatedWorkTerminal
    completionReviewsReady latestAcceptedCompletionReview
    Policy.Completion.closeable Policy.Completion.obligationsReady
    Policy.Completion.obligationSatisfied Policy.Completion.authoritativeReady
    Policy.Completion.relatedWorkTerminal Policy.Completion.reviewsReady
    Policy.Completion.latestAcceptedReviewClaim
  rfl

theorem close_work_emits_atomic_event (target : WorkId) (state : State)
    {transaction : CompletionTransaction}
    (accepted : closeWork target state = .ok transaction) :
    transaction.events = [.workCompleted transaction.target transaction.activation] := by
  unfold closeWork at accepted
  split at accepted
  · contradiction
  · split at accepted
    · contradiction
    · cases accepted
      exact decide_complete_emits target state _ _ (by assumption) (by assumption)

end AgentWorkbench.Kernel.Decide
