import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.DesignArchive
import AgentWorkbench.Adapter.PlanArchive
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Review
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Current
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Query
import AgentWorkbench.Cli.Describe

namespace AgentWorkbench.Cli

private def fail (message : String) : IO α := throw (IO.userError message)

private def writeJson [Lean.ToJson α] (value : α) : IO Unit :=
  IO.println (Lean.toJson value).compress

private def databasePath (root : System.FilePath) : System.FilePath :=
  root / ".agent-workbench" / "state.db"

private def openQueryStore (root : System.FilePath) : IO Store.ReadStore :=
  Store.openReadOnly (databasePath root)

private def loadDescribeState (root : System.FilePath) : IO ProjectState := do
  if ← (databasePath root).pathExists then
    Store.loadState (← Store.openReadOnly (databasePath root))
  else pure .empty

def runQuery (projectRoot : System.FilePath) : Query → IO Unit
  | .describe none => do
      writeJson (operationIndex (← loadDescribeState projectRoot))
  | .describe (some operation) => do
      let state ← loadDescribeState projectRoot
      match describedOperation? state operation with
      | some value => writeJson value
      | none => fail s!"unknown operation {operation}"
  | .designInspectSources targets => do
      writeJson (← AgentWorkbench.DesignSource.inspectAll projectRoot targets)
  | .designGet designId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match state.design? designId with
      | some value => writeJson value
      | none => fail s!"no DesignRevision {designId}"
  | .designSource designId target => do
      writeJson (← AgentWorkbench.DesignArchive.source projectRoot designId target)
  | .designDiff beforeId afterId => do
      writeJson (← AgentWorkbench.DesignArchive.diff projectRoot beforeId afterId)
  | .designExport designId => do
      writeJson (← AgentWorkbench.DesignArchive.exportArchive projectRoot designId)
  | .planInspectSources workId targets => do
      writeJson (← AgentWorkbench.PlanSource.inspectAll projectRoot workId targets)
  | .planGet planId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match state.plan? planId with
      | some value => writeJson value
      | none => fail s!"no Implementation Plan {planId}"
  | .planSource planId target => do
      writeJson (← AgentWorkbench.PlanArchive.source projectRoot planId target)
  | .planDiff beforeId afterId => do
      writeJson (← AgentWorkbench.PlanArchive.diff projectRoot beforeId afterId)
  | .planExport planId => do
      writeJson (← AgentWorkbench.PlanArchive.exportArchive projectRoot planId)
  | .workGet workId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match state.work? workId with
      | some value => writeJson value
      | none => fail s!"no Work {workId}"
  | .workAdoptionImpact workId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match workAdoptionImpact state workId with
      | .ok impact => writeJson impact
      | .error message => fail message
  | .entryGet entryId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match state.entry? entryId with
      | some value => writeJson value
      | none => fail s!"no LedgerEntry {entryId}"
  | .history afterOrder limit => do
      if limit == 0 || limit > 100 then fail "history limit must be between 1 and 100"
      let state ← Store.loadState (← openQueryStore projectRoot)
      writeJson (state.ledgerEntries.filter (fun entry => entry.order > afterOrder) |>.take limit)
  | .reviewContext reviewEntryId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match reviewInput? state reviewEntryId with
      | some value => writeJson value
      | none => fail s!"no Review entry {reviewEntryId}"
  | .reviewInspect reviewEntryId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match reviewInspection? state reviewEntryId with
      | some value => writeJson value
      | none => fail s!"no Review entry {reviewEntryId}"
  | .commandShow profileEntryId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      match resolveCommandProfile? projectRoot state profileEntryId with
      | some resolved => writeJson resolved
      | none => fail s!"no applicable Command Profile {profileEntryId}"
  | .proofDigest claimId => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      writeJson (← currentClaimDigest projectRoot state claimId)
  | .context => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      let inputs ← evaluateCurrentInputs projectRoot state
      writeJson (ContextResult.mk state.revision
        (projectContext? state inputs.observations inputs.claimDigests))
  | .ready => do
      let state ← Store.loadState (← openQueryStore projectRoot)
      let inputs ← evaluateCurrentInputs projectRoot state
      writeJson (ReadinessResult.mk state.revision
        (completionReady state inputs.observations inputs.claimDigests)
        (projectContext? state inputs.observations inputs.claimDigests))

end AgentWorkbench.Cli
