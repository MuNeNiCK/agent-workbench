import AgentWorkbench.Decision.Completion
import AgentWorkbench.Decision.Operation

namespace AgentWorkbench

inductive EvidenceGapKind where
  | missingEvidence
  | missingObservation
  | staleEvidence
  | missingInputDigest
  | staleReceipt
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CriterionGap where
  criterionId : String
  target : String
  kind : EvidenceGapKind
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ClaimGap where
  claimId : String
  kind : EvidenceGapKind
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignReference where
  id : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure WorkReference where
  id : String
  outcome : String
  status : WorkStatus
  resumeCondition : Option String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure EntryReference where
  id : String
  order : Nat
  kind : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CurrentContext where
  design : DesignReference
  work : WorkReference
  designSourceGaps : List String
  currentPlanId : Option String
  candidatePlanId : Option String
  planRequired : Bool
  unfinishedRequiredTasks : List EntryReference
  dependencyReadyTasks : List EntryReference
  commandProfiles : List EntryReference
  effectiveUserCorrections : List EntryReference
  relevantKpt : List EntryReference
  unresolvedAcceptedFindings : List EntryReference
  criterionGaps : List CriterionGap
  claimGaps : List ClaimGap
  applicableOperations : List String
  truncated : List String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ProjectContext where
  acceptedDesign : Option DesignReference
  openWorks : List WorkReference
  focused : Option CurrentContext
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def currentContextLimit : Nat := 32

def LedgerEntry.reference (entry : LedgerEntry) : EntryReference :=
  { id := entry.id, order := entry.order, kind := entry.payload.tag }

private def boundedReferences (entries : List LedgerEntry) : List EntryReference :=
  (entries.take currentContextLimit).map (·.reference)

private def criterionGap?
    (projection : CurrentProjection) (observations : List TargetObservation)
    (criterion : AcceptanceCriterion) : Option CriterionGap :=
  if criterionHasEvidence projection observations criterion then none
  else if !criterionEvidenceRecorded projection criterion then
    some { criterionId := criterion.id, target := criterion.target, kind := .missingEvidence }
  else if (currentSnapshot? observations criterion.target).isNone then
    some { criterionId := criterion.id, target := criterion.target, kind := .missingObservation }
  else
    some { criterionId := criterion.id, target := criterion.target, kind := .staleEvidence }

private def claimGap?
    (projection : CurrentProjection) (digests : List CurrentClaimDigest)
    (claim : LeanClaim) : Option ClaimGap :=
  if claimHasReceipt projection digests claim then none
  else if !claimReceiptRecorded projection claim then
    some { claimId := claim.id, kind := .missingEvidence }
  else if (uniqueBy? digests (·.claimId) claim.id).isNone then
    some { claimId := claim.id, kind := .missingInputDigest }
  else
    some { claimId := claim.id, kind := .staleReceipt }

private def isAcceptedFinding
    (projection : CurrentProjection) (findingEntry : LedgerEntry) : Bool :=
  findingDispositionIn? projection.entries findingEntry.id projection.work.id |>.any fun entry =>
    match entry.payload with
    | .reviewDisposition disposition =>
        disposition.decision == .accepted
    | _ => false

def currentContext?
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Option CurrentContext := do
  let projection ← currentProjection? state
  let unfinishedRequiredTasks := projection.entries.filter (fun entry =>
    match entry.payload with
      | .task task => task.required && !task.closed
      | _ => false)
  let currentPlan := state.currentPlanFor? projection.work.id
  let candidatePlan := state.implementationPlans.find? fun plan =>
    plan.workId == projection.work.id && plan.designRevision == projection.design.id &&
      plan.status == .candidate &&
      !(state.implementationPlans.any fun successor =>
        successor.predecessorPlanId == some plan.id && successor.status == .candidate)
  let dependencyReadyTasks := unfinishedRequiredTasks.filter fun entry =>
    match entry.payload with
    | .task task => task.dependencyLineageIds.all fun dependency =>
        projection.entries.any fun candidate => match candidate.payload with
        | .task dependencyTask =>
            dependencyTask.lineageId == some dependency && dependencyTask.closed &&
              !dependencyTask.retired
        | _ => false
    | _ => false
  let designSourceGaps :=
    if projection.design.sourceArchiveAvailable then []
    else ["historical source content unavailable"]
  let commandProfiles := projection.entries.filter (fun entry =>
    match entry.payload with | .commandProfile _ => true | _ => false)
  let effectiveUserCorrections := projection.entries.filter (fun entry =>
    match entry.payload with
    | .userCorrection correction =>
        correction.resolvedByEntryId.isNone &&
          correction.incorporatedIn != some projection.design.id
    | _ => false)
  let relevantKpt := projection.entries.filter (fun entry =>
    match entry.payload with | .kpt _ => true | _ => false)
  let unresolvedAcceptedFindings := projection.entries.filter (fun entry =>
    match entry.payload with
    | .finding finding =>
        isAcceptedFinding projection entry &&
        !acceptedFindingResolved projection observations entry finding
    | _ => false)
  let criterionGaps := projection.design.acceptanceCriteria.filterMap
    (criterionGap? projection observations)
  let claimGaps := projection.design.leanClaims.filterMap (claimGap? projection digests)
  let truncated :=
    [("designSourceGaps", designSourceGaps.length),
      ("unfinishedRequiredTasks", unfinishedRequiredTasks.length),
      ("dependencyReadyTasks", dependencyReadyTasks.length),
      ("commandProfiles", commandProfiles.length),
      ("effectiveUserCorrections", effectiveUserCorrections.length),
      ("relevantKpt", relevantKpt.length),
      ("unresolvedAcceptedFindings", unresolvedAcceptedFindings.length),
      ("criterionGaps", criterionGaps.length), ("claimGaps", claimGaps.length)]
    |>.filterMap (fun (name, count) => if count > currentContextLimit then some name else none)
  pure {
    design := { id := projection.design.id }
    work := {
      id := projection.work.id
      outcome := projection.work.outcome
      status := projection.work.status
      resumeCondition := projection.work.resumeCondition }
    designSourceGaps := designSourceGaps.take currentContextLimit
    currentPlanId := currentPlan.map (·.id)
    candidatePlanId := candidatePlan.map (·.id)
    planRequired := currentPlan.isNone
    unfinishedRequiredTasks := boundedReferences unfinishedRequiredTasks
    dependencyReadyTasks := boundedReferences dependencyReadyTasks
    commandProfiles := boundedReferences commandProfiles
    effectiveUserCorrections := boundedReferences effectiveUserCorrections
    relevantKpt := boundedReferences relevantKpt
    unresolvedAcceptedFindings := boundedReferences unresolvedAcceptedFindings
    criterionGaps := criterionGaps.take currentContextLimit
    claimGaps := claimGaps.take currentContextLimit
    applicableOperations := Operation.all.filter (operationApplicable state) |>.map (·.name)
    truncated }

def Work.reference (work : Work) : WorkReference :=
  { id := work.id, outcome := work.outcome, status := work.status
    resumeCondition := work.resumeCondition }

def projectContext?
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Option ProjectContext :=
  if state.acceptedDesignId.isNone && state.works.isEmpty then none else
  some {
    acceptedDesign := state.currentDesign?.map fun design => { id := design.id }
    openWorks := (state.works.filter fun work =>
      work.status != .completed && work.status != .withdrawn).take currentContextLimit |>.map Work.reference
    focused := currentContext? state observations digests }

end AgentWorkbench
