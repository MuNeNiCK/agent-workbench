import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.ExternalOperation
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority

namespace AgentWorkbench.Policy.Completion

open AgentWorkbench.Domain.Evidence

structure CompletionContext where
  targetActive : Bool
  dependentWorkTerminal : Bool
  phasesTerminal : Bool
  tasksComplete : Bool
  checklistsComplete : Bool
  reviewsClean : Bool
  findingsResolved : Bool
  repositoryClassified : Bool
  workRecordsLinked : Bool
  correctionsResolved : Bool
  obligations : List Obligation
deriving DecidableEq, Repr

def closeable (context : CompletionContext) : Bool :=
  context.targetActive && (
    context.dependentWorkTerminal &&
    context.phasesTerminal &&
    context.tasksComplete &&
    context.checklistsComplete &&
    context.reviewsClean &&
    context.findingsResolved &&
    context.repositoryClassified &&
    context.workRecordsLinked &&
    context.correctionsResolved &&
    obligationsCurrent context.obligations)

theorem completion_requires_current_obligations (context : CompletionContext)
    (accepted : closeable context = true) :
    obligationsCurrent context.obligations = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.2.2

theorem completion_requires_active_target (context : CompletionContext)
    (accepted : closeable context = true) :
    context.targetActive = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1

end AgentWorkbench.Policy.Completion
