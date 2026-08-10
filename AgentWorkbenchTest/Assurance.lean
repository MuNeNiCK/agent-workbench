import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.StoreCodec

namespace AgentWorkbenchTest.Assurance

open AgentWorkbench AgentWorkbenchTest

private def fixtureDigestPlaceholder : String :=
  "blake3:0000000000000000000000000000000000000000000000000000000000000000"

private def fixtureCounterexample
    (failureClass : AssuranceFailureClass) (rejectedCondition : String) : AssuranceCounterexample :=
  { failureClass, rejectedCondition, positiveProperty := "the artifact exists"
    witnessIds := ["criterion-1"] }

/-- Requirement-authored oracle for the fixture Contract.  It deliberately does not call
`derivedAssuranceContracts`, `AssuranceFailureClass.all`, or `rejectedCondition`. -/
private def expectedFixtureContract : AssuranceContract := {
  designRevisionId := "design-1"
  assuranceEpoch := fixtureDigestPlaceholder
  statementId := "statement-1"
  statementText := "the artifact exists"
  statementTextDigest := fixtureDigestPlaceholder
  sourceUnitIds := ["unit-1"]
  criterionIds := ["criterion-1"]
  implementationRequired := true
  scope := [
    { kind := .designRevision, id := "design-1", binding := "design-1" },
    { kind := .statement, id := "statement-1", binding := "the artifact exists" },
    { kind := .sourceUnit, id := "unit-1", binding := "blake3:unit" },
    { kind := .acceptanceCriterion, id := "criterion-1"
      binding := "{\"evidenceKind\":\"artifact\",\"id\":\"criterion-1\",\"statement\":" ++
        "\"the artifact observation succeeds\",\"statementId\":\"statement-1\"," ++
        "\"target\":\"file:artifact.txt\"}" },
    { kind := .implementationChoice, id := "statement-1", binding := "required" }]
  scopeDigest := fixtureDigestPlaceholder
  witnesses := [{
    id := "criterion-1", kind := .externalObservation, checkpoint := .completion
    independenceClass := "independent-artifact-observer"
    dependencyIds := ["criterion-1", "statement-1", "file:artifact.txt", "artifact"]
    producerBoundary := "authored:external-artifact-observer" }]
  counterexamples := [
    fixtureCounterexample .missingScope "a required scope member is absent",
    fixtureCounterexample .extraScope "an undeclared scope member is present",
    fixtureCounterexample .duplicateScope "a scope member occurs more than once",
    fixtureCounterexample .crossDesignScope "a scope member is bound to another Design",
    fixtureCounterexample .staleScope "a scope member no longer matches immutable Design content",
    fixtureCounterexample .missingWitness "a critical property has no witness",
    fixtureCounterexample .staleWitness "a witness input or checkpoint is stale",
    fixtureCounterexample .selfReferentialWitness "a witness derives its own authority",
    fixtureCounterexample .duplicateWitness "one witness is presented more than once",
    fixtureCounterexample .commonCauseOnlyWitness
      "purportedly independent witnesses share one undeclared cause",
    fixtureCounterexample .positiveOnlyEvidence "positive evidence has no bound negative case",
    fixtureCounterexample .ungroundedCounterexample
      "a counterexample does not negate the bound property",
    fixtureCounterexample .prematureImplementation
      "productive authority exists before assurance closure",
    fixtureCounterexample .staleAssuranceEpoch
      "evidence predates the current assurance epoch",
    fixtureCounterexample .assuranceOmissionAsPatchAuthority
      "an assurance omission is used directly as implementation authority"] }

private def strictDesign : DesignRevision :=
  let base := { design with assuranceSchemaVersion := 1 }
  { base with assuranceContracts := [expectedFixtureContract] }

private def stateWithDesign (value : DesignRevision) : ProjectState :=
  { baseState with designRevisions := [value] }

private def expectInvalidMatrix (value : DesignRevision) (message : String) : IO Unit :=
  expectError (validateState (stateWithDesign value)) message

private def invalidContractFor
    (failureClass : AssuranceFailureClass) (contract : AssuranceContract) : AssuranceContract :=
  let firstScope := contract.scope.head?.getD {
    kind := .statement, id := contract.statementId, binding := contract.statementText }
  let firstWitness := contract.witnesses.head?.getD {
    id := "missing", kind := .externalObservation, checkpoint := .completion
    independenceClass := "missing", dependencyIds := ["missing"]
    producerBoundary := "missing" }
  match failureClass with
  | .missingScope => { contract with scope := contract.scope.dropLast }
  | .extraScope => { contract with scope := contract.scope ++ [{
      kind := .sourceUnit, id := "extra", binding := "extra" }] }
  | .duplicateScope => { contract with scope := contract.scope ++ [firstScope] }
  | .crossDesignScope => { contract with scope := (({ firstScope with
      kind := .designRevision, id := "design-other", binding := "design-other" }) ::
      contract.scope.drop 1) }
  | .staleScope => { contract with scope := (({ firstScope with binding := "stale" }) ::
      contract.scope.drop 1) }
  | .missingWitness => { contract with witnesses := [] }
  | .staleWitness => { contract with witnesses := (({ firstWitness with
      dependencyIds := ["stale"] }) :: contract.witnesses.drop 1) }
  | .selfReferentialWitness => { contract with witnesses := (({ firstWitness with
      producerBoundary := "" }) :: contract.witnesses.drop 1) }
  | .duplicateWitness => { contract with witnesses := contract.witnesses ++ [firstWitness] }
  | .commonCauseOnlyWitness => { contract with witnesses := [firstWitness, {
      firstWitness with id := firstWitness.id ++ "-alias" }] }
  | .positiveOnlyEvidence => { contract with counterexamples := (
      contract.counterexamples.filter fun value =>
        value.failureClass != .positiveOnlyEvidence) }
  | .ungroundedCounterexample => { contract with counterexamples := (
      contract.counterexamples.map fun counterexample =>
        if counterexample.failureClass == .ungroundedCounterexample then
          { counterexample with positiveProperty := "another property" }
        else counterexample) }
  | .prematureImplementation => { contract with implementationRequired := false }
  | .staleAssuranceEpoch => { contract with assuranceEpoch := "design-stale" }
  | .assuranceOmissionAsPatchAuthority => { contract with counterexamples := (
      contract.counterexamples.filter fun value =>
        value.failureClass != .assuranceOmissionAsPatchAuthority) }

def run : IO Unit := do
  let closed := strictDesign
  fromExcept (validateState (stateWithDesign closed))
  expect (closed.assuranceContracts == [expectedFixtureContract])
    "product Contract derivation differs from the independent requirement-authored fixture"
  expect (closed.assuranceContracts.length == 1 && closed.assuranceClosed)
    "strict Design did not persist its exact Statement Contract universe"
  let noncritical := withCurrentAssurance { design with
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { noSelectionReason := some "no logical Claim is required" }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := false
      noImplementationReason := some "the existing behavior is observed without product work" }] }
  expect (noncritical.assuranceContracts.length == 1 && noncritical.assuranceClosed &&
    noncritical.assuranceContracts.head?.any fun contract =>
      !contract.implementationRequired && contract.counterexamples.isEmpty)
    "noncritical Statement did not retain its immutable nonproductive Contract"
  let legacy := { design with assuranceSchemaVersion := 0, assuranceContracts := [] }
  expect (legacy.assuranceSchemaVersion == 0 && legacy.assuranceContracts.isEmpty &&
    legacy.effectiveAssuranceContracts == legacy.derivedAssuranceContracts &&
    !legacy.assuranceClosed)
    "legacy Design was not confined to a nonproductive read boundary"
  let silentlyMissingScopeSource := withCurrentAssurance { design with sourceUnits := [] }
  expect (!silentlyMissingScopeSource.assuranceClosed)
    "Assurance closure silently dropped a selected missing scope source"

  expectInvalidMatrix { closed with assuranceContracts := [] }
    "version-one Design accepted a missing Assurance Contract"
  expectInvalidMatrix { closed with
      assuranceContracts := closed.assuranceContracts ++ closed.assuranceContracts }
    "version-one Design accepted a duplicate/extra Assurance Contract"
  let contract ← match closed.assuranceContracts.head? with
    | some value => pure value
    | none => throw (IO.userError "strict Assurance fixture has no Contract")
  expect (Validation.validContentDigest contract.statementTextDigest &&
      Validation.validContentDigest contract.scopeDigest &&
      Validation.validContentDigest contract.assuranceEpoch &&
      contract.assuranceEpoch != closed.id)
    "Contract identities are not cryptographic text, scope, and materialization digests"
  let materialized := closed.materializeAssuranceContracts ContentDigest.string
  let materializedContract ← match materialized.assuranceContracts.head? with
    | some value => pure value
    | none => throw (IO.userError "materialized Assurance fixture has no Contract")
  expect (materialized.assuranceClosed &&
      materializedContract.assuranceEpoch != contract.assuranceEpoch &&
      materializedContract.statementTextDigest != contract.statementTextDigest &&
      materializedContract.scopeDigest != contract.scopeDigest)
    "production Assurance materialization retained structural digest placeholders"
  let decoded ← AgentWorkbench.Store.Codec.decodeDesign
    (AgentWorkbench.Store.Codec.encode materialized)
  expect (decoded == materialized)
    "schema-one Assurance Contract did not round-trip with exact digest identities"
  let forgedDigest := "blake3:" ++ String.ofList (List.replicate 64 '1')
  let forged := { materialized with assuranceContracts := [{ materializedContract with
    scopeDigest := forgedDigest }] }
  let forgedRejected ← try
    let _ ← AgentWorkbench.Store.Codec.decodeDesign
      (AgentWorkbench.Store.Codec.encode forged)
    pure false
  catch _ => pure true
  expect forgedRejected
    "persisted Assurance Contract accepted a well-shaped but false scope digest"
  for failureClass in AssuranceFailureClass.all do
    expectInvalidMatrix { closed with
        assuranceContracts := [invalidContractFor failureClass contract] }
      s!"version-one Design accepted the negative Assurance route {reprStr failureClass}"
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      statementText := "stale Statement text" }] }
    "version-one Design accepted stale Contract scope"
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      statementTextDigest := "blake3:stale-statement-text" }] }
    "version-one Design accepted a stale Statement-text digest"
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      designRevisionId := "design-copied", assuranceEpoch := "design-copied@0" }] }
    "version-one Design accepted a cross-Design copied Contract"
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      scopeDigest := "blake3:stale-scope" }] }
    "version-one Design accepted a stale scope digest"
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      counterexamples := contract.counterexamples.dropLast }] }
    "version-one Design accepted an incomplete counterexample partition"
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      witnesses := [] }] }
    "version-one Design accepted a Contract without an independent witness"
  let duplicateIndependence := match contract.witnesses with
    | first :: second :: rest => first :: { second with
        independenceClass := first.independenceClass } :: rest
    | [first] => [first, first]
    | [] => []
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      witnesses := duplicateIndependence }] }
    "version-one Design accepted duplicate/common-cause witness independence"

  let siblingCriterion : AcceptanceCriterion := { criterion with
    id := "criterion-2"
    statement := "the same target passes an independently identified check" }
  let sameTargetDesign := withCurrentAssurance { design with
    acceptanceCriteria := [criterion, siblingCriterion]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { noSelectionReason := some "no logical Claim is required" }
      acceptanceCriteria := { selectedIds := [criterion.id, siblingCriterion.id] }
      implementationRequired := true }] }
  fromExcept (validateState (stateWithDesign sameTargetDesign))
  let siblingContract ← match sameTargetDesign.assuranceContracts.head? with
    | some value => pure value
    | none => throw (IO.userError "same-target Assurance fixture has no Contract")
  expect (siblingContract.witnesses.length == 2 &&
      (siblingContract.witnesses.map (·.independenceClass)).eraseDups.length == 2)
    "distinct same-target Criteria collapsed into one witness independence identity"
  let forgedCommonCause := match siblingContract.witnesses with
    | first :: second :: rest => first :: { second with
        independenceClass := first.independenceClass } :: rest
    | witnesses => witnesses
  expectInvalidMatrix { sameTargetDesign with assuranceContracts := [{
      siblingContract with witnesses := forgedCommonCause }] }
    "same-target Contract accepted a forged duplicate independence identity"
  let ungrounded := contract.counterexamples.map fun counterexample =>
    if counterexample.failureClass == .ungroundedCounterexample then
      { counterexample with positiveProperty := "another property" }
    else counterexample
  expectInvalidMatrix { closed with assuranceContracts := [{ contract with
      counterexamples := ungrounded }] }
    "version-one Design accepted an ungrounded counterexample"

  let unboundEvidence : LedgerEntry := { evidenceEntry with
    payload := .artifactObservation {
      taskEntryId := some taskEntry.id, outputScope := some criterion.target
      criterionId := some criterion.id, target := criterion.target
      snapshot := "blake3:artifact", operation := "inspect artifact"
      result := "artifact exists", successful := true
      producerAgentRun := work.responsibleAgentRun, assuranceBinding := none } }
  expectError (validateState { evidencedState with
      ledgerEntries := [taskEntry, unboundEvidence] })
    "schema-one evidence without an Assurance binding remained valid in the ledger"
  let staleEvidence : LedgerEntry := { evidenceEntry with
    payload := .artifactObservation {
      taskEntryId := some taskEntry.id, outputScope := some criterion.target
      criterionId := some criterion.id, target := criterion.target
      snapshot := "blake3:artifact", operation := "inspect artifact"
      result := "artifact exists", successful := true
      producerAgentRun := work.responsibleAgentRun
      assuranceBinding := some { (design.assuranceBindingForCriterion
        work.responsibleAgentRun criterion.id) with assuranceEpoch := "design-stale" } } }
  expectError (validateState { evidencedState with
      ledgerEntries := [taskEntry, staleEvidence] })
    "stale Assurance-bound evidence remained valid in the ledger"

  let request : DesignProposalRequest := {
    producerAgentRun := "assurance-proposer"
    changeRationale := "derive the exact Assurance Contract before implementation"
    sourceUnitDispositions := [{ unitId := sourceUnit.id, role := .requirement }]
    statements := [statement]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { noSelectionReason := some "no logical Claim is required" }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }]
    acceptanceCriteria := [criterion]
    assuranceContracts := some [{
      statementId := statement.id
      witnesses := expectedFixtureContract.witnesses.map fun witness => {
        id := witness.id, independenceClass := witness.independenceClass
        producerBoundary := witness.producerBoundary }
      counterexamples := expectedFixtureContract.counterexamples }] }
  let proposed := request.design baseState work.id design.sourceDocuments [sourceUnit]
  expect (proposed.assuranceSchemaVersion == 1 &&
    proposed.assuranceContracts.head?.any (fun contract =>
      contract.trustedBoundaryAssumptionIds == expectedFixtureContract.trustedBoundaryAssumptionIds &&
      contract.witnesses == expectedFixtureContract.witnesses &&
      contract.counterexamples == expectedFixtureContract.counterexamples) &&
    proposed.assuranceContracts != proposed.derivedAssuranceContracts &&
    proposed.assuranceClosed)
    "current proposal route did not preserve the independently authored Contract input"

  expect (!operationStructurallyApplicable (stateWithDesign { closed with
      assuranceContracts := [] }) .planMaterialize)
    "Plan materialization remained structurally productive with a missing Contract"
  expect (!operationStructurallyApplicable (stateWithDesign { closed with
      assuranceContracts := [] }) .profileDefine)
    "productive Task operations remained open with a missing Contract"

  IO.FS.withTempDir fun root => do
    let rejected ← try
      let _ ← startReview root baseState [] [] {
        entryId := "review-before-ready", reviewId := "review-before-ready"
        purpose := .implementation, reviewerAgentRun := "independent-reviewer" }
      pure false
    catch _ => pure true
    expect rejected "Implementation Review started before independent readiness"

  let finding : LedgerEntry := {
    id := "finding-assurance-omission", order := 20, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .finding {
      reviewId := "review-assurance", subject := {
        kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
      targetSourceId := work.id, target := s!"work:{work.id}"
      targetSnapshot := "blake3:review", producerAgentRuns := [work.responsibleAgentRun]
      summary := "the assurance partition omitted a counterexample" } }
  let omission : LedgerEntry := {
    id := "disposition-assurance-omission", order := 21, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .reviewDisposition {
      findingEntryId := finding.id, decision := .accepted, impact := .assuranceOmission
      impactSchemaVersion := 1
      reason := "return through successor Design authority"
      decidedByRun := work.responsibleAgentRun } }
  let omittedState := { baseState with ledgerEntries := [taskEntry, finding, omission] }
  expect (acceptedAssuranceOmissionForDesign omittedState design.id &&
    !designAssuranceStructurallyCurrent omittedState design &&
    !(acceptedImplementationFindingIds omittedState work.id design.id).contains finding.id &&
    !operationStructurallyApplicable omittedState .profileDefine)
    "accepted assurance omission could still authorize a Plan or productive operation"
  let parent := { design with status := DesignStatus.superseded }
  let successorBase : DesignRevision := withCurrentAssurance { design with
    id := "design-assurance-successor", parent := some parent.id
    status := .accepted, createdAfterEntryOrder := 22 }
  let returnState : ProjectState := { omittedState with
    acceptedDesignId := some successorBase.id
    designRevisions := [parent, successorBase]
    works := [{ work with designRevision := some successorBase.id }] }
  expectError (Validation.validateDesignRelations returnState successorBase)
    "successor Design omitted its accepted assurance-omission causal basis"
  let groundedSuccessor := { successorBase with changeBasisEntryIds := [finding.id] }
  let groundedState := { returnState with designRevisions := [parent, groundedSuccessor] }
  fromExcept (Validation.validateDesignRelations groundedState groundedSuccessor)

end AgentWorkbenchTest.Assurance
