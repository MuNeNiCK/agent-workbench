import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Domain.Validation
import AgentWorkbench.Decision.Operation
import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Current
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Profile
import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Guidance
import AgentWorkbench.Application.Review

namespace AgentWorkbench.Store

structure Store where private mk ::
  private connection : AgentWorkbench.SQLite.Connection

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
  | .commandProfile _ => "command-profile"
  | .commandExecution _ => "command-execution"
  | .artifactObservation _ => "artifact-observation"
  | .review _ => "review"
  | .finding _ => "finding"
  | .reviewDisposition _ => "review-disposition"
  | .reviewVerification _ => "review-verification"
  | .userCorrection _ => "user-correction"
  | .kpt _ => "kpt"
  | .leanProofReceipt _ => "lean-proof-receipt"

private def initializeSchema (connection : AgentWorkbench.SQLite.Connection) : IO Unit := do
  AgentWorkbench.SQLite.runScript connection "
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS project_metadata(
      singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
      schema_revision INTEGER NOT NULL,
      state_revision INTEGER NOT NULL,
      accepted_design_id TEXT,
      focused_work_id TEXT
    ) STRICT;
    CREATE TABLE IF NOT EXISTS design_revisions(
      id TEXT PRIMARY KEY,
      document TEXT NOT NULL
    ) STRICT;
    CREATE TABLE IF NOT EXISTS works(
      id TEXT PRIMARY KEY,
      design_revision TEXT NOT NULL,
      status TEXT NOT NULL,
      scope TEXT NOT NULL,
      document TEXT NOT NULL
    ) STRICT;
    CREATE INDEX IF NOT EXISTS works_by_design ON works(design_revision);
    CREATE INDEX IF NOT EXISTS works_by_scope_status ON works(scope, status);
    CREATE TABLE IF NOT EXISTS ledger_entries(
      id TEXT PRIMARY KEY,
      entry_order INTEGER NOT NULL UNIQUE,
      scope TEXT NOT NULL,
      work_id TEXT,
      design_revision TEXT,
      payload_kind TEXT NOT NULL,
      document TEXT NOT NULL
    ) STRICT;
    CREATE INDEX IF NOT EXISTS ledger_by_context
      ON ledger_entries(scope, work_id, design_revision, entry_order);
    CREATE INDEX IF NOT EXISTS ledger_by_kind
      ON ledger_entries(payload_kind, entry_order);
    INSERT OR IGNORE INTO project_metadata(
      singleton, schema_revision, state_revision, accepted_design_id, focused_work_id
    ) VALUES (1, 1, 0, NULL, NULL);"

def «open» (path : System.FilePath) : IO Store := do
  let connection ← AgentWorkbench.SQLite.open path
  initializeSchema connection
  pure { connection }

private def loadDocuments [Lean.FromJson α]
    (store : Store) (kind table : String) (orderBy : String) : IO (List α) := do
  let rows ← AgentWorkbench.SQLite.queryTextRows store.connection
    s!"SELECT document FROM {table} ORDER BY {orderBy}" #[] 1
  let mut values := []
  for row in rows do
    let source ← match row[0]? with
      | some value => pure value
      | none => fail s!"missing {kind} document column"
    values := values ++ [← decode kind source]
  pure values

def loadState (store : Store) : IO ProjectState := do
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
  let designs ← loadDocuments store "design revision" "design_revisions" "id"
  let works ← loadDocuments store "work" "works" "id"
  let entries ← loadDocuments store "ledger entry" "ledger_entries" "entry_order"
  let state : ProjectState :=
    { revision
      acceptedDesignId := textOption row[2]!
      focusedWorkId := textOption row[3]!
      designRevisions := designs
      works
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
        old.delegatedReviewDecisions != current.delegatedReviewDecisions then
      fail s!"transition mutated immutable Work identity {old.id}"
    if old.responsibleAgentRun != current.responsibleAgentRun then
      let recorded := (next.ledgerEntries.drop prior.ledgerEntries.length).any (fun entry =>
        entry.workId == some old.id && match entry.payload with
        | .workHandoff handoff =>
            handoff.predecessorRun == old.responsibleAgentRun &&
            handoff.successorRun == current.responsibleAgentRun
        | _ => false)
      if !recorded then fail s!"transition changed Work responsibility without a handoff {old.id}"
  for old in prior.ledgerEntries do
    match next.ledgerEntries.find? (fun value => value.id == old.id) with
    | some current =>
        if current != old then fail s!"transition mutated immutable ledger entry {old.id}"
    | none => fail s!"transition removed ledger entry {old.id}"

private def writeDesign (store : Store) (design : DesignRevision) : IO Unit :=
  AgentWorkbench.SQLite.execute store.connection
    "INSERT INTO design_revisions(id, document) VALUES (?1, ?2)
     ON CONFLICT(id) DO UPDATE SET document = excluded.document"
    #[design.id, encode design]

private def workStatus : WorkStatus → String
  | .focused => "focused"
  | .suspended => "suspended"
  | .blocked => "blocked"
  | .completed => "completed"

private def writeWork (store : Store) (work : Work) : IO Unit :=
  AgentWorkbench.SQLite.execute store.connection
    "INSERT INTO works(id, design_revision, status, scope, document)
     VALUES (?1, ?2, ?3, ?4, ?5)
     ON CONFLICT(id) DO UPDATE SET
       design_revision = excluded.design_revision,
       status = excluded.status,
       scope = excluded.scope,
       document = excluded.document"
    #[work.id, work.designRevision, workStatus work.status, work.scope, encode work]

private def insertEntry (store : Store) (entry : LedgerEntry) : IO Unit :=
  AgentWorkbench.SQLite.execute store.connection
    "INSERT INTO ledger_entries(
       id, entry_order, scope, work_id, design_revision, payload_kind, document
     ) VALUES (?1, ?2, ?3, NULLIF(?4, ''), NULLIF(?5, ''), ?6, ?7)"
    #[entry.id, toString entry.order, entry.scope, optionText entry.workId,
      optionText entry.designRevision, payloadKind entry.payload, encode entry]

private def commitOperation
    (store : Store) (operation : String)
    (expectedRevision : Nat) (next : ProjectState) : IO Unit := do
  fromExcept (validateState next)
  if next.revision != expectedRevision + 1 then
    fail s!"transition revision must advance exactly once from {expectedRevision}"
  AgentWorkbench.SQLite.transaction store.connection do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationApplicable prior operation do
      fail s!"operation is not applicable in the current state: {operation}"
    ensureAppendOnly prior next
    for design in next.designRevisions do writeDesign store design
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

private def updateOperation
    (store : Store) (operation : String)
    (transition : ProjectState → Except String ProjectState) : IO ProjectState := do
  let prior ← loadState store
  let next ← fromExcept (transition prior)
  commitOperation store operation prior.revision next
  pure next

def proposeDesignRequest
    (projectRoot : System.FilePath) (store : Store)
    (request : AgentWorkbench.DesignProposalRequest) : IO AgentWorkbench.DesignRevision := do
  let prior ← loadState store
  let (next, candidate) ← AgentWorkbench.proposeDesignRequest projectRoot prior request
  commitOperation store "design propose" prior.revision next
  pure candidate

def acceptDesignRequest
    (projectRoot : System.FilePath) (store : Store) (id : String) : IO ProjectState := do
  let prior ← loadState store
  let next ← AgentWorkbench.acceptDesignRequest projectRoot prior id
  commitOperation store "design accept" prior.revision next
  pure next

def startWorkRequest (store : Store) (request : AgentWorkbench.WorkStartRequest) : IO ProjectState :=
  updateOperation store "work start" (fun state => AgentWorkbench.startWorkRequest state request)

def suspendWork (store : Store) (workId condition : String) : IO ProjectState :=
  updateOperation store "work suspend" (fun state =>
    AgentWorkbench.suspendWork state workId condition)

def focusWork (store : Store) (workId : String) : IO ProjectState :=
  updateOperation store "work focus" (fun state => AgentWorkbench.focusWork state workId)

def resumeWork (store : Store) (workId : String) : IO ProjectState :=
  updateOperation store "work resume" (fun state => AgentWorkbench.focusWork state workId)

def adoptDesignForWork
    (store : Store) (workId entryId impact run : String) : IO ProjectState :=
  updateOperation store "work adopt-design" (fun state =>
    AgentWorkbench.adoptDesignForWork state workId entryId impact run)

def handoffWork
    (store : Store) (workId entryId successorRun reason : String) : IO ProjectState :=
  updateOperation store "work handoff" (fun state =>
    AgentWorkbench.handoffWork state workId entryId successorRun reason)

def addTask (store : Store) (request : AgentWorkbench.TaskAddRequest) : IO ProjectState :=
  updateOperation store "task add" (fun state => AgentWorkbench.addTask state request)

def closeTask (store : Store) (request : AgentWorkbench.TaskCloseRequest) : IO ProjectState :=
  updateOperation store "task close" (fun state => AgentWorkbench.closeTask state request)

def defineProfile
    (store : Store) (request : AgentWorkbench.ProfileDefineRequest) : IO ProjectState :=
  updateOperation store "profile define" (fun state => AgentWorkbench.defineProfile state request)

def replaceProfile
    (store : Store) (request : AgentWorkbench.ProfileReplaceRequest) : IO ProjectState :=
  updateOperation store "profile replace" (fun state => AgentWorkbench.replaceProfile state request)

def observeArtifact
    (projectRoot : System.FilePath) (store : Store)
    (request : AgentWorkbench.ArtifactObserveRequest) : IO ProjectState := do
  let prior ← loadState store
  let next ← AgentWorkbench.observeArtifact projectRoot prior request
  commitOperation store "artifact observe" prior.revision next
  pure next

def recordCorrection
    (store : Store) (request : AgentWorkbench.CorrectionRecordRequest) : IO ProjectState :=
  updateOperation store "correction record" (fun state =>
    AgentWorkbench.recordCorrection state request)

def supersedeCorrection
    (store : Store) (request : AgentWorkbench.CorrectionSupersedeRequest) : IO ProjectState :=
  updateOperation store "correction supersede" (fun state =>
    AgentWorkbench.supersedeCorrection state request)

def resolveCorrection
    (store : Store) (request : AgentWorkbench.CorrectionResolveRequest) : IO ProjectState :=
  updateOperation store "correction resolve" (fun state =>
    AgentWorkbench.resolveCorrection state request)

def incorporateCorrection
    (store : Store) (request : AgentWorkbench.CorrectionIncorporateRequest) : IO ProjectState :=
  updateOperation store "correction incorporate" (fun state =>
    AgentWorkbench.incorporateCorrection state request)

def recordKpt (store : Store) (request : AgentWorkbench.KptRecordRequest) : IO ProjectState :=
  updateOperation store "kpt record" (fun state => AgentWorkbench.recordKpt state request)

def applyKpt (store : Store) (request : AgentWorkbench.KptApplyRequest) : IO ProjectState :=
  updateOperation store "kpt apply" (fun state => AgentWorkbench.applyKpt state request)

def startReview
    (projectRoot : System.FilePath) (store : Store)
    (request : AgentWorkbench.ReviewStartRequest) : IO ProjectState := do
  let prior ← loadState store
  let next ← AgentWorkbench.startReview projectRoot prior request
  commitOperation store "review start" prior.revision next
  pure next

def resumeReview
    (projectRoot : System.FilePath) (store : Store)
    (request : AgentWorkbench.ReviewResumeRequest) : IO ProjectState := do
  let prior ← loadState store
  let next ← AgentWorkbench.resumeReview projectRoot prior request
  commitOperation store "review resume" prior.revision next
  pure next

def recordFinding
    (store : Store) (request : AgentWorkbench.FindingRecordRequest) : IO ProjectState :=
  updateOperation store "review finding" (fun state =>
    AgentWorkbench.recordFinding state request)

def recordDisposition
    (store : Store) (request : AgentWorkbench.DispositionRecordRequest) : IO ProjectState :=
  updateOperation store "review disposition" (fun state =>
    AgentWorkbench.recordDisposition state request)

def recordVerification
    (store : Store) (request : AgentWorkbench.VerificationRecordRequest) : IO ProjectState :=
  updateOperation store "review verify" (fun state =>
    AgentWorkbench.recordVerification state request)

def runCommandProfile
    (projectRoot : System.FilePath) (store : Store)
    (request : AgentWorkbench.CommandRunRequest) : IO AgentWorkbench.CommandRunResult := do
  let prior ← loadState store
  let (next, result) ← AgentWorkbench.runCommandProfile projectRoot prior request
  commitOperation store "command run" prior.revision next
  pure result

def runProofClaim
    (projectRoot : System.FilePath) (store : Store)
    (request : AgentWorkbench.ProofRunRequest) : IO AgentWorkbench.ProofRunResult := do
  let prior ← loadState store
  let (next, result) ← AgentWorkbench.runProofClaim projectRoot
    (AgentWorkbench.Runtime.layout projectRoot) prior request
  commitOperation store "proof run" prior.revision next
  pure result

def completeFocusedWork (projectRoot : System.FilePath) (store : Store) : IO ProjectState := do
  let prior ← loadState store
  let inputs ← AgentWorkbench.evaluateCurrentInputs projectRoot prior
  let next ← fromExcept
    (AgentWorkbench.completeFocusedWork prior inputs.observations inputs.claimDigests)
  commitOperation store "work complete" prior.revision next
  pure next

end AgentWorkbench.Store
