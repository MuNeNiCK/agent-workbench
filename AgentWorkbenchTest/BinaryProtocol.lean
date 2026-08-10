import AgentWorkbenchTest.Fixture
import AgentWorkbench.Cli.Describe

namespace AgentWorkbenchTest.BinaryProtocol

open AgentWorkbench AgentWorkbenchTest

private def executablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench.exe"
  else ".lake/build/bin/agent-workbench"

private def containsText (text fragment : String) : Bool :=
  (text.splitOn fragment).length > 1

private def invoke
    (root : System.FilePath) (operation : Operation)
    (input : Option Lean.Json) : IO IO.Process.Output :=
  IO.Process.output {
    cmd := executablePath.toString
    args := #["--project", root.toString] ++ (operation.name.splitOn " ").toArray }
    (input.map (·.compress))

private def rejectedBeforeSemanticDispatch (stderr : String) : Bool :=
  ["unknown command", "invalid JSON input", "invalid input for", "unknown fields",
    "missing native contract"].any (containsText stderr)

private partial def injectUnknownAt (path : List String) : Lean.Json → Lean.Json
  | .obj fields => match path with
      | [] => Lean.Json.mkObj (("inventedSystemField", Lean.Json.bool true) :: fields.toList)
      | key :: rest => Lean.Json.mkObj <| fields.toList.map fun (field, value) =>
          if field == key then (field, injectUnknownAt rest value) else (field, value)
  | .arr items => .arr (items.map (injectUnknownAt path))
  | value => value

private structure RevisionOnly where
  stateRevision : Nat
  deriving Lean.FromJson

private def currentRevision (root : System.FilePath) : IO Nat := do
  let output ← invoke root Operation.context none
  expect (output.exitCode == 0) s!"context failed: {output.stderr}"
  let json ← match Lean.Json.parse output.stdout with
    | .ok value => pure value
    | .error error => throw (IO.userError s!"invalid context JSON: {error}")
  match (Lean.fromJson? json : Except String RevisionOnly) with
  | .ok value => pure value.stateRevision
  | .error error => throw (IO.userError s!"context omitted state revision: {error}")

private def nestedDesignInput : DesignProposalRequest :=
  let statement : Statement := {
    id := "statement-1", text := "artifact must exist", assumptions := ["assumption-1"] }
  let criterion : AcceptanceCriterion := {
    id := "criterion-1", statementId := some statement.id
    statement := "artifact exists", target := "file:artifact.txt", evidenceKind := "artifact" }
  let claim : LeanClaim := {
    id := "claim-1"
    input := {
      statementId := statement.id, statementText := statement.text
      mapping := "the witness checks the selected proposition"
      proposition := "Example.Property", witness := "Example.property"
      assumptions := ["assumption-1"]
      proofRoot := ".agent-workbench/design/proofs/example"
      declaredSources := [{ path := "Example.lean", expectedDigest := some "blake3:digest" }]
      check := {
        executable := "lake", arguments := #["build"]
        workingDirectory := some ".", environment := #["PATH"] }
      toolchain := Runtime.toolchain } }
  {
    producerAgentRun := "agent-run-1", changeRationale := "record the Design"
    changeBasisEntryIds := ["correction-1"], amendsCandidate := some "design-1"
    sourceDocumentTargets := ["file:.agent-workbench/design/product/design.md"]
    sourceUnitDispositions := [{
      unitId := "source-unit-1", role := .requirement, reason := some "requirement" }]
    assumptions := [{
      id := "assumption-1", text := "the service is available"
      sourceUnitIds := ["source-unit-1"] }]
    statements := [statement]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := ["source-unit-1"]
      leanClaims := { selectedIds := [claim.id], noSelectionReason := none }
      acceptanceCriteria := { selectedIds := [criterion.id], noSelectionReason := none }
      implementationRequired := true, noImplementationReason := none }]
    removedStatements := [{
      statementId := "removed-statement", statementText := "old requirement"
      implementationRequired := false, noImplementationReason := some "superseded" }]
    acceptanceCriteria := [criterion], leanClaims := [claim]
    assuranceContracts := some [fixtureAssuranceInput statement [claim] [criterion]] }

private def nestedPlanInput : PlanProposalRequest := {
  predecessorPlanId := some "plan-1", producerAgentRun := "agent-run-1"
  reason := "implement the complete Design delta", changeBasisEntryIds := ["finding-1"]
  sourceDocumentTargets := ["file:.agent-workbench/design/plans/work-1/plan.md"]
  sourceUnitDispositions := [{ unitId := "plan-source-unit-1", stepId := some "step-1" }]
  statementDispositions := [{
    statementId := "statement-1", statementText := "artifact must exist"
    deltaKind := .added, stepIds := ["step-1"] }]
  steps := [{
    id := "step-1", description := "implement the Statement"
    dependsOnStepIds := ["step-0"], outputScopes := ["file:artifact.txt"]
    requiredClaimIds := ["claim-1"], verificationCriterionIds := ["criterion-1"]
    taskVerificationContracts := [{
      id := "verify-local-output", kind := .artifact, target := "file:artifact.txt" }]
    acceptedFindingEntryIds := ["finding-1"] }] }

private structure NestedArrayCase where
  path : List String
  renderedPath : String

private def designArrayCases : List NestedArrayCase := [
  { path := ["sourceUnitDispositions"], renderedPath := "sourceUnitDispositions[0]" },
  { path := ["assumptions"], renderedPath := "assumptions[0]" },
  { path := ["statements"], renderedPath := "statements[0]" },
  { path := ["statementCoverage"], renderedPath := "statementCoverage[0]" },
  { path := ["removedStatements"], renderedPath := "removedStatements[0]" },
  { path := ["acceptanceCriteria"], renderedPath := "acceptanceCriteria[0]" },
  { path := ["leanClaims"], renderedPath := "leanClaims[0]" },
  { path := ["assuranceContracts"], renderedPath := "assuranceContracts[0]" },
  { path := ["assuranceContracts", "witnesses"],
    renderedPath := "assuranceContracts[0].witnesses[0]" },
  { path := ["assuranceContracts", "counterexamples"],
    renderedPath := "assuranceContracts[0].counterexamples[0]" },
  { path := ["leanClaims", "input", "declaredSources"],
    renderedPath := "leanClaims[0].input.declaredSources[0]" }
]

private def planArrayCases : List NestedArrayCase := [
  { path := ["sourceUnitDispositions"], renderedPath := "sourceUnitDispositions[0]" },
  { path := ["statementDispositions"], renderedPath := "statementDispositions[0]" },
  { path := ["steps"], renderedPath := "steps[0]" },
  { path := ["steps", "taskVerificationContracts"],
    renderedPath := "steps[0].taskVerificationContracts[0]" }
]

private def expectNestedUnknownRejected
    (operation : Operation) (fixture : Lean.Json) (testCase : NestedArrayCase) : IO Unit :=
  IO.FS.withTempDir fun root => do
    let malformed := injectUnknownAt testCase.path fixture
    let missingOutput ← invoke root operation (some malformed)
    expect (missingOutput.exitCode != 0 &&
        containsText missingOutput.stderr s!"unknown fields for {operation.name}")
      (s!"missing project accepted nested uncontracted field at {operation.name}." ++
        s!"{testCase.renderedPath}: {missingOutput.stderr}")
    let database := root / ".agent-workbench" / "state.db"
    expect (!(← database.pathExists))
      s!"nested unknown field created state for {operation.name}.{testCase.renderedPath}"
    IO.FS.createDirAll (root / ".agent-workbench")
    let _ ← Store.open database
    let before ← currentRevision root
    let output ← invoke root operation (some malformed)
    expect (output.exitCode != 0 && containsText output.stderr s!"unknown fields for {operation.name}")
      (s!"public binary accepted nested uncontracted field at {operation.name}." ++
        s!"{testCase.renderedPath}: {output.stderr}")
    expect (containsText output.stderr s!"{testCase.renderedPath}.inventedSystemField")
      s!"rejection did not identify {operation.name}.{testCase.renderedPath}: {output.stderr}"
    let after ← currentRevision root
    expect (after == before)
      (s!"nested unknown field changed revision for {operation.name}." ++
        s!"{testCase.renderedPath}: {before} -> {after}")

def run : IO Unit := do
  let mutations := Operation.all.filter (·.kind == .mutation)
  for operation in mutations do
    let contract ← match AgentWorkbench.Cli.operationContract? operation.name with
      | some value => pure value
      | none => throw (IO.userError s!"mutation has no public binary contract: {operation.name}")
    IO.FS.withTempDir fun root => do
      let output ← invoke root operation contract.inputExample
      expect (!rejectedBeforeSemanticDispatch output.stderr)
        s!"public binary did not decode and dispatch mutation {operation.name}: {output.stderr}"
  IO.FS.withTempDir fun root => do
    let output ← IO.Process.output {
      cmd := executablePath.toString
      args := #["--project", root.toString, "work", "start"] }
      (some "{\"id\":\"work-1\",\"outcome\":\"x\",\"scope\":\"project\",\"responsibleAgentRun\":\"a\",\"invented\":true}")
    expect (output.exitCode != 0 && containsText output.stderr "unknown fields for work start")
      "public binary accepted an uncontracted mutation field"
  let designFixture := Lean.toJson nestedDesignInput
  for operation in [Operation.designPropose, Operation.designAmend] do
    for testCase in designArrayCases do
      expectNestedUnknownRejected operation designFixture testCase
  let planFixture := Lean.toJson nestedPlanInput
  for operation in [Operation.planPropose, Operation.planReplace] do
    for testCase in planArrayCases do
      expectNestedUnknownRejected operation planFixture testCase

end AgentWorkbenchTest.BinaryProtocol
