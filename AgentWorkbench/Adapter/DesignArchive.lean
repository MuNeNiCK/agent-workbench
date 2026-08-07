import Lean.Util.Diff
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Domain.DesignHistory

namespace AgentWorkbench.DesignArchive

private structure StoredSource where
  target : String
  mediaKind : String
  digest : String
  content : ByteArray

private structure Archive where
  design : DesignRevision
  sources : List StoredSource

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def databasePath (root : System.FilePath) : System.FilePath :=
  root / ".agent-workbench" / "state.db"

private def decodeDesign (source : String) : IO DesignRevision := do
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error error => fail s!"invalid persisted Design JSON: {error}"
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error error => fail s!"invalid persisted DesignRevision: {error}"

private def loadArchive (root : System.FilePath) (designId : String) : IO Archive := do
  let connection ← AgentWorkbench.SQLite.openReadOnly (databasePath root)
  let schema ← AgentWorkbench.SQLite.queryScalar connection
    "SELECT CAST(schema_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
  if schema != "2" then fail s!"Design archive requires schema revision 2; found {schema}"
  let documents ← AgentWorkbench.SQLite.queryTextRows connection
    "SELECT structured_document FROM design_revisions WHERE id = ?1" #[designId] 1
  let row ← match documents[0]? with
    | some value => pure value
    | none => fail s!"no DesignRevision {designId}"
  if documents.size != 1 then fail s!"DesignRevision identity is not unique: {designId}"
  let design ← decodeDesign row[0]!
  if !design.sourceArchiveAvailable then
    fail "historical source content unavailable"
  let rows ← AgentWorkbench.SQLite.queryTextRows connection
    "SELECT target, media_kind, digest, hex(content)
     FROM design_sources WHERE design_id = ?1 ORDER BY ordinal" #[designId] 4
  let blobs ← AgentWorkbench.SQLite.queryBlobRows connection
    "SELECT content FROM design_sources WHERE design_id = ?1 ORDER BY ordinal" #[designId]
  if rows.size != blobs.size || rows.size != design.sourceDocuments.length then
    fail s!"Design {designId} source archive is incomplete"
  let mut sources := []
  for index in [:rows.size] do
    let row := rows[index]!
    let content := blobs[index]!
    let manifest ← match design.sourceDocuments[index]? with
      | some value => pure value
      | none => fail s!"Design {designId} source archive is incomplete"
    if manifest.target != row[0]! || manifest.mediaKind != row[1]! ||
        manifest.snapshot != row[2]! ||
        ContentDigest.bytes content != row[2]! then
      fail s!"Design {designId} source archive differs from its immutable manifest"
    sources := sources ++ [{
      target := row[0]!, mediaKind := row[1]!, digest := row[2]!, content }]
  pure { design, sources }

private def publicSource (source : StoredSource) : ArchivedDesignSource :=
  { target := source.target
    mediaKind := source.mediaKind
    digest := source.digest
    contentBytes := source.content.data.toList.map (·.toNat) }

def source (root : System.FilePath) (designId target : String) : IO ArchivedDesignSource := do
  let archive ← loadArchive root designId
  match archive.sources.find? (·.target == target) with
  | some value => pure (publicSource value)
  | none => fail s!"Design {designId} has no archived source {target}"

def exportArchive (root : System.FilePath) (designId : String) : IO (List ArchivedDesignSource) := do
  let archive ← loadArchive root designId
  pure (archive.sources.map publicSource)

private def lineEdits (before after : String) : List DesignSourceLineEdit :=
  (Lean.Diff.diff (before.splitOn "\n").toArray (after.splitOn "\n").toArray).toList.map
    fun (action, line) => { action := toString action, line }

private def sourceDiff
    (before after : Option StoredSource) (target : String) : DesignSourceDiff :=
  match before, after with
  | none, some added =>
      { target, change := .added, afterDigest := some added.digest }
  | some deleted, none =>
      { target, change := .deleted, beforeDigest := some deleted.digest }
  | some old, some current =>
      if old.digest == current.digest then
        { target, change := .unchanged
          beforeDigest := some old.digest, afterDigest := some current.digest }
      else
        match String.fromUTF8? old.content, String.fromUTF8? current.content with
        | some oldText, some currentText =>
            { target, change := .changed
              beforeDigest := some old.digest, afterDigest := some current.digest
              lineEdits := lineEdits oldText currentText }
        | _, _ =>
            { target, change := .binaryChanged
              beforeDigest := some old.digest, afterDigest := some current.digest }
  | none, none => { target, change := .unchanged }

def diff
    (root : System.FilePath) (beforeDesignId afterDesignId : String) : IO DesignDiff := do
  let before ← loadArchive root beforeDesignId
  let after ← loadArchive root afterDesignId
  let targets := (before.sources.map (·.target) ++ after.sources.map (·.target)).foldl
    (fun unique target => if unique.contains target then unique else unique ++ [target]) []
  pure {
    beforeDesignId
    afterDesignId
    afterRationale := after.design.changeRationale
    afterBasisEntryIds := after.design.changeBasisEntryIds
    sources := targets.map fun target => sourceDiff
      (before.sources.find? (·.target == target))
      (after.sources.find? (·.target == target)) target }

end AgentWorkbench.DesignArchive
