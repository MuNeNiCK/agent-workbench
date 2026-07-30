import AgentWorkbench.Domain.Work

namespace AgentWorkbench.Domain.Evidence

open AgentWorkbench.Domain

structure AssuranceSpec where
  key : String
  description : String
  method : Design.AssuranceMethod
  basis : Work.DerivationBasis
deriving DecidableEq, Repr, BEq

def selectedAssurance (items : List Design.Item) : List AssuranceSpec :=
  items.flatMap fun item =>
    match item.acceptedRef? with
    | none => []
    | some accepted =>
        item.assurance.obligations.map fun obligation =>
          { key := obligation.key
            description := obligation.description
            method := obligation.method
            basis := .design [accepted] }

structure Spec where
  ref : EvidenceRef
  observation : String
  method : String
  environment : String
  inputs : List String
  acceptanceCondition : String
  trustedBoundary : String
  artifactIdentity : String
  basis : Work.DerivationBasis
deriving DecidableEq, Repr, BEq

def Spec.wellFormed (spec : Spec) : Bool :=
  !spec.ref.key.isEmpty &&
    !spec.observation.isEmpty &&
    !spec.method.isEmpty &&
    !spec.environment.isEmpty &&
    spec.inputs.all (fun input => !input.isEmpty) &&
    !spec.acceptanceCondition.isEmpty &&
    !spec.trustedBoundary.isEmpty &&
    !spec.artifactIdentity.isEmpty &&
    spec.basis.wellFormed

structure Result where
  spec : Spec
  observedValue : String
  passed : Bool
deriving DecidableEq, Repr, BEq

def Result.wellFormed (result : Result) : Bool :=
  result.spec.wellFormed && !result.observedValue.isEmpty

def Result.currentFor (result : Result) (currentSpec : Spec)
    (currentDesign : List DesignRef) (currentWork : List WorkRef) : Bool :=
  result.wellFormed &&
    result.spec == currentSpec &&
    match currentSpec.basis with
    | .design items => items.all fun item => currentDesign.contains item.ref
    | .workBoundary work =>
        currentWork.any fun candidate => candidate.key == work.key

structure FormalSpec where
  key : String
  design : DesignRef
  modules : List String
  oracle : Option String := none
  implementationSurfaces : List String
  cases : List String := []
  adapter : Option String := none
deriving DecidableEq, Repr, BEq

private def formalName (value : String) : Bool :=
  !value.isEmpty &&
    !value.contains ',' &&
    !value.contains '\n' &&
    !value.contains '\r' &&
    !value.contains '\t'

def FormalSpec.wellFormed (spec : FormalSpec) : Bool :=
  formalName spec.key &&
    !spec.design.key.isEmpty &&
    !spec.modules.isEmpty &&
    spec.modules.all formalName &&
    spec.modules.Nodup &&
    spec.implementationSurfaces.all formalName &&
    match spec.oracle with
    | none => false
    | some oracle =>
        formalName oracle &&
          match spec.adapter with
          | none =>
              spec.implementationSurfaces.isEmpty && spec.cases.isEmpty
          | some adapter =>
              formalName adapter &&
                !spec.implementationSurfaces.isEmpty &&
                !spec.cases.isEmpty &&
                spec.cases.all formalName

structure FormalResult where
  spec : FormalSpec
  toolIdentity : String
  checkedClosure : List String
  checkedArtifacts : List String
  oracleArtifact : Option String
  conformancePassed : Option Bool
  semanticPreview : String
  previewIdentity : String
deriving DecidableEq, Repr, BEq

structure FormalResultIdentity where
  key : String
  design : DesignRef
  previewIdentity : String
deriving DecidableEq, Repr, BEq

def FormalResult.identity (result : FormalResult) : FormalResultIdentity :=
  { key := result.spec.key
    design := result.spec.design
    previewIdentity := result.previewIdentity }

inductive ConformanceOutcome
  | notSelected
  | conformant
  | counterexample
  | executionFailure
deriving DecidableEq, Repr, BEq

def FormalResult.conformanceOutcome
    (result : FormalResult) : ConformanceOutcome :=
  match result.spec.adapter, result.conformancePassed with
  | none, none => .notSelected
  | some _, some true => .conformant
  | some _, some false => .counterexample
  | some _, none => .executionFailure
  | none, some _ => .executionFailure

def FormalResult.currentFor (result : FormalResult)
    (currentSpec : FormalSpec) (currentDesign : List DesignRef) : Bool :=
  result.spec == currentSpec &&
    currentSpec.wellFormed &&
    currentDesign.contains currentSpec.design &&
    !result.toolIdentity.isEmpty &&
    !result.semanticPreview.isEmpty &&
    !result.previewIdentity.isEmpty &&
    currentSpec.modules.all result.checkedClosure.contains &&
    !result.checkedArtifacts.isEmpty &&
    result.checkedArtifacts.all (fun artifact => !artifact.isEmpty) &&
    match currentSpec.oracle, result.oracleArtifact,
        currentSpec.adapter, result.conformancePassed with
    | some _, some artifact, none, none => !artifact.isEmpty
    | some _, some artifact, some _, some _ => !artifact.isEmpty
    | some _, some artifact, some _, none => !artifact.isEmpty
    | _, _, _, _ => false

def FormalResult.conformsFor (result : FormalResult)
    (currentSpec : FormalSpec) (currentDesign : List DesignRef) : Bool :=
  result.currentFor currentSpec currentDesign &&
    match result.conformanceOutcome with
    | .notSelected | .conformant => true
    | .counterexample | .executionFailure => false

end AgentWorkbench.Domain.Evidence
