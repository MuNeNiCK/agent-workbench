import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.StoreRecovery
import AgentWorkbench.Adapter.ManagedOutput
import AgentWorkbench.Adapter.OperationLock
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbench.Adapter.DesignClaim
import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Current
import AgentWorkbench.Application.Plan
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Profile
import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Guidance
import AgentWorkbench.Application.Review
import AgentWorkbench.Application.Mutation

namespace AgentWorkbench.Store

private def fail (message : String) : IO α := throw (IO.userError message)

private def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => fail message

private def updateOperation
    (store : WriteStore) (operation : Operation)
    (transition : ProjectState → Except String ProjectState) : IO ProjectState := do
  let prior ← loadState store
  let next ← fromExcept (transition prior)
  commitOperation store operation prior.revision next
  pure next

private def replayPreparedObservation
    (prior observed : ProjectState) (prepared : AgentWorkbench.PreparedMutation) : IO ProjectState := do
  let next ← fromExcept (prepared.executeApplicable prior)
  if next != observed then
    fail s!"{prepared.operation.name} observation differs from the production transition"
  pure next

private def proposeDesignRequest
    (projectRoot : System.FilePath) (store : WriteStore)
    (operation : Operation) (request : AgentWorkbench.DesignProposalRequest) : IO AgentWorkbench.DesignRevision := do
  if operation == .designPropose && request.amendsCandidate.isSome then
    fail "design propose cannot amend an existing candidate; use design amend"
  if operation == .designAmend && request.amendsCandidate.isNone then
    fail "design amend requires amendsCandidate"
  let prior ← loadState store
  let workId ← match prior.focusedWorkId with
    | some value => pure value
    | none => fail "Design proposal requires a focused Work"
  let captured ← AgentWorkbench.DesignSource.captureAll projectRoot request.sourceDocumentTargets
  let preparedClaims ← AgentWorkbench.DesignClaim.prepare projectRoot request.leanClaims
  let mut elaboratedClaims := []
  for claim in preparedClaims.claims do
    let baselines ← AgentWorkbench.ProofBuild.captureBaselines projectRoot claim
    let token := (AgentWorkbench.ContentDigest.string
      (s!"design:{prior.revision}:{claim.id}:" ++ (Lean.toJson claim.input).compress)).drop 7
      |>.toString
    let operationId := s!"design-proof-{token}"
    let layouts ← AgentWorkbench.ProofBuild.outputLayouts baselines token
    let manifest : AgentWorkbench.ProofBuild.ManagedOutputManifest := { layouts }
    beginManagedOperation store operationId prior.revision "restore-proof-outputs"
      (Lean.toJson manifest).compress
    let elaborated ← try
        AgentWorkbench.DesignClaim.elaborate projectRoot claim layouts
      catch error =>
        try
          AgentWorkbench.ProofBuild.restoreLayouts layouts
          clearManagedOperation store operationId
        catch _ => pure ()
        throw error
    try
      AgentWorkbench.ProofBuild.restoreLayouts layouts
      clearManagedOperation store operationId
    catch error => throw error
    elaboratedClaims := elaboratedClaims ++ [elaborated]
  let captured := captured ++ preparedClaims.sources
  let sources := captured.map fun source =>
    ({ target := source.target, mediaKind := source.mediaKind
       snapshot := source.digest } : AgentWorkbench.DesignSource)
  let units := captured.flatMap (·.units)
  let request := { request with leanClaims := elaboratedClaims }
  let raw := { request.design prior workId sources units with
    createdAfterEntryOrder := nextEntryOrder prior - 1 }
  let digestMaterial := Lean.toJson { raw with revisionContentDigest := "", status := .candidate }
  let candidate := { raw with revisionContentDigest := ContentDigest.string digestMaterial.compress }
  let prepared := if operation == .designPropose then
      AgentWorkbench.PreparedMutation.designPropose candidate
    else AgentWorkbench.PreparedMutation.designAmend candidate
  let next ← fromExcept (prepared.executeApplicable prior)
  commitDesignProposal store operation prior.revision next candidate captured
  pure candidate

private def proposePlanRequest
    (projectRoot : System.FilePath) (store : WriteStore) (operation : Operation)
    (request : AgentWorkbench.PlanProposalRequest) : IO AgentWorkbench.ImplementationPlan := do
  let prior ← loadState store
  let work ← match prior.currentWork? with
    | some value => pure value
    | none => fail "Plan proposal requires a focused Work"
  let captured ← AgentWorkbench.PlanSource.captureAll
    projectRoot work.id request.sourceDocumentTargets
  let sources := captured.map fun source =>
    ({ target := source.target, digest := source.digest } : AgentWorkbench.PlanSource)
  let units := captured.flatMap (·.units)
  let raw := request.plan prior work sources units
  let material := Lean.toJson { raw with contentDigest := "", status := .candidate }
  let candidate := { raw with contentDigest := ContentDigest.string material.compress }
  let prepared := if operation == .planPropose then
      AgentWorkbench.PreparedMutation.planPropose candidate
    else AgentWorkbench.PreparedMutation.planReplace candidate
  let next ← fromExcept (prepared.executeApplicable prior)
  commitPlanProposal store operation prior.revision next candidate captured
  pure candidate

private def materializePlanRequest
    (projectRoot : System.FilePath) (store : WriteStore) (planId : String) : IO ProjectState := do
  let prior ← loadState store
  let inputs ← AgentWorkbench.evaluateCurrentInputs projectRoot prior
  let next ← fromExcept
    ((AgentWorkbench.PreparedMutation.planMaterialize
      planId inputs.observations inputs.claimDigests).executeApplicable prior)
  commitOperation store .planMaterialize prior.revision next
  pure next

private def closeTaskRequest
    (projectRoot : System.FilePath) (store : WriteStore)
    (request : AgentWorkbench.TaskCloseRequest) : IO ProjectState := do
  let prior ← loadState store
  let inputs ← AgentWorkbench.evaluateCurrentInputs projectRoot prior
  let next ← fromExcept
    ((AgentWorkbench.PreparedMutation.taskClose request inputs.observations).executeApplicable prior)
  commitOperation store .taskClose prior.revision next
  pure next

private def observeArtifact
    (projectRoot : System.FilePath) (store : WriteStore)
    (request : AgentWorkbench.ArtifactObserveRequest) : IO ProjectState := do
  let prior ← loadState store
  let observed ← AgentWorkbench.observeArtifact projectRoot prior request
  let entry ← match observed.ledgerEntries.getLast? with
    | some value => pure value
    | none => fail "artifact observation produced no Ledger entry"
  let next ← replayPreparedObservation prior observed
    (.artifactObservation entry)
  commitOperation store .artifactObserve prior.revision next
  pure next

private def startReview
    (projectRoot : System.FilePath) (store : WriteStore)
    (request : AgentWorkbench.ReviewStartRequest) : IO ProjectState := do
  let prior ← loadState store
  let observed ← AgentWorkbench.startReview projectRoot prior request
  let entry ← match observed.ledgerEntries.getLast? with
    | some value => pure value
    | none => fail "fresh Review produced no Ledger entry"
  let next ← replayPreparedObservation prior observed (.reviewStart entry)
  commitOperation store .reviewStart prior.revision next
  pure next

private def resumeReview
    (projectRoot : System.FilePath) (store : WriteStore)
    (request : AgentWorkbench.ReviewResumeRequest) : IO ProjectState := do
  let prior ← loadState store
  let observed ← AgentWorkbench.resumeReview projectRoot prior request
  let entry ← match observed.ledgerEntries.getLast? with
    | some value => pure value
    | none => fail "resumed Review produced no Ledger entry"
  let next ← replayPreparedObservation prior observed (.reviewResume entry)
  commitOperation store .reviewResume prior.revision next
  pure next

private def runCommandProfile
    (projectRoot : System.FilePath) (store : WriteStore)
    (request : AgentWorkbench.CommandRunRequest)
    (postCommitVerification : IO Unit := pure ()) : IO AgentWorkbench.CommandRunResult := do
  let prior ← loadState store
  let resolved ← match AgentWorkbench.resolveCommandProfile? projectRoot prior request.profileEntryId with
    | some value => pure value
    | none => fail s!"no applicable Command Profile {request.profileEntryId}"
  let inputs ← AgentWorkbench.evaluateCurrentInputs projectRoot prior
  unless AgentWorkbench.commandAuthorized prior inputs.claimDigests resolved do
    fail "Command Profile is not authorized by the current Plan, Task, and Claim receipts"
  let outputScope ← match resolved.outputScope with
    | some value => pure value
    | none => fail "Command Profile has no managed output scope"
  let baseline ← AgentWorkbench.ManagedOutput.capture projectRoot outputScope
  let token := (AgentWorkbench.ContentDigest.string
    s!"{prior.revision}:{request.entryId}:{request.profileEntryId}").drop 7 |>.toString
  let operationId := s!"command-{token}"
  beginManagedOperation store operationId prior.revision "retain-command-output"
    (Codec.encode baseline)
  let cleanupUncommitted : IO Unit := do
    AgentWorkbench.ManagedOutput.restore projectRoot baseline
    clearManagedOperation store operationId
  let execution ← try
      AgentWorkbench.runCommandProfile projectRoot prior request
    catch error =>
      try cleanupUncommitted catch _ => pure ()
      throw error
  let (observed, result) := execution
  let next ← replayPreparedObservation prior observed (.commandExecution result.entry)
  let (successful, exitCode) := match result.entry.payload with
    | .commandExecution value => (value.successful, value.exitCode)
    | _ => (false, 0)
  if !successful then
    cleanupUncommitted
    fail s!"Command Profile {request.entryId} exited with code {exitCode} without recording successful evidence"
  try
    commitOperation store .commandRun prior.revision next (some operationId)
      postCommitVerification
  catch error =>
    let definitelyUncommitted ← try
        managedOperationDefinitelyUncommitted store operationId
      catch _ => pure false
    if definitelyUncommitted then
      try cleanupUncommitted catch _ => pure ()
    throw error
  try
    clearManagedOperation store operationId
  catch _ => pure ()
  pure result

private def runProofClaim
    (projectRoot : System.FilePath) (store : WriteStore)
    (request : AgentWorkbench.ProofRunRequest) : IO AgentWorkbench.ProofRunResult := do
  let prior ← loadState store
  let projection ← match AgentWorkbench.currentProjection? prior with
    | some value => pure value
    | none => fail "proof run requires a current Work and accepted Design"
  let claim ← match projection.design.claim? request.claimId with
    | some value => pure value
    | none => fail s!"current Design has no Lean claim {request.claimId}"
  let baselines ← AgentWorkbench.ProofBuild.captureBaselines projectRoot claim
  let token := (AgentWorkbench.ContentDigest.string
    s!"{prior.revision}:{request.entryId}:{request.claimId}").drop 7 |>.toString
  let operationId := s!"proof-{token}"
  let layouts ← AgentWorkbench.ProofBuild.outputLayouts baselines token
  let manifest : AgentWorkbench.ProofBuild.ManagedOutputManifest := { layouts }
  beginManagedOperation store operationId prior.revision "restore-proof-outputs"
    (Codec.encode manifest)
  let execution ← try
      AgentWorkbench.runProofClaim projectRoot
        (AgentWorkbench.Runtime.layout projectRoot) prior request baselines layouts
    catch error =>
      try
        AgentWorkbench.ProofBuild.restoreLayouts layouts
        clearManagedOperation store operationId
      catch _ => pure ()
      throw error
  let (observed, result) := execution
  let next ← replayPreparedObservation prior observed (.proofReceipt result.entry)
  commitOperation store .proofRun prior.revision next (some operationId)
  try
    AgentWorkbench.ProofBuild.restoreLayouts layouts
    clearManagedOperation store operationId
  catch _ => pure ()
  pure result

private def completeFocusedWork
    (projectRoot : System.FilePath) (store : WriteStore) : IO ProjectState := do
  let prior ← loadState store
  let inputs ← AgentWorkbench.evaluateCurrentInputs projectRoot prior
  let completionInput ← fromExcept
    (AgentWorkbench.completionInput prior inputs.observations inputs.claimDigests)
  let inputDigest := AgentWorkbench.ContentDigest.string (Lean.toJson completionInput).compress
  let next ← fromExcept ((AgentWorkbench.PreparedMutation.workComplete
    inputs.observations inputs.claimDigests inputDigest).executeApplicable prior)
  commitOperation store .workComplete prior.revision next
  pure next

private def executePureMutation
    (store : WriteStore) (mutation : AgentWorkbench.Mutation) : IO AgentWorkbench.MutationResult := do
  let prepared := AgentWorkbench.PreparedMutation.direct mutation
  let next ← updateOperation store mutation.operation (prepared.executeApplicable ·)
  pure <| match mutation.pureResultShape with
    | .state => .state next
    | .context => .context next

private def executeInitMutation (store : WriteStore) : IO AgentWorkbench.MutationResult := do
  let prepared := AgentWorkbench.PreparedMutation.direct .init
  let transition := if AgentWorkbench.Store.wasMigratedFromLegacy store then
      prepared.execute
    else prepared.executeApplicable
  let next ← updateOperation store .init transition
  pure (.state next)

private def executeMutationUnlocked
    (projectRoot : System.FilePath) (store : WriteStore)
    (postCommitVerification : IO Unit := pure ()) :
    AgentWorkbench.Mutation → IO AgentWorkbench.MutationResult
  | .init => executeInitMutation store
  | .designPropose request =>
      return .design (← proposeDesignRequest projectRoot store .designPropose request)
  | .designAmend request =>
      return .design (← proposeDesignRequest projectRoot store .designAmend request)
  | .workComplete => return .state (← completeFocusedWork projectRoot store)
  | .planPropose request =>
      return .plan (← proposePlanRequest projectRoot store .planPropose request)
  | .planReplace request =>
      return .plan (← proposePlanRequest projectRoot store .planReplace request)
  | .planMaterialize planId => return .state (← materializePlanRequest projectRoot store planId)
  | .taskClose request => return .state (← closeTaskRequest projectRoot store request)
  | .commandRun request =>
      return .command (← runCommandProfile projectRoot store request postCommitVerification)
  | .artifactObserve request => return .state (← observeArtifact projectRoot store request)
  | .proofRun request => return .proof (← runProofClaim projectRoot store request)
  | .reviewStart request => return .state (← startReview projectRoot store request)
  | .reviewResume request => return .state (← resumeReview projectRoot store request)
  | mutation@(.designAccept _) => executePureMutation store mutation
  | mutation@(.designReject _) => executePureMutation store mutation
  | mutation@(.workStart _) => executePureMutation store mutation
  | mutation@(.workFocus _) => executePureMutation store mutation
  | mutation@(.workSuspend _ _) => executePureMutation store mutation
  | mutation@(.workResume _) => executePureMutation store mutation
  | mutation@(.workHandoff _ _ _ _) => executePureMutation store mutation
  | mutation@(.workAdoptDesign _) => executePureMutation store mutation
  | mutation@(.workWithdraw _) => executePureMutation store mutation
  | mutation@(.profileDefine _) => executePureMutation store mutation
  | mutation@(.profileReplace _) => executePureMutation store mutation
  | mutation@(.correctionRecord _) => executePureMutation store mutation
  | mutation@(.correctionSupersede _) => executePureMutation store mutation
  | mutation@(.correctionResolve _) => executePureMutation store mutation
  | mutation@(.correctionIncorporate _) => executePureMutation store mutation
  | mutation@(.kptRecord _) => executePureMutation store mutation
  | mutation@(.kptApply _) => executePureMutation store mutation
  | mutation@(.reviewHandoff _) => executePureMutation store mutation
  | mutation@(.reviewFinding _) => executePureMutation store mutation
  | mutation@(.reviewDisposition _) => executePureMutation store mutation
  | mutation@(.reviewConclude _) => executePureMutation store mutation
  | mutation@(.reviewVerify _) => executePureMutation store mutation

def executeMutationWithPostCommitVerification
    (projectRoot database : System.FilePath)
    (mutation : AgentWorkbench.Mutation)
    (postCommitVerification : IO Unit) : IO AgentWorkbench.MutationResult :=
  AgentWorkbench.OperationLock.withProjectMutationLock projectRoot do
    match mutation with
    | .init => AgentWorkbench.Runtime.initializeProject projectRoot
    | _ => pure ()
    IO.FS.createDirAll (projectRoot / ".agent-workbench")
    let store ← AgentWorkbench.Store.open database
    recoverManagedOperations projectRoot store
    executeMutationUnlocked projectRoot store postCommitVerification mutation

def executeMutation
    (projectRoot database : System.FilePath)
    (mutation : AgentWorkbench.Mutation) : IO AgentWorkbench.MutationResult :=
  executeMutationWithPostCommitVerification projectRoot database mutation (pure ())


end AgentWorkbench.Store
