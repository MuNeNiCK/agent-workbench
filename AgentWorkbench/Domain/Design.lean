import Lean.Data.Json
import AgentWorkbench.Domain.ProofToolchain
import AgentWorkbench.Domain.DesignSourceGraph

namespace AgentWorkbench

inductive DesignStatus where
  | candidate
  | accepted
  | superseded
  /-- `replaced` is legacy; `rejected` is an explicit non-authoritative terminal status. -/
  | replaced
  | rejected
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Statement where
  id : String
  text : String
  assumptions : List String := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignSource where
  target : String
  mediaKind : String := "markdown"
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure AcceptanceCriterion where
  id : String
  statementId : Option String := none
  statement : String
  target : String
  evidenceKind : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure SourceInput where
  path : String
  expectedDigest : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CommandSpec where
  executable : String
  arguments : Array String := #[]
  workingDirectory : Option String := none
  /-- Names inherited from the caller environment. Values are never persisted. -/
  environment : Array String := #[]
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ClaimInput where
  statementId : String
  statementText : String
  mapping : String
  proposition : String
  witness : String
  assumptions : List String := []
  proofRoot : String
  declaredSources : List SourceInput
  check : CommandSpec
  toolchain : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure LeanClaim where
  id : String
  input : ClaimInput
  elaboratedPropositionDigest : String := ""
  propositionDependencies : List String := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- Closed classes of ways a pre-implementation assurance contract can be unsound.  Keeping this
as an inductive (rather than caller supplied strings) makes additions a schema and proof change. -/
inductive AssuranceFailureClass where
  | missingScope
  | extraScope
  | duplicateScope
  | crossDesignScope
  | staleScope
  | missingWitness
  | staleWitness
  | selfReferentialWitness
  | duplicateWitness
  | commonCauseOnlyWitness
  | positiveOnlyEvidence
  | ungroundedCounterexample
  | prematureImplementation
  | staleAssuranceEpoch
  | assuranceOmissionAsPatchAuthority
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def AssuranceFailureClass.all : List AssuranceFailureClass :=
  [.missingScope, .extraScope, .duplicateScope, .crossDesignScope, .staleScope,
   .missingWitness, .staleWitness, .selfReferentialWitness, .duplicateWitness,
   .commonCauseOnlyWitness, .positiveOnlyEvidence, .ungroundedCounterexample,
   .prematureImplementation, .staleAssuranceEpoch, .assuranceOmissionAsPatchAuthority]

def AssuranceFailureClass.rejectedCondition : AssuranceFailureClass → String
  | .missingScope => "a required scope member is absent"
  | .extraScope => "an undeclared scope member is present"
  | .duplicateScope => "a scope member occurs more than once"
  | .crossDesignScope => "a scope member is bound to another Design"
  | .staleScope => "a scope member no longer matches immutable Design content"
  | .missingWitness => "a critical property has no witness"
  | .staleWitness => "a witness input or checkpoint is stale"
  | .selfReferentialWitness => "a witness derives its own authority"
  | .duplicateWitness => "one witness is presented more than once"
  | .commonCauseOnlyWitness => "purportedly independent witnesses share one undeclared cause"
  | .positiveOnlyEvidence => "positive evidence has no bound negative case"
  | .ungroundedCounterexample => "a counterexample does not negate the bound property"
  | .prematureImplementation => "productive authority exists before assurance closure"
  | .staleAssuranceEpoch => "evidence predates the current assurance epoch"
  | .assuranceOmissionAsPatchAuthority =>
      "an assurance omission is used directly as implementation authority"

inductive AssuranceScopeKind where
  | designRevision
  | statement
  | assumption
  | sourceUnit
  | leanClaim
  | acceptanceCriterion
  | implementationChoice
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- One exact member of the closed same-Design scope. `binding` is immutable Design-owned content,
not a caller label; changing content therefore changes both the member and the scope digest. -/
structure AssuranceScopeMember where
  kind : AssuranceScopeKind
  id : String
  binding : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive AssuranceWitnessKind where
  | leanKernel
  | isolatedCommand
  | externalObservation
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive AssuranceCheckpoint where
  | preImplementation
  | completion
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- A Design-owned witness declaration. Independence and dependency identity are explicit so two
aliases of one witness cannot be counted as independent families. -/
structure AssuranceWitness where
  id : String
  kind : AssuranceWitnessKind
  checkpoint : AssuranceCheckpoint
  independenceClass : String
  dependencyIds : List String
  producerBoundary : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- A negative obligation is not merely a failure-class name: it records the rejected condition,
the exact positive property negated, and the witnesses that discharge that obligation. -/
structure AssuranceCounterexample where
  failureClass : AssuranceFailureClass
  rejectedCondition : String
  positiveProperty : String
  witnessIds : List String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- Immutable closure of one Statement. Critical (`implementationRequired`) Contracts additionally
require active witnesses and the complete negative partition before productive authority. -/
structure AssuranceContract where
  designRevisionId : String
  assuranceEpoch : String
  statementId : String
  statementText : String
  statementTextDigest : String := ""
  assumptionIds : List String := []
  trustedBoundaryAssumptionIds : List String := []
  sourceUnitIds : List String
  claimIds : List String := []
  criterionIds : List String := []
  implementationRequired : Bool
  scope : List AssuranceScopeMember
  scopeDigest : String
  witnesses : List AssuranceWitness
  counterexamples : List AssuranceCounterexample
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure AssuranceWitnessInput where
  id : String
  independenceClass : String
  producerBoundary : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- The Design author supplies the judgment-bearing part of one Contract.  Workbench derives the
same-Design scope, witness kind/checkpoint/input closure, and immutable identities, but it must not
invent witness independence, trusted boundaries, producer boundaries, or counterexample bindings
from the positive requirement it is meant to check. -/
structure AssuranceContractInput where
  statementId : String
  trustedBoundaryAssumptionIds : List String := []
  witnesses : List AssuranceWitnessInput
  counterexamples : List AssuranceCounterexample
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- Exact assurance identity carried by a proof receipt or observable evidence record. It prevents
evidence for another Design, epoch, Contract matrix, scope, or counterexample partition from being
reused merely because its Criterion or Claim ID happens to match. -/
structure AssuranceEvidenceBinding where
  designRevisionId : String
  assuranceEpoch : String
  contractStatementIds : List String
  scopeDigests : List String
  witnessIds : List String
  counterexampleBindings : List String
  producerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignRevision where
  id : String
  workId : Option String := none
  parent : Option String := none
  amendsCandidate : Option String := none
  createdAfterEntryOrder : Nat := 0
  status : DesignStatus := .candidate
  producerAgentRun : String
  changeRationale : String := "legacy source unavailable"
  changeBasisEntryIds : List String := []
  revisionContentDigest : String := ""
  sourceArchiveAvailable : Bool := false
  sourceDocuments : List DesignSource := []
  sourceUnits : List DesignSourceUnit := []
  sourceUnitDispositions : List SourceUnitDisposition := []
  assumptions : List DesignAssumption := []
  statements : List Statement
  statementCoverage : List StatementCoverage := []
  removedStatements : List RemovedStatementTombstone := []
  acceptanceCriteria : List AcceptanceCriterion
  leanClaims : List LeanClaim := []
  /-- Version zero is the read-only migration boundary for Designs persisted before contracts. -/
  assuranceSchemaVersion : Nat := 0
  assuranceContracts : List AssuranceContract := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def DesignRevision.assuranceScope
    (design : DesignRevision) (statement : Statement)
    (coverage : StatementCoverage) : List AssuranceScopeMember :=
  let assumptions := statement.assumptions.filterMap fun id =>
    design.assumptions.find? (·.id == id) |>.map fun value =>
      { kind := .assumption, id := value.id, binding := value.text }
  let sources := coverage.sourceUnitIds.filterMap fun id =>
    design.sourceUnits.find? (·.id == id) |>.map fun value =>
      { kind := .sourceUnit, id := value.id, binding := value.digest }
  let claims := coverage.leanClaims.selectedIds.filterMap fun id =>
    design.leanClaims.find? (·.id == id) |>.map fun value =>
      { kind := .leanClaim, id := value.id, binding := value.elaboratedPropositionDigest }
  let criteria := coverage.acceptanceCriteria.selectedIds.filterMap fun id =>
    design.acceptanceCriteria.find? (·.id == id) |>.map fun value =>
      { kind := .acceptanceCriterion, id := value.id
        binding := (Lean.toJson value).compress }
  [{ kind := .designRevision, id := design.id,
      binding := design.id },
    { kind := .statement, id := statement.id, binding := statement.text }] ++
    assumptions ++ sources ++ claims ++ criteria ++
    [{ kind := .implementationChoice, id := statement.id,
        binding := if coverage.implementationRequired then "required"
          else coverage.noImplementationReason.getD "no implementation" }]

private def DesignRevision.assuranceWitnesses
    (design : DesignRevision) (coverage : StatementCoverage) : List AssuranceWitness :=
  let claims := coverage.leanClaims.selectedIds.filterMap fun id =>
    design.leanClaims.find? (·.id == id) |>.map fun claim =>
      let dependencies := claim.input.declaredSources.map fun source =>
        s!"{source.path}@{source.expectedDigest.getD "unresolved"}"
      { id := claim.id, kind := .leanKernel, checkpoint := .preImplementation
        independenceClass := "lean:" ++ claim.elaboratedPropositionDigest ++ ":" ++
          (Lean.toJson dependencies).compress
        dependencyIds := dependencies
        producerBoundary := s!"claim:{claim.id}:pinned-kernel:{claim.input.toolchain}" }
  let criteria := coverage.acceptanceCriteria.selectedIds.filterMap fun id =>
    design.acceptanceCriteria.find? (·.id == id) |>.map fun criterion =>
      { id := criterion.id
        kind := if criterion.evidenceKind == "command" then .isolatedCommand
          else .externalObservation
        checkpoint := .completion
        independenceClass := "criterion:" ++ criterion.id ++ ":" ++
          (Lean.toJson criterion).compress
        dependencyIds := [criterion.id, criterion.statementId.getD "unbound-statement",
          criterion.target, criterion.evidenceKind]
        producerBoundary := s!"criterion:{criterion.id}:current-task-evidence-producer" }
  claims ++ criteria

private def assuranceDigestPlaceholder : String :=
  "blake3:0000000000000000000000000000000000000000000000000000000000000000"

/-- Canonical material for one assurance materialization.  The Adapter hashes this value before a
Design can enter persistent production state.  Keeping material construction pure lets the Domain
proofs cover the exact relation without importing a foreign hashing implementation. -/
def DesignRevision.assuranceEpochMaterial (design : DesignRevision) : String :=
  (Lean.toJson { design with
    status := .candidate, revisionContentDigest := "", assuranceContracts := [] }).compress

def DesignRevision.derivedAssuranceContracts (design : DesignRevision) : List AssuranceContract :=
  design.statementCoverage.filterMap fun coverage => do
    let statement ← design.statements.find? (·.id == coverage.statementId)
    let scope := design.assuranceScope statement coverage
    let witnesses := design.assuranceWitnesses coverage
    some {
      designRevisionId := design.id
      assuranceEpoch := assuranceDigestPlaceholder
      statementId := statement.id
      statementText := statement.text
      statementTextDigest := assuranceDigestPlaceholder
      assumptionIds := statement.assumptions
      trustedBoundaryAssumptionIds := statement.assumptions
      sourceUnitIds := coverage.sourceUnitIds
      claimIds := coverage.leanClaims.selectedIds
      criterionIds := coverage.acceptanceCriteria.selectedIds
      implementationRequired := coverage.implementationRequired
      scope := scope
      scopeDigest := assuranceDigestPlaceholder
      witnesses := witnesses
      counterexamples := if coverage.implementationRequired then
        AssuranceFailureClass.all.map fun failureClass => {
          failureClass
          rejectedCondition := failureClass.rejectedCondition
          positiveProperty := statement.text
          witnessIds := witnesses.map (·.id) }
        else [] }

/-- Combine system-derived closed scope with the independently authored assurance judgments.
Authored Contract and witness identifiers must be an exact permutation of the derived universes;
rejecting here prevents an extra input from being normalized away before Design validation. -/
def DesignRevision.assuranceContractsFromInputs
    (design : DesignRevision) (inputs : List AssuranceContractInput) :
    Except String (List AssuranceContract) := do
  let derived := design.derivedAssuranceContracts
  let expectedStatementIds := derived.map (·.statementId)
  let authoredStatementIds := inputs.map (·.statementId)
  if authoredStatementIds.mergeSort (· < ·) != expectedStatementIds.mergeSort (· < ·) ||
      authoredStatementIds.eraseDups != authoredStatementIds then
    throw "Assurance Contract inputs must contain every derived Statement ID exactly once and no others"
  let mut contracts := []
  for contract in derived do
    let input ← match inputs.find? (·.statementId == contract.statementId) with
      | some value => pure value
      | none => throw s!"missing Assurance Contract input {contract.statementId}"
    let expectedWitnessIds := contract.witnesses.map (·.id)
    let authoredWitnessIds := input.witnesses.map (·.id)
    if authoredWitnessIds.mergeSort (· < ·) != expectedWitnessIds.mergeSort (· < ·) ||
        authoredWitnessIds.eraseDups != authoredWitnessIds then
      throw s!"Assurance Contract {contract.statementId} must contain every derived witness ID exactly once and no others"
    let mut witnesses := []
    for expected in contract.witnesses do
      let authored ← match input.witnesses.find? (·.id == expected.id) with
        | some value => pure value
        | none => throw s!"missing Assurance witness input {expected.id}"
      witnesses := witnesses ++ [{ expected with
        independenceClass := authored.independenceClass
        producerBoundary := authored.producerBoundary }]
    contracts := contracts ++ [{ contract with
        trustedBoundaryAssumptionIds := input.trustedBoundaryAssumptionIds
        witnesses := witnesses
        counterexamples := input.counterexamples }]
  pure contracts

/-- Replace the proof-friendly digest placeholders with the production digest of each exact
canonical input.  Production adapters call this with the pinned BLAKE3 implementation. -/
def DesignRevision.materializeAssuranceContracts
    (design : DesignRevision) (digest : String → String) : DesignRevision :=
  let epoch := digest design.assuranceEpochMaterial
  let contracts := design.assuranceContracts.map fun contract => { contract with
    assuranceEpoch := epoch
    statementTextDigest := digest contract.statementText
    scopeDigest := digest (Lean.toJson contract.scope).compress }
  { design with assuranceContracts := contracts }

/-- Only schema-zero persisted Designs may use deterministic legacy derivation.  Every Design
created by the current proposal route persists the exact version-one matrix. -/
def DesignRevision.effectiveAssuranceContracts (design : DesignRevision) : List AssuranceContract :=
  if design.assuranceSchemaVersion == 0 && design.assuranceContracts.isEmpty then
    design.derivedAssuranceContracts
  else design.assuranceContracts

private def validAssuranceDigest (value : String) : Bool :=
  value.startsWith "blake3:" && value.length == 71

private def AssuranceContract.withDigestPlaceholders
    (contract : AssuranceContract) : AssuranceContract :=
  { contract with
    assuranceEpoch := assuranceDigestPlaceholder
    statementTextDigest := assuranceDigestPlaceholder
    scopeDigest := assuranceDigestPlaceholder }

def DesignRevision.assuranceClosed (design : DesignRevision) : Bool :=
  let contracts := design.effectiveAssuranceContracts
  design.assuranceSchemaVersion == 1 &&
    contracts.length == design.statementCoverage.length &&
    contracts.all (fun contract =>
      (design.derivedAssuranceContracts.find? (·.statementId == contract.statementId)).any
        fun expected => contract.withDigestPlaceholders == { expected with
          trustedBoundaryAssumptionIds := contract.trustedBoundaryAssumptionIds
          witnesses := contract.witnesses
          counterexamples := contract.counterexamples }) &&
    contracts.map (·.statementId) == design.statementCoverage.map (·.statementId) &&
    contracts.all fun contract =>
      let expectedScopeLength := 3 + contract.assumptionIds.length +
        contract.sourceUnitIds.length + contract.claimIds.length + contract.criterionIds.length
      validAssuranceDigest contract.assuranceEpoch &&
      validAssuranceDigest contract.statementTextDigest &&
      validAssuranceDigest contract.scopeDigest &&
      contract.scope.length == expectedScopeLength &&
      contract.scope.eraseDups == contract.scope &&
      contract.scope.all (fun member => !member.id.isEmpty && !member.binding.isEmpty) &&
      contract.trustedBoundaryAssumptionIds.eraseDups ==
        contract.trustedBoundaryAssumptionIds &&
      contract.trustedBoundaryAssumptionIds.all contract.assumptionIds.contains &&
      if contract.implementationRequired then
        !contract.witnesses.isEmpty &&
        contract.witnesses.all (fun witness =>
          !witness.id.isEmpty && !witness.independenceClass.isEmpty &&
            !witness.dependencyIds.isEmpty &&
            witness.dependencyIds.eraseDups == witness.dependencyIds &&
            !witness.producerBoundary.isEmpty &&
            (contract.claimIds ++ contract.criterionIds).contains witness.id &&
            ((design.derivedAssuranceContracts.find?
              (·.statementId == contract.statementId)).bind
                (fun expected => expected.witnesses.find? (·.id == witness.id))).any
              (fun expected =>
                witness.kind == expected.kind &&
                witness.checkpoint == expected.checkpoint &&
                witness.dependencyIds == expected.dependencyIds)) &&
        (contract.witnesses.map (·.id)).eraseDups == contract.witnesses.map (·.id) &&
        (contract.witnesses.map (·.id)).mergeSort (· < ·) ==
          (contract.claimIds ++ contract.criterionIds).mergeSort (· < ·) &&
        (contract.witnesses.map (·.independenceClass)).eraseDups ==
          contract.witnesses.map (·.independenceClass) &&
        (contract.counterexamples.map (·.failureClass) == AssuranceFailureClass.all) &&
        contract.counterexamples.all (fun counterexample =>
          !counterexample.rejectedCondition.isEmpty &&
            counterexample.positiveProperty == contract.statementText &&
            !counterexample.witnessIds.isEmpty &&
            counterexample.witnessIds.all (contract.witnesses.map (·.id)).contains)
      else contract.counterexamples.isEmpty

def DesignRevision.assuranceEvidenceBinding
    (design : DesignRevision) (producerAgentRun : String)
    (selects : AssuranceContract → Bool) : AssuranceEvidenceBinding :=
  let contracts := design.effectiveAssuranceContracts.filter selects
  { designRevisionId := design.id
    assuranceEpoch := contracts.head?.map (·.assuranceEpoch) |>.getD ""
    contractStatementIds := contracts.map (·.statementId)
    scopeDigests := contracts.map (·.scopeDigest)
    witnessIds := contracts.flatMap (·.witnesses.map (·.id)) |>.eraseDups
    counterexampleBindings := contracts.flatMap fun contract =>
      contract.counterexamples.map fun counterexample => (Lean.toJson counterexample).compress
    producerAgentRun }

def DesignRevision.assuranceBindingForCriterion
    (design : DesignRevision) (producerAgentRun criterionId : String) : AssuranceEvidenceBinding :=
  design.assuranceEvidenceBinding producerAgentRun (·.criterionIds.contains criterionId)

def DesignRevision.assuranceBindingForClaim
    (design : DesignRevision) (producerAgentRun claimId : String) : AssuranceEvidenceBinding :=
  design.assuranceEvidenceBinding producerAgentRun (·.claimIds.contains claimId)

def DesignRevision.assuranceBindingForTask
    (design : DesignRevision) (producerAgentRun : String) : AssuranceEvidenceBinding :=
  design.assuranceEvidenceBinding producerAgentRun (·.implementationRequired)

def DesignRevision.assuranceBindingCurrentForCriterion
    (design : DesignRevision) (producerAgentRun criterionId : String)
    (binding : Option AssuranceEvidenceBinding) : Bool :=
  let expected := design.assuranceBindingForCriterion producerAgentRun criterionId
  !expected.contractStatementIds.isEmpty && binding == some expected

def DesignRevision.assuranceBindingCurrentForClaim
    (design : DesignRevision) (producerAgentRun claimId : String)
    (binding : Option AssuranceEvidenceBinding) : Bool :=
  let expected := design.assuranceBindingForClaim producerAgentRun claimId
  !expected.contractStatementIds.isEmpty && binding == some expected

def DesignRevision.assuranceBindingCurrentForTask
    (design : DesignRevision) (producerAgentRun : String)
    (binding : Option AssuranceEvidenceBinding) : Bool :=
  let expected := design.assuranceBindingForTask producerAgentRun
  !expected.contractStatementIds.isEmpty && binding == some expected

end AgentWorkbench
