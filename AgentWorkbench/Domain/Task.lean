import Lean.Data.Json

namespace AgentWorkbench

structure TaskRecord where
  planId : Option String := none
  planStepId : Option String := none
  lineageId : Option String := none
  dependencyLineageIds : List String := []
  outputScopes : List String := []
  verificationCriterionIds : List String := []
  /-- Exact evidence entries used to close this Task. Completion revalidates
  these identities; evidence for a sibling Task cannot substitute for them. -/
  verificationEvidenceEntryIds : List String := []
  verificationTaskEntryId : Option String := none
  materializedAtOrder : Nat := 0
  retired : Bool := false
  criterionId : Option String := none
  description : String
  required : Bool
  closed : Bool
  deriving Repr, DecidableEq, Lean.ToJson

private structure PersistedTaskRecord where
  planId : Option String
  planStepId : Option String
  lineageId : Option String
  dependencyLineageIds : List String
  outputScopes : List String
  verificationCriterionIds : List String
  verificationEvidenceEntryIds : List String := []
  verificationTaskEntryId : Option String := none
  materializedAtOrder : Nat
  retired : Bool
  criterionId : Option String
  description : String
  required : Bool
  closed : Bool
  deriving Lean.FromJson

private structure LegacyTaskRecord where
  criterionId : Option String := none
  description : String
  required : Bool
  closed : Bool
  deriving Lean.FromJson

instance : Lean.FromJson TaskRecord where
  fromJson? json :=
    match (Lean.fromJson? json : Except String PersistedTaskRecord) with
    | .ok value => pure {
        planId := value.planId, planStepId := value.planStepId, lineageId := value.lineageId
        dependencyLineageIds := value.dependencyLineageIds, outputScopes := value.outputScopes
        verificationCriterionIds := value.verificationCriterionIds
        verificationEvidenceEntryIds := value.verificationEvidenceEntryIds
        verificationTaskEntryId := value.verificationTaskEntryId
        materializedAtOrder := value.materializedAtOrder, retired := value.retired
        criterionId := value.criterionId, description := value.description
        required := value.required, closed := value.closed }
    | .error currentError =>
        match (Lean.fromJson? json : Except String LegacyTaskRecord) with
        | .ok value => pure {
            criterionId := value.criterionId, description := value.description
            required := value.required, closed := value.closed }
        | .error legacyError =>
            throw s!"invalid Task: {currentError}; legacy: {legacyError}"

end AgentWorkbench
