import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.DesignArchive

namespace AgentWorkbenchTest.DesignArchive

open AgentWorkbench AgentWorkbenchTest

private def proposal
    (units : List DesignSourceUnit) (target rationale : String)
    (amends : Option String := none) (bases : List String := []) : DesignProposalRequest :=
  let statement : Statement := { id := "statement-archive", text := "The archived requirement is authoritative." }
  { producerAgentRun := "designer-archive"
    changeRationale := rationale
    changeBasisEntryIds := bases
    amendsCandidate := amends
    sourceDocumentTargets := [target]
    sourceUnitDispositions := units.map fun sourceUnit =>
      { unitId := sourceUnit.id, role := .requirement }
    statements := [statement]
    statementCoverage := [{
      statementId := statement.id
      sourceUnitIds := units.map (·.id)
      leanClaims := { noSelectionReason := some "no logical Claim is selected in this archive fixture" }
      acceptanceCriteria := { noSelectionReason := some "archive fidelity is checked by the route test" }
      implementationRequired := true }]
    acceptanceCriteria := [] }

private def proposedDesign (result : MutationResult) : IO DesignRevision :=
  match result with
  | .design value => pure value
  | _ => throw (IO.userError "Design proposal returned the wrong public result shape")

def run : IO Unit := do
  IO.FS.withTempDir fun root => do
    let workbenchRoot := root / ".agent-workbench"
    let sourceRoot := workbenchRoot / "design" / "product"
    let database := workbenchRoot / "state.db"
    let target := "file:.agent-workbench/design/product/archive.md"
    let sourcePath := sourceRoot / "archive.md"
    IO.FS.createDirAll sourceRoot
    IO.FS.createDirAll (workbenchRoot / "design" / "implementation")
    let _ ← Store.open database
    let _ ← Store.executeMutation root database (.workStart {
      id := "work-archive", outcome := "retain exact Design history", scope := "project"
      responsibleAgentRun := "designer-archive" })
    let original := "The first archived requirement.\n"
    IO.FS.writeFile sourcePath original
    let firstInspection ← AgentWorkbench.DesignSource.inspectAll root [target]
    let firstUnits := firstInspection.flatMap (·.units)
    let first ← proposedDesign (← Store.executeMutation root database
      (.designPropose (proposal firstUnits target "capture the initial exact source")))
    let _ ← Store.executeMutation root database (.reviewStart {
      entryId := "review-design-archive", reviewId := "review-design-archive"
      purpose := .design, targetDesignRevision := some first.id
      reviewerAgentRun := "reviewer-archive" })
    let _ ← Store.executeMutation root database (.reviewFinding {
      entryId := "finding-design-archive", reviewEntryId := "review-design-archive"
      subject := { kind := FindingSubjectKind.statement, id := "statement-archive", exactQuote := "The archived requirement is authoritative." }
      summary := "the Design source must be amended" })
    let _ ← Store.executeMutation root database (.reviewDisposition {
      entryId := "disposition-design-archive", findingEntryId := "finding-design-archive"
      decision := .accepted, reason := "amend the immutable candidate" })
    let changed := "The amended archived requirement.\n"
    IO.FS.writeFile sourcePath changed
    let archivedFirst ← AgentWorkbench.DesignArchive.source root first.id target
    expect (archivedFirst.contentBytes == original.toUTF8.data.toList.map (·.toNat))
      "Design source query reread changed live Markdown instead of the SQLite BLOB"
    let secondInspection ← AgentWorkbench.DesignSource.inspectAll root [target]
    let secondUnits := secondInspection.flatMap (·.units)
    let beforeRejectedAmendment ← Store.loadState (← Store.openReadOnly database)
    let omittedBasisRejected ← try
        let _ ← Store.executeMutation root database
          (.designAmend (proposal secondUnits target "omit the accepted Finding" (some first.id)))
        pure false
      catch _ => pure true
    expect omittedBasisRejected
      "candidate amendment omitted the accepted Design Finding that caused the change"
    let afterRejectedAmendment ← Store.loadState (← Store.openReadOnly database)
    expect (afterRejectedAmendment == beforeRejectedAmendment)
      "rejected candidate amendment changed Design history or revision"
    let second ← proposedDesign (← Store.executeMutation root database
      (.designAmend (proposal secondUnits target "amend the archived requirement"
        (some first.id) ["finding-design-archive"])))
    let state ← Store.loadState (← Store.openReadOnly database)
    expect ((state.design? first.id).any (·.status == .superseded) &&
      (state.design? second.id).any (·.amendsCandidate == some first.id))
      "candidate amendment did not preserve a distinct immutable lineage"
    let difference ← AgentWorkbench.DesignArchive.diff root first.id second.id
    expect (difference.sources.any (fun source => source.target == target &&
      source.change == .changed && source.beforeDigest.isSome && source.afterDigest.isSome))
      "Design diff did not derive the archived byte change"
    IO.FS.removeFile sourcePath
    let archivedSecond ← AgentWorkbench.DesignArchive.source root second.id target
    expect (archivedSecond.contentBytes == changed.toUTF8.data.toList.map (·.toNat))
      "Design source query depended on a deleted live draft"

  IO.FS.withTempDir fun root => do
    let workbenchRoot := root / ".agent-workbench"
    let sourceRoot := workbenchRoot / "design" / "product"
    let database := workbenchRoot / "state.db"
    IO.FS.createDirAll sourceRoot
    IO.FS.createDirAll (workbenchRoot / "design" / "implementation")
    let _ ← Store.open database
    let _ ← Store.executeMutation root database (.workStart {
      id := "work-byte-archive", outcome := "retain exact source byte forms", scope := "project"
      responsibleAgentRun := "designer-bytes" })
    let fixtures : List (String × ByteArray) := [
      ("lf.md", "LF source.\n".toUTF8),
      ("crlf.md", "CRLF source.\r\n".toUTF8),
      ("bom.md", ByteArray.mk #[0xef, 0xbb, 0xbf] ++ "BOM source.\n".toUTF8),
      ("no-final-newline.md", "No final newline.".toUTF8),
      ("empty.md", ByteArray.empty)]
    let mut targets := []
    for (name, bytes) in fixtures do
      IO.FS.writeBinFile (sourceRoot / name) bytes
      targets := targets ++ [s!"file:.agent-workbench/design/product/{name}"]
    let captured ← AgentWorkbench.DesignSource.inspectAll root targets
    let units := captured.flatMap (·.units)
    let statement : Statement := {
      id := "statement-byte-archive", text := "Exact source bytes remain unchanged." }
    let request : DesignProposalRequest := {
      producerAgentRun := "designer-bytes"
      changeRationale := "exercise every required source byte representation"
      sourceDocumentTargets := targets
      sourceUnitDispositions := units.map fun unit =>
        { unitId := unit.id, role := .requirement }
      statements := [statement]
      statementCoverage := [{
        statementId := statement.id, sourceUnitIds := units.map (·.id)
        leanClaims := { noSelectionReason := some "byte fidelity is externally observed" }
        acceptanceCriteria := { noSelectionReason := some "the archive route is direct evidence" }
        implementationRequired := true }]
      acceptanceCriteria := [] }
    let design ← proposedDesign (← Store.executeMutation root database (.designPropose request))
    for ((_, expected), target) in fixtures.zip targets do
      let archived ← AgentWorkbench.DesignArchive.source root design.id target
      expect (archived.contentBytes == expected.data.toList.map (·.toNat))
        s!"Design source archive normalized exact bytes for {target}"

  -- The archive adapter is byte-safe even for diagnostic content that is not valid UTF-8.
  -- Public proposal still requires Markdown/Lean semantic validation before these rows can become
  -- authority; this lower-level case establishes retrieval and binary diff without lossy decoding.
  IO.FS.withTempDir fun root => do
    let database := root / ".agent-workbench" / "state.db"
    IO.FS.createDirAll (root / ".agent-workbench")
    let _ ← Store.open database
    let connection ← AgentWorkbench.SQLite.open database
    let target := "file:.agent-workbench/design/product/binary.md"
    let beforeBytes := ByteArray.mk #[0x00, 0xff, 0x80, 0x0a]
    let afterBytes := ByteArray.mk #[0x00, 0xfe, 0x81]
    let storedDesign (id digest : String) : DesignRevision := {
      design with
      id := id
      status := .candidate
      revisionContentDigest := s!"blake3:revision-{id}"
      sourceDocuments := [{ target, snapshot := digest }] }
    let beforeDigest := ContentDigest.bytes beforeBytes
    let afterDigest := ContentDigest.bytes afterBytes
    for (revision, bytes, digest) in [
        (storedDesign "design-binary-before" beforeDigest, beforeBytes, beforeDigest),
        (storedDesign "design-binary-after" afterDigest, afterBytes, afterDigest)] do
      AgentWorkbench.SQLite.executeValues connection
        "INSERT INTO design_revisions(
           id, accepted_parent_id, amends_candidate_id, status, producer_run,
           change_rationale, revision_content_digest, structured_document
         ) VALUES (?1, NULL, NULL, 'candidate', ?2, ?3, ?4, ?5)"
        #[.text revision.id, .text revision.producerAgentRun,
          .text revision.changeRationale, .text revision.revisionContentDigest,
          .text (Lean.toJson revision).compress]
      AgentWorkbench.SQLite.executeValues connection
        "INSERT INTO design_sources(design_id, ordinal, target, media_kind, digest, content)
         VALUES (?1, 0, ?2, 'markdown', ?3, ?4)"
        #[.text revision.id, .text target, .text digest, .blob bytes]
    let archived ← AgentWorkbench.DesignArchive.source root "design-binary-before" target
    expect (archived.contentBytes == beforeBytes.data.toList.map (·.toNat))
      "Design archive could not retrieve arbitrary source bytes exactly"
    let difference ← AgentWorkbench.DesignArchive.diff root
      "design-binary-before" "design-binary-after"
    expect (difference.sources.any fun source =>
      source.target == target && source.change == .binaryChanged && source.lineEdits.isEmpty)
      "Design archive attempted a lossy text diff for invalid UTF-8 bytes"

end AgentWorkbenchTest.DesignArchive
