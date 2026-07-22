import AgentWorkbench.Kernel.Replay
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority
import AgentWorkbench.Policy.Completion
import AgentWorkbench.Policy.Update

namespace AgentWorkbench.Kernel.Decide

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Command
  | replaceWorkState (expectedRevision : Revision)
      (work : List Work.WorkUnit) (activations : List Work.Activation)
  | recordReviewClaim (expectedRevision : Revision) (claim : Review.Claim)
  | recordReviewAdjudication (expectedRevision : Revision)
      (adjudication : Review.Adjudication)
  | recordEvidence (expectedRevision : Revision) (evidence : Evidence.Evidence)
  | recordExternalOperation (expectedRevision : Revision)
      (attempt : ExternalOperation.Attempt)
  | recordObligation (expectedRevision : Revision) (obligation : Evidence.Obligation)
  | recordCompletionEvidence (expectedRevision : Revision)
      (facts : Work.CompletionFacts) (obligations : List Evidence.Obligation)
  | completeWork (expectedRevision : Revision) (target : WorkId)
deriving DecidableEq, Repr

def Command.expectedRevision : Command → Revision
  | .replaceWorkState revision _ _
  | .recordReviewClaim revision _
  | .recordReviewAdjudication revision _
  | .recordEvidence revision _
  | .recordExternalOperation revision _
  | .recordObligation revision _
  | .recordCompletionEvidence revision _ _
  | .completeWork revision _ => revision

structure DerivedEvents where
  events : List Event
  eventsNonempty : events ≠ []

def deriveEvents (command : Command) (state : State) : Except DomainError DerivedEvents :=
  match command with
  | .replaceWorkState _ work activations =>
      .ok ⟨[.replaceWork work, .replaceActivations activations], by simp⟩
  | .recordReviewClaim _ claim =>
      .ok ⟨[.reviewClaimed claim], by simp⟩
  | .recordReviewAdjudication _ adjudication =>
      .ok ⟨[.reviewAdjudicated adjudication], by simp⟩
  | .recordEvidence _ evidence =>
      .ok ⟨[.evidenceRecorded evidence], by simp⟩
  | .recordExternalOperation _ attempt =>
      .ok ⟨[.externalOperationRecorded attempt], by simp⟩
  | .recordObligation _ obligation =>
      .ok ⟨[.obligationRecorded obligation], by simp⟩
  | .recordCompletionEvidence _ facts obligations =>
      .ok ⟨[.completionEvidenceRecorded facts obligations], by simp⟩
  | .completeWork _ target =>
      match Work.activeFor state.activations target with
      | none => .error (.invalidTransition "target work is not active")
      | some activation =>
          if Policy.Completion.closeable target state.work state.activations
              state.completionFacts state.obligations then
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

theorem decide_rejection_has_no_effect (command : Command) (state : State)
    (error : DomainError) (rejected : decide command state = .error error) :
    committedEvents (decide command state) = [] ∧
    committedState (decide command state) state = state ∧
    (committedState (decide command state) state).revision = state.revision := by
  simp [committedEvents, committedState, rejected]

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
      state.completionFacts state.obligations = true
  · simp only [deriveEvents, active, ready, if_true] at accepted
    split at accepted
    · cases accepted
      rfl
    · contradiction
  · have notReady : Policy.Completion.closeable target state.work state.activations
        state.completionFacts state.obligations = false := by
      cases result : Policy.Completion.closeable target state.work state.activations
          state.completionFacts state.obligations with
      | false => rfl
      | true => exact (ready result).elim
    simp [deriveEvents, active, notReady] at accepted

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
