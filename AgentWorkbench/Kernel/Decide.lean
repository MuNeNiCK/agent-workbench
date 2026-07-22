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

end AgentWorkbench.Kernel.Decide
