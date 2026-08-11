import AgentWorkbenchProof.Operation

namespace AgentWorkbenchProof

open AgentWorkbench

/-- Exact accepted-Design identity for one production-effect owner and its verification
obligation. -/
structure ProductionEffectOwner where
  designRevisionId : String
  designRevisionDigest : String
  statementId : String
  statementText : String
  statementTextDigest : String
  criterionId : String
  criterionBinding : String
  deriving Repr, DecidableEq, Lean.ToJson

structure ProductionEffectGrounding where
  key : ProductionEffectKey
  owner : ProductionEffectOwner
  deriving Repr, DecidableEq

def productionEffectGroundingCount
    (matrix : List ProductionEffectGrounding) (key : ProductionEffectKey) : Nat :=
  matrix.countP fun grounding => grounding.key == key

def validProductionEffectOwner (owner : ProductionEffectOwner) : Bool :=
  !owner.designRevisionId.isEmpty && owner.designRevisionDigest.startsWith "blake3:" &&
    !owner.statementId.isEmpty && !owner.statementText.isEmpty &&
    owner.statementTextDigest.startsWith "blake3:" &&
    !owner.criterionId.isEmpty && !owner.criterionBinding.isEmpty

/-- Derive authority from the current accepted Design itself.  The caller selects only stable IDs;
it cannot supply the Design identity, content digests, Statement text, or Criterion binding that
the matrix is checked against. -/
def productionEffectOwnerFor?
    (state : ProjectState) (statementId criterionId : String) : Option ProductionEffectOwner := do
  let design ← state.currentDesign?
  let statement ← design.statements.find? (fun value => value.id == statementId)
  let criterion ← design.acceptanceCriteria.find? (fun value => value.id == criterionId)
  let contract ← design.effectiveAssuranceContracts.find?
    (fun value => value.statementId == statement.id)
  if criterion.statementId != some statement.id ||
      !contract.criterionIds.contains criterion.id ||
      contract.designRevisionId != design.id ||
      contract.statementText != statement.text then none
  else pure {
    designRevisionId := design.id
    designRevisionDigest := design.revisionContentDigest
    statementId := statement.id
    statementText := statement.text
    statementTextDigest := contract.statementTextDigest
    criterionId := criterion.id
    criterionBinding := (Lean.toJson criterion).compress }

/-- Reverse coverage is exact, not merely positive: every derived production pair occurs once,
every supplied row belongs to that universe, and every row binds the same current accepted-Design
Statement and verification obligation. -/
def validProductionEffectGroundingMatrixFor
    (effectUniverse : List ProductionEffectKey) (state : ProjectState)
    (statementId criterionId : String) (matrix : List ProductionEffectGrounding) : Bool :=
  match productionEffectOwnerFor? state statementId criterionId with
  | none => false
  | some currentOwner =>
      validProductionEffectOwner currentOwner &&
        (effectUniverse.all fun key =>
          effectUniverse.count key == 1 && productionEffectGroundingCount matrix key == 1) &&
        (matrix.all fun grounding =>
          effectUniverse.contains grounding.key && grounding.owner == currentOwner)

def validProductionEffectGroundingMatrix
    (state : ProjectState) (statementId criterionId : String)
    (matrix : List ProductionEffectGrounding) : Bool :=
  validProductionEffectGroundingMatrixFor productionEffectUniverse state statementId criterionId
    matrix

theorem valid_grounding_has_exactly_one_owner
    (state : ProjectState) (statementId criterionId : String)
    (currentOwner : ProductionEffectOwner)
    (authority : productionEffectOwnerFor? state statementId criterionId = some currentOwner)
    (matrix : List ProductionEffectGrounding)
    (valid : validProductionEffectGroundingMatrix state statementId criterionId matrix = true)
    (key : ProductionEffectKey) (member : key ∈ productionEffectUniverse) :
    productionEffectGroundingCount matrix key = 1 := by
  simp only [validProductionEffectGroundingMatrix, validProductionEffectGroundingMatrixFor,
    authority, Bool.and_eq_true] at valid
  have covered := List.all_eq_true.mp valid.1.2 key member
  have coveredBoth : (productionEffectUniverse.count key == 1) = true ∧
      (productionEffectGroundingCount matrix key == 1) = true := by
    simpa [Bool.and_eq_true] using covered
  simpa using coveredBoth.2

theorem valid_grounding_rows_are_current_and_in_scope
    (state : ProjectState) (statementId criterionId : String)
    (currentOwner : ProductionEffectOwner)
    (authority : productionEffectOwnerFor? state statementId criterionId = some currentOwner)
    (matrix : List ProductionEffectGrounding)
    (valid : validProductionEffectGroundingMatrix state statementId criterionId matrix = true)
    (grounding : ProductionEffectGrounding) (member : grounding ∈ matrix) :
    grounding.key ∈ productionEffectUniverse ∧ grounding.owner = currentOwner := by
  simp only [validProductionEffectGroundingMatrix, validProductionEffectGroundingMatrixFor,
    authority, Bool.and_eq_true] at valid
  have row := List.all_eq_true.mp valid.2 grounding member
  have rowBoth : (productionEffectUniverse.contains grounding.key) = true ∧
      (grounding.owner == currentOwner) = true := by
    simpa [Bool.and_eq_true] using row
  constructor
  · simpa using rowBoth.1
  · simpa using rowBoth.2

/-- Runtime effects come from the actual before/after state and are already a subset of the
declared operation effects. An exact matrix therefore gives every committed effect exactly one
current accepted-Design owner. -/
theorem successful_prepared_mutation_effect_has_exactly_one_owner
    (prepared : PreparedMutation) (prior next : ProjectState)
    (success : prepared.execute prior = .ok next)
    (statementId criterionId : String) (currentOwner : ProductionEffectOwner)
    (authority : productionEffectOwnerFor? prior statementId criterionId = some currentOwner)
    (matrix : List ProductionEffectGrounding)
    (valid : validProductionEffectGroundingMatrix prior statementId criterionId matrix = true)
    (effect : ProductionEffect) (actual : effect ∈ actualProductionEffects prior next) :
    productionEffectGroundingCount matrix { operation := prepared.operation, effect } = 1 := by
  have permitted := successful_prepared_mutation_respects_production_effect_universe
    prepared prior next success
  have effectPermitted : effect ∈ prepared.operation.permittedProductionEffects := by
    have allEffects := List.all_eq_true.mp permitted effect actual
    simpa using allEffects
  apply valid_grounding_has_exactly_one_owner prior statementId criterionId currentOwner authority
    matrix valid
  simp [productionEffectUniverse, Operation.mem_all prepared.operation, effectPermitted]

end AgentWorkbenchProof
