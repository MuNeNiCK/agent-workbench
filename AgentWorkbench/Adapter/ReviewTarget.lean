import AgentWorkbench.Domain.State
import AgentWorkbench.Domain.Lookup
import AgentWorkbench.Decision.Projection
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ContentDigest

namespace AgentWorkbench.ReviewTarget

structure Fixed where
  sourceId : String
  target : String
  snapshot : String
  manifest : List ReviewTargetComponent
  producerAgentRuns : List String

private def distinctStrings (values : List String) : List String :=
  values.foldl (fun found value =>
    if value.isEmpty || found.contains value then found else found ++ [value]) []

private def normalizeComponents
    (components : List ReviewTargetComponent) : List ReviewTargetComponent :=
  components.mergeSort (fun left right =>
    if left.kind == right.kind then left.id < right.id else left.kind < right.kind)

private def component
    (kind id snapshot : String) (producers : List String := []) : ReviewTargetComponent :=
  { kind, id, snapshot, producerAgentRuns := distinctStrings producers }

private def entryDigest (entry : LedgerEntry) : String :=
  ContentDigest.string (Lean.toJson entry).compress

private def entryProducerRuns (work : Work) (entry : LedgerEntry) : List String :=
  match entry.payload with
  | .commandExecution value => [value.producerAgentRun]
  | .artifactObservation value => [value.producerAgentRun]
  | .reviewDisposition value => [value.decidedByRun]
  | .workHandoff value => [value.predecessorRun, value.successorRun]
  | .workWithdrawal value => [value.withdrawnByRun]
  | .workResume value => [value.resumedByRun]
  | .workCompletion value => [value.completedByRun]
  | .designRejection value => [value.rejectedByRun]
  | .workDesignAdoption value => [value.adoptedByRun]
  | .task _ | .commandProfile _ | .userCorrection _ | .kpt _ | .leanProofReceipt _ =>
      [work.responsibleAgentRun]
  | .review value => value.producerAgentRuns
  | .finding value => value.producerAgentRuns
  | .reviewVerification value => [value.verifiedByRun]
  | .reviewHandoff value => [value.predecessorReviewerRun, value.successorReviewerRun]
  | .reviewConclusion value => [value.reviewerAgentRun]

private def acceptedDispositionIds (entries : List LedgerEntry) (workId : String) : List String :=
  entries.filterMap fun entry =>
    match entry.payload with
    | .finding _ =>
        if (findingDispositionIn? entries entry.id workId).any fun dispositionEntry =>
          match dispositionEntry.payload with
          | .reviewDisposition value => value.decision == .accepted
          | _ => false
        then some entry.id else none
    | _ => none

private def implementationLedgerComponents
    (projection : CurrentProjection) (plan : ImplementationPlan) : List ReviewTargetComponent :=
  let acceptedFindings := acceptedDispositionIds projection.entries projection.work.id
  projection.entries.filterMap fun entry =>
    let selected : Bool := match entry.payload with
      | .task value => value.planId == some plan.id && !value.retired
      | .commandExecution _ | .artifactObservation _ | .leanProofReceipt _ => true
      | .userCorrection _ => true
      | .finding _ => acceptedFindings.contains entry.id
      | .reviewDisposition value => acceptedFindings.contains value.findingEntryId &&
          ((findingDispositionIn? projection.entries value.findingEntryId projection.work.id)
            |>.any (·.id == entry.id))
      | _ => false
    if selected then
      some (component entry.payload.tag entry.id (entryDigest entry)
        (entryProducerRuns projection.work entry))
    else none

private def workProducerRuns (state : ProjectState) (work : Work) : List String :=
  distinctStrings <| state.ledgerEntries.flatMap (fun entry =>
    if entry.workId != some work.id then [] else
    match entry.payload with
    | .workHandoff value => [value.predecessorRun, value.successorRun]
    | _ => []) ++ [work.responsibleAgentRun]

private def workHistoryComponents
    (state : ProjectState) (work : Work) : List ReviewTargetComponent :=
  state.ledgerEntries.filterMap fun entry =>
    if entry.workId != some work.id then none else
    match entry.payload with
    | .workHandoff _ | .workDesignAdoption _ | .workWithdrawal _ | .workResume _ =>
        some (component entry.payload.tag entry.id (entryDigest entry)
          (entryProducerRuns work entry))
    | _ => none

private def currentTargetComponent?
    (projectRoot : System.FilePath) (entry : LedgerEntry) : IO (Option ReviewTargetComponent) := do
  match entry.payload with
  | .artifactObservation value =>
      let snapshot ← Snapshot.target projectRoot value.target
      if value.successful && snapshot == value.snapshot then
        pure (some (component "implementation_target" value.target snapshot
          [value.producerAgentRun]))
      else pure none
  | .commandExecution value =>
      match value.target, value.snapshot with
      | some target, some recorded =>
          let snapshot ← Snapshot.target projectRoot target
          if value.successful && snapshot == recorded then
            pure (some (component "implementation_target" target snapshot
              [value.producerAgentRun]))
          else pure none
      | _, _ => pure none
  | _ => pure none

private def currentTargetComponents
    (projectRoot : System.FilePath) (entries : List LedgerEntry) : IO (List ReviewTargetComponent) := do
  let mut result := []
  for entry in entries do
    if let some value ← currentTargetComponent? projectRoot entry then
      if !(result.any fun prior => prior.id == value.id && prior.snapshot == value.snapshot) then
        result := result ++ [value]
  pure result

private def plannedTargetComponents
    (projectRoot : System.FilePath) (work : Work) (entries : List LedgerEntry) : IO (List ReviewTargetComponent) := do
  let targets := distinctStrings <| entries.flatMap fun entry => match entry.payload with
    | .task task => if task.retired then [] else task.outputScopes
    | _ => []
  let mut result := []
  for target in targets do
    let snapshot ← Snapshot.target projectRoot target
    result := result ++ [component "implementation_target" target snapshot [work.responsibleAgentRun]]
  pure result

private def freezeDesign (state : ProjectState) (designId : String) : Except String Fixed := do
  let design ← match state.design? designId with
    | some value => pure value
    | none => throw s!"no DesignRevision {designId}"
  if !design.sourceArchiveAvailable then throw "historical source content unavailable"
  let manifest := [component "design" design.id design.revisionContentDigest
    [design.producerAgentRun]]
  pure {
    sourceId := design.id
    target := s!"design:{design.id}"
    snapshot := ContentDigest.string (Lean.toJson manifest).compress
    manifest
    producerAgentRuns := [design.producerAgentRun] }

private def freezeImplementation
    (projectRoot : System.FilePath) (state : ProjectState) : IO (Except String Fixed) := do
  let some projection := currentProjection? state
    | pure (.error "Implementation Review requires a current Work and accepted Design")
  let some plan := state.currentPlanFor? projection.work.id
    | pure (.error "Implementation Review requires the current implementation plan")
  let targetComponents ← currentTargetComponents projectRoot projection.entries
  let plannedComponents ← plannedTargetComponents projectRoot projection.work projection.entries
  let manifest := normalizeComponents (
    [ component "design" projection.design.id projection.design.revisionContentDigest
        [projection.design.producerAgentRun]
    , component "plan" plan.id plan.contentDigest [plan.producerAgentRun]
    , component "work" projection.work.id
        (ContentDigest.string (Lean.toJson projection.work).compress)
        (workProducerRuns state projection.work)
    ] ++ workHistoryComponents state projection.work ++
      implementationLedgerComponents projection plan ++ plannedComponents ++ targetComponents)
  let producers := distinctStrings (manifest.flatMap (·.producerAgentRuns))
  pure (.ok {
    sourceId := projection.work.id
    target := s!"work:{projection.work.id}"
    snapshot := ContentDigest.string (Lean.toJson manifest).compress
    manifest
    producerAgentRuns := producers })

def freeze
    (projectRoot : System.FilePath) (state : ProjectState) (purpose : ReviewPurpose)
    (targetDesignRevision : Option String) : IO (Except String Fixed) := do
  match purpose with
  | .design =>
      match targetDesignRevision with
      | some designId => pure (freezeDesign state designId)
      | none => pure (.error "Design Review requires targetDesignRevision")
  | .implementation =>
      if targetDesignRevision.isSome then
        pure (.error "Implementation Review target is derived from the current Work")
      else freezeImplementation projectRoot state

def refreeze
    (projectRoot : System.FilePath) (state : ProjectState) (prior : ReviewRecord) : IO (Except String Fixed) := do
  let _ := projectRoot
  let _ := state
  -- A resumed Review continues the immutable root target. Remediation is
  -- separately bound evidence and cannot rewrite what the root reviewer saw.
  pure (.ok {
    sourceId := prior.targetSourceId
    target := prior.target
    snapshot := prior.targetSnapshot
    manifest := prior.targetManifest
    producerAgentRuns := prior.producerAgentRuns })

def currentSnapshot
    (projectRoot : System.FilePath) (state : ProjectState)
    (purpose : ReviewPurpose) (target : String) : IO String := do
  match purpose with
  | .design =>
      if !target.startsWith "design:" then
        throw (IO.userError "Design Review target is not a DesignRevision")
      let designId := (target.drop 7).toString
      let design ← match state.design? designId with
        | some value => pure value
        | none => throw (IO.userError s!"no DesignRevision {designId}")
      if !design.sourceArchiveAvailable then
        throw (IO.userError "historical source content unavailable")
      let manifest := [component "design" design.id design.revisionContentDigest
        [design.producerAgentRun]]
      pure (ContentDigest.string (Lean.toJson manifest).compress)
  | .implementation =>
      let fixed ← match ← freezeImplementation projectRoot state with
        | .ok value => pure value
        | .error message => throw (IO.userError message)
      if fixed.target != target then
        throw (IO.userError "Implementation Review target is not the current Work")
      pure fixed.snapshot

end AgentWorkbench.ReviewTarget
