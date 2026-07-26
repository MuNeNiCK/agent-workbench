import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Evidence

namespace AgentWorkbench.Policy.Traceability

open AgentWorkbench.Domain

structure PlanItem where
  key : String
  requirements : List String
  completionBoundaries : List String
  validationGates : List String
deriving DecidableEq, Repr

def fromTraceItem (item : Design.TraceItem) : PlanItem :=
  { key := item.key
    requirements := item.requirements
    completionBoundaries := item.completionChecks
    validationGates := item.validationGates }

def covers (item : PlanItem) (requirement : String) : Bool :=
  item.requirements.contains requirement

def allCovered (requirements : List String) (items : List PlanItem) : Bool :=
  requirements.all fun requirement => items.any (covers · requirement)

def implementationBound (item : Design.TraceItem) : Bool :=
  !item.key.isEmpty && !item.requirements.isEmpty &&
  item.implementationWork.all (fun target => !target.isEmpty) &&
  item.tasks.all (fun task => !task.isEmpty) &&
  item.completionChecks.all (fun condition => !condition.isEmpty) &&
  item.checklists.all (fun checklist => !checklist.isEmpty) &&
  item.validationGates.all (fun gate => !gate.isEmpty) &&
  !(item.implementationWork ++ item.tasks ++ item.completionChecks ++
    item.checklists ++ item.validationGates).isEmpty

def ready (design : Design.DesignVersion)
    (approval : Design.Approval) (decomposition : Design.Decomposition) : Bool :=
  Design.decompositionCovers design approval decomposition

end AgentWorkbench.Policy.Traceability
