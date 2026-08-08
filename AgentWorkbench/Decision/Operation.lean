import AgentWorkbench.Decision.Finding
import AgentWorkbench.Decision.Completion
import AgentWorkbench.Decision.PlanCoverage
import AgentWorkbench.Domain.Operation

namespace AgentWorkbench

private def currentHasEntry
    (state : ProjectState) (predicate : EntryPayload → Bool) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection => projection.entries.any (fun entry => predicate entry.payload)

private def hasDependencyReadyTask (state : ProjectState) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection =>
      (state.currentPlanFor? projection.work.id).isSome && projection.entries.any fun entry =>
        match entry.payload with
        | .task task => task.planId.isSome && task.required && !task.closed && !task.retired &&
            task.dependencyLineageIds.all fun dependency => projection.entries.any fun candidate =>
              match candidate.payload with
              | .task dependencyTask =>
                  dependencyTask.lineageId == some dependency && dependencyTask.closed &&
                    !dependencyTask.retired
              | _ => false
        | _ => false

def planMaterializationStructurallyReady (state : ProjectState) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection =>
      match state.implementationPlans.find? fun plan =>
          plan.workId == projection.work.id && plan.designRevision == projection.design.id &&
            plan.status == .candidate &&
            !(state.implementationPlans.any fun successor =>
              successor.predecessorPlanId == some plan.id && successor.status == .candidate) with
      | none => false
      | some candidate =>
          let requiredFindings := acceptedImplementationFindingIds
            state projection.work.id projection.design.id
          let coveredFindings := candidate.steps.flatMap (·.acceptedFindingEntryIds)
          projection.design.leanClaims.all (claimReceiptRecorded projection) &&
            requiredFindings.all coveredFindings.contains

def completionStructurallyReady (state : ProjectState) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection =>
      projection.design.sourceArchiveAvailable &&
      (state.currentPlanFor? projection.work.id).isSome &&
      projection.entries.all (fun entry => match entry.payload with
        | .task task => !task.required || (task.closed &&
            task.verificationTaskEntryId.isSome &&
            !task.verificationEvidenceEntryIds.isEmpty)
        | .userCorrection correction => correction.resolvedByEntryId.isSome ||
            correction.incorporatedIn == some projection.design.id
        | .finding _ =>
            if !state.findingAccepted entry.id projection.work.id then true else
            projection.entries.any fun candidate => match candidate.payload with
              | .reviewVerification verification =>
                  verification.findingEntryId == entry.id && verification.resolved
              | _ => false
        | _ => true) &&
      projection.design.acceptanceCriteria.all (criterionEvidenceRecorded projection) &&
      projection.design.leanClaims.all (claimReceiptRecorded projection)

def operationStructurallyApplicable (state : ProjectState) (operation : Operation) : Bool :=
  let current := (currentProjection? state).isSome
  let unfocused := state.focusedWorkId.isNone
  let focusedWork := state.currentWork?.isSome
  match operation with
  | .init => state.revision == 0 && state.acceptedDesignId.isNone &&
      state.focusedWorkId.isNone && state.designRevisions.isEmpty && state.works.isEmpty &&
      state.implementationPlans.isEmpty && state.ledgerEntries.isEmpty
  | .describe | .designGet | .designInspectSources |
      .designSource | .designDiff | .designExport | .planGet | .planInspectSources |
      .planSource | .planDiff | .planExport | .workGet | .entryGet | .history |
      .context | .ready | .reviewContext | .reviewInspect | .workAdoptionImpact => true
  | .designPropose => focusedWork
  | .designAmend => focusedWork && state.designRevisions.any (fun design =>
      design.status == .candidate && design.workId == state.focusedWorkId)
  | .designAccept => state.designRevisions.any (fun design =>
      design.status == .candidate && design.parent == state.acceptedDesignId &&
        ((design.parent.isNone && state.focusedWorkId == design.workId) ||
          (design.parent.isSome && unfocused)))
  | .designReject => focusedWork && state.designRevisions.any (fun design =>
      design.status == .candidate && design.workId == state.focusedWorkId)
  | .workStart => unfocused
  | .workFocus => unfocused && state.works.any (fun work =>
      work.status == .active && work.designRevision == state.acceptedDesignId)
  | .workResume => unfocused && state.works.any (fun work =>
      work.status == .suspended && work.designRevision == state.acceptedDesignId)
  | .workAdoptDesign => unfocused && state.acceptedDesignId.isSome &&
      state.works.any (fun work => work.status == .suspended &&
        work.designRevision != state.acceptedDesignId)
  | .workSuspend | .workHandoff => focusedWork
  | .workWithdraw => state.works.any (fun work =>
      (work.status == .active || work.status == .suspended) &&
      state.ledgerEntries.any fun entry =>
        entry.workId == some work.id && !entryIsSuperseded state entry &&
        match entry.payload with
        | .userCorrection value => value.resolvedByEntryId.isNone && value.incorporatedIn.isNone
        | _ => false)
  | .planPropose => current && state.currentWork?.any (fun work =>
      !state.implementationPlans.any (·.workId == work.id))
  | .planReplace => current && state.currentWork?.any (fun work =>
      state.implementationPlans.any fun plan =>
        plan.workId == work.id && (plan.status == .current || plan.status == .candidate))
  | .planMaterialize => planMaterializationStructurallyReady state
  | .correctionRecord | .kptRecord => focusedWork
  | .workComplete => completionStructurallyReady state
  | .reviewStart => current || (focusedWork && state.designRevisions.any fun design =>
      design.status == .candidate && design.workId == state.focusedWorkId)
  | .taskReopenStale => current
  | .profileDefine => hasDependencyReadyTask state
  | .taskClose => hasDependencyReadyTask state
  | .profileReplace | .commandShow | .commandRun =>
      current && hasDependencyReadyTask state &&
        currentHasEntry state (fun | .commandProfile _ => true | _ => false)
  | .artifactObserve => hasDependencyReadyTask state && state.currentDesign?.any (fun design =>
      design.acceptanceCriteria.any (·.evidenceKind == "artifact"))
  | .correctionSupersede | .correctionResolve | .correctionIncorporate =>
      current && currentHasEntry state (fun
      | .userCorrection correction => correction.resolvedByEntryId.isNone &&
          correction.incorporatedIn.isNone
      | _ => false)
  | .kptApply => current && currentHasEntry state (fun
      | .kpt kpt => kpt.tryNext.isSome
      | _ => false)
  | .reviewResume =>
      focusedWork && state.ledgerEntries.any (fun entry =>
        entry.workId == state.focusedWorkId && match entry.payload with | .review _ => true | _ => false)
  | .reviewFinding => reviewFindingApplicable state
  | .reviewHandoff | .reviewConclude => focusedWork && state.ledgerEntries.any (fun entry =>
      entry.workId == state.focusedWorkId && match entry.payload with | .review _ => true | _ => false)
  | .reviewDisposition => focusedWork && state.ledgerEntries.any (fun entry =>
      entry.workId == state.focusedWorkId && match entry.payload with | .finding _ => true | _ => false)
  | .reviewVerify => current && currentHasEntry state (fun
      | .review review => review.context == .resume
      | _ => false)
  | .proofDigest | .proofRun =>
      current && state.currentDesign?.any (fun design => !design.leanClaims.isEmpty)

/-- User-facing applicability includes current external inputs. Persistence uses the structural
check only after the prepared transition has already validated these same immutable inputs. -/
def operationApplicable
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) (operation : Operation) : Bool :=
  operationStructurallyApplicable state operation && match operation with
  | .planMaterialize =>
      currentProjection? state |>.any fun projection =>
        projection.design.leanClaims.all (claimHasReceipt projection digests)
  | .taskReopenStale => hasStaleClosedTasks state observations
  | .workComplete => completionReady state observations digests
  | _ => true

end AgentWorkbench
