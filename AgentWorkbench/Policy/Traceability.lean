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
  item.requirements.contains requirement &&
  !item.completionBoundaries.isEmpty &&
  !item.validationGates.isEmpty

def allCovered (requirements : List String) (items : List PlanItem) : Bool :=
  requirements.all fun requirement => items.any (covers · requirement)

def implementationBound (item : Design.TraceItem) : Bool :=
  !item.key.isEmpty && !item.requirements.isEmpty &&
  !item.implementationWork.isEmpty &&
  item.implementationWork.all (fun target => !target.isEmpty) &&
  !item.completionChecks.isEmpty &&
  !item.validationGates.isEmpty

def ready (design : Design.DesignVersion)
    (decomposition : Design.Decomposition) : Bool :=
  let active := (design.requirements.filter (·.active)).map (·.key)
  design.approved && !design.owner.isEmpty && !design.contentDigest.isEmpty &&
  decomposition.design == design.id &&
  decomposition.designRevision == design.revision &&
  decomposition.accepted &&
  !decomposition.reviewer.isEmpty && !decomposition.adjudicator.isEmpty &&
  decomposition.reviewer != design.owner &&
  decomposition.reviewer != decomposition.adjudicator &&
  !decomposition.items.isEmpty &&
  decomposition.items.all implementationBound &&
  allCovered active (decomposition.items.map fromTraceItem) &&
  Design.decompositionCovers design decomposition

end AgentWorkbench.Policy.Traceability
