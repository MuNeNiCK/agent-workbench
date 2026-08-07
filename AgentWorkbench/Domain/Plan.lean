import AgentWorkbench.Domain.DesignSourceGraph

namespace AgentWorkbench

inductive PlanStatus where
  | candidate
  | current
  | superseded
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive StatementDeltaKind where
  | added
  | modified
  | removed
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure PlanSource where
  target : String
  digest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure PlanSourceUnitDisposition where
  unitId : String
  stepId : Option String := none
  noStepReason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure PlanStatementDisposition where
  statementId : String
  statementText : String
  deltaKind : StatementDeltaKind
  stepIds : List String := []
  noActionReason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure PlanStep where
  id : String
  description : String
  dependsOnStepIds : List String := []
  outputScopes : List String
  requiredClaimIds : List String := []
  verificationCriterionIds : List String := []
  acceptedFindingEntryIds : List String := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ImplementationPlan where
  id : String
  workId : String
  designRevision : String
  predecessorPlanId : Option String := none
  status : PlanStatus := .candidate
  producerAgentRun : String
  reason : String
  changeBasisEntryIds : List String := []
  contentDigest : String
  sourceArchiveAvailable : Bool := true
  sourceDocuments : List PlanSource
  sourceUnits : List DesignSourceUnit
  sourceUnitDispositions : List PlanSourceUnitDisposition
  statementDispositions : List PlanStatementDisposition
  steps : List PlanStep
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
