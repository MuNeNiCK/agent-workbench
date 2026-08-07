import Lean.Util.Diff
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Domain.DesignHistory
import AgentWorkbench.Domain.Plan

namespace AgentWorkbench.PlanArchive

private structure StoredSource where
  target : String
  digest : String
  content : ByteArray

private structure Archive where
  plan : ImplementationPlan
  sources : List StoredSource

private def fail (message : String) : IO α := throw (IO.userError message)

private def loadArchive (root : System.FilePath) (planId : String) : IO Archive := do
  let connection ← AgentWorkbench.SQLite.openReadOnly
    (root / ".agent-workbench" / "state.db")
  let schema ← AgentWorkbench.SQLite.queryScalar connection
    "SELECT CAST(schema_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
  if schema != "2" then fail s!"Plan archive requires schema revision 2; found {schema}"
  let documents ← AgentWorkbench.SQLite.queryTextRows connection
    "SELECT document FROM implementation_plans WHERE id = ?1" #[planId] 1
  let row ← match documents[0]? with
    | some value => pure value
    | none => fail s!"no Implementation Plan {planId}"
  let json ← match Lean.Json.parse row[0]! with
    | .ok value => pure value
    | .error error => fail s!"invalid persisted Plan JSON: {error}"
  let plan : ImplementationPlan ← match Lean.fromJson? json with
    | .ok value => pure value
    | .error error => fail s!"invalid persisted Implementation Plan: {error}"
  let rows ← AgentWorkbench.SQLite.queryTextTextBlobRows connection
    "SELECT target, digest, content FROM implementation_plan_sources
     WHERE plan_id = ?1 ORDER BY ordinal" #[planId]
  if rows.size != plan.sourceDocuments.length then fail s!"Plan {planId} source archive is incomplete"
  let mut sources := []
  for (manifest, index) in plan.sourceDocuments.zipIdx do
    let (target, digest, content) := rows[index]!
    if manifest.target != target || manifest.digest != digest ||
        ContentDigest.bytes content != digest then
      fail s!"Plan {planId} source archive differs from its immutable manifest"
    sources := sources ++ [{ target, digest, content }]
  pure { plan, sources }

private def publicSource (source : StoredSource) : ArchivedDesignSource :=
  { target := source.target, mediaKind := "markdown", digest := source.digest
    contentBytes := source.content.data.toList.map (·.toNat) }

def source (root : System.FilePath) (planId target : String) : IO ArchivedDesignSource := do
  let archive ← loadArchive root planId
  match archive.sources.find? (·.target == target) with
  | some value => pure (publicSource value)
  | none => fail s!"Plan {planId} has no archived source {target}"

def exportArchive (root : System.FilePath) (planId : String) : IO (List ArchivedDesignSource) := do
  let archive ← loadArchive root planId
  pure (archive.sources.map publicSource)

private def lineEdits (before after : String) : List DesignSourceLineEdit :=
  (Lean.Diff.diff (before.splitOn "\n").toArray (after.splitOn "\n").toArray).toList.map
    fun (action, line) => { action := toString action, line }

private def sourceDiff
    (before after : Option StoredSource) (target : String) : DesignSourceDiff :=
  match before, after with
  | none, some added => { target, change := .added, afterDigest := some added.digest }
  | some deleted, none => { target, change := .deleted, beforeDigest := some deleted.digest }
  | some old, some current =>
      if old.digest == current.digest then
        { target, change := .unchanged
          beforeDigest := some old.digest, afterDigest := some current.digest }
      else match String.fromUTF8? old.content, String.fromUTF8? current.content with
      | some oldText, some currentText =>
          { target, change := .changed
            beforeDigest := some old.digest, afterDigest := some current.digest
            lineEdits := lineEdits oldText currentText }
      | _, _ => {
          target
          change := .binaryChanged
          beforeDigest := some old.digest
          afterDigest := some current.digest }
  | none, none => { target, change := .unchanged }

def diff (root : System.FilePath) (beforePlanId afterPlanId : String) : IO PlanDiff := do
  let before ← loadArchive root beforePlanId
  let after ← loadArchive root afterPlanId
  let targets := (before.sources.map (·.target) ++ after.sources.map (·.target)).foldl
    (fun unique target => if unique.contains target then unique else unique ++ [target]) []
  pure {
    beforePlanId
    afterPlanId
    afterReason := after.plan.reason
    afterBasisEntryIds := after.plan.changeBasisEntryIds
    sources := targets.map fun target => sourceDiff
      (before.sources.find? (·.target == target))
      (after.sources.find? (·.target == target)) target }

end AgentWorkbench.PlanArchive
