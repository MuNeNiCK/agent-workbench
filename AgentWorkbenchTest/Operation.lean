import AgentWorkbenchTest.Fixture
import AgentWorkbench.Cli.Describe

namespace AgentWorkbenchTest.Operation

open AgentWorkbench AgentWorkbenchTest

private inductive PositiveRouteSuite where
  | installedArchive
  | publicRoute
  | publicDesignWorkRoute
  | migratedPublicRoute

/-- Closed constructor-to-positive-route bridge. It uses the production Mutation type rather than
operation names: adding a mutation makes this release-test assignment fail to compile until an
actual successful binary scenario owns it. -/
private def positiveRouteSuite : Mutation → PositiveRouteSuite
  | .init | .proofRun _ => .installedArchive
  | .workFocus _ => .migratedPublicRoute
  | .designAmend _ | .designReject _ | .workAdoptDesign _ | .workWithdraw _
  | .correctionIncorporate _ => .publicDesignWorkRoute
  | .designPropose _ | .designAccept _ | .workStart _ | .workSuspend _ _ | .workResume _
  | .workHandoff _ _ _ _ | .workComplete | .planPropose _ | .planReplace _
  | .planMaterialize _ | .taskClose _ | .profileDefine _ | .profileReplace _
  | .commandRun _ | .artifactObserve _ | .correctionRecord _ | .correctionSupersede _
  | .correctionResolve _ | .kptRecord _ | .kptApply _ | .reviewStart _ | .reviewResume _
  | .reviewHandoff _ | .reviewFinding _ | .reviewDisposition _ | .reviewConclude _
  | .reviewVerify _ => .publicRoute

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
