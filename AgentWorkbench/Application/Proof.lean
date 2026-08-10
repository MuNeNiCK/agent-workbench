import AgentWorkbench.Application.Ledger
import AgentWorkbench.Adapter.ProofInput
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbench.Adapter.ProofElaboration
import AgentWorkbench.Adapter.Process
import AgentWorkbench.Adapter.Runtime

namespace AgentWorkbench

structure ProofRunRequest where
  claimId : String
  entryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ProofRunResult where
  entry : LedgerEntry
  stdout : String
  stderr : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def findCurrentClaim (state : ProjectState) (claimId : String) : IO (CurrentProjection × LeanClaim) := do
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw (IO.userError "no current Work context")
  let claim ← match projection.design.claim? claimId with
    | some value => pure value
    | none => throw (IO.userError s!"current Design has no Lean claim {claimId}")
  pure (projection, claim)

def currentClaimDigest
    (projectRoot : System.FilePath) (state : ProjectState)
    (claimId : String) : IO CurrentClaimDigest := do
  let (_, claim) ← findCurrentClaim state claimId
  pure (← ProofInput.evaluate projectRoot (Runtime.layout projectRoot) claim).1

private def proofCommand
    (projectRoot : System.FilePath) (runtime : Runtime.Layout) (claim : LeanClaim) : CommandSpec :=
  let check := claim.input.check
  let proofRoot : System.FilePath := claim.input.proofRoot
  let defaultWorkingDirectory :=
    if proofRoot.isAbsolute then proofRoot.toString else (projectRoot / proofRoot).toString
  { executable := runtime.elanExecutable.toString
    arguments := #["run", ProofToolchain.identifier, check.executable] ++ check.arguments
    workingDirectory := match check.workingDirectory with
      | some configured => some configured
      | none => some defaultWorkingDirectory
    environment := check.environment }

private def proofRootPath (projectRoot : System.FilePath) (claim : LeanClaim) : System.FilePath :=
  let configured : System.FilePath := claim.input.proofRoot
  if configured.isAbsolute then configured else projectRoot / configured

private def sourceModule (source : SourceInput) : String :=
  source.path.replace "\\" "/" |>.dropEnd 5 |>.toString |>.replace "/" "."

private def checkerSource (claim : LeanClaim) : String :=
  let imports := claim.input.declaredSources.map (fun source => s!"import {sourceModule source}")
  let assumptions := claim.input.assumptions.map (fun name => s!"\"{name}\"")
  "import Lean.Elab.Command\nimport Lean.Util.CollectAxioms\n" ++ String.intercalate "\n" imports ++
    s!"\n\nexample : {claim.input.proposition} := by\n  exact {claim.input.witness}\n\n" ++
    s!"open Lean\nrun_cmd do\n" ++
    s!"  let actual := (← Lean.collectAxioms ``{claim.input.witness}).toList.map toString " ++
    s!"|>.mergeSort (· < ·)\n" ++
    s!"  let expected := [{String.intercalate ", " assumptions}] |>.mergeSort (· < ·)\n" ++
    "  unless actual == expected do\n" ++
    "    throwError m!\"kernel axioms {actual} differ from declared assumptions {expected}\"\n"

private def buildSourcesCommand
    (runtime : Runtime.Layout) (claim : LeanClaim) : CommandSpec :=
  { executable := runtime.elanExecutable.toString
    arguments := #["run", ProofToolchain.identifier, "lake", "-H", "-R", "--no-cache", "build"] ++
      claim.input.declaredSources.toArray.map (·.path) }

private def kernelCommand
    (runtime : Runtime.Layout) (checker : System.FilePath) : CommandSpec :=
  { executable := runtime.elanExecutable.toString
    arguments := #["run", ProofToolchain.identifier, "lake", "env", "lean", checker.toString] }

private def outputDigest (stdout stderr : String) : String :=
  let stdoutBytes := stdout.toUTF8.size
  let stderrBytes := stderr.toUTF8.size
  ContentDigest.string s!"{stdoutBytes}:{stdout}{stderrBytes}:{stderr}"

def runProofClaim
    (projectRoot : System.FilePath) (runtime : Runtime.Layout) (state : ProjectState)
    (request : ProofRunRequest) (baselines : List ProofBuild.OutputBaseline)
    (layouts : List ProofBuild.OutputLayout) : IO (ProjectState × ProofRunResult) := do
  let (projection, claim) ← findCurrentClaim state request.claimId
  if !(← runtime.elanExecutable.pathExists) then
    throw (IO.userError s!"project-local Elan is missing: {runtime.elanExecutable}")
  let proofRoot := proofRootPath projectRoot claim
  let (buildOutput, checks?) ← ProofBuild.withFreshOutputs layouts
    (do
      let buildDirectories ← ProofBuild.buildDirectories projectRoot runtime claim
      ProofBuild.validateDiscoveredOutputs baselines buildDirectories
      let (beforeDigest, sourceDigests) ← ProofInput.evaluate projectRoot runtime claim
      let result ← Process.executeWithOverrides projectRoot {
        (buildSourcesCommand runtime claim) with workingDirectory := some proofRoot.toString }
        #[("ELAN_HOME", runtime.elanHome.toString)]
      pure (result, beforeDigest, sourceDigests))
    (fun buildInput leanPaths => do
      let (beforeDigest, _) := buildInput
      let (builtDigest, _) ← ProofInput.evaluate projectRoot runtime claim
      if builtDigest != beforeDigest then
        throw (IO.userError "proof input changed while rebuilding declared sources")
      let elaboration ← ProofElaboration.run projectRoot proofRoot runtime claim leanPaths
      if elaboration.elaboratedPropositionDigest != claim.elaboratedPropositionDigest ||
          elaboration.propositionDependencies != claim.propositionDependencies then
        throw (IO.userError "elaborated proposition differs from the accepted Design Claim")
      let kernelResult ← IO.FS.withTempDir (fun temporary => do
        let checker := temporary / "Claim.lean"
        IO.FS.writeFile checker (checkerSource claim)
        Process.executeWithOverrides projectRoot {
          (kernelCommand runtime checker) with
            workingDirectory := some proofRoot.toString }
          #[("ELAN_HOME", runtime.elanHome.toString),
            ("LEAN_PATH", System.SearchPath.toString leanPaths)])
      let configuredResult ← Process.executeWithOverrides projectRoot
        (proofCommand projectRoot runtime claim) #[("ELAN_HOME", runtime.elanHome.toString)]
      let (checkedDigest, sourceDigests) ← ProofInput.evaluate projectRoot runtime claim
      if checkedDigest != beforeDigest then
        throw (IO.userError "proof input changed during fresh build, kernel check, or configured check")
      pure (kernelResult, configuredResult, checkedDigest, sourceDigests))
  let buildResult := buildOutput.1
  let initialDigest := buildOutput.2.1
  let initialSources := buildOutput.2.2
  let kernelSkipped := "kernel check skipped because the fresh build failed"
  let configuredSkipped := "configured check skipped because the fresh build failed"
  let kernelResult := checks?.map (·.1) |>.getD {
    exitCode := 1, stdout := "", stderr := kernelSkipped
    stdoutDigest := ContentDigest.string ""
    stderrDigest := ContentDigest.string kernelSkipped }
  let precheck := checks?.map (·.2.1) |>.getD {
    exitCode := 1, stdout := "", stderr := configuredSkipped
    stdoutDigest := ContentDigest.string ""
    stderrDigest := ContentDigest.string configuredSkipped }
  let digest := checks?.map (·.2.2.1) |>.getD initialDigest
  let sourceDigests := checks?.map (·.2.2.2) |>.getD initialSources
  let accepted := buildResult.exitCode == 0 && kernelResult.exitCode == 0 &&
    precheck.exitCode == 0
  let stdout := buildResult.stdout ++ kernelResult.stdout ++ precheck.stdout
  let stderr := buildResult.stderr ++ kernelResult.stderr ++ precheck.stderr
  let exitCode := if buildResult.exitCode != 0 then buildResult.exitCode
    else if kernelResult.exitCode != 0 then kernelResult.exitCode else precheck.exitCode
  if !accepted then
    throw (IO.userError s!"Lean Claim verification failed without recording a receipt:\n{stderr}")
  let entry : LedgerEntry :=
    { id := request.entryId
      order := nextEntryOrder state
      scope := projection.work.scope
      workId := some projection.work.id
      designRevision := some projection.design.id
      payload := .leanProofReceipt {
        claimId := claim.id
        claimInput := claim.input
        elaboratedPropositionDigest := claim.elaboratedPropositionDigest
        propositionDependencies := claim.propositionDependencies
        assumptionDependencies := claim.input.assumptions.mergeSort (· < ·)
        inputDigest := digest.inputDigest
        sourceDigests
        toolchain := ProofToolchain.identifier
        exitCode
        outputDigest := outputDigest stdout stderr
        kernelAccepted := accepted
        assuranceBinding := some <| projection.design.assuranceBindingForClaim
          projection.work.responsibleAgentRun claim.id } }
  let next ← match appendEntry state entry with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  pure (next, { entry, stdout, stderr })

end AgentWorkbench
