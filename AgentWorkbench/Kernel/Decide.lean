import AgentWorkbench.Kernel.Replay
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority
import AgentWorkbench.Policy.Completion
import AgentWorkbench.Policy.Update

namespace AgentWorkbench.Kernel.Decide

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

structure Command where
  expectedRevision : Revision
  events : List Event
  eventsNonempty : events ≠ []

structure AcceptedTransaction where
  events : List Event
  eventsNonempty : events ≠ []
  result : VerifiedState

def decide (command : Command) (state : State) : Except DomainError AcceptedTransaction :=
  if command.expectedRevision == state.revision then
    match replay command.events state with
    | .ok result => .ok ⟨command.events, command.eventsNonempty, result⟩
    | .error error => .error error
  else
    .error .staleRevision

theorem decide_preserves_valid (command : Command) (state : State)
    {transaction : AcceptedTransaction}
    (_accepted : decide command state = .ok transaction) :
    ValidState transaction.result.state :=
  transaction.result.valid

def committedState (result : Except DomainError AcceptedTransaction) (original : State) : State :=
  match result with
  | .ok transaction => transaction.result.state
  | .error _ => original

theorem decide_rejection_has_no_effect (command : Command) (state : State)
    (error : DomainError) (rejected : decide command state = .error error) :
    committedState (decide command state) state = state := by
  simp [committedState, rejected]

structure CompletionTransaction extends AcceptedTransaction where
  target : WorkId
  activation : ActivationId

def closeWork (target : WorkId) (state : State) : Except DomainError CompletionTransaction :=
  match Work.activeFor state.activations target with
  | none => .error (.invalidTransition "target work is not active")
  | some activation =>
      if Policy.Completion.closeable target state.work state.activations
          state.completionFacts state.obligations then
        match replay [.workCompleted target activation.id] state with
        | .ok result => .ok {
            events := [.workCompleted target activation.id]
            eventsNonempty := by simp
            result
            target
            activation := activation.id }
        | .error error => .error error
      else
        .error (.invalidTransition "completion obligations remain")

theorem close_work_preserves_valid (target : WorkId) (state : State)
    {transaction : CompletionTransaction}
    (_accepted : closeWork target state = .ok transaction) :
    ValidState transaction.result.state :=
  transaction.result.valid

theorem close_work_emits_atomic_event (target : WorkId) (state : State)
    {transaction : CompletionTransaction}
    (accepted : closeWork target state = .ok transaction) :
    transaction.events = [.workCompleted transaction.target transaction.activation] := by
  unfold closeWork at accepted
  split at accepted
  · contradiction
  · split at accepted
    · split at accepted
      · cases accepted
        rfl
      · contradiction
    · contradiction

end AgentWorkbench.Kernel.Decide
