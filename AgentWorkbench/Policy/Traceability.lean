import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Evidence

namespace AgentWorkbench.Policy.Traceability

structure PlanItem where
  key : String
  requirements : List String
  completionBoundaries : List String
  validationGates : List String
deriving DecidableEq, Repr

def covers (item : PlanItem) (requirement : String) : Bool :=
  item.requirements.contains requirement &&
  !item.completionBoundaries.isEmpty &&
  !item.validationGates.isEmpty

def allCovered (requirements : List String) (items : List PlanItem) : Bool :=
  requirements.all fun requirement => items.any (covers · requirement)

end AgentWorkbench.Policy.Traceability
