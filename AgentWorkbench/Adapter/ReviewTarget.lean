import AgentWorkbench.Domain.State
import AgentWorkbench.Domain.Lookup
import AgentWorkbench.Decision.Projection
import AgentWorkbench.Decision.ReviewInput
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ContentDigest

namespace AgentWorkbench.ReviewTarget

structure Fixed where
  sourceId : String
  target : String
  snapshot : String
  manifestVersion : Nat := 0
  manifest : List ReviewTargetComponent
  producerAgentRuns : List String

private def distinctStrings (values : List String) : List String :=
  values.foldl (fun found value =>
    if value.isEmpty || found.contains value then found else found ++ [value]) []

private def reviewCoverageOrder (state : ProjectState) : Nat :=
  state.ledgerEntries.foldl (fun found entry => max found entry.order) 0 + 1

private def component
    (kind id snapshot : String) (producers : List String := []) : ReviewTargetComponent :=
  { kind, id, snapshot, producerAgentRuns := distinctStrings producers }

private def implementationLedgerComponents
    (state : ProjectState) (projection : CurrentProjection)
    (plan : ImplementationPlan) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : List ReviewTargetComponent :=
  currentImplementationReviewLedgerEntries projection observations digests plan
    |>.map fun entry => reviewLedgerComponent state projection.work entry

private def plannedTargetComponents
    (projectRoot : System.FilePath) (producerAgentRun : String)
    (entries : List LedgerEntry) : IO (List ReviewTargetComponent) := do
  let targets := distinctStrings <| entries.flatMap fun entry => match entry.payload with
    | .task task => if task.retired then [] else task.outputScopes
    | _ => []
  let mut result := []
  for target in targets do
    let snapshot ← Snapshot.target projectRoot target
    result := result ++ [component "implementation_target" target snapshot [producerAgentRun]]
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
    (projectRoot : System.FilePath) (state : ProjectState)
    (observations : List TargetObservation) (digests : List CurrentClaimDigest) :
    IO (Except String Fixed) := do
  let some projection := currentProjection? state
    | pure (.error "Implementation Review requires a current Work and accepted Design")
  let some plan := state.currentPlanFor? projection.work.id
    | pure (.error "Implementation Review requires the current implementation plan")
  let coverageOrder := reviewCoverageOrder state
  let plannedComponents ← plannedTargetComponents projectRoot
    (responsibleWorkAgentRunAt state projection.work coverageOrder) projection.entries
  let manifest := normalizeReviewTargetComponents <| deduplicateReviewTargetComponents (
    [ component "design" projection.design.id projection.design.revisionContentDigest
        [projection.design.producerAgentRun]
    , component "plan" plan.id plan.contentDigest [plan.producerAgentRun]
    , component "work" projection.work.id
        (reviewWorkIdentitySnapshot projection.work)
        (reviewWorkProducerRunsAt state projection.work coverageOrder)
    ] ++ implementationLedgerComponents state projection plan observations digests ++
      plannedComponents)
  let producers := distinctStrings (manifest.flatMap (·.producerAgentRuns))
  pure (.ok {
    sourceId := projection.work.id
    target := s!"work:{projection.work.id}"
    snapshot := ContentDigest.string (Lean.toJson manifest).compress
    manifestVersion := 3
    manifest
    producerAgentRuns := producers })

def freeze
    (projectRoot : System.FilePath) (state : ProjectState) (purpose : ReviewPurpose)
    (targetDesignRevision : Option String) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : IO (Except String Fixed) := do
  match purpose with
  | .design =>
      match targetDesignRevision with
      | some designId => pure (freezeDesign state designId)
      | none => pure (.error "Design Review requires targetDesignRevision")
  | .implementation =>
      if targetDesignRevision.isSome then
        pure (.error "Implementation Review target is derived from the current Work")
      else freezeImplementation projectRoot state observations digests

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
    manifestVersion := prior.targetManifestVersion
    manifest := prior.targetManifest
    producerAgentRuns := prior.producerAgentRuns })

def currentSnapshot
    (projectRoot : System.FilePath) (state : ProjectState)
    (purpose : ReviewPurpose) (target : String) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : IO String := do
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
      let fixed ← match ← freezeImplementation projectRoot state observations digests with
        | .ok value => pure value
        | .error message => throw (IO.userError message)
      if fixed.target != target then
        throw (IO.userError "Implementation Review target is not the current Work")
      pure fixed.snapshot

end AgentWorkbench.ReviewTarget
