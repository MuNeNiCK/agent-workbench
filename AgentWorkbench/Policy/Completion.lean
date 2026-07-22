import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.ExternalOperation
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority

namespace AgentWorkbench.Policy.Completion

open AgentWorkbench.Domain.Evidence

open AgentWorkbench.Domain
open AgentWorkbench.Domain.Work

def factsComplete (facts : CompletionFacts) : Bool :=
  facts.dependentWorkTerminal &&
  facts.phasesTerminal &&
  facts.tasksComplete &&
  facts.checklistsComplete &&
  facts.reviewsClean &&
  facts.findingsResolved &&
  facts.repositoryClassified &&
  facts.workRecordsLinked &&
  facts.correctionsResolved

def completionFactsReady (allFacts : List CompletionFacts) (target : WorkId) : Bool :=
  allFacts.any fun facts => facts.work == target && factsComplete facts

def obligationsReady (obligations : List Obligation) (target : WorkId) : Bool :=
  let owned := forWork obligations target
  !owned.isEmpty && obligationsCurrent owned

def closeable (target : WorkId) (work : List WorkUnit) (activations : List Activation)
    (facts : List CompletionFacts) (obligations : List Obligation) : Bool :=
  obligationsReady obligations target && (
    (activeFor activations target).isSome &&
    workIsOpen work target &&
    completionFactsReady facts target)

theorem completion_requires_current_obligations (target : WorkId)
    (work : List WorkUnit) (activations : List Activation)
    (facts : List CompletionFacts) (obligations : List Obligation)
    (accepted : closeable target work activations facts obligations = true) :
    obligationsReady obligations target = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.1

theorem completion_requires_active_target (target : WorkId)
    (work : List WorkUnit) (activations : List Activation)
    (facts : List CompletionFacts) (obligations : List Obligation)
    (accepted : closeable target work activations facts obligations = true) :
    (activeFor activations target).isSome = true := by
  simp only [closeable, Bool.and_eq_true] at accepted
  exact accepted.2.1.1

end AgentWorkbench.Policy.Completion
