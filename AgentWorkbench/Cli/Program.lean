import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.Update
import Lean.Data.Json

namespace AgentWorkbench.Cli.Program

open AgentWorkbench
open AgentWorkbench.Domain
open AgentWorkbench.Kernel
open Lean

abbrev Request := Application.Service.Request
abbrev Response := Application.Service.Response

def version : String := "0.2.1"

def executeRequest :=
  Application.Service.executeRequest

def executeBootstrap :=
  Application.Service.execute Application.Service.bootstrapCommand
    Application.Service.initialStore

def renderDecision :=
  Application.Service.renderDecision

def renderBootstrap :=
  Application.Service.renderBootstrap

private def usage : String :=
  String.intercalate "\n" [
    "usage:",
    "  agent-workbench --state <path> init <owner> <outcome> <completion-boundary>",
    "  agent-workbench --state <path> start <revision> <owner> <outcome> <completion-boundary>",
    "  agent-workbench --state <path> status",
    "  agent-workbench --state <path> next",
    "  agent-workbench --state <path> continue <revision> <work> <activation>",
    "  agent-workbench --state <path> resume <revision> <work> <activation>",
    "  agent-workbench --state <path> doctor",
    "  agent-workbench --state <path> repair <revision> <history-hex> <observed-hex>",
    "  agent-workbench --state <path> update inspect",
    "  agent-workbench --state <path> update apply <source-schema> <source-digest> <target-schema>",
    "  agent-workbench --state <path> update restore <source-schema> <source-digest> <backup-digest> <backup-size> <target-schema> <target-digest> <durability>",
    "  agent-workbench --state <path> export <purpose> <ledger|evidence|review|correction|backup|design> <output>",
    "  agent-workbench --state <path> apply <request.json>"
  ]

private def parseNat (name value : String) : Except String Nat :=
  match value.toNat? with
  | some number => .ok number
  | none => .error s!"{name} must be a natural number"

private def hexDigit (value : Nat) : Char :=
  Char.ofNat (if value < 10 then 48 + value else 87 + value)

private def encodeHex (value : String) : String :=
  String.ofList <| value.toUTF8.data.toList.flatMap fun byte =>
    let number := byte.toNat
    [hexDigit (number / 16), hexDigit (number % 16)]

private def hexValue (digit : Char) : Option Nat :=
  let value := digit.toNat
  if 48 ≤ value && value ≤ 57 then some (value - 48)
  else if 97 ≤ value && value ≤ 102 then some (value - 87)
  else if 65 ≤ value && value ≤ 70 then some (value - 55)
  else none

private def decodeHex (name value : String) : Except String String := do
  let rec bytes : List Char → Except String (List UInt8)
    | [] => pure []
    | high :: low :: rest => do
        let some highValue := hexValue high
          | throw s!"{name} must be hexadecimal UTF-8"
        let some lowValue := hexValue low
          | throw s!"{name} must be hexadecimal UTF-8"
        return UInt8.ofNat (highValue * 16 + lowValue) :: (← bytes rest)
    | _ => throw s!"{name} must contain complete hexadecimal bytes"
  let payload := ByteArray.mk (← bytes value.toList).toArray
  match String.fromUTF8? payload with
  | some decoded => pure decoded
  | none => throw s!"{name} must encode valid UTF-8"

private def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error reason => throw <| IO.userError reason

private def jsonField (json : Json) (name : String) (α : Type)
    [FromJson α] : Except String α :=
  json.getObjValAs? α name

private def jsonListFieldD (json : Json) (name : String) (α : Type)
    [FromJson α] : Except String (List α) :=
  match json.getObjValD name with
  | .null => .ok []
  | value => fromJson? value

private def jsonOptionalFieldD (json : Json) (name : String) (α : Type)
    [FromJson α] : Except String (Option α) :=
  match json.getObjValD name with
  | .null => .ok none
  | value => some <$> fromJson? value

private def parsePurpose : String → Except String Review.Purpose
  | "design" => .ok .design
  | "decomposition" => .ok .decomposition
  | "design-conformance" => .ok .designConformance
  | "implementation-quality" => .ok .implementationQuality
  | _ => .error "purpose must be design, decomposition, design-conformance, or implementation-quality"

private def parseClaim : String → Except String ReviewClaim
  | "clean" => .ok .clean
  | "findings" => .ok .findings
  | _ => .error "claim must be clean or findings"

private def parseDecision : String → Except String OwnerDecision
  | "accepted" => .ok .accepted
  | "rejected" => .ok .rejected
  | _ => .error "decision must be accepted or rejected"

private def parseEvidenceKind : String → Except String EvidenceKind
  | "build" => .ok .build
  | "test" => .ok .test
  | "review" => .ok .review
  | "remediation" => .ok .remediation
  | _ => .error "evidence kind must be build, test, review, or remediation"

private structure ObservationInput where
  key : String
  kind : String
  summary : String
  evidence : String
deriving FromJson

private structure AdoptionRationaleInput where
  necessity : String
  simplerAlternativesInsufficient : String
  boundedScope : String
  complexityCost : String
deriving FromJson

private structure DispositionInput where
  observation : String
  decision : String
  reason : String
  changesAuthority : Bool := false
  successorDesign : Option Nat := none
  adoptionRationale : Option AdoptionRationaleInput := none
deriving FromJson

private structure RelatedWorkInput where
  work : Nat
  kind : String
deriving FromJson

private structure PhaseInput where
  key : String
  group : String
  order : Nat
  dependencies : List String
  tasks : List String
  reviews : List Nat
deriving FromJson

private structure ResultingScopeInput where
  key : String
  work : Nat
  owner : String
  outcome : String
  completionBoundary : String
deriving FromJson

private def parseObservationKind : String → Except String Review.ObservationKind
  | "risk" => .ok .risk
  | "proposal" => .ok .proposal
  | _ => .error "observation kind must be risk or proposal"

private def parseObservationDecision : String → Except String Review.ObservationDecision
  | "accepted" => .ok .accepted
  | "rejected" => .ok .rejected
  | "rescoped" => .ok .rescoped
  | "deferred" => .ok .deferred
  | "needs-evidence" => .ok .needsEvidence
  | _ => .error "observation decision must be accepted, rejected, rescoped, deferred, or needs-evidence"

private def observationFromInput (input : ObservationInput) :
    Except String Review.Observation := do
  return {
    key := input.key
    kind := ← parseObservationKind input.kind
    summary := input.summary
    evidence := input.evidence
  }

private def dispositionFromInput (input : DispositionInput) :
    Except String Review.ObservationDisposition := do
  return {
    observation := input.observation
    decision := ← parseObservationDecision input.decision
    reason := input.reason
    changesAuthority := input.changesAuthority
    successorDesign := input.successorDesign.map (⟨·⟩)
    adoptionRationale := input.adoptionRationale.map fun rationale => {
      necessity := rationale.necessity
      simplerAlternativesInsufficient := rationale.simplerAlternativesInsufficient
      boundedScope := rationale.boundedScope
      complexityCost := rationale.complexityCost
    }
  }

private def parseVerificationResult : String → Except String Review.VerificationResult
  | "verified" => .ok .verified
  | "not-fixed" => .ok .notFixed
  | "needs-evidence" => .ok .needsEvidence
  | _ => .error "verification result must be verified, not-fixed, or needs-evidence"

private def parseAuthorityOperation : String → Except String Design.AuthorityOperation
  | "create" => .ok .create
  | "amend" => .ok .amend
  | "retire" => .ok .retire
  | _ => .error "authority operation must be create, amend, or retire"

private def parseAuthorityKind : String → Except String Design.AuthorityKind
  | "design-artifact" => .ok .designArtifact
  | "rule" => .ok .rule
  | "instruction" => .ok .instruction
  | "work-obligation" => .ok .workObligation
  | _ => .error "authority kind must be design-artifact, rule, instruction, or work-obligation"

private def parseAuthorityLifetime : String → Except String Design.AuthorityLifetime
  | "finite" => .ok .finite
  | "persistent" => .ok .persistent
  | _ => .error "authority lifetime must be finite or persistent"

private def relatedWorkFromInput (input : RelatedWorkInput) :
    Except String Lifecycle.RelatedWorkRequirement := do
  let kind ← match input.kind with
    | "child" => pure Lifecycle.RelatedWorkKind.child
    | "dependency" => pure Lifecycle.RelatedWorkKind.dependency
    | _ => throw "related work kind must be child or dependency"
  return { work := ⟨input.work⟩, kind }

private def parseAttemptState : String → Except String ExternalOperation.AttemptState
  | "prepared" => .ok .prepared
  | "dispatched" => .ok .dispatched
  | "uncertain" => .ok .uncertain
  | "retryable" => .ok .retryable
  | "succeeded" => .ok .succeeded
  | "failed" => .ok .failed
  | "conflict" => .ok .conflict
  | _ => .error "external operation state must be prepared, dispatched, uncertain, retryable, succeeded, failed, or conflict"

private def parseExternalOperationKind :
    String → Except String ExternalOperation.OperationKind
  | "release" => .ok .release
  | "transport" => .ok .transport
  | "publication" => .ok .publication
  | _ => .error "external operation kind must be release, transport, or publication"

private def scopeFromJson (json : Json) : Except String Review.FrozenScope := do
  let designValue : Option Nat ← jsonField json "design" (Option Nat)
  let work : Nat ← jsonField json "work" Nat
  let phase ← jsonOptionalFieldD json "phase" String
  let repositorySnapshot : String ← jsonField json "repositorySnapshot" String
  let artifactDigest : String ← jsonField json "artifactDigest" String
  let purposeName : String ← jsonField json "purpose" String
  let purpose ← parsePurpose purposeName
  return {
    design := designValue.map (⟨·⟩)
    work := ⟨work⟩
    phase
    repositorySnapshot
    artifactDigest
    purpose
  }

private def readinessBasisFromJson (json : Json) : Except String Work.ReadinessBasis := do
  let design : Nat ← jsonField json "design" Nat
  let designRevision : Nat ← jsonField json "designRevision" Nat
  let decompositionKey : String ← jsonField json "decompositionKey" String
  let decompositionDigest : String ← jsonField json "decompositionDigest" String
  let repositorySnapshot : String ← jsonField json "repositorySnapshot" String
  let obligationKeys : List String ← jsonField json "obligationKeys" (List String)
  let evidenceRevision : Nat ← jsonField json "evidenceRevision" Nat
  let reviewPlan : Nat ← jsonField json "reviewPlan" Nat
  return {
    design := ⟨design⟩
    designRevision := ⟨designRevision⟩
    decompositionKey
    decompositionDigest
    repositorySnapshot
    obligationKeys
    evidenceRevision := ⟨evidenceRevision⟩
    reviewPlan := ⟨reviewPlan⟩
  }

private def commandFromJson (json : Json) : Except String (OperationId × Decide.Command) := do
  let operation : String ← jsonField json "operation" String
  let revisionValue : Nat ← jsonField json "expectedRevision" Nat
  let commandName : String ← jsonField json "command" String
  let revision : Revision := ⟨revisionValue⟩
  let command ← match commandName with
    | "register-work" => do
        let id : Nat ← jsonField json "work" Nat
        let owner : String ← jsonField json "owner" String
        let outcome : String ← jsonField json "outcome" String
        let completionBoundary : String ←
          jsonField json "completionBoundary" String
        pure <| .registerWork revision {
          id := ⟨id⟩
          status := .open
          owner
          outcome
          completionBoundary
        }
    | "register-follow-up" => do
        let source : Nat ← jsonField json "sourceWork" Nat
        let id : Nat ← jsonField json "work" Nat
        let activationId : Nat ← jsonField json "activation" Nat
        let owner : String ← jsonField json "owner" String
        let outcome : String ← jsonField json "outcome" String
        let completionBoundary : String ←
          jsonField json "completionBoundary" String
        let work : Work.WorkUnit := {
          id := ⟨id⟩
          status := .open
          owner
          outcome
          completionBoundary
        }
        let plan : Lifecycle.CompletionPlan := {
          work := work.id
          relatedWork := [{ work := ⟨source⟩, kind := .dependency }]
          phases := []
          tasks := []
          checklists := []
          reviews := []
          findings := []
          validations := []
          repositories := []
          corrections := []
          workRecords := []
        }
        let activation : Work.Activation := {
          id := ⟨activationId⟩
          work := work.id
          status := .active
          readyToResume := false
        }
        pure <| .registerFollowUp revision ⟨source⟩ work activation plan
    | "register-suspended-activation" => do
        let id : Nat ← jsonField json "activation" Nat
        let work : Nat ← jsonField json "work" Nat
        let parent : Option Nat ← jsonField json "parent" (Option Nat)
        let reason : String ← jsonField json "reason" String
        let returnPoint : String ← jsonField json "returnPoint" String
        let assumptions : List String ← jsonField json "assumptions" (List String)
        let resumeConditions : List String ←
          jsonField json "resumeConditions" (List String)
        pure <| .registerSuspendedActivation revision {
          id := ⟨id⟩
          work := ⟨work⟩
          status := .suspended
          readyToResume := false
          suspension := some { reason, returnPoint, assumptions, resumeConditions }
          parent := parent.map (⟨·⟩)
        }
    | "suspend-work" => do
        let work : Nat ← jsonField json "work" Nat
        let activation : Nat ← jsonField json "activation" Nat
        let reason : String ← jsonField json "reason" String
        let returnPoint : String ← jsonField json "returnPoint" String
        let assumptions : List String ← jsonField json "assumptions" (List String)
        let resumeConditions : List String ←
          jsonField json "resumeConditions" (List String)
        pure <| .suspendWork revision ⟨work⟩ ⟨activation⟩ {
          reason, returnPoint, assumptions, resumeConditions
        }
    | "revise-suspension" => do
        let work : Nat ← jsonField json "work" Nat
        let activation : Nat ← jsonField json "activation" Nat
        let reason : String ← jsonField json "reason" String
        let returnPoint : String ← jsonField json "returnPoint" String
        let assumptions : List String ← jsonField json "assumptions" (List String)
        let resumeConditions : List String ←
          jsonField json "resumeConditions" (List String)
        let basis ← readinessBasisFromJson json
        pure <| .reviseSuspension revision ⟨work⟩ ⟨activation⟩ {
          reason, returnPoint, assumptions, resumeConditions, basis := some basis
        }
    | "confirm-resume-readiness" => do
        let work : Nat ← jsonField json "work" Nat
        let activation : Nat ← jsonField json "activation" Nat
        let basis ← readinessBasisFromJson json
        pure <| .confirmResumeReadiness revision ⟨work⟩ ⟨activation⟩ basis
    | "import-design" => do
        let id : Nat ← jsonField json "design" Nat
        let designRevision : Nat ← jsonField json "designRevision" Nat
        let predecessor : Option Nat ← jsonField json "predecessor" (Option Nat)
        let owner : String ← jsonField json "owner" String
        let contentDigest : String ← jsonField json "contentDigest" String
        let requirementKeys : List String ← jsonField json "requirements" (List String)
        let negativeRequirements ←
          jsonListFieldD json "negativeRequirements" String
        let decisions : List String ← jsonField json "decisions" (List String)
        let validationGates : List String ←
          jsonField json "validationGates" (List String)
        let requirements := requirementKeys.map fun key =>
          ({ key
             active := true
             negativeValidationAuthority := negativeRequirements.contains key
           } : Design.Requirement)
        pure <| .importDesign revision {
          id := ⟨id⟩
          revision := ⟨designRevision⟩
          predecessor := predecessor.map (⟨·⟩)
          owner
          contentDigest
          requirements
          decisions
          validationGates
        }
    | "approve-design" => do
        let design : Nat ← jsonField json "design" Nat
        pure <| .approveDesign revision ⟨design⟩
    | "record-review-plan" => do
        let id : Nat ← jsonField json "plan" Nat
        let owner : String ← jsonField json "owner" String
        let reviewer : String ← jsonField json "reviewer" String
        let adjudicator : String ← jsonField json "adjudicator" String
        let caller : String ← jsonField json "caller" String
        let scope ← scopeFromJson json
        pure <| .recordReviewPlan revision {
          id := ⟨id⟩, owner, reviewer, adjudicator, caller, scope }
    | "record-review-claim" => do
        let id : Nat ← jsonField json "review" Nat
        let plan : Nat ← jsonField json "plan" Nat
        let work : Nat ← jsonField json "work" Nat
        let epoch : Nat ← jsonField json "epoch" Nat
        let claimName : String ← jsonField json "claim" String
        let reviewer : String ← jsonField json "reviewer" String
        let observationInputs ←
          jsonListFieldD json "observations" ObservationInput
        let observations ← observationInputs.mapM observationFromInput
        let scope ← scopeFromJson json
        let claim ← parseClaim claimName
        pure <| .recordReviewClaim revision {
          id := ⟨id⟩
          plan := ⟨plan⟩
          work := ⟨work⟩
          epoch := ⟨epoch⟩
          claim
          reviewer
          scope := some scope
          observations
        }
    | "record-review-adjudication" => do
        let review : Nat ← jsonField json "review" Nat
        let decisionName : String ← jsonField json "decision" String
        let adjudicator : String ← jsonField json "adjudicator" String
        let reason : String ← jsonField json "reason" String
        let dispositionInputs ←
          jsonListFieldD json "observations" DispositionInput
        let observations ← dispositionInputs.mapM dispositionFromInput
        let decision ← parseDecision decisionName
        pure <| .recordReviewAdjudication revision {
          review := ⟨review⟩
          decision
          adjudicator
          reason
          observations
        }
    | "record-review-finding" => do
        let key : String ← jsonField json "key" String
        let review : Nat ← jsonField json "review" Nat
        let blocking : Bool ← jsonField json "blocking" Bool
        let authority : String ← jsonField json "authority" String
        let failureAccount : String ← jsonField json "failureAccount" String
        let invariant : String ← jsonField json "invariant" String
        let remediationSurfaces : List String ←
          jsonField json "remediationSurfaces" (List String)
        pure <| .recordReviewFinding revision {
          key
          review := ⟨review⟩
          blocking
          authority
          failureAccount
          invariant
          remediationSurfaces
          accepted := false
          adjudicated := false
        }
    | "adjudicate-review-finding" => do
        let key : String ← jsonField json "key" String
        let principal : String ← jsonField json "adjudicator" String
        let reason : String ← jsonField json "reason" String
        let accepted : Bool ← jsonField json "accepted" Bool
        pure <| .adjudicateReviewFinding revision key principal reason accepted
    | "close-review-finding" => do
        let key : String ← jsonField json "key" String
        let attempt : Nat ← jsonField json "attempt" Nat
        let evidenceDigest : String ← jsonField json "evidenceDigest" String
        let repositorySnapshot : String ←
          jsonField json "repositorySnapshot" String
        pure <| .closeReviewFinding revision key {
          attempt, evidenceDigest, repositorySnapshot
        }
    | "verify-review-finding" => do
        let finding : String ← jsonField json "finding" String
        let attempt : Nat ← jsonField json "attempt" Nat
        let verifier : String ← jsonField json "verifier" String
        let evidenceDigest : String ← jsonField json "evidenceDigest" String
        let resultName : String ← jsonField json "result" String
        let scope ← scopeFromJson json
        let result ← parseVerificationResult resultName
        pure <| .verifyReviewFinding revision {
          finding
          attempt
          verifier
          scope
          evidenceDigest
          result
          accepted := false
        }
    | "adjudicate-finding-verification" => do
        let finding : String ← jsonField json "finding" String
        let attempt : Nat ← jsonField json "attempt" Nat
        let adjudicator : String ← jsonField json "adjudicator" String
        pure <| .adjudicateFindingVerification revision finding attempt adjudicator
    | "record-user-correction" => do
        let key : String ← jsonField json "key" String
        let scope : String ← jsonField json "scope" String
        let statement : String ← jsonField json "statement" String
        let work : Option Nat ← jsonField json "work" (Option Nat)
        let design : Option Nat ← jsonField json "design" (Option Nat)
        pure <| .recordUserCorrection revision {
          key
          scope
          statement
          resolved := false
          work := work.map (⟨·⟩)
          design := design.map (⟨·⟩)
        }
    | "record-kpt" => do
        let key : String ← jsonField json "key" String
        let work : Nat ← jsonField json "work" Nat
        let keep : List String ← jsonField json "keep" (List String)
        let problem : List String ← jsonField json "problem" (List String)
        let tryItems : List String ← jsonField json "try" (List String)
        let learningCandidate ←
          jsonOptionalFieldD json "learningCandidate" String
        pure <| .recordKpt revision {
          key
          work := ⟨work⟩
          keep
          problem
          tryItems
          learningCandidate
        }
    | "resolve-user-correction" => do
        let key : String ← jsonField json "key" String
        let reason : String ← jsonField json "reason" String
        pure <| .resolveUserCorrection revision key reason
    | "reject-user-proposal" => do
        let key : String ← jsonField json "key" String
        let reason : String ← jsonField json "reason" String
        pure <| .rejectUserProposal revision key reason
    | "record-authority-transition" => do
        let key : String ← jsonField json "key" String
        let correction : String ← jsonField json "correction" String
        let target : String ← jsonField json "target" String
        let operationName : String ← jsonField json "authorityOperation" String
        let kindName : String ← jsonField json "authorityKind" String
        let scope : String ← jsonField json "scope" String
        let work : Option Nat ← jsonField json "work" (Option Nat)
        let design : Option Nat ← jsonField json "design" (Option Nat)
        let lifetimeName : String ← jsonField json "lifetime" String
        let statement : String ← jsonField json "statement" String
        let reason : String ← jsonField json "reason" String
        let operation ← parseAuthorityOperation operationName
        let kind ← parseAuthorityKind kindName
        let lifetime ← parseAuthorityLifetime lifetimeName
        pure <| .recordAuthorityTransition revision {
          key
          correction
          target
          operation
          kind
          scope
          work := work.map (⟨·⟩)
          design := design.map (⟨·⟩)
          lifetime
          statement
          reason
        }
    | "record-decomposition" => do
        let key : String ← jsonField json "key" String
        let design : Nat ← jsonField json "design" Nat
        let work : Nat ← jsonField json "work" Nat
        let designRevision : Nat ← jsonField json "designRevision" Nat
        let contentDigest : String ← jsonField json "contentDigest" String
        let requirements : List String ← jsonField json "requirements" (List String)
        let implementationWork : List String ←
          jsonField json "implementationWork" (List String)
        let tasks : List String ← jsonField json "tasks" (List String)
        let completionChecks : List String ←
          jsonField json "completionChecks" (List String)
        let checklists : List String ← jsonField json "checklists" (List String)
        let validationGates : List String ←
          jsonField json "validationGates" (List String)
        let reviewer : String ← jsonField json "reviewer" String
        let adjudicator : String ← jsonField json "adjudicator" String
        let item : Design.TraceItem := {
          key
          requirements
          implementationWork
          tasks
          completionChecks
          checklists
          validationGates
        }
        pure <| .recordDecomposition revision {
          key
          design := ⟨design⟩
          work := ⟨work⟩
          designRevision := ⟨designRevision⟩
          contentDigest
          items := [item]
          reviewer
          adjudicator
          accepted := true
        }
    | "plan-completion" => do
        let work : Nat ← jsonField json "work" Nat
        let decomposition ← jsonOptionalFieldD json "decomposition" String
        let relatedWorkInputs : List RelatedWorkInput ←
          jsonField json "relatedWork" (List RelatedWorkInput)
        let relatedWork ← relatedWorkInputs.mapM relatedWorkFromInput
        let phaseInputs : List PhaseInput ← jsonField json "phases" (List PhaseInput)
        let phases := phaseInputs.map fun phase => ({
          key := phase.key
          group := phase.group
          order := phase.order
          dependencies := phase.dependencies
          tasks := phase.tasks
          reviews := phase.reviews.map (⟨·⟩)
        } : Lifecycle.PhaseSpec)
        let tasks : List String ← jsonField json "tasks" (List String)
        let checklists : List String ← jsonField json "checklists" (List String)
        let reviewValues : List Nat ← jsonField json "reviews" (List Nat)
        let obligations ← jsonListFieldD json "obligations" String
        let findings : List String ← jsonField json "findings" (List String)
        let validations : List String ← jsonField json "validations" (List String)
        let repositories : List String ← jsonField json "repositories" (List String)
        let corrections : List String ← jsonField json "corrections" (List String)
        let workRecords : List String ← jsonField json "workRecords" (List String)
        pure <| .planCompletion revision {
          work := ⟨work⟩
          decomposition
          relatedWork
          phases
          tasks
          checklists
          reviews := reviewValues.map (⟨·⟩)
          obligations
          findings
          validations
          repositories
          corrections
          workRecords
        }
    | "acknowledge-related-work-terminal" => do
        let work : Nat ← jsonField json "work" Nat
        let relatedWork : Nat ← jsonField json "relatedWork" Nat
        pure <| .acknowledgeRelatedWorkTerminal revision ⟨work⟩ ⟨relatedWork⟩
    | "record-scope-change" => do
        let key : String ← jsonField json "key" String
        let work : Nat ← jsonField json "work" Nat
        let kindName : String ← jsonField json "kind" String
        let causeName : String ← jsonField json "cause" String
        let principal : String ← jsonField json "principal" String
        let reason : String ← jsonField json "reason" String
        let sharedRecords : List String ←
          jsonField json "sharedRecords" (List String)
        let dependencies : List String ←
          jsonField json "dependencies" (List String)
        let dispositions : List String ←
          jsonField json "dispositions" (List String)
        let resultingScopeInputs : List ResultingScopeInput ←
          jsonField json "resultingScopes" (List ResultingScopeInput)
        let resultingScopes := resultingScopeInputs.map fun scope => ({
          key := scope.key
          work := ⟨scope.work⟩
          owner := scope.owner
          outcome := scope.outcome
          completionBoundary := scope.completionBoundary
        } : Lifecycle.ResultingScope)
        let kind ← match kindName with
          | "rescope" => pure Lifecycle.ScopeChangeKind.rescope
          | "split" => pure Lifecycle.ScopeChangeKind.split
          | _ => throw "scope change kind must be rescope or split"
        let cause ← match causeName with
          | "outcome" => pure Lifecycle.ScopeChangeCause.outcome
          | "owner" => pure Lifecycle.ScopeChangeCause.owner
          | "independent-lifecycle" =>
              pure Lifecycle.ScopeChangeCause.independentLifecycle
          | _ =>
              throw "scope change cause must be outcome, owner, or independent-lifecycle"
        pure <| .recordScopeChange revision {
          key
          work := ⟨work⟩
          kind
          cause
          principal
          reason
          sharedRecords
          dependencies
          dispositions
          resultingScopes
        }
    | "complete-phase" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        pure <| .completePhase revision ⟨work⟩ key
    | "complete-task" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        pure <| .completeTask revision ⟨work⟩ key
    | "complete-checklist" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        pure <| .completeChecklist revision ⟨work⟩ key
    | "resolve-finding" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        pure <| .resolveFinding revision ⟨work⟩ key
    | "pass-validation" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        let artifactDigest : String ← jsonField json "artifactDigest" String
        pure <| .passValidation revision ⟨work⟩ key artifactDigest
    | "classify-repository" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        let snapshotDigest : String ← jsonField json "snapshotDigest" String
        pure <| .classifyRepository revision ⟨work⟩ key snapshotDigest
    | "resolve-correction" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        pure <| .resolveCorrection revision ⟨work⟩ key
    | "link-work-record" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        let reference : String ← jsonField json "reference" String
        pure <| .linkWorkRecord revision ⟨work⟩ key reference
    | "record-repository-evidence" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        let repository : String ← jsonField json "repository" String
        let snapshot : String ← jsonField json "snapshot" String
        let commit : String ← jsonField json "commit" String
        let changedFiles : List String ← jsonField json "changedFiles" (List String)
        unless !repository.isEmpty && !snapshot.isEmpty && !commit.isEmpty &&
            !changedFiles.isEmpty &&
            changedFiles.all (fun file => !file.isEmpty) &&
            changedFiles.eraseDups.length == changedFiles.length do
          throw "repository evidence requires repository, snapshot, commit, and unique changed files"
        let reference := String.intercalate "\n" [
          s!"repository={repr repository}",
          s!"snapshot={repr snapshot}",
          s!"commit={repr commit}",
          s!"changed-files={repr changedFiles}"
        ]
        pure <| .linkWorkRecord revision ⟨work⟩ key reference
    | "record-obligation" => do
        let work : Nat ← jsonField json "work" Nat
        let key : String ← jsonField json "key" String
        let commandProfile : String ← jsonField json "commandProfile" String
        let invocation : String ← jsonField json "invocation" String
        let repository : String ← jsonField json "repository" String
        let snapshot : String ← jsonField json "snapshot" String
        let artifactDigest : String ← jsonField json "artifactDigest" String
        let kindName : String ← jsonField json "kind" String
        let requirements : List String ← jsonField json "requirements" (List String)
        let expectedProducer : String ← jsonField json "expectedProducer" String
        let expectedObservation : String ← jsonField json "expectedObservation" String
        let design : Nat ← jsonField json "design" Nat
        let designRevision : Nat ← jsonField json "designRevision" Nat
        let kind ← parseEvidenceKind kindName
        pure <| .recordObligation revision {
          work := ⟨work⟩
          key
          revision
          commandProfile
          invocation
          repository
          snapshot
          artifactDigest
          current := true
          kind
          requirements
          expectedProducer
          expectedObservation
          design := ⟨design⟩
          designRevision := ⟨designRevision⟩
        }
    | "record-evidence" => do
        let id : Nat ← jsonField json "evidence" Nat
        let work : Nat ← jsonField json "work" Nat
        let obligation : String ← jsonField json "obligation" String
        let observedRevision : Nat ← jsonField json "observedRevision" Nat
        let commandProfile : String ← jsonField json "commandProfile" String
        let invocation : String ← jsonField json "invocation" String
        let exitCode : Int ← jsonField json "exitCode" Int
        let repository : String ← jsonField json "repository" String
        let snapshot : String ← jsonField json "snapshot" String
        let artifactDigest : String ← jsonField json "artifactDigest" String
        let kindName : String ← jsonField json "kind" String
        let requirements : List String ← jsonField json "requirements" (List String)
        let producer : String ← jsonField json "producer" String
        let observedAt : String ← jsonField json "observedAt" String
        let design : Nat ← jsonField json "design" Nat
        let designRevision : Nat ← jsonField json "designRevision" Nat
        let kind ← parseEvidenceKind kindName
        pure <| .recordEvidence revision {
          id := ⟨id⟩
          work := ⟨work⟩
          obligation
          revision := ⟨observedRevision⟩
          commandProfile
          invocation
          exitCode
          repository
          snapshot
          artifactDigest
          current := true
          kind
          requirements
          producer
          observedAt
          design := ⟨design⟩
          designRevision := ⟨designRevision⟩
        }
    | "record-external-operation" => do
        let externalOperation : String ←
          jsonField json "externalOperation" String
        let target : String ← jsonField json "target" String
        let artifactDigest : String ← jsonField json "artifactDigest" String
        let expectedRemoteArtifactDigest : Option String ←
          jsonField json "expectedRemoteArtifactDigest" (Option String)
        let workValue ← jsonOptionalFieldD json "work" Nat
        let kindName : String ← jsonField json "kind" String
        let kind ← parseExternalOperationKind kindName
        pure <| .recordExternalOperation revision {
          operation := ⟨externalOperation⟩
          work := workValue.map (⟨·⟩)
          kind
          target := .confirmed target
          artifactDigest
          remotePrecondition := {
            expectedArtifactDigest := expectedRemoteArtifactDigest }
          state := .prepared
        }
    | "advance-external-operation" => do
        let externalOperation : String ←
          jsonField json "externalOperation" String
        let target : String ← jsonField json "target" String
        let artifactDigest : String ← jsonField json "artifactDigest" String
        let expectedRemoteArtifactDigest : Option String ←
          jsonField json "expectedRemoteArtifactDigest" (Option String)
        let workValue ← jsonOptionalFieldD json "work" Nat
        let kindName : String ← jsonField json "kind" String
        let stateName : String ← jsonField json "state" String
        let observationIdentity : Option String ←
          jsonField json "observationIdentity" (Option String)
        let observedArtifactDigest : Option String ←
          jsonField json "observedArtifactDigest" (Option String)
        let disposition : Option String ←
          jsonField json "disposition" (Option String)
        let state ← parseAttemptState stateName
        let kind ← parseExternalOperationKind kindName
        let observation := observationIdentity.map fun identity =>
          ({ identity, artifactDigest := observedArtifactDigest } :
            ExternalOperation.RemoteObservation)
        pure <| .advanceExternalOperation revision {
          operation := ⟨externalOperation⟩
          work := workValue.map (⟨·⟩)
          kind
          target := .confirmed target
          artifactDigest
          remotePrecondition := {
            expectedArtifactDigest := expectedRemoteArtifactDigest }
          state
          observation
          disposition
        }
    | "complete-work" => do
        let work : Nat ← jsonField json "work" Nat
        pure <| .completeWork revision ⟨work⟩
    | _ => throw s!"unsupported command: {commandName}"
  unless !operation.isEmpty do
    throw "operation must not be empty"
  return (⟨operation⟩, command)

private def loadStore (path : System.FilePath) : IO Projection.Store := do
  match ← Adapter.SQLite.inspect path with
  | .ok store => pure store
  | .error error => throw <| IO.userError s!"state open failed: {repr error}"

private def stateOutput (store : Projection.Store) : String :=
  match Projection.inspect store with
  | .fresh _ projection =>
      match projection.payload with
      | .decoded state =>
          let active := Work.activeActivations state.activations
          let openCorrections := state.corrections.filter (!·.resolved)
          let unresolvedFindings := state.reviewFindings.filter fun finding =>
            finding.blocking &&
              (!finding.adjudicated ||
                (finding.accepted &&
                  !state.claims.any (fun claim =>
                    claim.id == finding.review &&
                    state.findingVerifications.any fun verification =>
                      Review.verificationExact finding claim verification)))
          let activeText := match active.head? with
            | some activation =>
                s!"work={activation.work.value} activation={activation.id.value}"
            | none => "none"
          let lines := [
              "state: current",
              s!"revision: {state.revision.value}",
              s!"active: {activeText}",
              s!"open-corrections: {openCorrections.length}",
              s!"open-findings: {unresolvedFindings.length}"
            ] ++ openCorrections.map (fun correction =>
              s!"correction: key={correction.key} scope={correction.scope} work={repr (correction.work.map fun id => id.value)} design={repr (correction.design.map fun id => id.value)} statement={repr correction.statement}") ++
            unresolvedFindings.map (fun finding =>
              s!"finding: key={finding.key} authority={repr finding.authority} failure={repr finding.failureAccount}")
          String.intercalate "\n" lines
      | .decodeFailed reason => s!"state: unavailable\nreason: {repr reason}"
  | .missing _ _ => "state: repair-required\nreason: projection missing"
  | .stale _ _ _ => "state: repair-required\nreason: projection stale"
  | .corrupt _ _ _ _ => "state: repair-required\nreason: projection corrupt"
  | .ledgerCorrupt fault => s!"state: unavailable\nreason: {repr fault}"

private def nextOutput (path : System.FilePath) (store : Projection.Store) : String :=
  let pathArgument := repr path.toString
  match Resolver.next (Projection.inspect store) with
  | .blocked blocker =>
      let reason := match blocker with
        | .ledgerCorrupt _ => "authoritative ledger is corrupt"
        | .invalidState point =>
            s!"state invariants fail at revision {point.revision.value}"
        | .noActivation point =>
            s!"no active or resumable work at revision {point.revision.value}"
        | .noResumableActivation point candidates =>
            s!"no activation is ready to resume at revision {point.revision.value}; candidates={candidates.map fun (candidate : ActivationId) => candidate.value}"
        | .malformedInspection => "state projection cannot be decoded"
        | .nonExecutableAction _ => "resolved action is stale"
      s!"next: blocked\nreason: {reason}"
  | .action (.repairProjection _) =>
      "next: blocked\nreason: storage diagnosis changed while resolving; run next again"
  | .action (.initializeWork point) =>
      s!"next: executable\naction: start\nconstraints: revision={point.revision.value}\ncommand: agent-workbench --state {pathArgument} start {point.revision.value} <owner> <outcome> <completion-boundary>"
  | .action (.continueActiveWork point work activation) =>
      s!"next: executable\naction: continue\nconstraints: revision={point.revision.value} work={work.value} activation={activation.value}\ncommand: agent-workbench --state {pathArgument} continue {point.revision.value} {work.value} {activation.value}"
  | .action (.resumeSuspendedWork point work activation) =>
      s!"next: executable\naction: resume\nconstraints: revision={point.revision.value} work={work.value} activation={activation.value}\ncommand: agent-workbench --state {pathArgument} resume {point.revision.value} {work.value} {activation.value}"

private def startWork (path : System.FilePath) (expectedRevision : Nat)
    (owner outcome boundary : String) : IO Unit := do
  let command := Application.Service.bootstrapCommandAt ⟨expectedRevision⟩
  let command := match command with
    | .initializeWork revision work activation =>
        .initializeWork revision
          { work with owner, outcome, completionBoundary := boundary }
          activation
    | other => other
  match ← Adapter.SQLite.mutate path ⟨"start-work"⟩ ⟨expectedRevision⟩ command with
  | .ok result =>
      IO.println s!"accepted: start-work\nrevision: {result.store.ledger.storedHead.value}"
  | .error error => throw <| IO.userError s!"start rejected: {repr error}"

private def initializeStateArea (path : System.FilePath)
    (owner outcome boundary : String) : IO Unit := do
  if ← path.pathExists then
    throw <| IO.userError "state already exists"
  if let some parent := path.parent then
    IO.FS.createDirAll parent
  Adapter.SQLite.initializeStore path
  startWork path 0 owner outcome boundary
  IO.println s!"initialized: {path}"

private def continueWork (path : System.FilePath)
    (revisionValue workValue activationValue : Nat) : IO Unit := do
  let store ← loadStore path
  let action : Resolver.Action :=
    .continueActiveWork
      { ledger := store.ledger.id
        revision := ⟨revisionValue⟩
        historyDigest := store.ledger.storedHistoryDigest }
      ⟨workValue⟩ ⟨activationValue⟩
  if action.executable (Projection.inspect store) then
    IO.println s!"continued: work={workValue} activation={activationValue} revision={revisionValue}"
  else
    throw <| IO.userError "continue action is stale or does not match current state"

private def resumeWork (path : System.FilePath)
    (revisionValue workValue activationValue : Nat) : IO Unit := do
  let store ← loadStore path
  let point : Projection.LedgerPoint := {
    ledger := store.ledger.id
    revision := ⟨revisionValue⟩
    historyDigest := store.ledger.storedHistoryDigest
  }
  let action : Resolver.Action :=
    .resumeSuspendedWork point ⟨workValue⟩ ⟨activationValue⟩
  unless action.executable (Projection.inspect store) do
    throw <| IO.userError "resume action is stale or does not match current state"
  let operation : OperationId := ⟨s!"resume-{workValue}-{activationValue}-{revisionValue}"⟩
  let command := Decide.Command.resumeWork ⟨revisionValue⟩ ⟨workValue⟩ ⟨activationValue⟩
  match ← Adapter.SQLite.mutate path operation ⟨revisionValue⟩ command with
  | .ok result =>
      IO.println s!"resumed: work={workValue} activation={activationValue}\nrevision: {result.store.ledger.storedHead.value}"
  | .error error => throw <| IO.userError s!"resume rejected: {repr error}"

private def repair (path : System.FilePath) (revision : Nat)
    (historyDigest observedDigest : String) : IO Unit := do
  match ← Adapter.SQLite.diagnose path with
  | .error error => throw <| IO.userError s!"diagnosis failed: {repr error}"
  | .ok (.healthy _) =>
      throw <| IO.userError "repair rejected: projection repair is no longer required"
  | .ok (.projectionRepairRequired plan) =>
      unless plan.head.revision.value == revision &&
          plan.head.historyDigest.value == historyDigest &&
          plan.observedDigest == observedDigest do
        throw <| IO.userError "repair rejected: printed repair action is stale"
      match ← Adapter.SQLite.repairProjection path plan with
      | .ok receipt => IO.println s!"repaired: {receipt.adoptedDigest}"
      | .error error => throw <| IO.userError s!"repair failed: {repr error}"

private def backupRoot (path : System.FilePath) : System.FilePath :=
  path.parent.getD "." / "backups"

private def durabilityText :
    Adapter.DurableFilesystem.ReplacementDurability → String
  | .confirmed => "confirmed"
  | .uncertain => "uncertain"

private def parseDurability :
    String → Except String Adapter.DurableFilesystem.ReplacementDurability
  | "confirmed" => .ok .confirmed
  | "uncertain" => .ok .uncertain
  | _ => .error "durability must be confirmed or uncertain"

private def doctor (path : System.FilePath) : IO Unit := do
  match ← Adapter.SQLite.diagnose path with
  | .error error => throw <| IO.userError s!"diagnosis failed: {repr error}"
  | .ok (.healthy store) =>
      IO.println <| String.intercalate "\n" [
        "diagnosis: healthy",
        s!"revision: {store.ledger.storedHead.value}",
        s!"history-digest: {store.ledger.storedHistoryDigest.value}"
      ]
  | .ok (.projectionRepairRequired plan) =>
      IO.println <| String.intercalate "\n" [
        "diagnosis: projection-repair-required",
        s!"revision: {plan.head.revision.value}",
        s!"history-hex: {encodeHex plan.head.historyDigest.value}",
        s!"observed-hex: {encodeHex plan.observedDigest}"
      ]

private def inspectUpdate (path : System.FilePath) : IO Unit := do
  match ← Adapter.Update.inspect path with
  | .current point =>
      IO.println <| String.intercalate "\n" [
        "update: current",
        s!"schema: {point.schemaVersion}",
        s!"digest: {point.digest}"
      ]
  | .unsupported point =>
      throw <| IO.userError s!"update unsupported: schema={point.schemaVersion} digest={point.digest}"
  | .updateRequired plan =>
      IO.println <| String.intercalate "\n" [
        "update: required",
        s!"source-schema: {plan.source.schemaVersion}",
        s!"source-digest: {plan.source.digest}",
        s!"target-schema: {plan.targetVersion}",
        s!"arguments: update apply {plan.source.schemaVersion} {plan.source.digest} {plan.targetVersion}"
      ]

private def applyUpdate (path : System.FilePath) (sourceSchema : Nat)
    (sourceDigest : String) (targetSchema : Nat) : IO Unit := do
  let supplied : Adapter.Update.Plan := {
    source := { schemaVersion := sourceSchema, digest := sourceDigest }
    targetVersion := targetSchema
  }
  match ← Adapter.Update.inspect path with
  | .updateRequired current =>
      unless current = supplied do
        throw <| IO.userError "update rejected: supplied dry-run plan is stale"
  | .current _ =>
      throw <| IO.userError "update rejected: storage is already current"
  | .unsupported point =>
      throw <| IO.userError s!"update unsupported: schema={point.schemaVersion} digest={point.digest}"
  let receipt ← Adapter.Update.apply path (backupRoot path) supplied
  IO.println <| String.intercalate "\n" [
    "updated: true",
    s!"source-schema: {receipt.source.schemaVersion}",
    s!"source-digest: {receipt.source.digest}",
    s!"backup-digest: {receipt.backup.digest}",
    s!"backup-size: {receipt.backup.size}",
    s!"target-schema: {receipt.target.schemaVersion}",
    s!"target-digest: {receipt.target.digest}",
    s!"durability: {durabilityText receipt.targetDurability}",
    s!"restore-arguments: update restore {receipt.source.schemaVersion} {receipt.source.digest} {receipt.backup.digest} {receipt.backup.size} {receipt.target.schemaVersion} {receipt.target.digest} {durabilityText receipt.targetDurability}"
  ]

private def restoreUpdate (path : System.FilePath) (sourceSchema : Nat)
    (sourceDigest backupDigest : String) (backupSize targetSchema : Nat)
    (targetDigest : String)
    (durability : Adapter.DurableFilesystem.ReplacementDurability) : IO Unit := do
  let receipt : Adapter.Update.Receipt := {
    source := { schemaVersion := sourceSchema, digest := sourceDigest }
    backup := { digest := backupDigest, size := backupSize }
    target := { schemaVersion := targetSchema, digest := targetDigest }
    targetDurability := durability
  }
  let restored ← Adapter.Update.restore path (backupRoot path) receipt
  IO.println <| String.intercalate "\n" [
    "restored: true",
    s!"schema: {restored.restored.schemaVersion}",
    s!"digest: {restored.restored.digest}",
    s!"durability: {durabilityText restored.durability}"
  ]

private def currentState (store : Projection.Store) : IO Replay.State :=
  match Projection.inspect store with
  | .fresh _ projection =>
      match projection.payload with
      | .decoded state => pure state
      | .decodeFailed reason =>
          throw <| IO.userError s!"export unavailable: {repr reason}"
  | _ => throw <| IO.userError "export unavailable: storage requires repair"

private def backupSummary (path : System.FilePath) : IO String := do
  let root := backupRoot path
  if !(← root.pathExists) then return "[]"
  let entries ← root.readDir
  let names := entries.toList.filterMap fun entry =>
    let name := entry.fileName
    if name.startsWith "." then none else some name
  return s!"{repr names}"

private def exportClass (path : System.FilePath) (purpose className : String)
    (output : System.FilePath) : IO Unit := do
  if purpose.isEmpty then
    throw <| IO.userError "export purpose must not be empty"
  let store ← loadStore path
  let state ← currentState store
  let payload ← match className with
    | "ledger" => do
        let digest ← Adapter.DurableFilesystem.digest
          store.ledger.storedHistoryDigest.value.toUTF8
        pure s!"ledger={store.ledger.id.value}\nrevision={store.ledger.storedHead.value}\nhistory-digest={digest}"
    | "evidence" =>
        pure s!"obligations={repr state.obligations}\nevidence={repr state.evidence}"
    | "review" =>
        pure <| String.intercalate "\n" [
          s!"plans={repr state.reviewPlans}",
          s!"claims={repr state.claims}",
          s!"adjudications={repr state.adjudications}",
          s!"findings={repr state.reviewFindings}",
          s!"verifications={repr state.findingVerifications}"
        ]
    | "correction" =>
        let kpt := store.ledger.events.filterMap fun
          | .kptRecorded entry => some entry
          | _ => none
        pure <| String.intercalate "\n" [
          s!"corrections={repr state.corrections}",
          s!"authority-transitions={repr state.authorityTransitions}",
          s!"kpt={repr kpt}"
        ]
    | "backup" => backupSummary path
    | "design" =>
        pure <| String.intercalate "\n" [
          s!"designs={repr state.designs}",
          s!"approvals={repr state.designApprovals}",
          s!"decompositions={repr state.decompositions}"
        ]
    | _ =>
        throw <| IO.userError
          "export class must be ledger, evidence, review, correction, backup, or design"
  if let some parent := output.parent then
    Adapter.DurableFilesystem.ensureDirectory parent
  let content := String.intercalate "\n" [
    s!"purpose={purpose}",
    s!"class={className}",
    payload,
    ""
  ]
  let durability ← Adapter.DurableFilesystem.writeNew output content.toUTF8
  IO.println <|
    s!"exported: class={className} output={output} durability={durabilityText durability}"

private def nextForPath (path : System.FilePath) : IO String := do
  match ← Adapter.SQLite.diagnose path with
  | .error error => throw <| IO.userError s!"state diagnosis failed: {repr error}"
  | .ok (.healthy store) => pure (nextOutput path store)
  | .ok (.projectionRepairRequired plan) =>
      let pathArgument := repr path.toString
      let revision := plan.head.revision.value
      let history := encodeHex plan.head.historyDigest.value
      let observed := encodeHex plan.observedDigest
      pure <| String.intercalate "\n" [
        "next: executable",
        "action: repair",
        s!"constraints: revision={revision} history-hex={history} observed-hex={observed}",
        s!"command: agent-workbench --state {pathArgument} repair {revision} {history} {observed}"
      ]

private def applyFile (path requestPath : System.FilePath) : IO Unit := do
  let source ← IO.FS.readFile requestPath
  let json ← match Json.parse source with
    | .ok value => pure value
    | .error reason => throw <| IO.userError s!"invalid request JSON: {reason}"
  let (operation, command) ← match commandFromJson json with
    | .ok value => pure value
    | .error reason => throw <| IO.userError s!"invalid request: {reason}"
  match ← Adapter.SQLite.mutate path operation command.expectedRevision command with
  | .ok result =>
      IO.println <| String.intercalate "\n" [
        s!"accepted: {operation.value}",
        s!"revision: {result.store.ledger.storedHead.value}",
        s!"exact-retry: {result.exactRetry}"
      ]
  | .error error => throw <| IO.userError s!"request rejected: {repr error}"

private def dispatch (path : System.FilePath) : List String → IO Unit
  | ["init", owner, outcome, boundary] =>
      initializeStateArea path owner outcome boundary
  | ["start", revision, owner, outcome, boundary] => do
      let revision ← fromExcept (parseNat "revision" revision)
      startWork path revision owner outcome boundary
  | ["status"] => loadStore path >>= fun store => IO.println (stateOutput store)
  | ["next"] => nextForPath path >>= IO.println
  | ["continue", revision, work, activation] => do
      let revision ← fromExcept (parseNat "revision" revision)
      let work ← fromExcept (parseNat "work" work)
      let activation ← fromExcept (parseNat "activation" activation)
      continueWork path revision work activation
  | ["resume", revision, work, activation] => do
      let revision ← fromExcept (parseNat "revision" revision)
      let work ← fromExcept (parseNat "work" work)
      let activation ← fromExcept (parseNat "activation" activation)
      resumeWork path revision work activation
  | ["doctor"] => doctor path
  | ["repair", revision, historyHex, observedHex] => do
      let revision ← fromExcept (parseNat "revision" revision)
      let historyDigest ← fromExcept (decodeHex "history-hex" historyHex)
      let observedDigest ← fromExcept (decodeHex "observed-hex" observedHex)
      repair path revision historyDigest observedDigest
  | ["update", "inspect"] => inspectUpdate path
  | ["update", "apply", sourceSchema, sourceDigest, targetSchema] => do
      let sourceSchema ← fromExcept (parseNat "source-schema" sourceSchema)
      let targetSchema ← fromExcept (parseNat "target-schema" targetSchema)
      applyUpdate path sourceSchema sourceDigest targetSchema
  | ["update", "restore", sourceSchema, sourceDigest, backupDigest, backupSize,
      targetSchema, targetDigest, durability] => do
      let sourceSchema ← fromExcept (parseNat "source-schema" sourceSchema)
      let backupSize ← fromExcept (parseNat "backup-size" backupSize)
      let targetSchema ← fromExcept (parseNat "target-schema" targetSchema)
      let durability ← fromExcept (parseDurability durability)
      restoreUpdate path sourceSchema sourceDigest backupDigest backupSize
        targetSchema targetDigest durability
  | ["export", purpose, className, output] =>
      exportClass path purpose className output
  | ["apply", request] => applyFile path request
  | ["help"] | ["--help"] | ["-h"] => IO.println usage
  | _ => throw <| IO.userError usage

def run (args : List String) : IO Unit := do
  match args with
  | "--state" :: path :: arguments => dispatch path arguments
  | ["--version"] | ["version"] => IO.println s!"agent-workbench {version}"
  | ["help"] | ["--help"] | ["-h"] => IO.println usage
  | _ => throw <| IO.userError usage

end AgentWorkbench.Cli.Program
