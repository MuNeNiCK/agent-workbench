import AgentWorkbench.Domain.Identity

namespace AgentWorkbench.Domain.Design

open AgentWorkbench.Domain

inductive Role
  | goal
  | functionalRequirement
  | nonFunctionalRequirement
  | constraint
  | decision
  | projectStructure
  | projectFact
  | trustedBoundary
deriving DecidableEq, Repr, BEq

inductive AssuranceKind
  | formal
  | evidence
  | mixed
  | none
deriving DecidableEq, Repr, BEq

inductive AssuranceMethod
  | formal
  | evidence
deriving DecidableEq, Repr, BEq

structure AssuranceObligation where
  key : String
  method : AssuranceMethod
  description : String
deriving DecidableEq, Repr, BEq

structure AssuranceSelection where
  kind : AssuranceKind
  obligations : List AssuranceObligation
deriving DecidableEq, Repr, BEq

def AssuranceSelection.wellFormed (selection : AssuranceSelection) : Bool :=
  let descriptionsPresent :=
    selection.obligations.all fun obligation =>
      !obligation.key.isEmpty && !obligation.description.isEmpty
  let hasFormal := selection.obligations.any (·.method == .formal)
  let hasEvidence := selection.obligations.any (·.method == .evidence)
  descriptionsPresent &&
    (selection.obligations.map (·.key)).Nodup &&
    match selection.kind with
    | .none => selection.obligations.isEmpty
    | .formal => hasFormal && !hasEvidence
    | .evidence => hasEvidence && !hasFormal
    | .mixed => hasFormal && hasEvidence

inductive Authority
  | unaccepted
  | acceptedByCaller (decision : CallerDecision)
  | retiredByCaller (decision : CallerDecision)
deriving DecidableEq, Repr, BEq

structure ComplexityRationale where
  necessity : String
  simplerAlternativeInsufficient : String
  boundedScope : String
  maintenanceCost : String
deriving DecidableEq, Repr, BEq

def ComplexityRationale.wellFormed (rationale : ComplexityRationale) : Bool :=
  !rationale.necessity.isEmpty &&
    !rationale.simplerAlternativeInsufficient.isEmpty &&
    !rationale.boundedScope.isEmpty &&
    !rationale.maintenanceCost.isEmpty

structure Item where
  ref : DesignRef
  predecessor : Option DesignRef
  statement : String
  role : Role
  source : Source
  dependencies : List DesignRef
  assurance : AssuranceSelection
  addsComplexity : Bool := false
  complexityRationale : Option ComplexityRationale := none
  authority : Authority
deriving DecidableEq, Repr, BEq

def Item.wellFormed (item : Item) : Bool :=
  !item.ref.key.isEmpty &&
    !item.statement.isEmpty &&
    !item.source.id.value.isEmpty &&
    item.predecessor != some item.ref &&
    item.dependencies.Nodup &&
    !item.dependencies.contains item.ref &&
    item.assurance.wellFormed &&
    (if item.addsComplexity then
      item.source.kind == .agent &&
        match item.authority with
        | .unaccepted => item.complexityRationale.isNone
        | .acceptedByCaller decision =>
            decision.wellFormed &&
              item.complexityRationale.any ComplexityRationale.wellFormed
        | .retiredByCaller _ => false
    else
      item.complexityRationale.isNone &&
        match item.authority with
        | .unaccepted => true
        | .acceptedByCaller decision => decision.wellFormed
        | .retiredByCaller decision =>
            item.predecessor.isSome && decision.wellFormed)

structure AcceptedRef where
  ref : DesignRef
deriving DecidableEq, Repr, BEq

def Item.acceptedRef? (item : Item) : Option AcceptedRef :=
  match item.authority with
  | .unaccepted => none
  | .acceptedByCaller decision =>
      if decision.wellFormed && item.wellFormed then some ⟨item.ref⟩ else none
  | .retiredByCaller _ => none

structure OperatingInstruction where
  source : Source
  statement : String
  authority : CallerDecision
deriving DecidableEq, Repr, BEq

def OperatingInstruction.wellFormed (instruction : OperatingInstruction) : Bool :=
  !instruction.statement.isEmpty &&
    instruction.source == instruction.authority.source &&
    instruction.authority.wellFormed

inductive NonAuthoritativeKind
  | proposal
  | question
  | context
  | rejection
deriving DecidableEq, Repr, BEq

structure NonAuthoritativeRecord where
  kind : NonAuthoritativeKind
  statement : String
  target : Option String := none
deriving DecidableEq, Repr, BEq

inductive EffectContent
  | design (item : Item)
  | instruction (instruction : OperatingInstruction)
  | nonAuthoritative (record : NonAuthoritativeRecord)
deriving DecidableEq, Repr, BEq

structure Effect where
  source : Source
  content : EffectContent
deriving DecidableEq, Repr, BEq

def Effect.wellFormed (effect : Effect) : Bool :=
  !effect.source.id.value.isEmpty &&
    match effect.content with
    | .design item => item.source == effect.source && item.wellFormed
    | .instruction instruction =>
        instruction.source == effect.source && instruction.wellFormed
    | .nonAuthoritative record =>
        !record.statement.isEmpty &&
          match record.kind with
          | .rejection => record.target.any (fun target => !target.isEmpty)
          | _ => record.target.all (fun target => !target.isEmpty)

structure Package where
  effects : List Effect
deriving DecidableEq, Repr, BEq

def Package.wellFormed (package : Package) : Bool :=
  package.effects.all Effect.wellFormed

def Package.designItems (package : Package) : List Item :=
  package.effects.filterMap fun effect =>
    match effect.content with
    | .design item => some item
    | _ => none

def Package.instructions (package : Package) : List OperatingInstruction :=
  package.effects.filterMap fun effect =>
    match effect.content with
    | .instruction instruction => some instruction
    | _ => none

end AgentWorkbench.Domain.Design
