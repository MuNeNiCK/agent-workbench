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
      proposition := "True", witness := "designClaim", proofRoot := "proof"
      declaredSources := [{ path := "Proof.lean" }]
      check := { executable := "lake", arguments := #["build"] }
      toolchain := Runtime.toolchain } }

def operationContracts : List OperationContract :=
  [ noInput "init" "initialize project-local runtime and state"
  , noInput "describe" "list operations; append an operation name for its contract"
  , contract "design propose" "propose a successor; parent/status/order are derived"
      ({ producerAgentRun := "agent-run-1", statements := [statement]
         acceptanceCriteria := [criterion, artifactCriterion]
         leanClaims := [claim] } : DesignProposalRequest)
  , contract "design accept" "accept a candidate while no Work is focused"
      ({ id := "design-1" } : IdInput)
  , contract "design get" "read a DesignRevision by ID" ({ id := "design-1" } : IdInput)
  , contract "work start" "start Work on the accepted Design; status/binding are derived"
      ({ id := "work-1", outcome := "produce the accepted artifact", scope := "project"
         responsibleAgentRun := "agent-run-1"
         delegatedReviewDecisions := [.accepted, .rejected, .replaced] } : WorkStartRequest)
  , contract "work get" "read Work by ID" ({ id := "work-1" } : IdInput)
  , contract "work focus" "focus a resumable Work" ({ id := "work-1" } : IdInput)
  , contract "work resume" "resume a resumable Work" ({ id := "work-1" } : IdInput)
  , contract "work suspend" "suspend focused Work with an explicit return condition"
      ({ workId := "work-1", resumeCondition := "continue after requirement clarification" } : SuspendInput)
  , contract "work handoff" "transfer responsibility without replacing Work"
      ({ workId := "work-1", entryId := "handoff-1", successorRun := "agent-run-2"
         reason := "continue the same Work in another agent run" } : HandoffInput)
  , contract "work adopt-design" "bind suspended Work to the accepted successor after impact inspection"
      ({ workId := "work-1", entryId := "adoption-1"
         impactDisposition := "re-observe successor-bound evidence"
         agentRun := "agent-run-1" } : AdoptDesignInput)
  , noInput "work complete" "complete focused Work only when derived readiness is true"
  , contract "task add" "add a current-bound Task; order/status/binding are derived"
      ({ entryId := "task-1", criterionId := some criterion.id
         description := "create artifact", required := true } : TaskAddRequest)
  , contract "task close" "close and supersede a current Task"
      ({ entryId := "task-closed", taskEntryId := "task-1" } : TaskCloseRequest)
  , contract "profile define" "define a current-bound Command Profile"
      ({ entryId := "profile-1", purpose := "verify artifact"
         target := some "file:artifact.txt"
         command := { executable := "test", arguments := #["-f", "artifact.txt"] } } : ProfileDefineRequest)
  , contract "profile replace" "replace a current Command Profile"
      ({ entryId := "profile-2", profileEntryId := "profile-1"
         purpose := "verify artifact", target := some "file:artifact.txt"
         command := { executable := "test", arguments := #["-s", "artifact.txt"] } } : ProfileReplaceRequest)
  , contract "artifact observe" "record criterion evidence; target/snapshot/binding are derived"
      ({ entryId := "evidence-1", criterionId := artifactCriterion.id
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
         purpose := ReviewPurpose.implementation, targetSourceId := "command-1"
         reviewerAgentRun := "reviewer-run-1" } : ReviewStartRequest)
  , contract "review resume" "continue the same Review; identity/target/reviewer are derived"
      ({ entryId := "review-resume", continuesEntryId := "review-fresh"
       } : ReviewResumeRequest)
  , contract "review finding" "record an advisory Finding under an existing Review"
      ({ entryId := "finding-1", reviewEntryId := "review-fresh"
         subject := findingSubject
         mismatchEvidenceId := "evidence-1", summary := "artifact does not match" } : FindingRecordRequest)
  , contract "review disposition" "resolve advisory authority using the responsible Work run"
      ({ entryId := "disposition-1", findingEntryId := "finding-1"
         decision := DispositionDecision.accepted, reason := "evidence confirms mismatch" } : DispositionRecordRequest)
  , contract "review verify" "verify a Finding through resumed Review and exact evidence"
      ({ entryId := "verification-1", findingEntryId := "finding-1"
         reviewEntryId := "review-resume", evidenceEntryId := "evidence-2" } : VerificationRecordRequest)
  , contract "review context" "read isolated fresh/resume Review input"
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

def operationIndex (state : ProjectState) : OperationIndex :=
  { instruction := "use an applicable operation; run `describe OPERATION` for its native contract"
    operations := operationContracts.map (·.operation)
    applicableOperations := operationContracts.filter
      (fun contract => operationApplicable state contract.operation) |>.map (·.operation) }

def operationContract? (operation : String) : Option OperationContract :=
  operationContracts.find? (·.operation == operation)

def describedOperation? (state : ProjectState) (operation : String) : Option OperationContract :=
  (operationContract? operation).map fun contract =>
    { contract with applicable := operationApplicable state operation }

end AgentWorkbench.Cli
