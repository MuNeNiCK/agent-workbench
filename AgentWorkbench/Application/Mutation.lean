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
  | workBindRemediation (request : WorkRemediationBindingRequest)
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
  | .workBindRemediation _ => .workBindRemediation
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
  | .workBindRemediation request => some (fun state => bindRemediationWork state request)
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
  | .workBindRemediation _
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

def completedWorkAuthorityPreserved (prior next : ProjectState) : Bool :=
  prior.works.all fun old =>
    old.status != .completed ||
      ((next.work? old.id).any (·.status == .completed) &&
        workCompletionEntries next old == workCompletionEntries prior old)

def pureTransitionPostcondition (prior next : ProjectState) : Bool :=
  next.revision == prior.revision + 1 && workIdentityPreserved prior next &&
    immutableHistoryPreserved prior next && completedWorkAuthorityPreserved prior next

private def designStatusEffect
    (prior next : DesignStatus) : ProductionEffect :=
  match prior, next with
  | .candidate, .accepted => .designCandidateAccepted
  | .accepted, .superseded => .designAcceptedSuperseded
  | .candidate, .superseded => .designCandidateSuperseded
  | .candidate, .rejected => .designCandidateRejected
  | _, _ => .invalidStateChange

private def workStatusEffect
    (prior next : WorkStatus) : ProductionEffect :=
  match prior, next with
  | .active, .suspended => .workActiveSuspended
  | .suspended, .active => .workSuspendedActive
  | .active, .withdrawn => .workActiveWithdrawn
  | .suspended, .withdrawn => .workSuspendedWithdrawn
  | .active, .completed => .workActiveCompleted
  | _, _ => .invalidStateChange

private def planStatusEffect
    (prior next : PlanStatus) : ProductionEffect :=
  match prior, next with
  | .candidate, .superseded => .planCandidateSuperseded
  | .candidate, .current => .planCandidateCurrent
  | .current, .superseded => .planCurrentSuperseded
  | _, _ => .invalidStateChange

private def designEffects (prior next : ProjectState) : List ProductionEffect :=
  let existing := prior.designRevisions.flatMap fun old =>
    match next.design? old.id with
    | none => [.invalidStateChange]
    | some current =>
        if old == current then []
        else if { old with status := current.status } == current then
          [designStatusEffect old.status current.status]
        else [.invalidStateChange]
  let inserted := next.designRevisions.filterMap fun current =>
    if (prior.design? current.id).isSome then none else some .designInserted
  existing ++ inserted

private def workEffects (prior next : ProjectState) : List ProductionEffect :=
  let existing := prior.works.flatMap fun old =>
    match next.work? old.id with
    | none => [.invalidStateChange]
    | some current =>
        let normalized := { old with
          designRevision := current.designRevision
          status := current.status
          responsibleAgentRun := current.responsibleAgentRun
          resumeCondition := current.resumeCondition }
        if normalized != current then [.invalidStateChange]
        else
          (if old.designRevision != current.designRevision then [.workDesignChanged] else []) ++
          (if old.status != current.status then
            [workStatusEffect old.status current.status] else []) ++
          (if old.responsibleAgentRun != current.responsibleAgentRun then
            [.workResponsibleChanged] else []) ++
          (if old.resumeCondition != current.resumeCondition then
            [.workResumeConditionChanged] else [])
  let inserted := next.works.filterMap fun current =>
    if (prior.work? current.id).isSome then none else some .workInserted
  existing ++ inserted

private def planEffects (prior next : ProjectState) : List ProductionEffect :=
  let existing := prior.implementationPlans.flatMap fun old =>
    match next.plan? old.id with
    | none => [.invalidStateChange]
    | some current =>
        if old == current then []
        else if { old with status := current.status } == current then
          [planStatusEffect old.status current.status]
        else [.invalidStateChange]
  let inserted := next.implementationPlans.filterMap fun current =>
    if (prior.plan? current.id).isSome then none else some .planInserted
  existing ++ inserted

private def ledgerEffects (prior next : ProjectState) : List ProductionEffect :=
  if next.ledgerEntries.take prior.ledgerEntries.length != prior.ledgerEntries then
    [.invalidStateChange]
  else next.ledgerEntries.drop prior.ledgerEntries.length |>.map fun _ => .ledgerAppended

/-- Actual production effects are derived from complete before/after records, not asserted by the
operation implementation. Any unclassified mutation of an existing record becomes an invalid
effect and cannot be committed. -/
def actualProductionEffects (prior next : ProjectState) : List ProductionEffect :=
  (if next.revision == prior.revision + 1 then [.stateRevisionAdvanced]
    else [.invalidStateChange]) ++
  (if prior.acceptedDesignId != next.acceptedDesignId then [.acceptedDesignChanged] else []) ++
  (if prior.focusedWorkId != next.focusedWorkId then [.focusedWorkChanged] else []) ++
  designEffects prior next ++ workEffects prior next ++ planEffects prior next ++
    ledgerEffects prior next

def productionEffectsPermitted
    (operation : Operation) (prior next : ProjectState) : Bool :=
  (actualProductionEffects prior next).all
    operation.permittedProductionEffects.contains

def semanticTransitionPostcondition
    (operation : Operation) (prior next : ProjectState) : Bool :=
  pureTransitionPostcondition prior next && productionEffectsPermitted operation prior next

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
      (input : CompletionInput) (inputDigest : String)
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
  | .workComplete _ _ _ _ => .workComplete
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
  | .workComplete observations digests input inputDigest =>
      completeFocusedWork state observations digests input inputDigest
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
      .workComplete values _ _ _ => values
  | _ => []

def PreparedMutation.currentClaimDigests : PreparedMutation → List CurrentClaimDigest
  | .planMaterialize _ _ values | .workComplete _ values _ _ => values
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
