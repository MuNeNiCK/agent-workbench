import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Command
import AgentWorkbench.Adapter.Process
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ReviewTarget

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
        criterionId := request.criterionId
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
