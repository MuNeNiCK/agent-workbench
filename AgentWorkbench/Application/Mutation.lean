import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Plan
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Profile
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Guidance
import AgentWorkbench.Application.Review
import AgentWorkbench.Domain.Operation
import AgentWorkbench.Decision.Operation

namespace AgentWorkbench

/-- Every state-changing public request. Queries are intentionally a separate CLI path. -/
inductive Mutation where
  | init
  | designPropose (request : DesignProposalRequest)
  | designAmend (request : DesignProposalRequest)
  | designAccept (designId : String)
  | designReject (request : DesignRejectRequest)
  | workStart (request : WorkStartRequest)
  | workFocus (workId : String)
  | workSuspend (workId resumeCondition : String)
  | workResume (request : WorkResumeRequest)
  | workHandoff (workId entryId successorRun reason : String)
  | workAdoptDesign (request : WorkAdoptDesignRequest)
  | workWithdraw (request : WorkWithdrawRequest)
  | workComplete
  | planPropose (request : PlanProposalRequest)
  | planReplace (request : PlanProposalRequest)
  | planMaterialize (planId : String)
  | taskClose (request : TaskCloseRequest)
  | taskReopenStale
  | profileDefine (request : ProfileDefineRequest)
  | profileReplace (request : ProfileReplaceRequest)
  | commandRun (request : CommandRunRequest)
  | artifactObserve (request : ArtifactObserveRequest)
  | proofRun (request : ProofRunRequest)
  | correctionRecord (request : CorrectionRecordRequest)
  | correctionSupersede (request : CorrectionSupersedeRequest)
  | correctionResolve (request : CorrectionResolveRequest)
  | correctionIncorporate (request : CorrectionIncorporateRequest)
  | kptRecord (request : KptRecordRequest)
  | kptApply (request : KptApplyRequest)
  | reviewStart (request : ReviewStartRequest)
  | reviewResume (request : ReviewResumeRequest)
  | reviewHandoff (request : ReviewHandoffRequest)
  | reviewFinding (request : FindingRecordRequest)
  | reviewDisposition (request : DispositionRecordRequest)
  | reviewConclude (request : ReviewConclusionRequest)
  | reviewVerify (request : VerificationRecordRequest)
  deriving Repr

def Mutation.operation : Mutation → Operation
  | .init => .init
  | .designPropose _ => .designPropose
  | .designAmend _ => .designAmend
  | .designAccept _ => .designAccept
  | .designReject _ => .designReject
  | .workStart _ => .workStart
  | .workFocus _ => .workFocus
  | .workSuspend _ _ => .workSuspend
  | .workResume _ => .workResume
  | .workHandoff _ _ _ _ => .workHandoff
  | .workAdoptDesign _ => .workAdoptDesign
  | .workWithdraw _ => .workWithdraw
  | .workComplete => .workComplete
  | .planPropose _ => .planPropose
  | .planReplace _ => .planReplace
  | .planMaterialize _ => .planMaterialize
  | .taskClose _ => .taskClose
  | .taskReopenStale => .taskReopenStale
  | .profileDefine _ => .profileDefine
  | .profileReplace _ => .profileReplace
  | .commandRun _ => .commandRun
  | .artifactObserve _ => .artifactObserve
  | .proofRun _ => .proofRun
  | .correctionRecord _ => .correctionRecord
  | .correctionSupersede _ => .correctionSupersede
  | .correctionResolve _ => .correctionResolve
  | .correctionIncorporate _ => .correctionIncorporate
  | .kptRecord _ => .kptRecord
  | .kptApply _ => .kptApply
  | .reviewStart _ => .reviewStart
  | .reviewResume _ => .reviewResume
  | .reviewHandoff _ => .reviewHandoff
  | .reviewFinding _ => .reviewFinding
  | .reviewDisposition _ => .reviewDisposition
  | .reviewConclude _ => .reviewConclude
  | .reviewVerify _ => .reviewVerify

/-- Public mutations whose observation has already been captured execute these actual production
transitions. `none` means that the Store must first obtain the classified external observation. -/
def Mutation.pureTransition? : Mutation → Option (ProjectState → Except String ProjectState)
  | .init => some (fun state => validated { state with revision := state.revision + 1 })
  | .designAccept designId => some (fun state => acceptDesign state designId)
  | .designReject request => some (fun state => rejectDesign state request)
  | .workStart request => some (fun state => startWorkRequest state request)
  | .workFocus workId => some (fun state => focusWork state workId)
  | .workSuspend workId condition => some (fun state => suspendWork state workId condition)
  | .workResume request => some (fun state => resumeWork state request)
  | .workHandoff workId entryId successorRun reason =>
      some (fun state => handoffWork state workId entryId successorRun reason)
  | .workAdoptDesign request => some (fun state => adoptDesignForWork state request)
  | .workWithdraw request => some (fun state => withdrawWork state request)
  | .profileDefine request => some (fun state => defineProfile state request)
  | .profileReplace request => some (fun state => replaceProfile state request)
  | .correctionRecord request => some (fun state => recordCorrection state request)
  | .correctionSupersede request => some (fun state => supersedeCorrection state request)
  | .correctionResolve request => some (fun state => resolveCorrection state request)
  | .correctionIncorporate request => some (fun state => incorporateCorrection state request)
  | .kptRecord request => some (fun state => recordKpt state request)
  | .kptApply request => some (fun state => applyKpt state request)
  | .reviewHandoff request => some (fun state => handoffReview state request)
  | .reviewFinding request => some (fun state => recordFinding state request)
  | .reviewDisposition request => some (fun state => recordDisposition state request)
  | .reviewConclude request => some (fun state => concludeReview state request)
  | .reviewVerify request => some (fun state => recordVerification state request)
  | .designPropose _ | .designAmend _ | .workComplete
  | .planPropose _ | .planReplace _ | .planMaterialize _ | .taskClose _ | .taskReopenStale
  | .commandRun _ | .artifactObserve _ | .proofRun _
  | .reviewStart _ | .reviewResume _ => none

inductive MutationResultShape where
  | state | context
  deriving Repr, DecidableEq

def Mutation.pureResultShape : Mutation → MutationResultShape
  | .workStart _ | .workFocus _ | .workResume _ | .workHandoff _ _ _ _ => .context
  | .designAccept _ | .designReject _ | .workSuspend _ _ | .workAdoptDesign _
  | .workWithdraw _ | .taskClose _ | .taskReopenStale | .profileDefine _ | .profileReplace _
  | .correctionRecord _ | .correctionSupersede _ | .correctionResolve _
  | .correctionIncorporate _ | .kptRecord _ | .kptApply _ | .reviewHandoff _
  | .reviewFinding _ | .reviewDisposition _ | .reviewConclude _ | .reviewVerify _
  | .init | .designPropose _ | .designAmend _ | .workComplete | .planPropose _
  | .planReplace _ | .planMaterialize _ | .commandRun _ | .artifactObserve _
  | .proofRun _ | .reviewStart _ | .reviewResume _ => .state

def workIdentityPreserved (prior next : ProjectState) : Bool :=
  prior.works.all fun old => next.works.any fun current =>
    current.id == old.id && current.outcome == old.outcome && current.scope == old.scope &&
      current.baselineDesignRevision == old.baselineDesignRevision

def immutableHistoryPreserved (prior next : ProjectState) : Bool :=
  next.ledgerEntries.take prior.ledgerEntries.length == prior.ledgerEntries &&
  prior.designRevisions.all fun old => next.designRevisions.any fun current =>
    current.id == old.id && { old with status := current.status } == current

def pureTransitionPostcondition (prior next : ProjectState) : Bool :=
  next.revision == prior.revision + 1 && workIdentityPreserved prior next &&
    immutableHistoryPreserved prior next

/-- Closed top-level authority surface. A mutation may change only components
listed for its public semantic operation. This is checked before commit and is
also consumed by the private exhaustive Lean proof. -/
inductive StateComponent where
  | acceptedDesign | focusedWork | designs | works | plans | ledger
  deriving Repr, DecidableEq

def StateComponent.all : List StateComponent :=
  [.acceptedDesign, .focusedWork, .designs, .works, .plans, .ledger]

def Operation.permittedStateComponents : Operation → List StateComponent
  | .init => []
  | .designPropose | .designAmend => [.designs]
  | .designAccept => [.acceptedDesign, .designs, .works]
  | .designReject => [.designs, .ledger]
  | .workStart => [.focusedWork, .works]
  | .workFocus => [.focusedWork]
  | .workSuspend => [.focusedWork, .works]
  | .workResume => [.focusedWork, .works, .ledger]
  | .workHandoff | .workAdoptDesign => [.works, .ledger]
  | .workWithdraw | .workComplete => [.focusedWork, .works, .ledger]
  | .planPropose | .planReplace => [.plans]
  | .planMaterialize => [.plans, .ledger]
  | .taskClose | .taskReopenStale | .profileDefine | .profileReplace | .artifactObserve
  | .correctionRecord | .correctionSupersede | .correctionResolve | .correctionIncorporate
  | .kptRecord | .kptApply | .reviewStart | .reviewResume | .reviewHandoff
  | .reviewFinding | .reviewConclude | .reviewVerify
  | .commandRun | .proofRun => [.ledger]
  | .reviewDisposition => [.focusedWork, .works, .ledger]
  | .describe | .designGet | .designInspectSources | .designSource | .designDiff
  | .designExport | .workGet | .workAdoptionImpact | .planGet | .planInspectSources
  | .planSource | .planDiff | .planExport | .reviewContext | .reviewInspect
  | .entryGet | .history | .context | .ready | .commandShow | .proofDigest => []

def stateComponentUnchanged
    (component : StateComponent) (prior next : ProjectState) : Bool :=
  match component with
  | .acceptedDesign => prior.acceptedDesignId == next.acceptedDesignId
  | .focusedWork => prior.focusedWorkId == next.focusedWorkId
  | .designs => prior.designRevisions == next.designRevisions
  | .works => prior.works == next.works
  | .plans => prior.implementationPlans == next.implementationPlans
  | .ledger => prior.ledgerEntries == next.ledgerEntries

def transitionEffectsPermitted
    (operation : Operation) (prior next : ProjectState) : Bool :=
  StateComponent.all.all fun component =>
    operation.permittedStateComponents.contains component ||
      stateComponentUnchanged component prior next

def semanticTransitionPostcondition
    (operation : Operation) (prior next : ProjectState) : Bool :=
  pureTransitionPostcondition prior next && transitionEffectsPermitted operation prior next

def Mutation.executePure (mutation : Mutation) (state : ProjectState) : Except String ProjectState :=
  match mutation.pureTransition? with
  | none => .error s!"mutation {mutation.operation.name} requires an external observation"
  | some transition =>
      match transition state with
      | .error message => .error message
      | .ok next =>
          if semanticTransitionPostcondition mutation.operation state next then validated next
          else .error "pure mutation violated its revision, authority, history, or effect boundary"

/-- A public mutation after every required external observation has been fixed as immutable input. -/
inductive PreparedMutation where
  | direct (mutation : Mutation)
  | designPropose (candidate : DesignRevision)
  | designAmend (candidate : DesignRevision)
  | planPropose (candidate : ImplementationPlan)
  | planReplace (candidate : ImplementationPlan)
  | planMaterialize (planId : String) (observations : List TargetObservation)
      (claimDigests : List CurrentClaimDigest)
  | taskClose (request : TaskCloseRequest) (observations : List TargetObservation)
  | taskReopenStale (observations : List TargetObservation)
  | workComplete (observations : List TargetObservation) (claimDigests : List CurrentClaimDigest)
      (inputDigest : String)
  | artifactObservation (entry : LedgerEntry)
  | commandExecution (entry : LedgerEntry)
  | proofReceipt (entry : LedgerEntry)
  | reviewStart (entry : LedgerEntry)
  | reviewResume (entry : LedgerEntry)
  deriving Repr

def PreparedMutation.operation : PreparedMutation → Operation
  | .direct mutation => mutation.operation
  | .designPropose _ => .designPropose
  | .designAmend _ => .designAmend
  | .planPropose _ => .planPropose
  | .planReplace _ => .planReplace
  | .planMaterialize _ _ _ => .planMaterialize
  | .taskClose _ _ => .taskClose
  | .taskReopenStale _ => .taskReopenStale
  | .workComplete _ _ _ => .workComplete
  | .artifactObservation _ => .artifactObserve
  | .commandExecution _ => .commandRun
  | .proofReceipt _ => .proofRun
  | .reviewStart _ => .reviewStart
  | .reviewResume _ => .reviewResume

private def appendObserved
    (state : ProjectState) (entry : LedgerEntry)
    (accepts : EntryPayload → Bool) (kind : String) : Except String ProjectState := do
  if !accepts entry.payload then throw s!"prepared {kind} has the wrong payload"
  appendEntry state entry

def PreparedMutation.transition
    (prepared : PreparedMutation) (state : ProjectState) : Except String ProjectState :=
  match prepared with
  | .direct mutation => mutation.executePure state
  | .designPropose candidate | .designAmend candidate => proposeDesign state candidate
  | .planPropose candidate | .planReplace candidate => proposePlan state candidate
  | .planMaterialize planId observations digests =>
      materializePlan state planId observations digests
  | .taskClose request observations => closeTask state observations request
  | .taskReopenStale observations => reopenStaleTasks state observations
  | .workComplete observations digests inputDigest =>
      completeFocusedWork state observations digests inputDigest
  | .artifactObservation entry =>
      appendObserved state entry (fun | .artifactObservation _ => true | _ => false) "artifact observation"
  | .commandExecution entry =>
      appendObserved state entry (fun | .commandExecution _ => true | _ => false) "command execution"
  | .proofReceipt entry =>
      appendObserved state entry (fun | .leanProofReceipt _ => true | _ => false) "proof receipt"
  | .reviewStart entry =>
      appendObserved state entry (fun | .review value => value.context == .fresh | _ => false) "fresh Review"
  | .reviewResume entry =>
      appendObserved state entry (fun | .review value => value.context == .resume | _ => false) "resumed Review"

def PreparedMutation.execute
    (prepared : PreparedMutation) (state : ProjectState) : Except String ProjectState :=
  match prepared.transition state with
  | .error message => .error message
  | .ok next =>
      if semanticTransitionPostcondition prepared.operation state next then validated next
      else .error "prepared mutation violated its revision, authority, history, or effect boundary"

def PreparedMutation.currentObservations : PreparedMutation → List TargetObservation
  | .planMaterialize _ values _ | .taskClose _ values | .taskReopenStale values |
      .workComplete values _ _ => values
  | _ => []

def PreparedMutation.currentClaimDigests : PreparedMutation → List CurrentClaimDigest
  | .planMaterialize _ _ values | .workComplete _ values _ => values
  | _ => []

def PreparedMutation.executeApplicable
    (prepared : PreparedMutation) (state : ProjectState) : Except String ProjectState :=
  if operationApplicable state prepared.currentObservations prepared.currentClaimDigests
      prepared.operation then prepared.execute state
  else .error s!"operation is not applicable in the current state: {prepared.operation.name}"

inductive MutationResult where
  | state (value : ProjectState)
  | context (value : ProjectState)
  | design (value : DesignRevision)
  | plan (value : ImplementationPlan)
  | command (value : CommandRunResult)
  | proof (value : ProofRunResult)

end AgentWorkbench
