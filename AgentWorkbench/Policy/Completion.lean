import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Lifecycle

namespace AgentWorkbench.Policy.Completion

open AgentWorkbench.Domain

def authoritativeReady (target : WorkId) (work : List Work.WorkUnit)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (lifecycle : List Lifecycle.CompletionState) : Bool :=
  match Lifecycle.forWork lifecycle target with
  | none => false
  | some state =>
      Lifecycle.relatedWorkTerminal work state.plan.relatedWork &&
      Lifecycle.recordsReady state &&
      Lifecycle.reviewsReady state claims adjudications

def closeable (target : WorkId) (work : List Work.WorkUnit)
    (activations : List Work.Activation) (claims : List Review.Claim)
    (adjudications : List Review.Adjudication)
    (lifecycle : List Lifecycle.CompletionState) : Bool :=
  (Work.activeFor activations target).isSome &&
  Work.workIsOpen work target &&
  authoritativeReady target work claims adjudications lifecycle

theorem completion_requires_authoritative_lifecycle (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (lifecycle : List Lifecycle.CompletionState)
    (accepted : closeable target work activations claims adjudications lifecycle = true) :
    authoritativeReady target work claims adjudications lifecycle = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.2

theorem completion_requires_active_target (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (lifecycle : List Lifecycle.CompletionState)
    (accepted : closeable target work activations claims adjudications lifecycle = true) :
    (Work.activeFor activations target).isSome = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1.1

end AgentWorkbench.Policy.Completion
