import AgentWorkbench.Kernel.Gates
import AgentWorkbench.Domain.Work

namespace AgentWorkbench.Kernel.Resolver

open AgentWorkbench.Kernel.Replay

inductive Action
  | initializeWork
  | continueActiveWork
  | resumeSuspendedWork
  | completeWork
deriving DecidableEq, Repr

def allowedActions (state : State) : List Action :=
  if state.work.isEmpty then
    [.initializeWork]
  else if (Domain.Work.activeActivations state.activations).isEmpty then
    [.resumeSuspendedWork]
  else
    [.continueActiveWork, .completeWork]

def next (state : State) : Option Action :=
  (allowedActions state).head?

theorem next_is_allowed (state : State) {action : Action}
    (selected : next state = some action) :
    action ∈ allowedActions state := by
  unfold next at selected
  generalize actionsEq : allowedActions state = actions at selected ⊢
  cases actions with
  | nil => simp at selected
  | cons first rest =>
      simp only [List.head?_cons, Option.some.injEq] at selected
      subst action
      simp

end AgentWorkbench.Kernel.Resolver
