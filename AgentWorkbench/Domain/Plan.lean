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

inductive TaskVerificationKind where
  | command
  | artifact
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure TaskVerificationContract where
  id : String
  kind : TaskVerificationKind
  target : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure PlanStep where
  id : String
  description : String
  dependsOnStepIds : List String := []
  outputScopes : List String
  requiredClaimIds : List String := []
  verificationCriterionIds : List String := []
  taskVerificationContracts : List TaskVerificationContract := []
  acceptedFindingEntryIds : List String := []
  deriving Repr, DecidableEq

instance : Lean.ToJson PlanStep where
  toJson value := Lean.Json.mkObj <|
    [("id", Lean.toJson value.id),
     ("description", Lean.toJson value.description),
     ("dependsOnStepIds", Lean.toJson value.dependsOnStepIds),
     ("outputScopes", Lean.toJson value.outputScopes),
     ("requiredClaimIds", Lean.toJson value.requiredClaimIds),
     ("verificationCriterionIds", Lean.toJson value.verificationCriterionIds)] ++
    (if value.taskVerificationContracts.isEmpty then [] else
      [("taskVerificationContracts", Lean.toJson value.taskVerificationContracts)]) ++
    [("acceptedFindingEntryIds", Lean.toJson value.acceptedFindingEntryIds)]

private structure PersistedPlanStep where
  id : String
  description : String
  dependsOnStepIds : List String := []
  outputScopes : List String
  requiredClaimIds : List String := []
  verificationCriterionIds : List String := []
  taskVerificationContracts : Option (List TaskVerificationContract) := none
  acceptedFindingEntryIds : List String := []
  deriving Lean.FromJson

instance : Lean.FromJson PlanStep where
  fromJson? json := do
    let value ← (Lean.fromJson? json : Except String PersistedPlanStep)
    pure {
      id := value.id, description := value.description
      dependsOnStepIds := value.dependsOnStepIds, outputScopes := value.outputScopes
      requiredClaimIds := value.requiredClaimIds
      verificationCriterionIds := value.verificationCriterionIds
      taskVerificationContracts := value.taskVerificationContracts.getD []
      acceptedFindingEntryIds := value.acceptedFindingEntryIds }

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
