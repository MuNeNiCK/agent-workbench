import AgentWorkbench.Adapter.DesignClaim
import AgentWorkbench.Domain.Validation.Design
import AgentWorkbench.Application.Proof

namespace AgentWorkbenchTest.DesignClaim

open AgentWorkbench

private def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw (IO.userError message)

private def systemRuntime : IO Runtime.Layout := do
  let home ← match ← IO.getEnv "ELAN_HOME" with
    | some value => pure value
    | none => match ← IO.getEnv (if System.Platform.isWindows then "USERPROFILE" else "HOME") with
      | some value => pure (value ++ "/.elan")
      | none => throw (IO.userError "cannot locate the test Elan home")
  let executable : System.FilePath := System.FilePath.mk home / "bin" /
    (if System.Platform.isWindows then "elan.exe" else "elan")
  pure { elanExecutable := executable, elanHome := home }

private def claim : LeanClaim := {
  id := "claim-design-source"
  input := {
    statementId := "statement-design-source"
    statementText := "The selected proposition is true."
    mapping := "ClaimP is the selected Design property."
    proposition := "Claims.ClaimP"
    witness := "Claims.claimW"
    proofRoot := ".agent-workbench/design/proofs/claim"
    declaredSources := [{ path := "Claims.lean" }]
    check := { executable := "lake", arguments := #["build"] }
    toolchain := ProofToolchain.identifier } }

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let proofRoot := root / ".agent-workbench" / "design" / "proofs" / "claim"
    IO.FS.createDirAll proofRoot
    IO.FS.writeFile (proofRoot / "lean-toolchain") (ProofToolchain.identifier ++ "\n")
    IO.FS.writeFile (proofRoot / "lakefile.lean")
      "import Lake\nopen Lake DSL\npackage claim\n@[default_target] lean_lib Claims\n"
    let dependency := "namespace Claims.Imported\ndef Base : Prop := True\nend Claims.Imported\n"
    let source := "import Claims.Imported\nnamespace Claims\ndef ClaimP : Prop := Imported.Base\ntheorem claimW : ClaimP := by trivial\nend Claims\n"
    IO.FS.createDirAll (proofRoot / "Claims")
    IO.FS.writeFile (proofRoot / "Claims" / "Imported.lean") dependency
    IO.FS.writeFile (proofRoot / "Claims.lean") source
    let runtime ← systemRuntime
    let incompleteRejected ← try
        let _ ← AgentWorkbench.DesignClaim.prepareWithRuntime root runtime [claim]
        pure false
      catch _ => pure true
    expect incompleteRejected "Design Claim accepted an omitted local Lean dependency"
    let completeClaim := { claim with input := { claim.input with declaredSources :=
      [{ path := "Claims.lean" }, { path := "Claims/Imported.lean" }] } }
    let prepared ← AgentWorkbench.DesignClaim.prepareWithRuntime root runtime [completeClaim]
    let bound ← match prepared.claims with
      | [value] => pure value
      | _ => throw (IO.userError "Design Claim preparation did not return exactly one Claim")
    expect (prepared.sources.length == 2 &&
      prepared.sources.all (fun value => value.mediaKind == "lean"))
      "Design Claim source closure was not captured as immutable Lean sources"
    let capturedDigest ← match prepared.sources.find? (·.target.endsWith "/Claims.lean") with
      | some value => pure value.digest
      | none => throw (IO.userError "Design Claim primary source capture is missing")
    expect (bound.input.declaredSources.head?.bind (·.expectedDigest) == some capturedDigest)
      "Design Claim did not derive its source digest from captured bytes"

    IO.FS.writeFile (proofRoot / "Claims.lean") (source ++ "-- changed after capture\n")
    let staleRejected ← try
        let _ ← AgentWorkbench.ProofInput.evaluate root runtime bound
        pure false
      catch _ => pure true
    expect staleRejected "a post-capture Lean source edit remained current"
    IO.FS.writeFile (proofRoot / "Claims.lean") source

    let baselines ← AgentWorkbench.ProofBuild.captureBaselines root bound
    let layouts ← AgentWorkbench.ProofBuild.outputLayouts baselines "design-claim-test"
    let elaborated ← AgentWorkbench.DesignClaim.elaborateWithRuntime
      root runtime bound layouts
    expect (Validation.validContentDigest elaborated.elaboratedPropositionDigest &&
      !elaborated.propositionDependencies.isEmpty)
      "Design proposal did not pin the elaborated proposition and its dependency set"
    expect (!(← (proofRoot / ".lake" / "build").pathExists))
      "Design Claim elaboration left a build output that was absent before the operation"

    let designUnit : DesignSourceUnit := {
      id := "unit-design-claim"
      target := "file:.agent-workbench/design/product/claim.md"
      path := ".agent-workbench/design/product/claim.md"
      kind := .paragraph, text := elaborated.input.statementText
      digest := "blake3:design-claim-unit" }
    let archivedLean := prepared.sources.map fun source =>
      ({ target := source.target, mediaKind := source.mediaKind
         snapshot := source.digest } : AgentWorkbench.DesignSource)
    let design : DesignRevision := {
      id := "design-claim", workId := some "work-claim", status := .accepted
      producerAgentRun := "agent-claim", changeRationale := "verify the selected Claim route"
      revisionContentDigest := "blake3:design-claim", sourceArchiveAvailable := true
      sourceDocuments := [{ target := designUnit.target, snapshot := "blake3:markdown" }] ++ archivedLean
      sourceUnits := [designUnit]
      sourceUnitDispositions := [{ unitId := designUnit.id, role := .requirement }]
      statements := [{ id := elaborated.input.statementId, text := elaborated.input.statementText }]
      statementCoverage := [{
        statementId := elaborated.input.statementId, sourceUnitIds := [designUnit.id]
        leanClaims := { selectedIds := [elaborated.id] }
        acceptanceCriteria := { noSelectionReason := some "the Claim has no external observation" }
        implementationRequired := false
        noImplementationReason := some "this fixture checks the Design Claim itself" }]
      acceptanceCriteria := [], leanClaims := [elaborated] }
    let work : Work := {
      id := "work-claim", outcome := "verify the selected Claim route", scope := "project"
      designRevision := some design.id, status := .active
      responsibleAgentRun := "agent-claim" }
    let state : ProjectState := {
      revision := 1, acceptedDesignId := some design.id, focusedWorkId := some work.id
      designRevisions := [design], works := [work] }
    let proofBaselines ← AgentWorkbench.ProofBuild.captureBaselines root elaborated
    let proofLayouts ← AgentWorkbench.ProofBuild.outputLayouts proofBaselines "proof-route-test"
    let (proved, result) ← AgentWorkbench.runProofClaim root runtime state
      { entryId := "receipt-claim", claimId := elaborated.id } proofBaselines proofLayouts
    expect (match result.entry.payload with
      | .leanProofReceipt receipt => receipt.kernelAccepted &&
          receipt.elaboratedPropositionDigest == elaborated.elaboratedPropositionDigest &&
          receipt.propositionDependencies == elaborated.propositionDependencies &&
          receipt.assumptionDependencies.isEmpty
      | _ => false)
      "successful Claim route did not record the complete immutable receipt identity"
    expect (proved.revision == state.revision + 1 && proved.ledgerEntries.length == 1)
      "successful Claim route did not advance exactly one semantic revision"

    IO.FS.writeFile (proofRoot / "Claims.lean") (source ++ "-- stale before proof\n")
    let failedWithoutReceipt ← try
        let baselines ← AgentWorkbench.ProofBuild.captureBaselines root elaborated
        let failedLayouts ← AgentWorkbench.ProofBuild.outputLayouts baselines "proof-failure-test"
        let _ ← AgentWorkbench.runProofClaim root runtime state
          { entryId := "receipt-failed", claimId := elaborated.id } baselines failedLayouts
        pure false
      catch _ => pure true
    expect failedWithoutReceipt
      "stale Claim input produced a proof receipt instead of rejecting before state mutation"

end AgentWorkbenchTest.DesignClaim
