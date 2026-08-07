import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Completion

open AgentWorkbench AgentWorkbenchTest

private def observations : List TargetObservation :=
  [{ target := criterion.target, snapshot := "blake3:artifact" }]

private def currentClosedState : IO ProjectState :=
  fromExcept <| closeTask evidencedState observations {
    entryId := "task-closed", taskEntryId := "task-open" }

private def expectCompletionRejected (state : ProjectState) (message : String) : IO Unit := do
  let before := state
  expectError (completeFocusedWork state observations [] "blake3:completion-input") message
  expect (state == before) s!"{message}; rejected pure completion changed its input state"

def run : IO Unit := do
  let noPlan : ProjectState := { baseState with
    implementationPlans := [], ledgerEntries := [] }
  fromExcept (validateState noPlan)
  expectCompletionRejected noPlan "completion accepted a Work without a current Plan"

  fromExcept (validateState baseState)
  expectCompletionRejected baseState "completion accepted an open required Task"

  let closed ← currentClosedState
  let closedEntry ← match closed.entry? "task-closed" with
    | some value => pure value
    | none => throw (IO.userError "closed Task fixture is missing")
  let noEvidence : ProjectState := { closed with
    ledgerEntries := [taskEntry, closedEntry] }
  fromExcept (validateState noEvidence)
  expectCompletionRejected noEvidence "completion accepted a Criterion without current evidence"

  let corrected ← fromExcept <| recordCorrection closed {
    entryId := "correction-completion-gap", content := "change the current expected result" }
  fromExcept (validateState corrected)
  expectCompletionRejected corrected "completion ignored an effective User Correction"

  IO.FS.withTempDir fun root => do
    IO.FS.writeFile (root / "artifact.txt") "observed"
    let reviewed ← startReview root closed {
      entryId := "review-completion-gap", reviewId := "review-completion-gap"
      purpose := .implementation, reviewerAgentRun := "reviewer-completion-gap" }
    let found ← fromExcept <| recordFinding reviewed {
      entryId := "finding-completion-gap", reviewEntryId := "review-completion-gap"
      subject := { kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
      summary := "the accepted implementation still needs remediation" }
    let disposed ← fromExcept <| recordDisposition found {
      entryId := "disposition-completion-gap", findingEntryId := "finding-completion-gap"
      decision := .accepted, reason := "the fixed target establishes the gap" }
    fromExcept (validateState disposed)
    expectCompletionRejected disposed "completion ignored an unresolved accepted Finding"

  let claim : LeanClaim := {
    id := "claim-completion-gap"
    elaboratedPropositionDigest := "blake3:elaborated-claim"
    propositionDependencies := ["True"]
    input := {
      statementId := statement.id, statementText := statement.text
      mapping := "the selected proposition represents the accepted Statement"
      proposition := "CompletionDesign.Property", witness := "CompletionDesign.property"
      proofRoot := ".agent-workbench/design/proofs/completion"
      declaredSources := [{
        path := "CompletionDesign.lean", expectedDigest := some "blake3:claim-source" }]
      check := { executable := "lake", arguments := #["build"] }
      toolchain := ProofToolchain.identifier } }
  let claimDesign : DesignRevision := { design with
    sourceDocuments := design.sourceDocuments ++ [{
      target := "file:.agent-workbench/design/proofs/completion/CompletionDesign.lean"
      mediaKind := "lean", snapshot := "blake3:claim-source" }]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { selectedIds := [claim.id] }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }]
    leanClaims := [claim] }
  let noClaimReceipt : ProjectState := { closed with designRevisions := [claimDesign] }
  fromExcept (validateState noClaimReceipt)
  expectCompletionRejected noClaimReceipt "completion accepted a selected Claim without a receipt"

end AgentWorkbenchTest.Completion
