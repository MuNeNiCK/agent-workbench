import AgentWorkbenchProof.Operation

namespace AgentWorkbenchProof

open AgentWorkbench

/-- Exact accepted-Design identity for one production-effect owner and its verification
obligation. The product proof is generic; repository-private evidence supplies the current
accepted Design values. -/
structure ProductionEffectOwner where
  designRevisionId : String
  designRevisionDigest : String
  statementId : String
  statementText : String
  statementTextDigest : String
  criterionId : String
  criterionBinding : String
  deriving Repr, DecidableEq

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

/-- Reverse coverage is exact, not merely positive: every derived production pair occurs once,
every supplied row belongs to that universe, and every row binds the same current accepted-Design
Statement and verification obligation. -/
def validProductionEffectGroundingMatrixFor
    (effectUniverse : List ProductionEffectKey) (currentOwner : ProductionEffectOwner)
    (matrix : List ProductionEffectGrounding) : Bool :=
  validProductionEffectOwner currentOwner &&
    (effectUniverse.all fun key =>
      effectUniverse.count key == 1 && productionEffectGroundingCount matrix key == 1) &&
    (matrix.all fun grounding =>
      effectUniverse.contains grounding.key && grounding.owner == currentOwner
    )

def validProductionEffectGroundingMatrix
    (currentOwner : ProductionEffectOwner)
    (matrix : List ProductionEffectGrounding) : Bool :=
  validProductionEffectGroundingMatrixFor productionEffectUniverse currentOwner matrix

theorem valid_grounding_has_exactly_one_owner
    (currentOwner : ProductionEffectOwner) (matrix : List ProductionEffectGrounding)
    (valid : validProductionEffectGroundingMatrix currentOwner matrix = true)
    (key : ProductionEffectKey) (member : key ∈ productionEffectUniverse) :
    productionEffectGroundingCount matrix key = 1 := by
  simp only [validProductionEffectGroundingMatrix, validProductionEffectGroundingMatrixFor,
    Bool.and_eq_true] at valid
  have covered := List.all_eq_true.mp valid.1.2 key member
  have coveredBoth : (productionEffectUniverse.count key == 1) = true ∧
      (productionEffectGroundingCount matrix key == 1) = true := by
    simpa [Bool.and_eq_true] using covered
  simpa using coveredBoth.2

theorem valid_grounding_rows_are_current_and_in_scope
    (currentOwner : ProductionEffectOwner) (matrix : List ProductionEffectGrounding)
    (valid : validProductionEffectGroundingMatrix currentOwner matrix = true)
    (grounding : ProductionEffectGrounding) (member : grounding ∈ matrix) :
    grounding.key ∈ productionEffectUniverse ∧ grounding.owner = currentOwner := by
  simp only [validProductionEffectGroundingMatrix, validProductionEffectGroundingMatrixFor,
    Bool.and_eq_true] at valid
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
    (currentOwner : ProductionEffectOwner) (matrix : List ProductionEffectGrounding)
    (valid : validProductionEffectGroundingMatrix currentOwner matrix = true)
    (effect : ProductionEffect) (actual : effect ∈ actualProductionEffects prior next) :
    productionEffectGroundingCount matrix { operation := prepared.operation, effect } = 1 := by
  have permitted := successful_prepared_mutation_respects_production_effect_universe
    prepared prior next success
  have effectPermitted : effect ∈ prepared.operation.permittedProductionEffects := by
    have allEffects := List.all_eq_true.mp permitted effect actual
    simpa using allEffects
  apply valid_grounding_has_exactly_one_owner currentOwner matrix valid
  simp [productionEffectUniverse, Operation.mem_all prepared.operation, effectPermitted]

end AgentWorkbenchProof
