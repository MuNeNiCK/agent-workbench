import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.StoreSchemaInventory

namespace AgentWorkbench.StoreSchema

inductive OpenResult where
  | created
  | migrated
  | current
  deriving Repr, DecidableEq

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def createSchemaV2 (connection : AgentWorkbench.SQLite.Connection) : IO Unit :=
  AgentWorkbench.SQLite.runScript connection "
    CREATE TABLE IF NOT EXISTS project_metadata(
      singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
      schema_revision INTEGER NOT NULL,
      state_revision INTEGER NOT NULL,
      accepted_design_id TEXT,
      focused_work_id TEXT
    ) STRICT;
    CREATE TABLE IF NOT EXISTS design_revisions(
      id TEXT PRIMARY KEY,
      accepted_parent_id TEXT,
      amends_candidate_id TEXT,
      status TEXT NOT NULL,
      producer_run TEXT NOT NULL,
      change_rationale TEXT NOT NULL,
      revision_content_digest TEXT NOT NULL,
      structured_document TEXT NOT NULL,
      FOREIGN KEY(accepted_parent_id) REFERENCES design_revisions(id),
      FOREIGN KEY(amends_candidate_id) REFERENCES design_revisions(id)
    ) STRICT;
    CREATE TABLE IF NOT EXISTS design_change_bases(
      design_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      ledger_entry_id TEXT NOT NULL,
      PRIMARY KEY(design_id, ordinal),
      UNIQUE(design_id, ledger_entry_id),
      FOREIGN KEY(design_id) REFERENCES design_revisions(id),
      FOREIGN KEY(ledger_entry_id) REFERENCES ledger_entries(id)
    ) STRICT;
    CREATE TABLE IF NOT EXISTS design_sources(
      design_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      target TEXT NOT NULL,
      media_kind TEXT NOT NULL,
      digest TEXT NOT NULL,
      content BLOB NOT NULL,
      PRIMARY KEY(design_id, target),
      UNIQUE(design_id, ordinal),
      FOREIGN KEY(design_id) REFERENCES design_revisions(id)
    ) STRICT;
    CREATE TABLE IF NOT EXISTS works(
      id TEXT PRIMARY KEY,
      status TEXT NOT NULL,
      scope TEXT NOT NULL,
      outcome TEXT NOT NULL,
      baseline_design_id TEXT,
      design_revision_id TEXT,
      responsible_run TEXT NOT NULL,
      resume_condition TEXT,
      document TEXT NOT NULL
    ) STRICT;
    CREATE INDEX IF NOT EXISTS works_by_design ON works(design_revision_id);
    CREATE INDEX IF NOT EXISTS works_by_scope_status ON works(scope, status);
    CREATE TABLE IF NOT EXISTS implementation_plans(
      id TEXT PRIMARY KEY,
      work_id TEXT NOT NULL,
      design_revision_id TEXT NOT NULL,
      predecessor_plan_id TEXT,
      status TEXT NOT NULL,
      producer_run TEXT NOT NULL,
      reason TEXT NOT NULL,
      content_digest TEXT NOT NULL,
      document TEXT NOT NULL,
      FOREIGN KEY(work_id) REFERENCES works(id),
      FOREIGN KEY(design_revision_id) REFERENCES design_revisions(id),
      FOREIGN KEY(predecessor_plan_id) REFERENCES implementation_plans(id)
    ) STRICT;
    CREATE UNIQUE INDEX IF NOT EXISTS one_current_plan_per_work
      ON implementation_plans(work_id) WHERE status = 'current';
    CREATE TABLE IF NOT EXISTS implementation_plan_change_bases(
      plan_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      ledger_entry_id TEXT NOT NULL,
      PRIMARY KEY(plan_id, ordinal),
      UNIQUE(plan_id, ledger_entry_id),
      FOREIGN KEY(plan_id) REFERENCES implementation_plans(id),
      FOREIGN KEY(ledger_entry_id) REFERENCES ledger_entries(id)
    ) STRICT;
    CREATE TABLE IF NOT EXISTS implementation_plan_sources(
      plan_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      target TEXT NOT NULL,
      digest TEXT NOT NULL,
      content BLOB NOT NULL,
      PRIMARY KEY(plan_id, target),
      UNIQUE(plan_id, ordinal),
      FOREIGN KEY(plan_id) REFERENCES implementation_plans(id)
    ) STRICT;
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
    CREATE TABLE IF NOT EXISTS managed_operations(
      operation_id TEXT PRIMARY KEY,
      expected_state_revision INTEGER NOT NULL,
      recovery_policy TEXT NOT NULL,
      manifest TEXT NOT NULL,
      committed_state_revision INTEGER
    ) STRICT;
    INSERT OR IGNORE INTO project_metadata(
      singleton, schema_revision, state_revision, accepted_design_id, focused_work_id
    ) VALUES (1, 2, 0, NULL, NULL);"

private def migrateV1ToV2 (connection : AgentWorkbench.SQLite.Connection) : IO Unit :=
  AgentWorkbench.SQLite.runScript connection "
    ALTER TABLE design_revisions RENAME TO design_revisions_v1;
    CREATE TABLE design_revisions(
      id TEXT PRIMARY KEY,
      accepted_parent_id TEXT,
      amends_candidate_id TEXT,
      status TEXT NOT NULL,
      producer_run TEXT NOT NULL,
      change_rationale TEXT NOT NULL,
      revision_content_digest TEXT NOT NULL,
      structured_document TEXT NOT NULL,
      FOREIGN KEY(accepted_parent_id) REFERENCES design_revisions(id),
      FOREIGN KEY(amends_candidate_id) REFERENCES design_revisions(id)
    ) STRICT;
    INSERT INTO design_revisions(
      id, accepted_parent_id, amends_candidate_id, status, producer_run,
      change_rationale, revision_content_digest, structured_document
    ) SELECT id,
      json_extract(document, '$.parent'),
      NULL,
      json_extract(document, '$.status'),
      json_extract(document, '$.producerAgentRun'),
      'legacy source unavailable',
      '',
      document
    FROM design_revisions_v1;
    DROP TABLE design_revisions_v1;
    CREATE TABLE design_change_bases(
      design_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      ledger_entry_id TEXT NOT NULL,
      PRIMARY KEY(design_id, ordinal),
      UNIQUE(design_id, ledger_entry_id),
      FOREIGN KEY(design_id) REFERENCES design_revisions(id),
      FOREIGN KEY(ledger_entry_id) REFERENCES ledger_entries(id)
    ) STRICT;
    DROP INDEX IF EXISTS works_by_design;
    DROP INDEX IF EXISTS works_by_scope_status;
    ALTER TABLE works RENAME TO works_v1;
    CREATE TABLE works(
      id TEXT PRIMARY KEY,
      status TEXT NOT NULL,
      scope TEXT NOT NULL,
      outcome TEXT NOT NULL,
      baseline_design_id TEXT,
      design_revision_id TEXT,
      responsible_run TEXT NOT NULL,
      resume_condition TEXT,
      document TEXT NOT NULL
    ) STRICT;
    INSERT INTO works(
      id, status, scope, outcome, baseline_design_id, design_revision_id,
      responsible_run, resume_condition, document
    )
      SELECT id,
        CASE status WHEN 'focused' THEN 'active' WHEN 'blocked' THEN 'suspended' ELSE status END,
        scope,
        json_extract(document, '$.outcome'),
        NULL,
        design_revision,
        json_extract(document, '$.responsibleAgentRun'),
        NULLIF(json_extract(document, '$.resumeCondition'), ''),
        document FROM works_v1;
    DROP TABLE works_v1;
    CREATE INDEX works_by_design ON works(design_revision_id);
    CREATE INDEX works_by_scope_status ON works(scope, status);
    CREATE TABLE design_sources(
      design_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      target TEXT NOT NULL,
      media_kind TEXT NOT NULL,
      digest TEXT NOT NULL,
      content BLOB NOT NULL,
      PRIMARY KEY(design_id, target),
      UNIQUE(design_id, ordinal),
      FOREIGN KEY(design_id) REFERENCES design_revisions(id)
    ) STRICT;
    CREATE TABLE managed_operations(
      operation_id TEXT PRIMARY KEY,
      expected_state_revision INTEGER NOT NULL,
      recovery_policy TEXT NOT NULL,
      manifest TEXT NOT NULL,
      committed_state_revision INTEGER
    ) STRICT;
    CREATE TABLE implementation_plans(
      id TEXT PRIMARY KEY,
      work_id TEXT NOT NULL,
      design_revision_id TEXT NOT NULL,
      predecessor_plan_id TEXT,
      status TEXT NOT NULL,
      producer_run TEXT NOT NULL,
      reason TEXT NOT NULL,
      content_digest TEXT NOT NULL,
      document TEXT NOT NULL,
      FOREIGN KEY(work_id) REFERENCES works(id),
      FOREIGN KEY(design_revision_id) REFERENCES design_revisions(id),
      FOREIGN KEY(predecessor_plan_id) REFERENCES implementation_plans(id)
    ) STRICT;
    CREATE UNIQUE INDEX one_current_plan_per_work
      ON implementation_plans(work_id) WHERE status = 'current';
    CREATE TABLE implementation_plan_change_bases(
      plan_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      ledger_entry_id TEXT NOT NULL,
      PRIMARY KEY(plan_id, ordinal),
      UNIQUE(plan_id, ledger_entry_id),
      FOREIGN KEY(plan_id) REFERENCES implementation_plans(id),
      FOREIGN KEY(ledger_entry_id) REFERENCES ledger_entries(id)
    ) STRICT;
    CREATE TABLE implementation_plan_sources(
      plan_id TEXT NOT NULL,
      ordinal INTEGER NOT NULL,
      target TEXT NOT NULL,
      digest TEXT NOT NULL,
      content BLOB NOT NULL,
      PRIMARY KEY(plan_id, target),
      UNIQUE(plan_id, ordinal),
      FOREIGN KEY(plan_id) REFERENCES implementation_plans(id)
    ) STRICT;
    UPDATE project_metadata SET schema_revision = 2 WHERE singleton = 1;"

def initializeStoreSchema (connection : AgentWorkbench.SQLite.Connection) : IO OpenResult := do
  AgentWorkbench.SQLite.runScript connection "PRAGMA foreign_keys = ON;"
  let metadataTables ← AgentWorkbench.SQLite.queryScalar connection
    "SELECT CAST(COUNT(*) AS TEXT) FROM sqlite_master
     WHERE type = 'table' AND name = 'project_metadata'" #[]
  if metadataTables == "0" then
    AgentWorkbench.SQLite.immediateTransaction connection do
      createSchemaV2 connection
    pure .created
  else
    let stored ← AgentWorkbench.SQLite.queryScalar connection
      "SELECT CAST(schema_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
    if stored == "1" then
      AgentWorkbench.SQLite.immediateTransaction connection do
        migrateV1ToV2 connection
      pure .migrated
    else if stored != "2" then
      fail s!"unsupported schema revision {stored}; expected 1 or 2"
    else pure .current


end AgentWorkbench.StoreSchema
