import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.StoreSchema
import AgentWorkbench.Domain.Validation
import AgentWorkbench.Decision.Operation
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
import AgentWorkbench.Adapter.ManagedOutput
import AgentWorkbench.Adapter.OperationLock
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbench.Adapter.DesignClaim

namespace AgentWorkbench.Store

inductive Access where
  | readOnly
  | readWrite

structure Store (access : Access) where private mk ::
  private connection : AgentWorkbench.SQLite.Connection
  private migratedFromLegacy : Bool := false

abbrev ReadStore := Store .readOnly
abbrev WriteStore := Store .readWrite

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => fail message

private def encode [Lean.ToJson α] (value : α) : String :=
  (Lean.toJson value).compress

private def decode [Lean.FromJson α] (kind source : String) : IO α := do
  let json ← fromExcept (Lean.Json.parse source)
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error error => fail s!"invalid persisted {kind}: {error}"

private structure LegacyDesignSource where
  target : String
  snapshot : String
  deriving Lean.FromJson

private structure LegacyAcceptanceCriterion where
  id : String
  statement : String
  target : String
  evidenceKind : String
  deriving Lean.FromJson

private structure LegacyLeanClaim where
  id : String
  input : ClaimInput
  deriving Lean.FromJson

private structure LegacyDesignRevision where
  id : String
  parent : Option String := none
  createdAfterEntryOrder : Nat := 0
  status : DesignStatus := .candidate
  producerAgentRun : String
  sourceDocuments : List LegacyDesignSource := []
  statements : List Statement
  acceptanceCriteria : List LegacyAcceptanceCriterion
  leanClaims : List LegacyLeanClaim := []
  deriving Lean.FromJson

private def LegacyDesignRevision.upgrade (legacy : LegacyDesignRevision) : DesignRevision :=
  { id := legacy.id
    parent := legacy.parent
    createdAfterEntryOrder := legacy.createdAfterEntryOrder
    status := legacy.status
    producerAgentRun := legacy.producerAgentRun
    changeRationale := "legacy source unavailable"
    sourceDocuments := legacy.sourceDocuments.map fun source =>
      { target := source.target, snapshot := source.snapshot }
    statements := legacy.statements
    acceptanceCriteria := legacy.acceptanceCriteria.map fun criterion =>
      { id := criterion.id, statement := criterion.statement, target := criterion.target,
        evidenceKind := criterion.evidenceKind }
    leanClaims := legacy.leanClaims.map fun claim => { id := claim.id, input := claim.input } }

private def decodeDesign (source : String) : IO DesignRevision := do
  let json ← fromExcept (Lean.Json.parse source)
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error currentError =>
      match Lean.fromJson? json with
      | .ok legacy => pure (LegacyDesignRevision.upgrade legacy)
      | .error legacyError =>
          fail s!"invalid persisted design revision: {currentError}; legacy: {legacyError}"

private def designStatusName : DesignStatus → String
  | .candidate => "candidate"
  | .accepted => "accepted"
  | .superseded => "superseded"
  | .replaced => "replaced"
  | .rejected => "rejected"

private def parseNat (field value : String) : IO Nat :=
  match value.toNat? with
  | some parsed => pure parsed
  | none => fail s!"invalid persisted {field}: {value}"

private def optionText (value : Option String) : String :=
  value.getD ""

private def textOption (value : String) : Option String :=
  if value.isEmpty then none else some value

private def payloadKind : EntryPayload → String
  | .task _ => "task"
  | .workDesignAdoption _ => "work-design-adoption"
  | .workHandoff _ => "work-handoff"
  | .workWithdrawal _ => "work-withdrawal"
  | .workCompletion _ => "work-completion"
  | .designRejection _ => "design-rejection"
  | .commandProfile _ => "command-profile"
  | .commandExecution _ => "command-execution"
  | .artifactObservation _ => "artifact-observation"
  | .review _ => "review"
  | .finding _ => "finding"
  | .reviewDisposition _ => "review-disposition"
  | .reviewVerification _ => "review-verification"
  | .reviewHandoff _ => "review-handoff"
  | .reviewConclusion _ => "review-conclusion"
  | .userCorrection _ => "user-correction"
  | .kpt _ => "kpt"
  | .leanProofReceipt _ => "lean-proof-receipt"

def «open» (path : System.FilePath) : IO WriteStore := do
  let connection ← AgentWorkbench.SQLite.open path
  let schemaResult ← AgentWorkbench.StoreSchema.initializeStoreSchema connection
  pure { connection, migratedFromLegacy := schemaResult == .migrated }

/-- Opens existing authoritative state without schema creation or migration capability. -/
def openReadOnly (path : System.FilePath) : IO ReadStore := do
  let connection ← AgentWorkbench.SQLite.openReadOnly path
  pure { connection }

def wasMigratedFromLegacy (store : WriteStore) : Bool :=
  store.migratedFromLegacy

def recoverManagedOperations (projectRoot : System.FilePath) (store : WriteStore) : IO Unit := do
  let rows ← AgentWorkbench.SQLite.queryTextRows store.connection
    "SELECT operation_id, CAST(expected_state_revision AS TEXT), recovery_policy, manifest,
       COALESCE(CAST(committed_state_revision AS TEXT), '')
     FROM managed_operations ORDER BY operation_id" #[] 5
  let stateRevision ← AgentWorkbench.SQLite.queryScalar store.connection
    "SELECT CAST(state_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
  for row in rows do
    let operationId := row[0]!
    let policy := row[2]!
    if policy != "restore-proof-outputs" && policy != "retain-command-output" then
      fail s!"managed operation {operationId} has unknown recovery policy"
    if row[4]!.isEmpty then
      if row[1]! != stateRevision then
        fail s!"uncommitted managed operation {operationId} has stale expected revision"
    else if row[4]! != stateRevision then
      fail s!"committed managed operation {operationId} does not match authoritative revision"
    if policy == "restore-proof-outputs" then
      let manifest ← decode (α := AgentWorkbench.ProofBuild.ManagedOutputManifest)
        "managed operation manifest" row[3]!
      AgentWorkbench.ProofBuild.restoreLayouts manifest.layouts
    else if row[4]!.isEmpty then
      let baseline ← decode (α := AgentWorkbench.ManagedOutput.Baseline)
        "managed command-output baseline" row[3]!
      AgentWorkbench.ManagedOutput.restore projectRoot baseline
    AgentWorkbench.SQLite.immediateTransaction store.connection do
      AgentWorkbench.SQLite.execute store.connection
        "DELETE FROM managed_operations WHERE operation_id = ?1" #[operationId]

private def loadDocuments [Lean.FromJson α] {access : Access}
    (store : Store access) (kind table : String) (orderBy : String) : IO (List α) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows store.connection
    s!"SELECT document FROM {table} ORDER BY {orderBy}" #[] 1
  let mut values := []
  for row in rows do
    let source ← match row[0]? with
      | some value => pure value
      | none => fail s!"missing {kind} document column"
    values := values ++ [← decode kind source]
  pure values

private def loadDesigns {access : Access} (store : Store access) : IO (List DesignRevision) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows store.connection
    "SELECT id, COALESCE(accepted_parent_id, ''), COALESCE(amends_candidate_id, ''),
       status, producer_run, change_rationale, revision_content_digest, structured_document
     FROM design_revisions ORDER BY id" #[] 8
  let mut values := []
  for row in rows do
    let design ← decodeDesign row[7]!
    if design.id != row[0]! || optionText design.parent != row[1]! ||
        optionText design.amendsCandidate != row[2]! ||
        designStatusName design.status != row[3]! ||
        design.producerAgentRun != row[4]! || design.changeRationale != row[5]! ||
        design.revisionContentDigest != row[6]! then
      fail s!"normalized Design columns differ from persisted document: {design.id}"
    let bases ← AgentWorkbench.SQLite.queryTextRows store.connection
      "SELECT ledger_entry_id FROM design_change_bases
       WHERE design_id = ?1 ORDER BY ordinal" #[design.id] 1
    if bases.toList.map (fun basis => basis[0]!) != design.changeBasisEntryIds then
      fail s!"Design change bases differ from persisted document: {design.id}"
    values := values ++ [design]
  pure values

private def workStatusName : WorkStatus → String
  | .active => "active"
  | .suspended => "suspended"
  | .completed => "completed"
  | .withdrawn => "withdrawn"

private def loadWorks {access : Access} (store : Store access) : IO (List Work) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows store.connection
    "SELECT id, status, scope, outcome,
       COALESCE(baseline_design_id, ''), COALESCE(design_revision_id, ''),
       responsible_run, COALESCE(resume_condition, ''), document
     FROM works ORDER BY id" #[] 9
  let mut values := []
  for row in rows do
    let work : Work ← decode "work" row[8]!
    if work.id != row[0]! || workStatusName work.status != row[1]! ||
        work.scope != row[2]! || work.outcome != row[3]! ||
        optionText work.baselineDesignRevision != row[4]! ||
        optionText work.designRevision != row[5]! ||
        work.responsibleAgentRun != row[6]! ||
        optionText work.resumeCondition != row[7]! then
      fail s!"normalized Work columns differ from persisted document: {work.id}"
    values := values ++ [work]
  pure values

private def planStatusName : PlanStatus → String
  | .candidate => "candidate"
  | .current => "current"
  | .superseded => "superseded"

private def loadPlans {access : Access} (store : Store access) : IO (List ImplementationPlan) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows store.connection
    "SELECT id, work_id, design_revision_id, COALESCE(predecessor_plan_id, ''),
       status, producer_run, reason, content_digest, document
     FROM implementation_plans ORDER BY id" #[] 9
  let mut values := []
  for row in rows do
    let plan : ImplementationPlan ← decode "Implementation Plan" row[8]!
    if plan.id != row[0]! || plan.workId != row[1]! ||
        plan.designRevision != row[2]! || optionText plan.predecessorPlanId != row[3]! ||
        planStatusName plan.status != row[4]! || plan.producerAgentRun != row[5]! ||
        plan.reason != row[6]! || plan.contentDigest != row[7]! then
      fail s!"normalized Plan columns differ from persisted document: {plan.id}"
    let bases ← AgentWorkbench.SQLite.queryTextRows store.connection
      "SELECT ledger_entry_id FROM implementation_plan_change_bases
       WHERE plan_id = ?1 ORDER BY ordinal" #[plan.id] 1
    if bases.toList.map (fun basis => basis[0]!) != plan.changeBasisEntryIds then
      fail s!"Plan change bases differ from persisted document: {plan.id}"
    let sources ← AgentWorkbench.SQLite.queryTextTextBlobRows store.connection
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

private def validateDesignArchives {access : Access}
    (store : Store access) (designs : List DesignRevision) : IO Unit := do
  for design in designs do
    let textRows ← AgentWorkbench.SQLite.queryTextRows store.connection
      "SELECT target, media_kind, digest FROM design_sources
       WHERE design_id = ?1 ORDER BY ordinal" #[design.id] 3
    let blobs ← AgentWorkbench.SQLite.queryBlobRows store.connection
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
      let material := Lean.toJson {
        design with revisionContentDigest := "", status := .candidate }
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

def loadState {access : Access} (store : Store access) : IO ProjectState := do
  let metadata ← AgentWorkbench.SQLite.queryTextRows store.connection
    "SELECT CAST(schema_revision AS TEXT), CAST(state_revision AS TEXT),
      COALESCE(accepted_design_id, ''), COALESCE(focused_work_id, '')
     FROM project_metadata WHERE singleton = 1" #[] 4
  let row ← match metadata[0]? with
    | some value => pure value
    | none => fail "project metadata is missing"
  if metadata.size != 1 || row.size != 4 then fail "project metadata is not singular"
  let storedSchema ← parseNat "schema revision" row[0]!
  if storedSchema != schemaRevision then
    fail s!"unsupported schema revision {storedSchema}; expected {schemaRevision}"
  let revision ← parseNat "state revision" row[1]!
  let designs ← loadDesigns store
  validateDesignArchives store designs
  let works ← loadWorks store
  let plans ← loadPlans store
  let entries ← loadDocuments store "ledger entry" "ledger_entries" "entry_order"
  validateReviewManifests entries
  let state : ProjectState :=
    { revision
      acceptedDesignId := textOption row[2]!
      focusedWorkId := textOption row[3]!
      designRevisions := designs
      works
      implementationPlans := plans
      ledgerEntries := entries }
  fromExcept (validateState state)
  pure state

private def ensureAppendOnly (prior next : ProjectState) : IO Unit := do
  if next.ledgerEntries.take prior.ledgerEntries.length != prior.ledgerEntries then
    fail "transition reordered or replaced the existing ledger prefix"
  for old in prior.designRevisions do
    let current ← match next.designRevisions.find? (fun value => value.id == old.id) with
      | some value => pure value
      | none => fail s!"transition removed design {old.id}"
    if { old with status := current.status } != current then
      fail s!"transition mutated immutable design content {old.id}"
  for old in prior.works do
    let current ← match next.works.find? (fun value => value.id == old.id) with
      | some value => pure value
      | none => fail s!"transition removed Work {old.id}"
    if old.outcome != current.outcome || old.scope != current.scope ||
        old.baselineDesignRevision != current.baselineDesignRevision then
      fail s!"transition mutated immutable Work identity {old.id}"
    if old.responsibleAgentRun != current.responsibleAgentRun then
      let recorded := (next.ledgerEntries.drop prior.ledgerEntries.length).any (fun entry =>
        entry.workId == some old.id && match entry.payload with
        | .workHandoff handoff =>
            handoff.predecessorRun == old.responsibleAgentRun &&
            handoff.successorRun == current.responsibleAgentRun
        | _ => false)
      if !recorded then fail s!"transition changed Work responsibility without a handoff {old.id}"
  for old in prior.implementationPlans do
    let current ← match next.implementationPlans.find? (fun value => value.id == old.id) with
      | some value => pure value
      | none => fail s!"transition removed Plan {old.id}"
    if { old with status := current.status } != current then
      fail s!"transition mutated immutable Plan content {old.id}"
  for old in prior.ledgerEntries do
    match next.ledgerEntries.find? (fun value => value.id == old.id) with
    | some current =>
        if current != old then fail s!"transition mutated immutable ledger entry {old.id}"
    | none => fail s!"transition removed ledger entry {old.id}"

private def insertDesign (store : WriteStore) (design : DesignRevision) : IO Unit := do
  AgentWorkbench.SQLite.executeValues store.connection
    "INSERT INTO design_revisions(
       id, accepted_parent_id, amends_candidate_id, status, producer_run,
       change_rationale, revision_content_digest, structured_document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    #[.text design.id,
      design.parent.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      design.amendsCandidate.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text (designStatusName design.status),
      .text design.producerAgentRun, .text design.changeRationale,
      .text design.revisionContentDigest, .text (encode design)]
  for (basis, ordinal) in design.changeBasisEntryIds.zipIdx do
    AgentWorkbench.SQLite.execute store.connection
      "INSERT INTO design_change_bases(design_id, ordinal, ledger_entry_id)
       VALUES (?1, ?2, ?3)" #[design.id, toString ordinal, basis]

private def updateDesignStatus (store : WriteStore) (design : DesignRevision) : IO Unit := do
  AgentWorkbench.SQLite.execute store.connection
    "UPDATE design_revisions SET status = ?1, structured_document = ?2 WHERE id = ?3"
    #[designStatusName design.status, encode design, design.id]
  if (← AgentWorkbench.SQLite.changes store.connection) != 1 then
    fail s!"Design status update did not target exactly one row: {design.id}"

private def persistDesignChanges
    (store : WriteStore) (prior next : ProjectState) : IO Unit := do
  for design in next.designRevisions do
    match prior.designRevisions.find? (fun old => old.id == design.id) with
    | none => insertDesign store design
    | some old =>
        if old.status != design.status then updateDesignStatus store design

private def insertPlan (store : WriteStore) (plan : ImplementationPlan) : IO Unit := do
  AgentWorkbench.SQLite.executeValues store.connection
    "INSERT INTO implementation_plans(
       id, work_id, design_revision_id, predecessor_plan_id, status,
       producer_run, reason, content_digest, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    #[.text plan.id, .text plan.workId, .text plan.designRevision,
      plan.predecessorPlanId.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text (planStatusName plan.status), .text plan.producerAgentRun,
      .text plan.reason, .text plan.contentDigest, .text (encode plan)]
  for (basis, ordinal) in plan.changeBasisEntryIds.zipIdx do
    AgentWorkbench.SQLite.execute store.connection
      "INSERT INTO implementation_plan_change_bases(plan_id, ordinal, ledger_entry_id)
       VALUES (?1, ?2, ?3)" #[plan.id, toString ordinal, basis]

private def updatePlanStatus (store : WriteStore) (plan : ImplementationPlan) : IO Unit := do
  AgentWorkbench.SQLite.execute store.connection
    "UPDATE implementation_plans SET status = ?1, document = ?2 WHERE id = ?3"
    #[planStatusName plan.status, encode plan, plan.id]
  if (← AgentWorkbench.SQLite.changes store.connection) != 1 then
    fail s!"Plan status update did not target exactly one row: {plan.id}"

private def persistPlanChanges
    (store : WriteStore) (prior next : ProjectState) : IO Unit := do
  for plan in next.implementationPlans do
    match prior.implementationPlans.find? (fun old => old.id == plan.id) with
    | none => insertPlan store plan
    | some old => if old.status != plan.status then updatePlanStatus store plan

private def insertPlanSource
    (store : WriteStore) (planId : String) (ordinal : Nat)
    (source : AgentWorkbench.PlanSource.Captured) : IO Unit :=
  AgentWorkbench.SQLite.executeValues store.connection
    "INSERT INTO implementation_plan_sources(plan_id, ordinal, target, digest, content)
     VALUES (?1, ?2, ?3, ?4, ?5)"
    #[.text planId, .text (toString ordinal), .text source.target,
      .text source.digest, .blob source.content]

private def insertDesignSource
    (store : WriteStore) (designId : String) (ordinal : Nat)
    (source : AgentWorkbench.DesignSource.Captured) : IO Unit :=
  AgentWorkbench.SQLite.executeValues store.connection
    "INSERT INTO design_sources(design_id, ordinal, target, media_kind, digest, content)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    #[.text designId, .text (toString ordinal), .text source.target,
      .text source.mediaKind, .text source.digest, .blob source.content]

private def writeWork (store : WriteStore) (work : Work) : IO Unit :=
  AgentWorkbench.SQLite.executeValues store.connection
    "INSERT INTO works(
       id, status, scope, outcome, baseline_design_id, design_revision_id,
       responsible_run, resume_condition, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
     ON CONFLICT(id) DO UPDATE SET
       status = excluded.status,
       scope = excluded.scope,
       outcome = excluded.outcome,
       design_revision_id = excluded.design_revision_id,
       responsible_run = excluded.responsible_run,
       resume_condition = excluded.resume_condition,
       document = excluded.document"
    #[.text work.id, .text (workStatusName work.status), .text work.scope, .text work.outcome,
      work.baselineDesignRevision.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      work.designRevision.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text work.responsibleAgentRun,
      work.resumeCondition.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text (encode work)]

private def insertEntry (store : WriteStore) (entry : LedgerEntry) : IO Unit :=
  AgentWorkbench.SQLite.execute store.connection
    "INSERT INTO ledger_entries(
       id, entry_order, scope, work_id, design_revision, payload_kind, document
     ) VALUES (?1, ?2, ?3, NULLIF(?4, ''), NULLIF(?5, ''), ?6, ?7)"
    #[entry.id, toString entry.order, entry.scope, optionText entry.workId,
      optionText entry.designRevision, payloadKind entry.payload, encode entry]

def commitOperation
    (store : WriteStore) (operation : Operation)
    (expectedRevision : Nat) (next : ProjectState)
    (managedOperationId : Option String := none) : IO Unit := do
  fromExcept (validateState next)
  if next.revision != expectedRevision + 1 then
    fail s!"transition revision must advance exactly once from {expectedRevision}"
  AgentWorkbench.SQLite.immediateTransaction store.connection do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationApplicable prior operation ||
        (operation == .init && store.migratedFromLegacy) do
      fail s!"operation is not applicable in the current state: {operation.name}"
    ensureAppendOnly prior next
    persistDesignChanges store prior next
    persistPlanChanges store prior next
    for work in next.works do writeWork store work
    for entry in next.ledgerEntries.drop prior.ledgerEntries.length do insertEntry store entry
    AgentWorkbench.SQLite.execute store.connection
      "UPDATE project_metadata SET
         state_revision = ?1,
         accepted_design_id = NULLIF(?2, ''),
         focused_work_id = NULLIF(?3, '')
       WHERE singleton = 1 AND state_revision = ?4"
      #[toString next.revision, optionText next.acceptedDesignId,
        optionText next.focusedWorkId, toString expectedRevision]
    if (← AgentWorkbench.SQLite.changes store.connection) != 1 then
      fail "concurrent project metadata update rejected"
    if let some operationId := managedOperationId then
      AgentWorkbench.SQLite.execute store.connection
        "UPDATE managed_operations SET committed_state_revision = ?1
         WHERE operation_id = ?2 AND expected_state_revision = ?3
           AND committed_state_revision IS NULL"
        #[toString next.revision, operationId, toString expectedRevision]
      if (← AgentWorkbench.SQLite.changes store.connection) != 1 then
        fail "managed operation commit marker was not advanced atomically"
  let committed ← loadState store
  if committed != next then fail "committed state differs from the validated transition result"

def commitDesignProposal
    (store : WriteStore) (operation : Operation) (expectedRevision : Nat) (next : ProjectState)
    (design : DesignRevision) (sources : List AgentWorkbench.DesignSource.Captured) : IO Unit := do
  fromExcept (validateState next)
  if next.revision != expectedRevision + 1 then
    fail s!"transition revision must advance exactly once from {expectedRevision}"
  AgentWorkbench.SQLite.immediateTransaction store.connection do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationApplicable prior operation do
      fail s!"{operation.name} is not applicable in the authoritative state"
    ensureAppendOnly prior next
    persistDesignChanges store prior next
    persistPlanChanges store prior next
    for work in next.works do writeWork store work
    for (source, ordinal) in sources.zipIdx do insertDesignSource store design.id ordinal source
    AgentWorkbench.SQLite.execute store.connection
      "UPDATE project_metadata SET state_revision = ?1 WHERE singleton = 1 AND state_revision = ?2"
      #[toString next.revision, toString expectedRevision]
    if (← AgentWorkbench.SQLite.changes store.connection) != 1 then
      fail "concurrent project metadata update rejected"
  let committed ← loadState store
  if committed != next then
    let message := s!"committed Design proposal differs from its validated result " ++
      s!"(revision={committed.revision == next.revision}, " ++
      s!"designs={committed.designRevisions == next.designRevisions}, " ++
      s!"works={committed.works == next.works}, plans={committed.implementationPlans == next.implementationPlans}, " ++
      s!"ledger={committed.ledgerEntries == next.ledgerEntries}, designRows=" ++
      reprStr (committed.designRevisions.zip next.designRevisions |>.map fun pair =>
        (pair.1.id, pair.2.id, pair.1 == pair.2)) ++ ")"
    fail message

def commitPlanProposal
    (store : WriteStore) (operation : Operation) (expectedRevision : Nat) (next : ProjectState)
    (plan : ImplementationPlan) (sources : List AgentWorkbench.PlanSource.Captured) : IO Unit := do
  fromExcept (validateState next)
  if next.revision != expectedRevision + 1 then
    fail s!"transition revision must advance exactly once from {expectedRevision}"
  AgentWorkbench.SQLite.immediateTransaction store.connection do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationApplicable prior operation do
      fail s!"{operation.name} is not applicable in the authoritative state"
    ensureAppendOnly prior next
    persistPlanChanges store prior next
    for (source, ordinal) in sources.zipIdx do insertPlanSource store plan.id ordinal source
    AgentWorkbench.SQLite.execute store.connection
      "UPDATE project_metadata SET state_revision = ?1 WHERE singleton = 1 AND state_revision = ?2"
      #[toString next.revision, toString expectedRevision]
    if (← AgentWorkbench.SQLite.changes store.connection) != 1 then
      fail "concurrent project metadata update rejected"
  let committed ← loadState store
  if committed != next then fail "committed Plan proposal differs from its validated result"

def beginManagedOperation
    (store : WriteStore) (operationId : String) (expectedRevision : Nat)
    (recoveryPolicy manifest : String) : IO Unit :=
  AgentWorkbench.SQLite.immediateTransaction store.connection do
    AgentWorkbench.SQLite.execute store.connection
      "INSERT INTO managed_operations(
         operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
       ) VALUES (?1, ?2, ?3, ?4, NULL)"
      #[operationId, toString expectedRevision, recoveryPolicy, manifest]

def clearManagedOperation (store : WriteStore) (operationId : String) : IO Unit :=
  AgentWorkbench.SQLite.immediateTransaction store.connection do
    AgentWorkbench.SQLite.execute store.connection
      "DELETE FROM managed_operations WHERE operation_id = ?1" #[operationId]

end AgentWorkbench.Store
