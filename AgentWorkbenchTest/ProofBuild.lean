import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbenchProof.ProductionGrounding

namespace AgentWorkbenchTest.ProofBuild

open AgentWorkbench

private def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw (IO.userError message)

private def result (exitCode : Nat) : Process.Result :=
  { exitCode, stdout := "", stderr := "", stdoutDigest := "", stderrDigest := "" }

private def baseline (directory : System.FilePath) (existed : Bool) :
    AgentWorkbench.ProofBuild.OutputBaseline :=
  { directory, existed, parentExisted := true }

private def layouts (value : AgentWorkbench.ProofBuild.OutputBaseline) (token : String) :=
  AgentWorkbench.ProofBuild.outputLayouts [value] token

private def directoryEntries (path : System.FilePath) : IO (List String) := do
  let entries ← path.readDir
  pure (entries.toList.map (·.fileName) |>.mergeSort (· < ·))

private def exerciseExistingOutput (parent : System.FilePath) : IO Unit := do
  let output := parent / "existing"
  IO.FS.createDirAll output
  IO.FS.writeFile (output / "preserved") "original output"
  let before ← directoryEntries parent
  let (buildOutput, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs
    (← layouts (baseline output true) "existing")
    (do
      expect (!(← output.pathExists)) "existing output remained visible during rebuild"
      IO.FS.createDirAll (output / "lib" / "lean")
      IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
      pure (result 0, ()))
    (fun _ paths => do
      expect (!(← output.pathExists)) "normal output became visible during isolated check"
      let some leanPath := paths.head?
        | throw (IO.userError "isolated check received no Lean output path")
      expect ((← (leanPath / "Fresh.olean").pathExists))
        "isolated check did not receive the fresh output"
      pure (result 0))
  expect (buildOutput.1.exitCode == 0 && checkResult?.map (·.exitCode) == some 0)
    "successful isolated build did not return both results"
  expect ((← IO.FS.readFile (output / "preserved")) == "original output")
    "existing output content was not restored"
  expect ((← directoryEntries parent) == before)
    "successful isolated build changed its parent directory"

private def exerciseFailedBuild (parent : System.FilePath) : IO Unit := do
  let output := parent / "failed"
  IO.FS.createDirAll output
  IO.FS.writeFile (output / "preserved") "original output"
  let before ← directoryEntries parent
  let (buildOutput, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs
    (← layouts (baseline output true) "failed")
    (do
      IO.FS.createDirAll output
      IO.FS.writeFile (output / "partial") "failed build output"
      pure (result 1, ()))
    (fun _ _ => (throw (IO.userError "check ran after a failed build") : IO Process.Result))
  expect (buildOutput.1.exitCode == 1 && checkResult?.isNone)
    "failed build did not skip its isolated check"
  expect ((← IO.FS.readFile (output / "preserved")) == "original output")
    "failed build did not restore existing output"
  expect (!(← (output / "partial").pathExists))
    "failed build left partial output"
  expect ((← directoryEntries parent) == before)
    "failed build changed its parent directory"

private def exerciseAbsentOutput (parent : System.FilePath) : IO Unit := do
  let output := parent / "absent"
  let before ← directoryEntries parent
  let (_, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs
    (← layouts (baseline output false) "absent")
    (do
      IO.FS.createDirAll (output / "lib" / "lean")
      IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
      pure (result 0, ()))
    (fun _ _ => pure (result 0))
  expect (checkResult?.map (·.exitCode) == some 0)
    "absent-output check did not run"
  expect (!(← output.pathExists)) "initially absent output remained after proof check"
  expect ((← directoryEntries parent) == before)
    "initially absent output changed its parent directory"

private def exerciseCallbackFailure (parent : System.FilePath) : IO Unit := do
  let output := parent / "failed-callback"
  IO.FS.createDirAll output
  IO.FS.writeFile (output / "preserved") "original output"
  let before ← directoryEntries parent
  let mut failed := false
  try
    let _ ← AgentWorkbench.ProofBuild.withFreshOutputs
      (← layouts (baseline output true) "callback")
      (do
        IO.FS.createDirAll (output / "lib" / "lean")
        IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
        pure (result 0, ()))
      (fun _ _ => (throw (IO.userError "isolated callback failed") : IO Unit))
  catch _ =>
    failed := true
  expect failed "isolated callback failure did not propagate"
  expect ((← IO.FS.readFile (output / "preserved")) == "original output")
    "isolated callback failure did not restore existing output"
  expect ((← directoryEntries parent) == before)
    "isolated callback failure changed its parent directory"

private def exerciseAbsentOutputParent (root : System.FilePath) : IO Unit := do
  let package := root / "package-without-lake-state"
  IO.FS.createDirAll package
  let output := package / ".lake" / "build"
  let (_, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs
    (← layouts { directory := output, existed := false, parentExisted := false } "absent-parent")
    (do
      IO.FS.createDirAll (output / "lib" / "lean")
      IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
      pure (result 0, ()))
    (fun _ _ => pure (result 0))
  expect (checkResult?.map (·.exitCode) == some 0)
    "absent-parent check did not run"
  expect (!(← (package / ".lake").pathExists))
    "proof operation left a Lake state parent that was initially absent"

private def exerciseLakeImportOutputs : IO Unit := do
  let build := "/package/.lake/build/lib/lean/Blake3"
  let outputs := #[
    build ++ ".olean",
    build ++ ".ir",
    build ++ ".olean.server",
    build ++ ".olean.private"]
  expect (AgentWorkbench.ProofBuild.oleanOutputs outputs == #[build ++ ".olean"])
    "proof discovery treated a non-.olean Lake artifact as an imported module output"

private def exerciseProductionEffectGroundingCounterexamples : IO Unit := do
  let owner : AgentWorkbenchProof.ProductionEffectOwner := {
    designRevisionId := "design-current"
    designRevisionDigest := "blake3:current-design"
    statementId := "statement-effects"
    statementText := "every production effect has an exact owner"
    statementTextDigest := "blake3:current-statement"
    criterionId := "criterion-effects"
    criterionBinding := "{\"evidenceKind\":\"command\",\"id\":\"criterion-effects\",\"statement\":\"production proof\",\"statementId\":\"statement-effects\",\"target\":\"tree:AgentWorkbench\"}" }
  let statement : Statement := {
    id := owner.statementId, text := owner.statementText }
  let criterion : AcceptanceCriterion := {
    id := owner.criterionId, statementId := some owner.statementId
    statement := "production proof", target := "tree:AgentWorkbench", evidenceKind := "command" }
  let contract : AssuranceContract := {
    designRevisionId := owner.designRevisionId
    assuranceEpoch := "blake3:current-epoch"
    statementId := owner.statementId
    statementText := owner.statementText
    statementTextDigest := owner.statementTextDigest
    sourceUnitIds := []
    criterionIds := [owner.criterionId]
    implementationRequired := true
    scope := []
    scopeDigest := "blake3:current-scope"
    witnesses := []
    counterexamples := [] }
  let design : DesignRevision := {
    id := owner.designRevisionId
    status := .accepted
    producerAgentRun := "design-producer"
    revisionContentDigest := owner.designRevisionDigest
    statements := [statement]
    acceptanceCriteria := [criterion]
    assuranceSchemaVersion := 1
    assuranceContracts := [contract] }
  let authorityState : ProjectState := {
    ProjectState.empty with acceptedDesignId := some design.id, designRevisions := [design] }
  let firstKey : ProductionEffectKey := {
    operation := .workComplete, effect := .workActiveCompleted }
  let secondKey : ProductionEffectKey := {
    operation := .reviewDisposition, effect := .ledgerAppended }
  let addedBranch : ProductionEffectKey := {
    operation := .reviewDisposition, effect := .workActiveSuspended }
  let effectUniverse := [firstKey, secondKey]
  let matrix : List AgentWorkbenchProof.ProductionEffectGrounding := [
    { key := firstKey, owner }, { key := secondKey, owner }]
  let validFor := AgentWorkbenchProof.validProductionEffectGroundingMatrixFor effectUniverse
    authorityState owner.statementId owner.criterionId
  expect (validFor matrix)
    "independent production-effect grounding fixture is not closed"
  expect (!validFor matrix.tail)
    "production-effect grounding accepted a missing effect"
  expect (!validFor (matrix ++ [{ key := addedBranch, owner }]))
    "production-effect grounding accepted an extra effect"
  expect (!validFor (matrix ++ [{ key := firstKey, owner }]))
    "production-effect grounding accepted a duplicate effect"
  let crossDesign := { owner with designRevisionId := "design-other" }
  expect (!validFor (matrix.map fun grounding => { grounding with owner := crossDesign }))
    "production-effect grounding accepted a cross-Design owner"
  let stale := { owner with statementTextDigest := "blake3:stale-statement" }
  expect (!validFor (matrix.map fun grounding => { grounding with owner := stale }))
    "production-effect grounding accepted a stale owner"
  expect (!AgentWorkbenchProof.validProductionEffectGroundingMatrixFor
    (effectUniverse ++ [addedBranch]) authorityState owner.statementId owner.criterionId matrix)
    "a branch added inside an existing operation bypassed reverse grounding"

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let parent := root / "outputs"
    IO.FS.createDirAll parent
    exerciseExistingOutput parent
    exerciseFailedBuild parent
    exerciseAbsentOutput parent
    exerciseCallbackFailure parent
    exerciseAbsentOutputParent root
    exerciseLakeImportOutputs
    exerciseProductionEffectGroundingCounterexamples

end AgentWorkbenchTest.ProofBuild
