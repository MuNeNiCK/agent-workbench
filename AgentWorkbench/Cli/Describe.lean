import AgentWorkbench.Cli.Protocol
import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Profile
import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Guidance
import AgentWorkbench.Application.Review
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Plan
import AgentWorkbench.Decision.Operation

namespace AgentWorkbench.Cli

structure OperationContract where
  operation : String
  summary : String
  inputExample : Option Lean.Json := none
  applicable : Bool := false
  deriving Lean.ToJson

structure OperationIndex where
  instruction : String
  operations : List String
  applicableOperations : List String
  deriving Lean.ToJson

private def contract [Lean.ToJson α]
    (operation summary : String) (input : α) : OperationContract :=
  { operation, summary, inputExample := some (Lean.toJson input) }

private def noInput (operation summary : String) : OperationContract :=
  { operation, summary }

private def statement : Statement :=
  { id := "statement-1", text := "artifact must exist" }

private def criterion : AcceptanceCriterion :=
  { id := "criterion-command", statement := "artifact command succeeds"
    target := "file:artifact.txt", evidenceKind := "command" }

private def artifactCriterion : AcceptanceCriterion :=
  { id := "criterion-artifact", statement := "artifact observation succeeds"
    target := "file:observed.txt", evidenceKind := "artifact" }

private def findingSubject : FindingSubject :=
  { kind := .criterion, id := criterion.id, exactQuote := criterion.statement }

private def claim : LeanClaim :=
  { id := "claim-1"
    input := {
      statementId := statement.id, statementText := statement.text
      mapping := "the Lean witness checks the selected Design proposition"
      proposition := "ExampleDesign.Property", witness := "ExampleDesign.property"
      proofRoot := ".agent-workbench/design/proofs/example"
      declaredSources := [{ path := "ExampleDesign.lean" }]
      check := { executable := "lake", arguments := #["build"] }
      toolchain := Runtime.toolchain } }

def operationContracts : List OperationContract :=
  [ noInput "init" "initialize project-local runtime and state"
  , noInput "describe" "list operations; append an operation name for its contract"
  , contract "design propose" "propose a successor; parent/status/order are derived"
      ({ producerAgentRun := "agent-run-1", changeRationale := "record the initial Design"
         sourceDocumentTargets := ["file:.agent-workbench/design/product/design.md"]
         sourceUnitDispositions := [], statementCoverage := []
         statements := [statement]
         acceptanceCriteria := [criterion, artifactCriterion]
         leanClaims := [claim] } : DesignProposalRequest)
  , contract "design amend" "replace a candidate with an immutable amended candidate"
      ({ producerAgentRun := "agent-run-1", changeRationale := "address the accepted correction"
         amendsCandidate := some "design-1", sourceUnitDispositions := []
         sourceDocumentTargets := ["file:.agent-workbench/design/product/design.md"]
         statementCoverage := [], statements := [statement]
         acceptanceCriteria := [criterion, artifactCriterion], leanClaims := [claim] } :
        DesignProposalRequest)
  , contract "design accept" "accept a candidate while no Work is focused"
      ({ id := "design-1" } : IdInput)
  , contract "design reject" "reject a candidate without changing accepted Design"
      ({ designId := "design-1", entryId := "design-rejection-1"
         reason := "candidate does not satisfy the fixed requirement" } : DesignRejectRequest)
  , contract "design get" "read a DesignRevision by ID" ({ id := "design-1" } : IdInput)
  , contract "design inspect-sources" "inspect non-authoritative Design drafts without changing state"
      ({ sourceDocumentTargets := ["file:.agent-workbench/design/product/design.md"] } :
        DesignSourceInspectionInput)
  , contract "design source" "read one exact archived Design source from SQLite"
      ({ designId := "design-1", target := "file:.agent-workbench/design/product/design.md" } :
        DesignSourceInput)
  , contract "design diff" "compare two immutable archived Design source sets"
      ({ beforeDesignId := "design-1", afterDesignId := "design-2" } : DesignDiffInput)
  , contract "design export" "stream an ordered exact-byte archive to standard output"
      ({ id := "design-1" } : IdInput)
  , contract "plan propose" "propose a complete Work-bound implementation Plan"
      ({ producerAgentRun := "agent-run-1", reason := "implement the accepted Design delta"
         sourceDocumentTargets := ["file:.agent-workbench/design/plans/work-1/plan.md"]
         sourceUnitDispositions := [], statementDispositions := [], steps := [] } :
        PlanProposalRequest)
  , contract "plan inspect-sources" "inspect non-authoritative Plan drafts without changing state"
      ({ workId := "work-1"
         sourceDocumentTargets := ["file:.agent-workbench/design/plans/work-1/plan.md"] } :
        PlanSourceInspectionInput)
  , contract "plan replace" "replace the current candidate head or current Plan"
      ({ predecessorPlanId := some "plan-1", producerAgentRun := "agent-run-1"
         reason := "incorporate the accepted change"
         sourceDocumentTargets := ["file:.agent-workbench/design/plans/work-1/plan.md"]
         sourceUnitDispositions := [], statementDispositions := [], steps := [] } :
        PlanProposalRequest)
  , contract "plan materialize" "atomically make a candidate Plan current and derive its Task graph"
      ({ id := "plan-1" } : IdInput)
  , contract "plan get" "read an immutable Implementation Plan by ID"
      ({ id := "plan-1" } : IdInput)
  , contract "plan source" "read one exact archived Plan source from SQLite"
      ({ planId := "plan-1", target := "file:.agent-workbench/design/plans/work-1/plan.md" } :
        PlanSourceInput)
  , contract "plan diff" "compare two immutable archived Plan source sets"
      ({ beforePlanId := "plan-1", afterPlanId := "plan-2" } : PlanDiffInput)
  , contract "plan export" "stream an ordered exact-byte Plan archive to standard output"
      ({ id := "plan-1" } : IdInput)
  , contract "work start" "start Work on the accepted Design; status/binding are derived"
      ({ id := "work-1", outcome := "produce the accepted artifact", scope := "project"
         responsibleAgentRun := "agent-run-1" } : WorkStartRequest)
  , contract "work get" "read Work by ID" ({ id := "work-1" } : IdInput)
  , contract "work focus" "focus a resumable Work" ({ id := "work-1" } : IdInput)
  , contract "work resume" "resume only with recorded condition-satisfaction evidence" ({
      workId := "work-1", entryId := "resume-1"
      satisfaction := "the required clarification is recorded"
      basisEntryIds := ["correction-1"], agentRun := "responsible-agent" } : WorkResumeRequest)
  , contract "work suspend" "suspend focused Work with an explicit return condition"
      ({ workId := "work-1", resumeCondition := "continue after requirement clarification" } : SuspendInput)
  , contract "work handoff" "transfer responsibility without replacing Work"
      ({ workId := "work-1", entryId := "handoff-1", successorRun := "agent-run-2"
         reason := "continue the same Work in another agent run" } : HandoffInput)
  , contract "work adopt-design" "bind suspended Work to the accepted successor after impact inspection"
      ({ workId := "work-1", entryId := "adoption-1"
         agentRun := "agent-run-1" } : WorkAdoptDesignRequest)
  , contract "work adoption-impact" "derive the exact successor-Design impact before adoption"
      ({ id := "work-1" } : IdInput)
  , contract "work withdraw" "terminate Work unsuccessfully under an effective User Correction"
      ({ workId := "work-1", entryId := "withdrawal-1", correctionEntryId := "correction-1"
         reason := "the user withdrew this outcome" } : WorkWithdrawRequest)
  , noInput "work complete" "complete focused Work only when derived readiness is true"
  , contract "task close" "close and supersede a current Task"
      ({ entryId := "task-closed", taskEntryId := "task-1" } : TaskCloseRequest)
  , contract "profile define" "define a current-bound Command Profile"
      ({ entryId := "profile-1", purpose := "verify artifact"
         taskEntryId := "task-plan-1-step-1", inputTargets := []
         outputScope := "file:artifact.txt", criterionIds := [criterion.id]
         command := { executable := "test", arguments := #["-f", "artifact.txt"] } } : ProfileDefineRequest)
  , contract "profile replace" "replace a current Command Profile"
      ({ entryId := "profile-2", profileEntryId := "profile-1"
         purpose := "verify artifact", taskEntryId := "task-plan-1-step-1"
         inputTargets := [], outputScope := "file:artifact.txt", criterionIds := [criterion.id]
         command := { executable := "test", arguments := #["-s", "artifact.txt"] } } : ProfileReplaceRequest)
  , contract "artifact observe" "record criterion evidence; target/snapshot/binding are derived"
      ({ entryId := "evidence-1", taskEntryId := "task-plan-1-step-1"
         criterionId := artifactCriterion.id
         operation := "inspect artifact", result := "artifact exists"
         successful := true } : ArtifactObserveRequest)
  , contract "correction record" "record current user intent"
      ({ entryId := "correction-1", content := "change the accepted outcome" } : CorrectionRecordRequest)
  , contract "correction supersede" "replace a current correction with newer user intent"
      ({ entryId := "correction-2", correctionEntryId := "correction-1"
         content := "use the clarified accepted outcome" } : CorrectionSupersedeRequest)
  , contract "correction resolve" "resolve a correction through a later same-bound action"
      ({ entryId := "correction-resolved", correctionEntryId := "correction-1"
         actionEntryId := "command-1", reason := "the action applies the correction" } : CorrectionResolveRequest)
  , contract "correction incorporate" "resolve a correction with the current strict successor Design"
      ({ entryId := "correction-incorporated", correctionEntryId := "correction-1" } : CorrectionIncorporateRequest)
  , contract "kpt record" "record non-normative Keep/Problem/Try learning"
      ({ entryId := "kpt-1", tryNext := some "use the project verification profile" } : KptRecordRequest)
  , contract "kpt apply" "bind an earlier Try to a later same-bound action"
      ({ entryId := "kpt-applied", kptEntryId := "kpt-1"
         actionEntryId := "command-1", outcome := "Try applied successfully" } : KptApplyRequest)
  , contract "review start" "start a fresh Review; snapshot/binding are derived"
      ({ entryId := "review-fresh", reviewId := "review-1"
         purpose := ReviewPurpose.implementation
         reviewerAgentRun := "reviewer-run-1" } : ReviewStartRequest)
  , contract "review resume" "continue the same Review; identity/target/reviewer are derived"
      ({ entryId := "review-resume", continuesEntryId := "review-fresh"
       } : ReviewResumeRequest)
  , contract "review handoff" "transfer the same fixed Review to an independent reviewer"
      ({ entryId := "review-handoff-1", reviewEntryId := "review-fresh"
         successorReviewerRun := "reviewer-run-2", reason := "reviewer is unavailable" } :
        ReviewHandoffRequest)
  , contract "review finding" "record an advisory Finding under an existing Review"
      ({ entryId := "finding-1", reviewEntryId := "review-fresh"
         subject := findingSubject, summary := "artifact does not match" } : FindingRecordRequest)
  , contract "review disposition" "resolve advisory authority using the responsible Work run"
      ({ entryId := "disposition-1", findingEntryId := "finding-1"
         decision := DispositionDecision.accepted, reason := "evidence confirms mismatch" } : DispositionRecordRequest)
  , contract "review conclude" "record the active reviewer's advisory conclusion"
      ({ entryId := "review-conclusion-1", reviewEntryId := "review-fresh"
         clean := true, summary := "no findings" } : ReviewConclusionRequest)
  , contract "review verify" "verify a Finding through resumed Review and exact evidence"
      ({ entryId := "verification-1", findingEntryId := "finding-1"
         reviewEntryId := "review-resume", evidenceEntryId := "evidence-2" } : VerificationRecordRequest)
  , contract "review context" "read isolated fresh/resume Review input"
      ({ id := "review-fresh" } : IdInput)
  , contract "review inspect" "read the complete persisted Review lineage"
      ({ id := "review-fresh" } : IdInput)
  , contract "entry get" "read a resulting LedgerEntry by ID" ({ id := "task-1" } : IdInput)
  , contract "history" "read bounded history after an order"
      ({ afterOrder := 0, limit := 50 } : HistoryInput)
  , noInput "context" "read bounded current context"
  , noInput "ready" "derive completion readiness from current evidence"
  , contract "command show" "show one resolved applicable Command Profile"
      ({ id := "profile-1" } : IdInput)
  , contract "command run" "execute that profile and derive execution evidence"
      ({ profileEntryId := "profile-1", entryId := "command-1"
         criterionId := some criterion.id } : CommandRunRequest)
  , contract "proof digest" "derive the current complete proof-input identity"
      ({ id := "claim-1" } : IdInput)
  , contract "proof run" "run configured preparation and pinned generated kernel checker"
      ({ claimId := "claim-1", entryId := "proof-1" } : ProofRunRequest)
  ]

def operationIndex (state : ProjectState) (inputs : CurrentInputs) : OperationIndex :=
  { instruction := "use an applicable operation; run `describe OPERATION` for its native contract"
    operations := operationContracts.map (·.operation)
    applicableOperations := operationContracts.filter
      (fun contract => Operation.parse? contract.operation |>.any
        (operationApplicable state inputs.observations inputs.claimDigests))
      |>.map (·.operation) }

def operationContract? (operation : String) : Option OperationContract :=
  operationContracts.find? (·.operation == operation)

/-- The recursive field shape accepted by one native JSON operation. `.value` deliberately makes
no claim about scalar representation; objects and array elements retain their own field shape. -/
inductive InputSchema where
  | value
  | object (fields : List (String × InputSchema))
  | array (item : InputSchema)
  deriving Repr, Inhabited

private partial def schemaFromExample : Lean.Json → InputSchema
  | .obj fields => .object (fields.toList.map fun (key, value) => (key, schemaFromExample value))
  | .arr items => .array (items[0]?.map schemaFromExample |>.getD .value)
  | _ => .value

private def typedSchema [Lean.ToJson α] (value : α) : InputSchema :=
  schemaFromExample (Lean.toJson value)

/-- A type-checked recursive schema for strict JSON field validation. The human-facing examples
stay concise. Structured arrays are populated only in this schema witness, so an empty example can
never erase the object fields accepted for an array element. -/
def operationInputSchema? (operation : String) : Option InputSchema :=
  let designSchema : DesignProposalRequest := {
    producerAgentRun := "agent-run-1"
    changeRationale := "record the Design"
    changeBasisEntryIds := ["correction-1"]
    amendsCandidate := some "design-1"
    sourceDocumentTargets := ["file:.agent-workbench/design/product/design.md"]
    sourceUnitDispositions := [{
      unitId := "source-unit-1", role := .requirement, reason := some "authoritative requirement" }]
    assumptions := [{
      id := "assumption-1", text := "the service is available"
      sourceUnitIds := ["source-unit-1"] }]
    statements := [{ statement with assumptions := ["assumption-1"] }]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := ["source-unit-1"]
      leanClaims := { selectedIds := [claim.id] }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }]
    removedStatements := [{
      statementId := "removed-statement", statementText := "superseded requirement"
      implementationRequired := false, noImplementationReason := some "removed by the successor" }]
    acceptanceCriteria := [criterion, artifactCriterion]
    leanClaims := [claim] }
  let planSchema : PlanProposalRequest := {
    predecessorPlanId := some "plan-1"
    producerAgentRun := "agent-run-1"
    reason := "implement the complete Design delta"
    changeBasisEntryIds := ["finding-1"]
    sourceDocumentTargets := ["file:.agent-workbench/design/plans/work-1/plan.md"]
    sourceUnitDispositions := [{
      unitId := "plan-source-unit-1", stepId := some "step-1" }]
    statementDispositions := [{
      statementId := statement.id, statementText := statement.text
      deltaKind := .added, stepIds := ["step-1"] }]
    steps := [{
      id := "step-1", description := "implement the Statement"
      outputScopes := [criterion.target]
      requiredClaimIds := [claim.id]
      verificationCriterionIds := [criterion.id]
      taskVerificationContracts := [{
        id := "verify-step-output", kind := .command, target := criterion.target }]
      acceptedFindingEntryIds := ["finding-1"] }] }
  match operation with
  | "design propose" | "design amend" => some (typedSchema designSchema)
  | "plan propose" | "plan replace" => some (typedSchema planSchema)
  | _ => (operationContract? operation).bind (·.inputExample) |>.map schemaFromExample

def describedOperation?
    (state : ProjectState) (inputs : CurrentInputs) (operation : String) : Option OperationContract :=
  (operationContract? operation).map fun contract =>
    let applicable := (Operation.parse? operation).any
      (operationApplicable state inputs.observations inputs.claimDigests)
    { contract with applicable }

end AgentWorkbench.Cli
