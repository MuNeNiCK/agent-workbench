import AgentWorkbenchTest.Fixture
import AgentWorkbenchTest.RouteReceipt
import AgentWorkbench.Cli.Describe

namespace AgentWorkbenchTest.Operation

open AgentWorkbench AgentWorkbenchTest

open RouteReceipt

/-- The expected owning suite is exhaustive over the independent public Operation universe.
Queries have no mutation receipt. Adding a mutation operation creates a new explicit case here. -/
private def positiveRouteSuite : Operation → Option Suite
  | .workFocus => some .migratedPublicRoute
  | .designAmend | .designReject | .workAdoptDesign | .workWithdraw
  | .correctionIncorporate => some .publicDesignWorkRoute
  | .init | .designPropose | .designAccept | .workStart | .workSuspend | .workResume
  | .workHandoff | .workComplete | .planPropose | .planReplace | .planMaterialize
  | .taskClose | .taskReopenStale | .profileDefine | .profileReplace | .commandRun | .artifactObserve
  | .proofRun | .correctionRecord | .correctionSupersede | .correctionResolve
  | .kptRecord | .kptApply | .reviewStart | .reviewResume | .reviewHandoff
  | .reviewFinding | .reviewDisposition | .reviewConclude | .reviewVerify => some .publicRoute
  | .describe | .designGet | .designInspectSources | .designSource | .designDiff
  | .designExport | .workGet | .workAdoptionImpact | .planGet | .planInspectSources
  | .planSource | .planDiff | .planExport | .reviewContext | .reviewInspect
  | .entryGet | .history | .context | .ready | .commandShow | .proofDigest => none

/-- Mutation payload constructors cannot escape the typed Operation-to-suite assignment. -/
private theorem mutation_has_positive_route (mutation : Mutation) :
    (positiveRouteSuite mutation.operation).isSome := by
  cases mutation <;> rfl

def verifyPositiveRouteReceipts : IO Unit := do
  let actual ← RouteReceipt.recorded
  let expected := Operation.all.filterMap fun operation =>
    (positiveRouteSuite operation).map fun suite => ({ suite, operation } : Receipt)
  for receipt in expected do
    expect (actual.contains receipt)
      s!"public mutation has no successful linked-binary route receipt: {receipt.operation.name}"

def run : IO Unit := do
  let initialized ← fromExcept <| Mutation.init.executePure ProjectState.empty
  expect (initialized.revision == 1)
    "successful init did not advance authoritative state exactly once"
  expect (!operationApplicable initialized [] [] .init)
    "init remained applicable after authoritative initialization"
  let noPlan : ProjectState := { baseState with implementationPlans := [], ledgerEntries := [] }
  expect (!operationApplicable noPlan [] [] .workComplete)
    "completion was advertised without a structurally valid completion request"
  let claim : LeanClaim := {
    id := "claim-applicability"
    elaboratedPropositionDigest := "blake3:proposition"
    propositionDependencies := ["True"]
    input := {
      statementId := statement.id, statementText := statement.text
      mapping := "the proposition represents the Statement"
      proposition := "Applicability.Property", witness := "Applicability.property"
      proofRoot := ".agent-workbench/design/proofs/applicability"
      declaredSources := [{ path := "Applicability.lean", expectedDigest := some "blake3:source" }]
      check := { executable := "lake", arguments := #["build"] }
      toolchain := ProofToolchain.identifier } }
  let claimDesign : DesignRevision := { design with
    leanClaims := [claim]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { selectedIds := [claim.id] }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }] }
  let receipt : LeanProofReceiptRecord := {
    claimId := claim.id, claimInput := claim.input
    elaboratedPropositionDigest := claim.elaboratedPropositionDigest
    propositionDependencies := claim.propositionDependencies
    assumptionDependencies := [], inputDigest := "blake3:old-input"
    sourceDigests := [{ path := "Applicability.lean", digest := "blake3:source" }]
    toolchain := ProofToolchain.identifier, exitCode := 0
    outputDigest := "blake3:output", kernelAccepted := true }
  let receiptEntry : LedgerEntry := {
    id := "receipt-applicability", order := 2, scope := work.scope
    workId := some work.id, designRevision := some claimDesign.id
    payload := .leanProofReceipt receipt }
  let candidate := { plan with status := PlanStatus.candidate }
  let staleState : ProjectState := { baseState with
    designRevisions := [claimDesign], implementationPlans := [candidate]
    ledgerEntries := [receiptEntry] }
  let currentDigest : CurrentClaimDigest := {
    claimId := claim.id, claimInput := claim.input
    elaboratedPropositionDigest := claim.elaboratedPropositionDigest
    propositionDependencies := claim.propositionDependencies
    sourceDigests := receipt.sourceDigests, inputDigest := "blake3:new-input" }
  expect (operationStructurallyApplicable staleState .planMaterialize)
    "stale Claim fixture lacks the intended structural Plan request"
  expect (!operationApplicable staleState [] [currentDigest] .planMaterialize)
    "Plan materialization was advertised with a stale Claim receipt"
  let staleContext := currentContext? staleState [] [currentDigest]
  expect (staleContext.all fun value => !value.applicableOperations.contains "plan materialize")
    "current context advertised Plan materialization with a stale Claim receipt"
  let names := AgentWorkbench.Operation.all.map (·.name)
  expect (names.all fun name => names.count name == 1)
    "closed public operation inventory contains duplicate names"
  expect (AgentWorkbench.Operation.all.all fun operation =>
    AgentWorkbench.Operation.parse? operation.name == some operation)
    "closed public operation parser does not cover every constructor"
  expect (AgentWorkbench.Operation.all.all fun operation =>
    AgentWorkbench.Operation.parseCommand? (operation.name.splitOn " ") == some operation)
    "native command classifier does not cover every public operation constructor"
  expect (!names.contains "task add" && !names.contains "formal-check" &&
    !names.contains "Foundation" && !names.contains "Continuity")
    "removed or internal implementation vocabulary leaked into public operations"
  for required in ["design amend", "design reject", "work withdraw", "review handoff",
      "review conclude", "review inspect"] do
    expect (names.contains required) s!"public operation is missing: {required}"
  let contracts := AgentWorkbench.Cli.operationContracts.map (·.operation)
  expect (contracts.length == names.length && names.all contracts.contains &&
    contracts.all names.contains)
    "public operation inventory and native contracts do not cover the same closed route"

end AgentWorkbenchTest.Operation
