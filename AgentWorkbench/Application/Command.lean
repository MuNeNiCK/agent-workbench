import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Command
import AgentWorkbench.Adapter.Process
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ReviewTarget
import AgentWorkbench.Application.Current

namespace AgentWorkbench

structure CommandRunRequest where
  profileEntryId : String
  entryId : String
  criterionId : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CommandRunResult where
  entry : LedgerEntry
  stdout : String
  stderr : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def runCommandProfile
    (projectRoot : System.FilePath) (state : ProjectState)
    (request : CommandRunRequest) : IO (ProjectState × CommandRunResult) := do
  let projectRoot ← IO.FS.realPath projectRoot
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw (IO.userError "no current Work context")
  let resolved ← match resolveCommandProfile? projectRoot state request.profileEntryId with
    | some value => pure value
    | none => throw (IO.userError s!"no applicable Command Profile {request.profileEntryId}")
  if let some criterionId := request.criterionId then
    unless resolved.criterionIds.contains criterionId do
      throw (IO.userError s!"criterion {criterionId} is not bound to the Command Profile")
  let inputs ← evaluateCurrentInputs projectRoot state
  unless commandAuthorized state inputs.claimDigests resolved do
    throw (IO.userError
      "Command Profile requires the current Plan, an open dependency-ready Task, and current Claim receipts")
  let mut inputSnapshots := []
  for target in resolved.inputTargets do
    inputSnapshots := inputSnapshots ++ [{
      target, snapshot := ← Snapshot.target projectRoot target }]
  let result ← Process.execute projectRoot resolved.command
  let snapshot ← match resolved.target with
    | some identity =>
        if identity.startsWith "design:" then
          pure (some (← ReviewTarget.currentSnapshot projectRoot state .design identity))
        else
          pure (some (← Snapshot.target projectRoot identity))
    | none => pure none
  let entry : LedgerEntry :=
    { id := request.entryId
      order := nextEntryOrder state
      scope := projection.work.scope
      workId := some projection.work.id
      designRevision := some projection.design.id
      payload := .commandExecution {
        profileEntryId := resolved.profileEntryId
        taskEntryId := resolved.taskEntryId
        outputScope := resolved.outputScope
        criterionId := request.criterionId
        inputSnapshots := some inputSnapshots
        target := resolved.target
        snapshot
        command := resolved.command
        exitCode := result.exitCode
        stdoutDigest := result.stdoutDigest
        stderrDigest := result.stderrDigest
        successful := result.exitCode == 0
        producerAgentRun := projection.work.responsibleAgentRun } }
  let next ← match appendEntry state entry with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  pure (next, { entry, stdout := result.stdout, stderr := result.stderr })

end AgentWorkbench
