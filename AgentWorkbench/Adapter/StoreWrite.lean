import AgentWorkbench.Adapter.StoreRead
import AgentWorkbench.Adapter.StoreSchema
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource
import AgentWorkbench.Decision.Operation

namespace AgentWorkbench.Store

structure WriteStore where private mk ::
  private connection : AgentWorkbench.SQLite.Connection
  migratedFromLegacy : Bool := false

instance : ReadableStore WriteStore where
  connection store := store.connection

/-- Write capability is defined only in the mutation-side Store module. Query
roots import `StoreRead` and cannot name this accessor or the write opener. -/
def writeConnection (store : WriteStore) : AgentWorkbench.SQLite.Connection :=
  store.connection

def «open» (path : System.FilePath) : IO WriteStore := do
  let connection ← AgentWorkbench.SQLite.open path
  let schemaResult ← AgentWorkbench.StoreSchema.initializeStoreSchema connection
  pure { connection, migratedFromLegacy := schemaResult == .migrated }

def wasMigratedFromLegacy (store : WriteStore) : Bool :=
  store.migratedFromLegacy

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => fail message

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
  AgentWorkbench.SQLite.executeValues (writeConnection store)
    "INSERT INTO design_revisions(
       id, accepted_parent_id, amends_candidate_id, status, producer_run,
       change_rationale, revision_content_digest, structured_document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    #[.text design.id,
      design.parent.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      design.amendsCandidate.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text (Codec.designStatusName design.status),
      .text design.producerAgentRun, .text design.changeRationale,
      .text design.revisionContentDigest, .text (Codec.encode design)]
  for (basis, ordinal) in design.changeBasisEntryIds.zipIdx do
    AgentWorkbench.SQLite.execute (writeConnection store)
      "INSERT INTO design_change_bases(design_id, ordinal, ledger_entry_id)
       VALUES (?1, ?2, ?3)" #[design.id, toString ordinal, basis]

private def updateDesignStatus (store : WriteStore) (design : DesignRevision) : IO Unit := do
  AgentWorkbench.SQLite.execute (writeConnection store)
    "UPDATE design_revisions SET status = ?1, structured_document = ?2 WHERE id = ?3"
    #[Codec.designStatusName design.status, Codec.encode design, design.id]
  if (← AgentWorkbench.SQLite.changes (writeConnection store)) != 1 then
    fail s!"Design status update did not target exactly one row: {design.id}"

private def persistDesignChanges
    (store : WriteStore) (prior next : ProjectState) : IO Unit := do
  for design in next.designRevisions do
    match prior.designRevisions.find? (fun old => old.id == design.id) with
    | none => insertDesign store design
    | some old =>
        if old.status != design.status then updateDesignStatus store design

private def insertPlan (store : WriteStore) (plan : ImplementationPlan) : IO Unit := do
  AgentWorkbench.SQLite.executeValues (writeConnection store)
    "INSERT INTO implementation_plans(
       id, work_id, design_revision_id, predecessor_plan_id, status,
       producer_run, reason, content_digest, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    #[.text plan.id, .text plan.workId, .text plan.designRevision,
      plan.predecessorPlanId.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text (Codec.planStatusName plan.status), .text plan.producerAgentRun,
      .text plan.reason, .text plan.contentDigest, .text (Codec.encode plan)]
  for (basis, ordinal) in plan.changeBasisEntryIds.zipIdx do
    AgentWorkbench.SQLite.execute (writeConnection store)
      "INSERT INTO implementation_plan_change_bases(plan_id, ordinal, ledger_entry_id)
       VALUES (?1, ?2, ?3)" #[plan.id, toString ordinal, basis]

private def updatePlanStatus (store : WriteStore) (plan : ImplementationPlan) : IO Unit := do
  AgentWorkbench.SQLite.execute (writeConnection store)
    "UPDATE implementation_plans SET status = ?1, document = ?2 WHERE id = ?3"
    #[Codec.planStatusName plan.status, Codec.encode plan, plan.id]
  if (← AgentWorkbench.SQLite.changes (writeConnection store)) != 1 then
    fail s!"Plan status update did not target exactly one row: {plan.id}"

private def persistPlanChanges
    (store : WriteStore) (prior next : ProjectState) : IO Unit := do
  -- Release the partial unique index before promoting a successor. StoreRead
  -- orders IDs lexicographically, so plan-10 may otherwise be visited before
  -- plan-9 and collide while both rows are momentarily current.
  for plan in next.implementationPlans do
    match prior.implementationPlans.find? (fun old => old.id == plan.id) with
    | some old =>
        if old.status == .current && plan.status != .current then updatePlanStatus store plan
    | none => pure ()
  for plan in next.implementationPlans do
    match prior.implementationPlans.find? (fun old => old.id == plan.id) with
    | none => insertPlan store plan
    | some old =>
        if old.status != plan.status && old.status != .current then updatePlanStatus store plan

private def insertPlanSource
    (store : WriteStore) (planId : String) (ordinal : Nat)
    (source : AgentWorkbench.PlanSource.Captured) : IO Unit :=
  AgentWorkbench.SQLite.executeValues (writeConnection store)
    "INSERT INTO implementation_plan_sources(plan_id, ordinal, target, digest, content)
     VALUES (?1, ?2, ?3, ?4, ?5)"
    #[.text planId, .text (toString ordinal), .text source.target,
      .text source.digest, .blob source.content]

private def insertDesignSource
    (store : WriteStore) (designId : String) (ordinal : Nat)
    (source : AgentWorkbench.DesignSource.Captured) : IO Unit :=
  AgentWorkbench.SQLite.executeValues (writeConnection store)
    "INSERT INTO design_sources(design_id, ordinal, target, media_kind, digest, content)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    #[.text designId, .text (toString ordinal), .text source.target,
      .text source.mediaKind, .text source.digest, .blob source.content]

private def writeWork (store : WriteStore) (work : Work) : IO Unit :=
  AgentWorkbench.SQLite.executeValues (writeConnection store)
    "INSERT INTO works(
       id, status, scope, outcome, baseline_design_id, design_revision_id,
       responsible_run, resume_condition, migration_diagnostic, document
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
     ON CONFLICT(id) DO UPDATE SET
       status = excluded.status,
       scope = excluded.scope,
       outcome = excluded.outcome,
       design_revision_id = excluded.design_revision_id,
       responsible_run = excluded.responsible_run,
       resume_condition = excluded.resume_condition,
       migration_diagnostic = excluded.migration_diagnostic,
       document = excluded.document"
    #[.text work.id, .text (Codec.workStatusName work.status), .text work.scope, .text work.outcome,
      work.baselineDesignRevision.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      work.designRevision.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text work.responsibleAgentRun,
      work.resumeCondition.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      work.migrationDiagnostic.map AgentWorkbench.SQLite.Value.text |>.getD .null,
      .text (Codec.encode work)]

private def insertEntry (store : WriteStore) (entry : LedgerEntry) : IO Unit :=
  AgentWorkbench.SQLite.execute (writeConnection store)
    "INSERT INTO ledger_entries(
       id, entry_order, scope, work_id, design_revision, payload_kind, document
     ) VALUES (?1, ?2, ?3, NULLIF(?4, ''), NULLIF(?5, ''), ?6, ?7)"
    #[entry.id, toString entry.order, entry.scope, Codec.optionText entry.workId,
      Codec.optionText entry.designRevision, Codec.payloadKind entry.payload, Codec.encode entry]

def commitOperation
    (store : WriteStore) (operation : Operation)
    (expectedRevision : Nat) (next : ProjectState)
    (managedOperationId : Option String := none)
    (postCommitVerification : IO Unit := pure ()) : IO Unit := do
  fromExcept (validateState next)
  if next.revision != expectedRevision + 1 then
    fail s!"transition revision must advance exactly once from {expectedRevision}"
  AgentWorkbench.SQLite.immediateTransaction (writeConnection store) do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationStructurallyApplicable prior operation ||
        (operation == .init && store.migratedFromLegacy) do
      fail s!"operation is not applicable in the current state: {operation.name}"
    ensureAppendOnly prior next
    persistDesignChanges store prior next
    persistPlanChanges store prior next
    for work in next.works do writeWork store work
    for entry in next.ledgerEntries.drop prior.ledgerEntries.length do insertEntry store entry
    AgentWorkbench.SQLite.execute (writeConnection store)
      "UPDATE project_metadata SET
         state_revision = ?1,
         accepted_design_id = NULLIF(?2, ''),
         focused_work_id = NULLIF(?3, '')
       WHERE singleton = 1 AND state_revision = ?4"
      #[toString next.revision, Codec.optionText next.acceptedDesignId,
        Codec.optionText next.focusedWorkId, toString expectedRevision]
    if (← AgentWorkbench.SQLite.changes (writeConnection store)) != 1 then
      fail "concurrent project metadata update rejected"
    if let some operationId := managedOperationId then
      AgentWorkbench.SQLite.execute (writeConnection store)
        "UPDATE managed_operations SET committed_state_revision = ?1
         WHERE operation_id = ?2 AND expected_state_revision = ?3
           AND committed_state_revision IS NULL"
        #[toString next.revision, operationId, toString expectedRevision]
      if (← AgentWorkbench.SQLite.changes (writeConnection store)) != 1 then
        fail "managed operation commit marker was not advanced atomically"
  postCommitVerification
  let committed ← loadState store
  if committed != next then
    let fields := [
      ("revision", committed.revision == next.revision),
      ("acceptedDesignId", committed.acceptedDesignId == next.acceptedDesignId),
      ("focusedWorkId", committed.focusedWorkId == next.focusedWorkId),
      ("designRevisions", committed.designRevisions == next.designRevisions),
      ("works", committed.works == next.works),
      ("implementationPlans", committed.implementationPlans == next.implementationPlans),
      ("ledgerEntries", committed.ledgerEntries == next.ledgerEntries)]
    let differing := fields.filterMap fun (name, equal) => if equal then none else some name
    fail s!"committed state differs from the validated transition result: {differing}"

/-- Returns true only when the durable managed-operation row proves that its authority transaction
has not committed. Errors and missing rows are deliberately not interpreted as an uncommitted
operation: after the commit boundary, cleanup must be conservative and recovery-led. -/
def managedOperationDefinitelyUncommitted
    (store : WriteStore) (operationId : String) : IO Bool := do
  let rows ← AgentWorkbench.SQLite.queryTextRows (writeConnection store)
    "SELECT COALESCE(CAST(committed_state_revision AS TEXT), '')
     FROM managed_operations WHERE operation_id = ?1"
    #[operationId] 1
  pure <| rows.size == 1 && rows[0]![0]!.isEmpty

def commitDesignProposal
    (store : WriteStore) (operation : Operation) (expectedRevision : Nat) (next : ProjectState)
    (design : DesignRevision) (sources : List AgentWorkbench.DesignSource.Captured) : IO Unit := do
  fromExcept (validateState next)
  if next.revision != expectedRevision + 1 then
    fail s!"transition revision must advance exactly once from {expectedRevision}"
  AgentWorkbench.SQLite.immediateTransaction (writeConnection store) do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationStructurallyApplicable prior operation do
      fail s!"{operation.name} is not applicable in the authoritative state"
    ensureAppendOnly prior next
    persistDesignChanges store prior next
    persistPlanChanges store prior next
    for work in next.works do writeWork store work
    for (source, ordinal) in sources.zipIdx do insertDesignSource store design.id ordinal source
    AgentWorkbench.SQLite.execute (writeConnection store)
      "UPDATE project_metadata SET state_revision = ?1 WHERE singleton = 1 AND state_revision = ?2"
      #[toString next.revision, toString expectedRevision]
    if (← AgentWorkbench.SQLite.changes (writeConnection store)) != 1 then
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
  AgentWorkbench.SQLite.immediateTransaction (writeConnection store) do
    let prior ← loadState store
    if prior.revision != expectedRevision then
      fail s!"stale state revision {expectedRevision}; current revision is {prior.revision}"
    unless operationStructurallyApplicable prior operation do
      fail s!"{operation.name} is not applicable in the authoritative state"
    ensureAppendOnly prior next
    persistPlanChanges store prior next
    for (source, ordinal) in sources.zipIdx do insertPlanSource store plan.id ordinal source
    AgentWorkbench.SQLite.execute (writeConnection store)
      "UPDATE project_metadata SET state_revision = ?1 WHERE singleton = 1 AND state_revision = ?2"
      #[toString next.revision, toString expectedRevision]
    if (← AgentWorkbench.SQLite.changes (writeConnection store)) != 1 then
      fail "concurrent project metadata update rejected"
  let committed ← loadState store
  if committed != next then fail "committed Plan proposal differs from its validated result"

def beginManagedOperation
    (store : WriteStore) (operationId : String) (expectedRevision : Nat)
    (recoveryPolicy manifest : String) : IO Unit :=
  AgentWorkbench.SQLite.immediateTransaction (writeConnection store) do
    AgentWorkbench.SQLite.execute (writeConnection store)
      "INSERT INTO managed_operations(
         operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
       ) VALUES (?1, ?2, ?3, ?4, NULL)"
      #[operationId, toString expectedRevision, recoveryPolicy, manifest]

def clearManagedOperation (store : WriteStore) (operationId : String) : IO Unit :=
  AgentWorkbench.SQLite.immediateTransaction (writeConnection store) do
    AgentWorkbench.SQLite.execute (writeConnection store)
      "DELETE FROM managed_operations WHERE operation_id = ?1" #[operationId]

end AgentWorkbench.Store
