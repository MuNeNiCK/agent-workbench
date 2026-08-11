import AgentWorkbench.Adapter.StoreBase
import AgentWorkbench.Adapter.StoreCodec
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Domain.Validation

namespace AgentWorkbench.Store

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => fail message

private def loadDocuments [Lean.FromJson α] [ReadableStore S]
    (store : S) (kind table : String) (orderBy : String) : IO (List α) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
    s!"SELECT document FROM {table} ORDER BY {orderBy}" #[] 1
  let mut values := []
  for row in rows do
    let source ← match row[0]? with
      | some value => pure value
      | none => fail s!"missing {kind} document column"
    values := values ++ [← Codec.decode kind source]
  pure values

private def loadDesigns [ReadableStore S] (store : S) : IO (List DesignRevision) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
    "SELECT id, COALESCE(accepted_parent_id, ''), COALESCE(amends_candidate_id, ''),
       status, producer_run, change_rationale, revision_content_digest, structured_document
     FROM design_revisions ORDER BY id" #[] 8
  let mut values := []
  for row in rows do
    let design ← Codec.decodeDesign row[7]!
    if design.id != row[0]! || Codec.optionText design.parent != row[1]! ||
        Codec.optionText design.amendsCandidate != row[2]! ||
        Codec.designStatusName design.status != row[3]! ||
        design.producerAgentRun != row[4]! || design.changeRationale != row[5]! ||
        design.revisionContentDigest != row[6]! then
      fail s!"normalized Design columns differ from persisted document: {design.id}"
    let bases ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
      "SELECT ledger_entry_id FROM design_change_bases
       WHERE design_id = ?1 ORDER BY ordinal" #[design.id] 1
    if bases.toList.map (fun basis => basis[0]!) != design.changeBasisEntryIds then
      fail s!"Design change bases differ from persisted document: {design.id}"
    values := values ++ [design]
  pure values

private def loadWorks [ReadableStore S] (store : S) : IO (List Work) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
    "SELECT id, status, scope, outcome,
       COALESCE(baseline_design_id, ''), COALESCE(design_revision_id, ''),
       responsible_run, COALESCE(resume_condition, ''),
       COALESCE(migration_diagnostic, ''), document
     FROM works ORDER BY id" #[] 10
  let mut values := []
  for row in rows do
    let work : Work ← Codec.decode "work" row[9]!
    if work.id != row[0]! || Codec.workStatusName work.status != row[1]! ||
        work.scope != row[2]! || work.outcome != row[3]! ||
        Codec.optionText work.baselineDesignRevision != row[4]! ||
        Codec.optionText work.designRevision != row[5]! ||
        work.responsibleAgentRun != row[6]! ||
        Codec.optionText work.resumeCondition != row[7]! ||
        Codec.optionText work.migrationDiagnostic != row[8]! then
      fail s!"normalized Work columns differ from persisted document: {work.id}"
    values := values ++ [work]
  pure values

private def loadPlans [ReadableStore S] (store : S) : IO (List ImplementationPlan) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
    "SELECT id, work_id, design_revision_id, COALESCE(predecessor_plan_id, ''),
       status, producer_run, reason, content_digest, document
     FROM implementation_plans ORDER BY id" #[] 9
  let mut values := []
  for row in rows do
    let plan : ImplementationPlan ← Codec.decode "Implementation Plan" row[8]!
    if plan.id != row[0]! || plan.workId != row[1]! ||
        plan.designRevision != row[2]! || Codec.optionText plan.predecessorPlanId != row[3]! ||
        Codec.planStatusName plan.status != row[4]! || plan.producerAgentRun != row[5]! ||
        plan.reason != row[6]! || plan.contentDigest != row[7]! then
      fail s!"normalized Plan columns differ from persisted document: {plan.id}"
    let bases ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
      "SELECT ledger_entry_id FROM implementation_plan_change_bases
       WHERE plan_id = ?1 ORDER BY ordinal" #[plan.id] 1
    if bases.toList.map (fun basis => basis[0]!) != plan.changeBasisEntryIds then
      fail s!"Plan change bases differ from persisted document: {plan.id}"
    let sources ← AgentWorkbench.SQLite.queryTextTextBlobRows (readConnection store)
      "SELECT target, digest, content FROM implementation_plan_sources
       WHERE plan_id = ?1 ORDER BY ordinal" #[plan.id]
    if sources.size != plan.sourceDocuments.length then
      fail s!"Plan {plan.id} source archive is incomplete"
    for (manifest, index) in plan.sourceDocuments.zipIdx do
      let (target, digest, content) := sources[index]!
      if manifest.target != target || manifest.digest != digest ||
          ContentDigest.bytes content != digest then
        fail s!"Plan {plan.id} source archive differs from its immutable manifest"
    let material := Lean.toJson { plan with contentDigest := "", status := .candidate }
    if ContentDigest.string material.compress != plan.contentDigest then
      fail s!"Plan {plan.id} immutable content digest is invalid"
    values := values ++ [plan]
  pure values

private def validateDesignArchives [ReadableStore S]
    (store : S) (designs : List DesignRevision) : IO Unit := do
  for design in designs do
    let textRows ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
      "SELECT target, media_kind, digest FROM design_sources
       WHERE design_id = ?1 ORDER BY ordinal" #[design.id] 3
    let blobs ← AgentWorkbench.SQLite.queryBlobRows (readConnection store)
      "SELECT content FROM design_sources
       WHERE design_id = ?1 ORDER BY ordinal" #[design.id]
    if !design.sourceArchiveAvailable then
      unless textRows.isEmpty && blobs.isEmpty do
        fail s!"legacy Design {design.id} unexpectedly has archived sources"
    else
      if textRows.size != blobs.size || textRows.size != design.sourceDocuments.length then
        fail s!"Design {design.id} source archive is incomplete"
      for (source, index) in design.sourceDocuments.zipIdx do
        let row ← match textRows[index]? with
          | some value => pure value
          | none => fail s!"Design {design.id} source archive is incomplete"
        let content ← match blobs[index]? with
          | some value => pure value
          | none => fail s!"Design {design.id} source archive is incomplete"
        if source.target != row[0]! || source.mediaKind != row[1]! ||
            source.snapshot != row[2]! || ContentDigest.bytes content != row[2]! then
          fail s!"Design {design.id} source archive differs from its immutable manifest"
      let material := Codec.designDigestMaterial design
      if ContentDigest.string material.compress != design.revisionContentDigest then
        fail s!"Design {design.id} immutable content digest is invalid"

private def validateReviewManifests (entries : List LedgerEntry) : IO Unit := do
  for entry in entries do
    match entry.payload with
    | .review review =>
        unless review.targetManifest.isEmpty do
          if ContentDigest.string (Lean.toJson review.targetManifest).compress !=
              review.targetSnapshot then
            fail s!"Review {entry.id} fixed manifest digest is invalid"
    | _ => pure ()

def loadState [ReadableStore S] (store : S) : IO ProjectState := do
  let metadata ← AgentWorkbench.SQLite.queryTextRows (readConnection store)
    "SELECT CAST(schema_revision AS TEXT), CAST(state_revision AS TEXT),
      COALESCE(accepted_design_id, ''), COALESCE(focused_work_id, '')
     FROM project_metadata WHERE singleton = 1" #[] 4
  let row ← match metadata[0]? with
    | some value => pure value
    | none => fail "project metadata is missing"
  if metadata.size != 1 || row.size != 4 then fail "project metadata is not singular"
  let storedSchema ← Codec.parseNat "schema revision" row[0]!
  if storedSchema != schemaRevision then
    fail s!"unsupported schema revision {storedSchema}; expected {schemaRevision}"
  let revision ← Codec.parseNat "state revision" row[1]!
  let designs ← loadDesigns store
  validateDesignArchives store designs
  let works ← loadWorks store
  let plans ← loadPlans store
  let entries ← loadDocuments store "ledger entry" "ledger_entries" "entry_order"
  validateReviewManifests entries
  let rawState : ProjectState :=
    { revision
      acceptedDesignId := Codec.textOption row[2]!
      focusedWorkId := Codec.textOption row[3]!
      designRevisions := designs
      works
      implementationPlans := plans
      ledgerEntries := entries }
  let state := restoreCompletionMonotonicProjection rawState
  fromExcept (validateState state)
  pure state

end AgentWorkbench.Store
