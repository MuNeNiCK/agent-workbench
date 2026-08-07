import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource

namespace AgentWorkbenchTest.Migration

open AgentWorkbench AgentWorkbenchTest

private def createV1Database (path : System.FilePath) (focused : Bool := true) : IO Unit := do
  let connection ← AgentWorkbench.SQLite.open path
  AgentWorkbench.SQLite.runScript connection "
    PRAGMA foreign_keys = ON;
    CREATE TABLE project_metadata(
      singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
      schema_revision INTEGER NOT NULL,
      state_revision INTEGER NOT NULL,
      accepted_design_id TEXT,
      focused_work_id TEXT
    ) STRICT;
    CREATE TABLE design_revisions(id TEXT PRIMARY KEY, document TEXT NOT NULL) STRICT;
    CREATE TABLE works(
      id TEXT PRIMARY KEY,
      design_revision TEXT NOT NULL,
      status TEXT NOT NULL,
      scope TEXT NOT NULL,
      document TEXT NOT NULL
    ) STRICT;
    CREATE INDEX works_by_design ON works(design_revision);
    CREATE INDEX works_by_scope_status ON works(scope, status);
    CREATE TABLE ledger_entries(
      id TEXT PRIMARY KEY,
      entry_order INTEGER NOT NULL UNIQUE,
      scope TEXT NOT NULL,
      work_id TEXT,
      design_revision TEXT,
      payload_kind TEXT NOT NULL,
      document TEXT NOT NULL
    ) STRICT;
    CREATE INDEX ledger_by_context ON ledger_entries(scope, work_id, design_revision, entry_order);
    CREATE INDEX ledger_by_kind ON ledger_entries(payload_kind, entry_order);
    INSERT INTO project_metadata VALUES (1, 1, 7, 'design-v1', 'work-v1');"
  let legacyClaimInput : ClaimInput := {
    statementId := "statement-v1", statementText := "retain legacy work"
    mapping := "legacy Design-time mapping", proposition := "Legacy.Workflow"
    witness := "Legacy.workflow", assumptions := ["propext"]
    proofRoot := ".agent-workbench/self-application"
    declaredSources := [{ path := "Legacy.lean" }]
    check := { executable := "lake", arguments := #["build"] }
    toolchain := ProofToolchain.identifier }
  let designDocument :=
    "{\"id\":\"design-v1\",\"parent\":null,\"createdAfterEntryOrder\":0," ++
    "\"status\":\"accepted\",\"producerAgentRun\":\"agent-old\"," ++
    "\"sourceDocuments\":[{\"target\":\"file:legacy-design.md\",\"snapshot\":\"blake3:legacy-source\"}]," ++
    "\"statements\":[{\"id\":\"statement-v1\",\"text\":\"retain legacy work\",\"assumptions\":[]}]," ++
    "\"acceptanceCriteria\":[{\"id\":\"criterion-v1\",\"statementId\":null," ++
    "\"statement\":\"retain legacy task\",\"target\":\"file:legacy\",\"evidenceKind\":\"artifact\"}]," ++
    "\"leanClaims\":[{\"id\":\"claim-v1\",\"input\":" ++
      (Lean.toJson legacyClaimInput).compress ++ "}]}"
  let workDocument :=
    "{\"id\":\"work-v1\",\"outcome\":\"continue the retained outcome\"," ++
    "\"scope\":\"project\",\"designRevision\":\"design-v1\",\"status\":\"focused\"," ++
    "\"responsibleAgentRun\":\"agent-old\",\"delegatedReviewDecisions\":[]," ++
    "\"resumeCondition\":null}"
  AgentWorkbench.SQLite.execute connection
    "INSERT INTO design_revisions(id, document) VALUES (?1, ?2)"
    #["design-v1", designDocument]
  AgentWorkbench.SQLite.execute connection
    "INSERT INTO works(id, design_revision, status, scope, document)
     VALUES (?1, ?2, ?3, ?4, ?5)"
    #["work-v1", "design-v1", "focused", "project", workDocument]
  let reviewEntry : LedgerEntry := {
    id := "review-v1", order := 1, scope := "project"
    workId := some "work-v1", designRevision := some "design-v1"
    payload := .review {
      reviewId := "review-v1", purpose := .design, context := .fresh
      targetSourceId := "design-v1", target := "design:design-v1"
      targetSnapshot := "legacy-design-snapshot", producerAgentRuns := ["agent-old"]
      reviewerAgentRun := "reviewer-old" } }
  let currentReviewDocument := (Lean.toJson reviewEntry).compress
  let legacyReviewDocument := currentReviewDocument.replace
    "\"producerAgentRuns\":[\"agent-old\"]" "\"producerAgentRun\":\"agent-old\""
  if legacyReviewDocument == currentReviewDocument then
    throw (IO.userError "test fixture did not produce a legacy Review document")
  AgentWorkbench.SQLite.execute connection
    "INSERT INTO ledger_entries(
       id, entry_order, scope, work_id, design_revision, payload_kind, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    #["review-v1", "1", "project", "work-v1", "design-v1", "review", legacyReviewDocument]
  let taskEntry : LedgerEntry := {
    id := "task-v1", order := 2, scope := "project"
    workId := some "work-v1", designRevision := some "design-v1"
    payload := .task {
      criterionId := some "criterion-v1", description := "retained legacy task"
      required := true, closed := false } }
  AgentWorkbench.SQLite.execute connection
    "INSERT INTO ledger_entries(
       id, entry_order, scope, work_id, design_revision, payload_kind, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    #[taskEntry.id, "2", taskEntry.scope, "work-v1", "design-v1", "task",
      (Lean.toJson taskEntry).compress]
  let receiptEntry : LedgerEntry := {
    id := "proof-v1", order := 3, scope := "project"
    workId := some "work-v1", designRevision := some "design-v1"
    payload := .leanProofReceipt {
      claimId := "claim-v1", claimInput := legacyClaimInput
      inputDigest := "blake3:legacy-input"
      sourceDigests := [{ path := "Legacy.lean", digest := "blake3:legacy-source" }]
      toolchain := ProofToolchain.identifier, exitCode := 0
      outputDigest := "blake3:legacy-output", kernelAccepted := true } }
  let currentReceiptDocument := (Lean.toJson receiptEntry).compress
  let legacyReceiptDocument := currentReceiptDocument
    |>.replace "\"elaboratedPropositionDigest\":\"\"," ""
    |>.replace "\"propositionDependencies\":[]," ""
    |>.replace "\"assumptionDependencies\":[]," ""
  if legacyReceiptDocument == currentReceiptDocument then
    throw (IO.userError "test fixture did not produce a v0.2.7 proof receipt")
  AgentWorkbench.SQLite.execute connection
    "INSERT INTO ledger_entries(
       id, entry_order, scope, work_id, design_revision, payload_kind, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    #[receiptEntry.id, "3", receiptEntry.scope, "work-v1", "design-v1",
      "lean-proof-receipt", legacyReceiptDocument]
  if !focused then
    AgentWorkbench.SQLite.execute connection
      "UPDATE project_metadata SET focused_work_id = NULL WHERE singleton = 1" #[]
    AgentWorkbench.SQLite.execute connection
      "UPDATE works SET status = 'active', document = replace(document,
       '\"status\":\"focused\"', '\"status\":\"active\"') WHERE id = 'work-v1'" #[]

def run : IO Unit := do
  IO.FS.withTempDir fun root => do
    let database := root / "state.db"
    createV1Database database
    let store ← Store.open database
    let migrated ← Store.loadState store
    let design ← match migrated.design? "design-v1" with
      | some value => pure value
      | none => throw (IO.userError "migration lost the accepted Design")
    let work ← match migrated.work? "work-v1" with
      | some value => pure value
      | none => throw (IO.userError "migration lost the focused Work")
    expect (migrated.revision == 7 && migrated.acceptedDesignId == some design.id &&
      migrated.focusedWorkId == some work.id)
      "migration changed the authoritative revision or current selectors"
    expect (!design.sourceArchiveAvailable && design.changeRationale == "legacy source unavailable")
      "migration fabricated archived Design source authority"
    expect (design.sourceDocuments ==
      [({ target := "file:legacy-design.md", snapshot := "blake3:legacy-source" } : DesignSource)])
      "migration could not decode the exact v0.2.7 Design source shape"
    expect (work.status == .active && work.baselineDesignRevision.isNone &&
      work.designRevision == some design.id && work.outcome == "continue the retained outcome")
      "migration changed retained Work identity or failed to map its lifecycle"
    expect (migrated.entry? "review-v1" |>.any fun entry => match entry.payload with
      | .review value => value.producerAgentRuns == ["agent-old"]
      | _ => false)
      "migration did not convert legacy Review producer provenance"
    expect (migrated.entry? "task-v1" |>.any fun entry => match entry.payload with
      | .task value => value.planId.isNone && value.criterionId == some "criterion-v1" &&
          value.required && !value.closed
      | _ => false)
      "migration did not retain the open v0.2.7 Task as legacy history"
    expect (migrated.entry? "proof-v1" |>.any fun entry => match entry.payload with
      | .leanProofReceipt value => value.claimId == "claim-v1" &&
          value.elaboratedPropositionDigest.isEmpty && value.propositionDependencies.isEmpty &&
          value.assumptionDependencies.isEmpty
      | _ => false)
      "migration could not retain a v0.2.7 proof receipt as stale history"
    let connection ← AgentWorkbench.SQLite.openReadOnly database
    expect ((← AgentWorkbench.SQLite.queryScalar connection
      "SELECT CAST(schema_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]) == "2")
      "migration did not advance the schema revision atomically"
    let reopened ← Store.loadState (← Store.openReadOnly database)
    expect (reopened == migrated) "migrated state did not survive a read-only reopen"

    -- A retained v0.2.7 Work moves forward through a strict successor. Its old manual Tasks stay
    -- readable history but do not contaminate the v0.2.8 Plan-derived Task graph.
    let privateRoot := root / ".agent-workbench" / "design"
    let designRoot := privateRoot / "product"
    IO.FS.createDirAll designRoot
    IO.FS.createDirAll (privateRoot / "implementation")
    let designTarget := "file:.agent-workbench/design/product/successor.md"
    IO.FS.writeFile (designRoot / "successor.md") "retain legacy work\n"
    let designUnits := (← AgentWorkbench.DesignSource.inspectAll root [designTarget]).flatMap (·.units)
    let statement : Statement := { id := "statement-v1", text := "retain legacy work" }
    let criterion : AcceptanceCriterion := {
      id := "criterion-v1", statementId := some statement.id
      statement := "retain legacy task", target := "file:legacy", evidenceKind := "artifact" }
    let proposed ← Store.executeMutation root database (.designPropose {
      producerAgentRun := "agent-old", changeRationale := "adopt the complete v0.2.8 source graph"
      sourceDocumentTargets := [designTarget]
      sourceUnitDispositions := designUnits.map fun unit =>
        { unitId := unit.id, role := .requirement }
      statements := [statement]
      statementCoverage := [{
        statementId := statement.id, sourceUnitIds := designUnits.map (·.id)
        leanClaims := { noSelectionReason := some "no logical Claim is selected" }
        acceptanceCriteria := { selectedIds := [criterion.id] }
        implementationRequired := true }]
      acceptanceCriteria := [criterion] })
    let successor ← match proposed with
      | .design value => pure value
      | _ => throw (IO.userError "successor Design proposal returned the wrong result")
    let _ ← Store.executeMutation root database
      (.workSuspend "work-v1" "adopt the v0.2.8 successor")
    let _ ← Store.executeMutation root database (.designAccept successor.id)
    let _ ← Store.executeMutation root database (.workAdoptDesign {
      workId := "work-v1", entryId := "adoption-v2", agentRun := "agent-old" })
    let _ ← Store.executeMutation root database (.workResume {
      workId := "work-v1", entryId := "resume-v2"
      satisfaction := "the accepted successor was adopted"
      basisEntryIds := ["adoption-v2"], agentRun := "agent-old" })

    let planRoot := privateRoot / "plans" / "work-v1"
    IO.FS.createDirAll planRoot
    let planTarget := "file:.agent-workbench/design/plans/work-v1/plan.md"
    IO.FS.writeFile (planRoot / "plan.md") "implement retained work\n"
    let planUnits := (← AgentWorkbench.PlanSource.inspectAll root "work-v1" [planTarget]).flatMap (·.units)
    let step : PlanStep := {
      id := "step-v2", description := "implement retained work"
      outputScopes := [criterion.target], verificationCriterionIds := [criterion.id] }
    let planResult ← Store.executeMutation root database (.planPropose {
      producerAgentRun := "agent-old", reason := "materialize the successor delta"
      sourceDocumentTargets := [planTarget]
      sourceUnitDispositions := planUnits.map fun unit =>
        { unitId := unit.id, stepId := some step.id }
      statementDispositions := [{
        statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := [step.id] }]
      steps := [step] })
    let plan ← match planResult with
      | .plan value => pure value
      | _ => throw (IO.userError "successor Plan proposal returned the wrong result")
    let _ ← Store.executeMutation root database (.planMaterialize plan.id)
    let advanced ← Store.loadState (← Store.openReadOnly database)
    expect (advanced.currentPlanFor? "work-v1" |>.any (·.id == plan.id))
      "migrated Work did not obtain its successor Plan"
    expect (advanced.ledgerEntries.any fun entry => entry.id == "task-v1")
      "successor materialization erased legacy Task history"
    expect (advanced.ledgerEntries.countP (fun entry => match entry.payload with
      | .task task => task.planId == some plan.id && !task.retired
      | _ => false) == plan.steps.length)
      "legacy Tasks contaminated the successor Plan-derived Task graph"

  -- v0.2.7 completed status predates immutable WorkCompletion records. Migration retains it only
  -- when its bound Design is explicitly marked source-unavailable; new Designs cannot use this
  -- compatibility boundary to bypass completion authority.
  IO.FS.withTempDir fun root => do
    let database := root / "state.db"
    createV1Database database
    let connection ← AgentWorkbench.SQLite.open database
    AgentWorkbench.SQLite.execute connection
      "UPDATE project_metadata SET focused_work_id = NULL WHERE singleton = 1" #[]
    AgentWorkbench.SQLite.execute connection
      "UPDATE works SET status = 'completed', document = replace(document,
       '\"status\":\"focused\"', '\"status\":\"completed\"') WHERE id = 'work-v1'" #[]
    let migrated ← Store.loadState (← Store.open database)
    expect ((migrated.work? "work-v1").any (·.status == .completed) &&
      migrated.ledgerEntries.all fun entry => match entry.payload with
        | .workCompletion _ => false
        | _ => true)
      "migration rejected or fabricated authority for a v0.2.7 completed Work"

  -- A legacy blocked status cannot be translated silently. The retained Work
  -- exposes an explicit recovery diagnostic before any later resume attempt.
  IO.FS.withTempDir fun root => do
    let database := root / "state.db"
    createV1Database database
    let connection ← AgentWorkbench.SQLite.open database
    AgentWorkbench.SQLite.execute connection
      "UPDATE project_metadata SET focused_work_id = NULL WHERE singleton = 1" #[]
    AgentWorkbench.SQLite.execute connection
      "UPDATE works SET status = 'blocked', document = replace(document,
       '\"status\":\"focused\"', '\"status\":\"blocked\"') WHERE id = 'work-v1'" #[]
    let migrated ← Store.loadState (← Store.open database)
    expect ((migrated.work? "work-v1").any fun value =>
      value.status == .suspended && value.migrationDiagnostic.isSome)
      "blocked migration changed status without an explicit recovery diagnostic"

  -- Init is the upgrade transition used by setup when a read-only context reports schema revision
  -- 1. The migrated Store owns that exceptional applicability and still advances semantic state
  -- exactly once.
  IO.FS.withTempDir fun root => do
    let database := root / "state.db"
    createV1Database database
    let store ← Store.open database
    expect (Store.wasMigratedFromLegacy store)
      "v0.2.7 schema migration was not exposed to the init transition"
    let prior ← Store.loadState store
    let next ← match (PreparedMutation.direct .init).execute prior with
      | .ok value => pure value
      | .error message => throw (IO.userError message)
    Store.commitOperation store .init prior.revision next
    let migrated ← Store.loadState (← Store.openReadOnly database)
    expect (migrated.revision == 8 && migrated.acceptedDesignId == some "design-v1" &&
      migrated.focusedWorkId == some "work-v1")
      "native init did not preserve selectors or advance state exactly once during migration"

  -- `work focus` has a real public positive route for a v0.2.7 active Work whose selector was
  -- intentionally absent. This is the migration boundary that can produce active/unfocused state.
  IO.FS.withTempDir fun root => do
    let workbenchRoot := root / ".agent-workbench"
    IO.FS.createDirAll workbenchRoot
    let database := workbenchRoot / "state.db"
    createV1Database database false
    let executable := if System.Platform.isWindows then
        ".lake/build/bin/agent-workbench.exe"
      else ".lake/build/bin/agent-workbench"
    let output ← IO.Process.output {
      cmd := executable
      args := #["--project", root.toString, "work", "focus"] }
      (some "{\"id\":\"work-v1\"}")
    expect (output.exitCode == 0)
      s!"public work focus route did not migrate and focus active Work: {output.stderr}"
    let focused ← Store.loadState (← Store.openReadOnly database)
    expect (focused.focusedWorkId == some "work-v1" &&
      (focused.work? "work-v1").any (·.status == .active))
      "public work focus route did not retain the migrated Work identity"

end AgentWorkbenchTest.Migration
