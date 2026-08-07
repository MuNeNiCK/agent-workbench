namespace AgentWorkbench.StoreSchema

/-- Closed inventory of columns that carry authoritative or recovery state. The executable schema
is checked against this inventory. Adding a constructor also makes the private proof ownership map
incomplete until the new column is assigned. -/
inductive PersistedColumn where
  | metadataSingleton | metadataSchemaRevision | metadataStateRevision
  | metadataAcceptedDesign | metadataFocusedWork
  | designId | designAcceptedParent | designAmendsCandidate | designStatus | designProducer
  | designRationale | designDigest | designDocument
  | designBasisDesign | designBasisOrdinal | designBasisLedgerEntry
  | designSourceDesign | designSourceOrdinal | designSourceTarget | designSourceMediaKind
  | designSourceDigest | designSourceContent
  | workId | workStatus | workScope | workOutcome | workBaselineDesign | workDesign
  | workResponsible | workResumeCondition | workMigrationDiagnostic | workDocument
  | planId | planWork | planDesign | planPredecessor | planStatus | planProducer | planReason
  | planDigest | planDocument
  | planBasisPlan | planBasisOrdinal | planBasisLedgerEntry
  | planSourcePlan | planSourceOrdinal | planSourceTarget | planSourceDigest | planSourceContent
  | ledgerId | ledgerOrder | ledgerScope | ledgerWork | ledgerDesign | ledgerPayloadKind
  | ledgerDocument
  | managedOperationId | managedExpectedRevision | managedRecoveryPolicy | managedManifest
  | managedCommittedRevision
  deriving Repr, DecidableEq

def PersistedColumn.table : PersistedColumn → String
  | .metadataSingleton | .metadataSchemaRevision | .metadataStateRevision
  | .metadataAcceptedDesign | .metadataFocusedWork => "project_metadata"
  | .designId | .designAcceptedParent | .designAmendsCandidate | .designStatus | .designProducer
  | .designRationale | .designDigest | .designDocument => "design_revisions"
  | .designBasisDesign | .designBasisOrdinal | .designBasisLedgerEntry => "design_change_bases"
  | .designSourceDesign | .designSourceOrdinal | .designSourceTarget | .designSourceMediaKind
  | .designSourceDigest | .designSourceContent => "design_sources"
  | .workId | .workStatus | .workScope | .workOutcome | .workBaselineDesign | .workDesign
  | .workResponsible | .workResumeCondition | .workMigrationDiagnostic | .workDocument => "works"
  | .planId | .planWork | .planDesign | .planPredecessor | .planStatus | .planProducer
  | .planReason | .planDigest | .planDocument => "implementation_plans"
  | .planBasisPlan | .planBasisOrdinal | .planBasisLedgerEntry =>
      "implementation_plan_change_bases"
  | .planSourcePlan | .planSourceOrdinal | .planSourceTarget | .planSourceDigest
  | .planSourceContent => "implementation_plan_sources"
  | .ledgerId | .ledgerOrder | .ledgerScope | .ledgerWork | .ledgerDesign | .ledgerPayloadKind
  | .ledgerDocument => "ledger_entries"
  | .managedOperationId | .managedExpectedRevision | .managedRecoveryPolicy | .managedManifest
  | .managedCommittedRevision => "managed_operations"

def PersistedColumn.name : PersistedColumn → String
  | .metadataSingleton => "singleton"
  | .metadataSchemaRevision => "schema_revision"
  | .metadataStateRevision => "state_revision"
  | .metadataAcceptedDesign => "accepted_design_id"
  | .metadataFocusedWork => "focused_work_id"
  | .designId => "id"
  | .designAcceptedParent => "accepted_parent_id"
  | .designAmendsCandidate => "amends_candidate_id"
  | .designStatus => "status"
  | .designProducer => "producer_run"
  | .designRationale => "change_rationale"
  | .designDigest => "revision_content_digest"
  | .designDocument => "structured_document"
  | .designBasisDesign => "design_id"
  | .designBasisOrdinal => "ordinal"
  | .designBasisLedgerEntry => "ledger_entry_id"
  | .designSourceDesign => "design_id"
  | .designSourceOrdinal => "ordinal"
  | .designSourceTarget => "target"
  | .designSourceMediaKind => "media_kind"
  | .designSourceDigest => "digest"
  | .designSourceContent => "content"
  | .workId => "id"
  | .workStatus => "status"
  | .workScope => "scope"
  | .workOutcome => "outcome"
  | .workBaselineDesign => "baseline_design_id"
  | .workDesign => "design_revision_id"
  | .workResponsible => "responsible_run"
  | .workResumeCondition => "resume_condition"
  | .workMigrationDiagnostic => "migration_diagnostic"
  | .workDocument => "document"
  | .planId => "id"
  | .planWork => "work_id"
  | .planDesign => "design_revision_id"
  | .planPredecessor => "predecessor_plan_id"
  | .planStatus => "status"
  | .planProducer => "producer_run"
  | .planReason => "reason"
  | .planDigest => "content_digest"
  | .planDocument => "document"
  | .planBasisPlan => "plan_id"
  | .planBasisOrdinal => "ordinal"
  | .planBasisLedgerEntry => "ledger_entry_id"
  | .planSourcePlan => "plan_id"
  | .planSourceOrdinal => "ordinal"
  | .planSourceTarget => "target"
  | .planSourceDigest => "digest"
  | .planSourceContent => "content"
  | .ledgerId => "id"
  | .ledgerOrder => "entry_order"
  | .ledgerScope => "scope"
  | .ledgerWork => "work_id"
  | .ledgerDesign => "design_revision"
  | .ledgerPayloadKind => "payload_kind"
  | .ledgerDocument => "document"
  | .managedOperationId => "operation_id"
  | .managedExpectedRevision => "expected_state_revision"
  | .managedRecoveryPolicy => "recovery_policy"
  | .managedManifest => "manifest"
  | .managedCommittedRevision => "committed_state_revision"

def PersistedColumn.all : List PersistedColumn :=
  [.metadataSingleton, .metadataSchemaRevision, .metadataStateRevision,
   .metadataAcceptedDesign, .metadataFocusedWork,
   .designId, .designAcceptedParent, .designAmendsCandidate, .designStatus, .designProducer,
   .designRationale, .designDigest, .designDocument,
   .designBasisDesign, .designBasisOrdinal, .designBasisLedgerEntry,
   .designSourceDesign, .designSourceOrdinal, .designSourceTarget, .designSourceMediaKind,
   .designSourceDigest, .designSourceContent,
   .workId, .workStatus, .workScope, .workOutcome, .workBaselineDesign, .workDesign,
   .workResponsible, .workResumeCondition, .workMigrationDiagnostic, .workDocument,
   .planId, .planWork, .planDesign, .planPredecessor, .planStatus, .planProducer, .planReason,
   .planDigest, .planDocument,
   .planBasisPlan, .planBasisOrdinal, .planBasisLedgerEntry,
   .planSourcePlan, .planSourceOrdinal, .planSourceTarget, .planSourceDigest, .planSourceContent,
   .ledgerId, .ledgerOrder, .ledgerScope, .ledgerWork, .ledgerDesign, .ledgerPayloadKind,
   .ledgerDocument,
   .managedOperationId, .managedExpectedRevision, .managedRecoveryPolicy, .managedManifest,
   .managedCommittedRevision]

def persistedTableNames : List String :=
  PersistedColumn.all.map PersistedColumn.table |>.eraseDups

def persistedColumnNames (table : String) : List String :=
  PersistedColumn.all.filter (PersistedColumn.table · == table) |>.map PersistedColumn.name

end AgentWorkbench.StoreSchema
