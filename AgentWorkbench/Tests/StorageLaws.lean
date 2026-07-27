import AgentWorkbench.Adapter.Update
import AgentWorkbench.Adapter.ExternalOperation

open AgentWorkbench
open AgentWorkbench.Domain
open SQLite.Blob

namespace AgentWorkbench.Tests.StorageLaws

def expect (condition : Bool) (message : String) : IO Unit := do
  unless condition do throw <| IO.userError message

def expectFailure (action : IO α) (message : String) : IO Unit := do
  let failed ← try
    let _ ← action
    pure false
  catch _ => pure true
  expect failed message

def execSql (ledger : System.FilePath) (sql : String) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec sql

def load (ledger : System.FilePath) : IO Kernel.Projection.Store := do
  match ← Adapter.SQLite.inspect ledger with
  | .ok store => pure store
  | .error error => throw <| IO.userError s!"store inspection failed: {repr error}"

def mutate (ledger : System.FilePath) (operation : String)
    (revision : Nat) (command : Kernel.Decide.Command) : IO Adapter.SQLite.MutationOutcome := do
  match ← Adapter.SQLite.mutate ledger ⟨operation⟩ ⟨revision⟩ command with
  | .ok outcome => pure outcome
  | .error error => throw <| IO.userError s!"storage mutation failed: {repr error}"

def bootstrap (ledger : System.FilePath) : IO Adapter.SQLite.MutationOutcome :=
  mutate ledger "bootstrap" 0 Application.Service.bootstrapCommand

def testWorkContractPersistence (root : System.FilePath) : IO Unit := do
  let ledger := root / "work-contract.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let aggregate : Domain.Work.WorkUnit :=
    { id := ⟨2⟩
      status := .open
      owner := "aggregate-owner"
      outcome := "deliver one coherent aggregate result"
      completionBoundary := "all aggregate acceptance checks pass" }
  let registered ← mutate ledger "register-aggregate"
    initialized.store.ledger.storedHead.value
    (.registerWork initialized.store.ledger.storedHead aggregate)
  let recovered ← load ledger
  let state ←
    match (Application.Service.status recovered).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "work contract is not recoverable"
  expect (state.work.contains aggregate)
    "fresh SQLite reconstruction split or rewrote aggregate work"
  expectFailure (mutate ledger "register-empty-outcome"
    registered.store.ledger.storedHead.value
    (.registerWork registered.store.ledger.storedHead
      { aggregate with id := ⟨3⟩, outcome := "" }))
    "SQLite mutation accepted work without an outcome"
  expectFailure (mutate ledger "register-empty-boundary"
    registered.store.ledger.storedHead.value
    (.registerWork registered.store.ledger.storedHead
      { aggregate with id := ⟨3⟩, completionBoundary := "" }))
    "SQLite mutation accepted work without a completion boundary"
  expect ((← load ledger) == recovered)
    "rejected work contract mutation changed durable state"

def storageDesign : Domain.Design.DesignVersion :=
  { id := ⟨1⟩
    revision := ⟨1⟩
    owner := "bootstrap-owner"
    contentDigest := "sha256:storage-law-design"
    requirements := [{ key := "evidence-integrity", active := true }]
    decisions := ["stored evidence binds an exact design version"]
    validationGates := ["storage-evidence-matrix"] }

def reviewPurposeDesign : Domain.Design.DesignVersion :=
  { id := ⟨2⟩
    revision := ⟨1⟩
    owner := "bootstrap-owner"
    contentDigest := "sha256:review-purpose-design"
    requirements := [{ key := "review-authority", active := true }]
    decisions := ["completion requires two typed reviews"]
    validationGates := ["review-purpose-matrix"] }

def testRecoveryAndRetry (root : System.FilePath) : IO Unit := do
  let ledger := root / "recovery.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  expect (initialized.store.ledger.storedHead == ⟨1⟩)
    "bootstrap did not durably advance the ledger"
  let reconstructed ← load ledger
  expect (reconstructed == initialized.store)
    "fresh-process reconstruction did not recover the exact committed store"
  let unrelated : Domain.ExternalOperation.Attempt := {
    operation := ⟨"unrelated-after-bootstrap"⟩
    target := .confirmed "remote:unrelated-after-bootstrap"
    artifactDigest := "proof:unrelated"
    state := .prepared }
  match ← Adapter.SQLite.mutate ledger ⟨"unrelated"⟩ ⟨1⟩
      (.recordExternalOperation ⟨1⟩ unrelated) with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"unrelated transaction failed: {repr error}"
  let retry ← Adapter.SQLite.mutate ledger ⟨"bootstrap"⟩ ⟨99⟩
    Application.Service.bootstrapCommand
  match retry with
  | .ok outcome =>
      expect outcome.exactRetry "committed retry was not identified as exact"
      expect (outcome.receipt == initialized.receipt)
        "committed retry did not return the canonical receipt"
      expect (outcome.store.ledger.storedHead == ⟨2⟩)
        "exact retry advanced the ledger"
  | .error error => throw <| IO.userError s!"exact retry rejected: {repr error}"
  let conflicting : Adapter.DurableFilesystem.ArtifactRef := {
    digest := "sha3-256:conflict", size := 1 }
  match ← Adapter.SQLite.mutate ledger ⟨"bootstrap"⟩ ⟨1⟩
      Application.Service.bootstrapCommand [conflicting] (some root) with
  | .error .operationConflict => pure ()
  | other => throw <| IO.userError s!"payload conflict was not rejected: {repr other}"
  match ← Adapter.SQLite.mutate ledger ⟨"unseen-stale"⟩ ⟨0⟩
      Application.Service.bootstrapCommand with
  | .error .staleRevision => pure ()
  | other => throw <| IO.userError s!"unseen stale mutation was not rejected: {repr other}"
  expect ((← load ledger).ledger.storedHead == ⟨2⟩)
    "rejected mutation changed authoritative state"

def testOperationJournalCorruption (root : System.FilePath) : IO Unit := do
  let cases := [
    ("deleted", "DELETE FROM operations WHERE operation_id='bootstrap'"),
    ("request", "UPDATE operations SET request_payload=x'00' WHERE operation_id='bootstrap'"),
    ("result", "UPDATE operations SET result_digest='substituted' WHERE operation_id='bootstrap'"),
    ("receipt", "UPDATE operations SET receipt=x'00' WHERE operation_id='bootstrap'"),
    ("event-binding", "UPDATE events SET operation_id='substituted' WHERE revision=1")]
  for (name, sql) in cases do
    let ledger := root / s!"journal-{name}.sqlite3"
    Adapter.SQLite.initializeStore ledger
    let _ ← bootstrap ledger
    execSql ledger "UPDATE projection SET payload = x'00' WHERE singleton = 1"
    let repairPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"journal repair fixture failed: {repr other}"
    execSql ledger sql
    let before ← IO.FS.readBinFile ledger
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"journal corruption {name} was accepted: {repr other}"
    match ← Adapter.SQLite.diagnose ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"diagnosis accepted journal corruption {name}: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted journal corruption {name}: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == before)
      s!"journal-corrupt repair attempt changed bytes: {name}"

def testCrashRollback (root : System.FilePath) : IO Unit := do
  let ledger := root / "crash-rollback.sqlite3"
  Adapter.SQLite.initializeStore ledger
  expectFailure
    (Adapter.SQLite.mutateWithHook ledger ⟨"crash"⟩ ⟨0⟩
      Application.Service.bootstrapCommand [] none (pure ()) (pure ())
      (throw <| IO.userError "injected crash after journal write"))
    "injected transaction crash was reported as success"
  expect ((← load ledger).ledger.storedHead == ⟨0⟩)
    "transaction crash left a partial journal or event commit"

def crashChild (ledger : System.FilePath) : IO Unit := do
  let _ ← Adapter.SQLite.mutateWithHook ledger ⟨"process-crash"⟩ ⟨0⟩
    Application.Service.bootstrapCommand [] none (pure ()) (pure ()) (IO.Process.forceExit 86)
  throw <| IO.userError "crash failpoint returned"

def testProcessCrashRecovery (root : System.FilePath) : IO Unit := do
  let ledger := root / "process-crash.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--crash-child", ledger.toString] }
  expect (result.exitCode == 86) "storage crash subprocess did not reach the failpoint"
  expect ((← load ledger).ledger.storedHead == ⟨0⟩)
    "subprocess crash left a partial authoritative commit"

def testSingleWriter (root : System.FilePath) : IO Unit := do
  let ledger := root / "single-writer.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let left ← IO.asTask <| Adapter.SQLite.mutate ledger ⟨"writer-left"⟩
    ⟨0⟩ Application.Service.bootstrapCommand
  let right ← IO.asTask <| Adapter.SQLite.mutate ledger ⟨"writer-right"⟩
    ⟨0⟩ Application.Service.bootstrapCommand
  let leftResult ← IO.ofExcept left.get
  let rightResult ← IO.ofExcept right.get
  let accepted := [leftResult, rightResult].countP fun result => result.isOk
  let stale := [leftResult, rightResult].countP fun
    | .error .staleRevision => true
    | _ => false
  expect (accepted == 1 && stale == 1)
    "single-writer coordination did not serialize competing revisions"
  expect ((← load ledger).ledger.storedHead == ⟨1⟩)
    "competing writers committed more than one authoritative transaction"

def testConcurrentInspection (root : System.FilePath) : IO Unit := do
  let ledger := root / "concurrent-inspection.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  for index in [0:24] do
    let before ← load ledger
    let revision := before.ledger.storedHead
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"inspect-race-{index}"⟩
      target := .confirmed s!"remote:inspect-race-{index}"
      artifactDigest := s!"proof:{index}"
      state := .prepared }
    let inspection ← IO.asTask (Adapter.SQLite.inspect ledger)
    match ← Adapter.SQLite.mutate ledger ⟨s!"inspect-writer-{index}"⟩ revision
        (.recordExternalOperation revision attempt) with
    | .ok _ => pure ()
    | .error error => throw <| IO.userError s!"concurrent writer failed: {repr error}"
    match ← IO.ofExcept inspection.get with
    | .ok _ => pure ()
    | .error error => throw <| IO.userError s!"coherent inspection reported corruption: {repr error}"

def testRelativeReplacement (root : System.FilePath) : IO Unit := do
  let run (staged current : System.FilePath) (payload : String) : IO Unit := do
    if ← staged.pathExists then IO.FS.removeFile staged
    if ← current.pathExists then IO.FS.removeFile current
    IO.FS.writeBinFile staged payload.toUTF8
    let _ ← Adapter.DurableFilesystem.replace staged current
    expect ((← IO.FS.readBinFile current) == payload.toUTF8)
      s!"replacement outcome disagreed with adopted bytes for {current}"
    IO.FS.removeFile current
  run ".storage-laws-bare.stage" ".storage-laws-bare.current" "bare"
  run "./.storage-laws-dot.stage" "./.storage-laws-dot.current" "dot"
  run (root / "absolute.stage") (root / "absolute.current") "absolute"

def corruptProjection (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec "UPDATE projection SET payload = x'00' WHERE singleton = 1"

def corruptHistory (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.exec "UPDATE metadata SET history_digest = 'corrupt' WHERE singleton = 1"

def testReadOnlyFaultDetection (root : System.FilePath) : IO Unit := do
  let projectionLedger := root / "projection-fault.sqlite3"
  Adapter.SQLite.initializeStore projectionLedger
  let _ ← bootstrap projectionLedger
  corruptProjection projectionLedger
  let before ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile projectionLedger)
  match ← Adapter.SQLite.inspect projectionLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"projection corruption was not detected: {repr other}"
  let after ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile projectionLedger)
  expect (before == after) "inspection wrote while diagnosing projection corruption"
  let repairPlan ← match ← Adapter.SQLite.diagnose projectionLedger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"projection repair plan was not exposed: {repr other}"
  let repairReceipt ← match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .ok receipt => pure receipt
  | .error error => throw <| IO.userError s!"explicit projection repair failed: {repr error}"
  let _ ← load projectionLedger
  corruptProjection projectionLedger
  match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .ok retry => expect (retry == repairReceipt) "projection repair retry changed receipt"
  | .error error => throw <| IO.userError s!"exact projection repair retry failed: {repr error}"
  let _ ← load projectionLedger
  let laterAttempt : Domain.ExternalOperation.Attempt := {
    operation := ⟨"after-projection-repair"⟩
    target := .confirmed "remote:after-projection-repair"
    artifactDigest := "proof:after-projection-repair"
    state := .prepared }
  let advanced ← match ← Adapter.SQLite.mutate projectionLedger ⟨"after-projection-repair"⟩
      ⟨1⟩ (.recordExternalOperation ⟨1⟩ laterAttempt) with
    | .ok outcome => pure outcome
    | .error error => throw <| IO.userError s!"post-repair advancement failed: {repr error}"
  let advancedBefore ← IO.FS.readBinFile projectionLedger
  match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .ok retry =>
      expect (retry == repairReceipt)
        "historical projection repair retry changed receipt"
  | .error error =>
      throw <| IO.userError s!"historical projection repair retry failed: {repr error}"
  expect ((← IO.FS.readBinFile projectionLedger) == advancedBefore &&
      (← load projectionLedger) == advanced.store)
    "historical projection repair retry changed current authoritative state"
  let changedHistorical := { repairPlan with observedDigest := repairPlan.observedDigest ++ "-changed" }
  match ← Adapter.SQLite.repairProjection projectionLedger changedHistorical with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"changed historical repair plan was accepted: {repr other}"
  execSql projectionLedger "UPDATE projection_repairs SET adopted_digest='tampered'"
  let tamperedBefore ← IO.FS.readBinFile projectionLedger
  match ← Adapter.SQLite.repairProjection projectionLedger repairPlan with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"tampered repair receipt was accepted: {repr other}"
  expect ((← IO.FS.readBinFile projectionLedger) == tamperedBefore)
    "tampered repair receipt rejection changed storage"

  let projectionColumnCases := [
    ("revision", "UPDATE projection SET revision='999' WHERE singleton=1"),
    ("history", "UPDATE projection SET history_digest='corrupt' WHERE singleton=1"),
    ("state", "UPDATE projection SET state_digest='corrupt' WHERE singleton=1")]
  for (name, sql) in projectionColumnCases do
    let ledger := root / s!"projection-column-{name}.sqlite3"
    Adapter.SQLite.initializeStore ledger
    let _ ← bootstrap ledger
    execSql ledger sql
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"projection column corruption {name} was accepted: {repr other}"
    let columnBefore ← IO.FS.readBinFile ledger
    let columnPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"projection column diagnosis {name} failed: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == columnBefore)
      s!"projection column diagnosis wrote storage: {name}"
    match ← Adapter.SQLite.repairProjection ledger columnPlan with
    | .ok _ => pure ()
    | .error error =>
        throw <| IO.userError s!"projection column repair {name} failed: {repr error}"
    let _ ← load ledger

  let historyLedger := root / "history-fault.sqlite3"
  Adapter.SQLite.initializeStore historyLedger
  let _ ← bootstrap historyLedger
  corruptHistory historyLedger
  let historyBefore ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile historyLedger)
  match ← Adapter.SQLite.inspect historyLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"history corruption was not detected: {repr other}"
  let historyAfter ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile historyLedger)
  expect (historyBefore == historyAfter) "inspection silently repaired corrupt history"
  match ← Adapter.SQLite.diagnose historyLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"corrupt history exposed a repair action: {repr other}"

def testArtifactsAndEvidence (root : System.FilePath) : IO Unit := do
  let ledger := root / "artifacts.sqlite3"
  let artifactRoot := Adapter.SQLite.artifactRoot ledger
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let bytes := "exact evidence artifact".toUTF8
  let reference ← Adapter.DurableFilesystem.stage artifactRoot bytes
  let duplicate ← Adapter.DurableFilesystem.stage artifactRoot bytes
  expect (duplicate == reference) "exact artifact retry changed its identity"
  let operation : Domain.ExternalOperation.Attempt := {
    operation := ⟨"artifact-operation"⟩
    target := .confirmed "remote:artifact-operation"
    artifactDigest := reference.digest
    state := .prepared }
  match ← Adapter.SQLite.mutate ledger ⟨"artifact-event-missing"⟩
      initialized.store.ledger.storedHead
      (.recordExternalOperation initialized.store.ledger.storedHead operation) with
  | .error (.artifactInvalid _) => pure ()
  | other => throw <| IO.userError s!"missing staged artifact was accepted: {repr other}"
  expect ((← load ledger).ledger.storedHead == initialized.store.ledger.storedHead)
    "rejected artifact reference advanced the ledger"
  let recorded ← Adapter.SQLite.mutate ledger ⟨"artifact-event"⟩
    initialized.store.ledger.storedHead
    (.recordExternalOperation initialized.store.ledger.storedHead operation)
    [reference] (some artifactRoot)
  let artifactStore ← match recorded with
    | .ok outcome => pure outcome.store
    | .error error => throw <| IO.userError s!"artifact transaction rejected: {repr error}"
  match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
  | .ok recovered => expect (recovered == artifactStore) "artifact-backed recovery drifted"
  | .error error => throw <| IO.userError s!"valid artifact rejected: {repr error}"
  IO.FS.removeFile (Adapter.DurableFilesystem.objectPath artifactRoot reference)
  match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"missing committed artifact was not detected: {repr other}"

  let evidenceLedger := root / "evidence.sqlite3"
  Adapter.SQLite.initializeStore evidenceLedger
  let evidenceInitialized ← bootstrap evidenceLedger
  let designed ← mutate evidenceLedger "design" 1
    (.importDesign ⟨1⟩ storageDesign)
  let designScope : Domain.Review.FrozenScope :=
    { design := some storageDesign.id
      work := ⟨1⟩
      repositorySnapshot := "snapshot:storage-design"
      artifactDigest := storageDesign.contentDigest
      purpose := .design }
  let designPlan : Domain.Review.Plan :=
    { id := ⟨700⟩
      owner := storageDesign.owner
      reviewer := "storage-design-reviewer"
      adjudicator := storageDesign.owner
      caller := storageDesign.owner
      scope := designScope }
  let planned ← mutate evidenceLedger "design-review-plan"
    designed.store.ledger.storedHead.value
    (.recordReviewPlan designed.store.ledger.storedHead designPlan)
  let designClaim : Domain.Review.Claim :=
    { id := ⟨700⟩
      plan := designPlan.id
      work := designScope.work
      epoch := ⟨0⟩
      claim := .clean
      reviewer := designPlan.reviewer
      scope := some designScope }
  let claimed ← mutate evidenceLedger "design-review-claim"
    planned.store.ledger.storedHead.value
    (.recordReviewClaim planned.store.ledger.storedHead designClaim)
  let adjudicated ← mutate evidenceLedger "design-review-adjudication"
    claimed.store.ledger.storedHead.value
    (.recordReviewAdjudication claimed.store.ledger.storedHead
      { review := designClaim.id
        decision := .accepted
        adjudicator := designPlan.adjudicator
        reason := "the evidence design matches its frozen review scope" })
  let approved ← mutate evidenceLedger "design-approval"
    adjudicated.store.ledger.storedHead.value
    (.approveDesign adjudicated.store.ledger.storedHead storageDesign.id)
  let obligation : Domain.Evidence.Obligation := {
    work := ⟨1⟩, key := "evidence-integrity"
    revision := approved.store.ledger.storedHead
    commandProfile := "storage-laws"
    invocation := ".lake/build/bin/storage-laws --fault-matrix"
    repository := "main", snapshot := "commit:test"
    artifactDigest := "proof:test", current := true
    requirements := ["evidence-integrity"], expectedProducer := "storage-law-runner"
    expectedObservation := "storage-law-observation"
    design := ⟨1⟩, designRevision := ⟨1⟩ }
  let obligated ← mutate evidenceLedger "obligation"
    approved.store.ledger.storedHead.value
    (.recordObligation approved.store.ledger.storedHead obligation)
  let item : Domain.Evidence.Evidence := {
    id := ⟨1⟩
    work := ⟨1⟩
    obligation := "evidence-integrity"
    revision := obligation.revision
    commandProfile := "storage-laws"
    invocation := ".lake/build/bin/storage-laws --fault-matrix"
    exitCode := 0
    repository := "main"
    snapshot := "commit:test"
    artifactDigest := "proof:test"
    current := true
    requirements := obligation.requirements
    producer := obligation.expectedProducer
    observedAt := "storage-law-observation"
    design := obligation.design
    designRevision := obligation.designRevision }
  let evidenced ← mutate evidenceLedger "evidence"
    obligated.store.ledger.storedHead.value
    (.recordEvidence obligated.store.ledger.storedHead item)
  let recovered ← load evidenceLedger
  expect (recovered == evidenced.store) "exact evidence did not survive reconstruction"
  let current ← match (Kernel.Projection.inspect recovered).currentState? with
    | some state => pure state
    | none => throw <| IO.userError "recovered evidence projection is not fresh"
  expect (current.evidence.any fun found =>
      found.invocation == item.invocation && found.repository == item.repository &&
      found.snapshot == item.snapshot && found.artifactDigest == item.artifactDigest)
    "typed exact evidence identities were not retained"
  expect (evidenceInitialized.store.ledger.storedHead == ⟨1⟩)
    "evidence fixture bootstrap drifted"
  expect (designed.store.ledger.storedHead == ⟨2⟩)
    "evidence fixture design import drifted"

def testCorrectionPersistence (root : System.FilePath) : IO Unit := do
  let ledger := root / "corrections.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let correction : Domain.Design.Correction := {
    key := "durable-correction"
    scope := "workflow"
    statement := "recover before planning"
    resolved := false
    work := some ⟨1⟩ }
  let recorded ← mutate ledger "record-correction"
    initialized.store.ledger.storedHead.value
    (.recordUserCorrection initialized.store.ledger.storedHead correction)
  let recovered ← load ledger
  let recoveredState ←
    match (Application.Service.status recovered).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "fresh correction session is not recoverable"
  expect (recoveredState.corrections.contains correction)
    "fresh SQLite session lost the durable correction"
  expect (recoveredState.reviewPlans.isEmpty)
    "fresh SQLite session introduced planning before correction recovery"
  expect ((Application.Service.queryGate
      (.correctionsReady correction.scope) recovered).value ==
        .blocked "an applicable durable user correction remains unresolved")
    "fresh SQLite session did not expose the correction readiness blocker"
  match (Application.Service.resolve recovered).value with
  | .blocked _ => pure ()
  | .action _ =>
      throw <| IO.userError
        "fresh SQLite session selected continue with an applicable correction"
  let transition : Domain.Design.AuthorityTransition := {
    key := "durable-correction-authority-v1"
    correction := correction.key
    target := "durable-correction-rule"
    operation := .create
    kind := .rule
    scope := correction.scope
    work := correction.work
    design := correction.design
    lifetime := .persistent
    statement := correction.statement
    reason := "the correction establishes a persistent scoped authority" }
  let resolved ← mutate ledger "record-authority-transition"
    recorded.store.ledger.storedHead.value
    (.recordAuthorityTransition recorded.store.ledger.storedHead transition)
  match (Application.Service.resolve resolved.store).value with
  | .action (.continueActiveWork _ work _) =>
      expect (work == ⟨1⟩)
        "resolved correction restored continue for the wrong work"
  | other =>
      throw <| IO.userError s!"resolved correction did not restore continue: {repr other}"
  let promoted ← load ledger
  let promotedState ←
    match (Application.Service.status promoted).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "promoted correction session is not recoverable"
  expect (promotedState.corrections.any fun item =>
    item.key == correction.key && item.resolved)
    "fresh SQLite session lost resolved correction provenance"
  expect (promotedState.authorityTransitions.contains transition)
    "fresh SQLite session lost authority transition provenance"

def testReviewPurposePersistence (root : System.FilePath) : IO Unit := do
  let run (name : String) (qualityArtifact? qualitySnapshot? : Option String)
      (adjudicateQuality expectedReviewsReady expectedCloseable : Bool)
      (repositorySnapshot : String := "snapshot:review-purpose")
      (validationArtifact : String := "sha256:reviewed-implementation")
      (evidenceSnapshot : String := "snapshot:review-purpose")
      (evidenceArtifact : String := "sha256:reviewed-implementation") :
      IO Unit := do
    let ledger := root / s!"review-purpose-{name}.sqlite3"
    Adapter.SQLite.initializeStore ledger
    let initialized ← bootstrap ledger
    let imported ← mutate ledger s!"{name}-design"
      initialized.store.ledger.storedHead.value
      (.importDesign initialized.store.ledger.storedHead reviewPurposeDesign)
    let scope (purpose : Domain.Review.Purpose) (snapshot artifact : String) :
        Domain.Review.FrozenScope :=
      { design := some reviewPurposeDesign.id
        work := ⟨1⟩
        repositorySnapshot := snapshot
        artifactDigest := artifact
        purpose }
    let recordReview (operation : String) (id : Nat)
        (purpose : Domain.Review.Purpose) (snapshot artifact reviewer : String)
        (adjudicate : Bool)
        (store : Kernel.Projection.Store) : IO Kernel.Projection.Store := do
      let plan : Domain.Review.Plan :=
        { id := ⟨id⟩
          owner := reviewPurposeDesign.owner
          reviewer
          adjudicator := reviewPurposeDesign.owner
          caller := reviewPurposeDesign.owner
          scope := scope purpose snapshot artifact }
      let planned ← mutate ledger s!"{operation}-plan"
        store.ledger.storedHead.value
        (.recordReviewPlan store.ledger.storedHead plan)
      let claim : Domain.Review.Claim :=
        { id := ⟨id⟩
          plan := plan.id
          work := ⟨1⟩
          epoch := ⟨0⟩
          claim := .clean
          reviewer
          scope := some plan.scope }
      let claimed ← mutate ledger s!"{operation}-claim"
        planned.store.ledger.storedHead.value
        (.recordReviewClaim planned.store.ledger.storedHead claim)
      if adjudicate then
        let adjudicated ← mutate ledger s!"{operation}-adjudication"
          claimed.store.ledger.storedHead.value
          (.recordReviewAdjudication claimed.store.ledger.storedHead
            { review := claim.id, decision := .accepted
              adjudicator := reviewPurposeDesign.owner
              reason := "the stored claim matches the frozen review scope" })
        pure adjudicated.store
      else
        pure claimed.store
    let defaultSnapshot := "snapshot:review-purpose"
    let store ← recordReview "design-review" 1 .design
      defaultSnapshot reviewPurposeDesign.contentDigest "design-reviewer" true
      imported.store
    let approved ← mutate ledger s!"{name}-approval"
      store.ledger.storedHead.value
      (.approveDesign store.ledger.storedHead reviewPurposeDesign.id)
    let decompositionDigest := "sha256:review-purpose-decomposition"
    let store ← recordReview "decomposition-review" 2 .decomposition
      defaultSnapshot decompositionDigest "decomposition-reviewer" true
      approved.store
    let decomposition : Domain.Design.Decomposition :=
      { key := "review-purpose-decomposition"
        design := reviewPurposeDesign.id
        work := ⟨1⟩
        designRevision := reviewPurposeDesign.revision
        contentDigest := decompositionDigest
        items := [{
          key := "review-purpose"
          requirements := ["review-authority"]
          implementationWork := ["typed review purposes"]
          tasks := ["enforce both completion reviews"]
          completionChecks := ["review purpose laws"]
          checklists := ["both purposes reviewed"]
          validationGates := ["review-purpose-matrix"] }]
        reviewer := "decomposition-reviewer"
        adjudicator := reviewPurposeDesign.owner
        accepted := true }
    let decomposed ← mutate ledger s!"{name}-decomposition"
      store.ledger.storedHead.value
      (.recordDecomposition store.ledger.storedHead decomposition)
    let implementationArtifact := "sha256:reviewed-implementation"
    let store ← recordReview "conformance-review" 3 .designConformance
      defaultSnapshot implementationArtifact "conformance-reviewer" true
      decomposed.store
    let store ←
      match qualityArtifact?, qualitySnapshot? with
      | some qualityArtifact, some qualitySnapshot =>
        recordReview "quality-review" 4 .implementationQuality
          qualitySnapshot qualityArtifact "quality-reviewer" adjudicateQuality
          store
      | _, _ =>
        pure store
    let completionPlan : Domain.Lifecycle.CompletionPlan :=
      { work := ⟨1⟩
        decomposition := some "review-purpose-decomposition"
        relatedWork := []
        phases := []
        tasks := []
        checklists := []
        reviews :=
          if qualityArtifact?.isSome && qualitySnapshot?.isSome then
            [⟨3⟩, ⟨4⟩]
          else
            [⟨3⟩]
        obligations := ["completion-proof"]
        findings := []
        validations := ["validation"]
        repositories := ["repository"]
        corrections := []
        workRecords := [] }
    let planned ← mutate ledger s!"{name}-completion-plan"
      store.ledger.storedHead.value
      (.planCompletion store.ledger.storedHead completionPlan)
    let classified ← mutate ledger s!"{name}-repository"
      planned.store.ledger.storedHead.value
      (.classifyRepository planned.store.ledger.storedHead ⟨1⟩
        "repository" repositorySnapshot)
    let validated ← mutate ledger s!"{name}-validation"
      classified.store.ledger.storedHead.value
      (.passValidation classified.store.ledger.storedHead ⟨1⟩
        "validation" validationArtifact)
    let obligation : Domain.Evidence.Obligation :=
      { work := ⟨1⟩
        key := "completion-proof"
        revision := validated.store.ledger.storedHead
        commandProfile := "storage-laws"
        invocation := ".lake/build/bin/storage-laws"
        repository := "main"
        snapshot := evidenceSnapshot
        artifactDigest := evidenceArtifact
        current := true
        requirements := ["review-authority"]
        expectedProducer := "storage-law-runner"
        expectedObservation := s!"{name}-observation"
        design := reviewPurposeDesign.id
        designRevision := reviewPurposeDesign.revision }
    let obligated ← mutate ledger s!"{name}-obligation"
      validated.store.ledger.storedHead.value
      (.recordObligation validated.store.ledger.storedHead obligation)
    let evidence : Domain.Evidence.Evidence :=
      { id := ⟨50⟩
        work := obligation.work
        obligation := obligation.key
        revision := obligation.revision
        commandProfile := obligation.commandProfile
        invocation := obligation.invocation
        exitCode := 0
        repository := obligation.repository
        snapshot := obligation.snapshot
        artifactDigest := obligation.artifactDigest
        current := true
        requirements := obligation.requirements
        producer := obligation.expectedProducer
        observedAt := obligation.expectedObservation
        design := obligation.design
        designRevision := obligation.designRevision }
    let _ ← mutate ledger s!"{name}-evidence"
      obligated.store.ledger.storedHead.value
      (.recordEvidence obligated.store.ledger.storedHead evidence)
    let recovered ← load ledger
    let state ←
      match (Application.Service.status recovered).value.currentState? with
      | some state => pure state
      | none => throw <| IO.userError "review-purpose projection is not recoverable"
    expect (Policy.Completion.requiredReviewsReady ⟨1⟩
      state.work state.reviewPlans state.decompositions state.claims state.adjudications
      state.reviewFindings state.findingVerifications state.lifecycle ==
        expectedReviewsReady)
      s!"fresh SQLite reconstruction changed required review readiness: {name}"
    expect (Policy.Completion.closeable ⟨1⟩ state.work state.activations
      state.claims state.adjudications state.reviewPlans state.reviewFindings
      state.findingVerifications state.lifecycle state.evidence state.obligations
      state.designs state.designApprovals state.decompositions state.corrections ==
        expectedCloseable)
      s!"fresh SQLite reconstruction changed completion binding: {name}"
    if qualityArtifact?.isSome && qualitySnapshot?.isSome &&
        adjudicateQuality then
      expect (Policy.Completion.purposeReviewReady ⟨1⟩
        (some reviewPurposeDesign.id) .designConformance
        state.reviewPlans state.claims state.adjudications
        state.reviewFindings state.findingVerifications)
        s!"fresh SQLite reconstruction invalidated conformance review: {name}"
      expect (Policy.Completion.purposeReviewReady ⟨1⟩
        (some reviewPurposeDesign.id) .implementationQuality
        state.reviewPlans state.claims state.adjudications
        state.reviewFindings state.findingVerifications)
        s!"fresh SQLite reconstruction invalidated quality review: {name}"
  let implementationArtifact := "sha256:reviewed-implementation"
  let defaultSnapshot := "snapshot:review-purpose"
  run "complete" (some implementationArtifact) (some defaultSnapshot) true true true
  run "single-selected-review" none none false true true
  run "mismatched-artifact" (some "sha256:different-artifact")
    (some defaultSnapshot) true true false
  run "mismatched-snapshot" (some implementationArtifact)
    (some "snapshot:different") true true false
  run "missing-quality-adjudication" (some implementationArtifact)
    (some defaultSnapshot) false false false
  run "mismatched-repository" (some implementationArtifact)
    (some defaultSnapshot) true true false "snapshot:different"
  run "mismatched-validation-artifact" (some implementationArtifact)
    (some defaultSnapshot) true true false defaultSnapshot
    "sha256:different-artifact"
  run "mismatched-evidence-snapshot" (some implementationArtifact)
    (some defaultSnapshot) true true false defaultSnapshot implementationArtifact
    "snapshot:different"
  run "mismatched-evidence-artifact" (some implementationArtifact)
    (some defaultSnapshot) true true false defaultSnapshot implementationArtifact
    defaultSnapshot
    "sha256:different-artifact"

def testFindingAttemptPersistence (root : System.FilePath) : IO Unit := do
  let ledger := root / "finding-attempts.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let apply (operation : String)
      (command : Revision → Kernel.Decide.Command)
      (store : Kernel.Projection.Store) : IO Kernel.Projection.Store := do
    let outcome ← mutate ledger operation store.ledger.storedHead.value
      (command store.ledger.storedHead)
    pure outcome.store
  let store ← apply "finding-design" (fun revision =>
    .importDesign revision reviewPurposeDesign) initialized.store
  let scope : Domain.Review.FrozenScope :=
    { design := some reviewPurposeDesign.id
      work := ⟨1⟩
      repositorySnapshot := "snapshot:before-remediation"
      artifactDigest := "sha256:before-remediation"
      purpose := .design }
  let plan : Domain.Review.Plan :=
    { id := ⟨100⟩
      owner := "bootstrap-owner"
      reviewer := "finding-reviewer"
      adjudicator := "bootstrap-owner"
      caller := "bootstrap-owner"
      scope }
  let store ← apply "finding-plan" (fun revision =>
    .recordReviewPlan revision plan) store
  let claim : Domain.Review.Claim :=
    { id := ⟨100⟩
      plan := plan.id
      work := scope.work
      epoch := ⟨0⟩
      claim := .findings
      reviewer := plan.reviewer
      scope := some scope }
  let store ← apply "finding-claim" (fun revision =>
    .recordReviewClaim revision claim) store
  let finding : Domain.Review.Finding :=
    { key := "durable-finding"
      review := claim.id
      blocking := true
      authority := "review-authority"
      failureAccount := "overwriting a failed verification would erase review history"
      invariant := "failed verification preserves immutable history"
      remediationSurfaces := ["kernel"]
      accepted := false
      adjudicated := false }
  let store ← apply "finding-record" (fun revision =>
    .recordReviewFinding revision finding) store
  let store ← apply "finding-adjudicate" (fun revision =>
    .adjudicateReviewFinding revision finding.key plan.adjudicator
      "the immutable verification history violation is confirmed" true) store
  let firstAttempt : Domain.Review.ClosureAttempt :=
    { attempt := 1
      evidenceDigest := "sha256:first-fix"
      repositorySnapshot := "snapshot:first-fix" }
  let store ← apply "finding-close-1" (fun revision =>
    .closeReviewFinding revision finding.key firstAttempt) store
  let firstVerification : Domain.Review.Verification :=
    { finding := finding.key
      attempt := firstAttempt.attempt
      verifier := "first-verifier"
      scope := { scope with
        repositorySnapshot := firstAttempt.repositorySnapshot
        artifactDigest := "sha256:first-fix" }
      evidenceDigest := firstAttempt.evidenceDigest
      result := .notFixed
      accepted := false }
  let store ← apply "finding-verify-1" (fun revision =>
    .verifyReviewFinding revision firstVerification) store
  let _ ← apply "finding-verification-adjudicate-1" (fun revision =>
    .adjudicateFindingVerification revision finding.key
      firstAttempt.attempt plan.adjudicator) store
  let recoveredAfterFailure ← load ledger
  let failedState ←
    match (Application.Service.status recoveredAfterFailure).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "failed finding attempt is not recoverable"
  expect (failedState.reviewFindings.any fun record =>
    record.key == finding.key &&
      record.closureAttempts == [firstAttempt])
    "fresh SQLite reconstruction lost the first immutable closure attempt"
  expect (failedState.findingVerifications.any fun verification =>
    verification.finding == finding.key && verification.attempt == 1 &&
      verification.result == .notFixed && verification.adjudicated)
    "fresh SQLite reconstruction lost the failed verification result"
  let secondAttempt : Domain.Review.ClosureAttempt :=
    { attempt := 2
      evidenceDigest := "sha256:second-fix"
      repositorySnapshot := "snapshot:second-fix" }
  let store ← apply "finding-close-2" (fun revision =>
    .closeReviewFinding revision finding.key secondAttempt)
    recoveredAfterFailure
  let secondVerification : Domain.Review.Verification :=
    { finding := finding.key
      attempt := secondAttempt.attempt
      verifier := "second-verifier"
      scope := { scope with
        repositorySnapshot := secondAttempt.repositorySnapshot
        artifactDigest := "sha256:second-fix" }
      evidenceDigest := secondAttempt.evidenceDigest
      result := .needsEvidence
      accepted := false }
  let store ← apply "finding-verify-2" (fun revision =>
    .verifyReviewFinding revision secondVerification) store
  let _ ← apply "finding-verification-adjudicate-2" (fun revision =>
    .adjudicateFindingVerification revision finding.key
      secondAttempt.attempt plan.adjudicator) store
  let recoveredAfterIncomplete ← load ledger
  let incompleteState ←
    match (Application.Service.status recoveredAfterIncomplete).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "incomplete finding attempt is not recoverable"
  expect (incompleteState.reviewFindings.any fun record =>
    record.key == finding.key &&
      record.closureAttempts == [firstAttempt, secondAttempt])
    "fresh SQLite reconstruction lost the needs-evidence closure attempt"
  expect (incompleteState.findingVerifications.any fun verification =>
    verification.finding == finding.key && verification.attempt == 2 &&
      verification.result == .needsEvidence && verification.adjudicated)
    "fresh SQLite reconstruction lost the needs-evidence result"
  expectFailure
    (mutate ledger "finding-close-history-conflict"
      recoveredAfterIncomplete.ledger.storedHead.value
      (.closeReviewFinding recoveredAfterIncomplete.ledger.storedHead
        finding.key secondAttempt))
    "fresh SQLite session accepted a historical closure attempt rewrite"
  expectFailure
    (mutate ledger "finding-verification-history-conflict"
      recoveredAfterIncomplete.ledger.storedHead.value
      (.verifyReviewFinding recoveredAfterIncomplete.ledger.storedHead
        { secondVerification with result := .verified }))
    "fresh SQLite session accepted a historical verification result rewrite"
  let thirdAttempt : Domain.Review.ClosureAttempt :=
    { attempt := 3
      evidenceDigest := "sha256:third-fix"
      repositorySnapshot := "snapshot:third-fix" }
  let store ← apply "finding-close-3" (fun revision =>
    .closeReviewFinding revision finding.key thirdAttempt)
    recoveredAfterIncomplete
  let thirdVerification : Domain.Review.Verification :=
    { finding := finding.key
      attempt := thirdAttempt.attempt
      verifier := "third-verifier"
      scope := { scope with
        repositorySnapshot := thirdAttempt.repositorySnapshot
        artifactDigest := "sha256:third-fix" }
      evidenceDigest := thirdAttempt.evidenceDigest
      result := .verified
      accepted := false }
  let store ← apply "finding-verify-3" (fun revision =>
    .verifyReviewFinding revision thirdVerification) store
  let _ ← apply "finding-verification-adjudicate-3" (fun revision =>
    .adjudicateFindingVerification revision finding.key
      thirdAttempt.attempt plan.adjudicator) store
  let recoveredAfterSuccess ← load ledger
  let successfulState ←
    match (Application.Service.status recoveredAfterSuccess).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "successful finding attempt is not recoverable"
  expect (successfulState.reviewFindings.any fun record =>
    record.key == finding.key &&
      record.closureAttempts == [firstAttempt, secondAttempt, thirdAttempt])
    "fresh SQLite reconstruction rewrote closure attempt history"
  expect (successfulState.findingVerifications.any fun verification =>
    verification.finding == finding.key && verification.attempt == 1 &&
      verification.result == .notFixed && verification.adjudicated)
    "successful retry rewrote the historical failed verification"
  expect (successfulState.findingVerifications.any fun verification =>
    verification.finding == finding.key && verification.attempt == 2 &&
      verification.result == .needsEvidence && verification.adjudicated)
    "successful retry rewrote the historical needs-evidence verification"
  expect (Policy.Authority.blockingFindingsClosed claim.id
    successfulState.claims successfulState.reviewFindings
    successfulState.findingVerifications)
    "fresh SQLite reconstruction did not authorize the latest verified attempt"

def testExternalOperationReconciliation (root : System.FilePath) : IO Unit := do
  let ledger := root / "external-operation.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let intent : Domain.ExternalOperation.Attempt := {
    operation := ⟨"publish-artifact"⟩
    target := .confirmed "immutable-remote-object"
    artifactDigest := "proof:release-artifact"
    state := .prepared }
  let prepared ← mutate ledger "publish-artifact-intent"
    initialized.store.ledger.storedHead.value
    (.recordExternalOperation initialized.store.ledger.storedHead intent)
  let beforeDispatch ← load ledger
  let preparedState ← match (Application.Service.status beforeDispatch).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "prepared external intent was not recoverable"
  expect (preparedState.externalOperations == [intent])
    "external intent and attempt identity were not durable before dispatch"

  let dispatched := { intent with state := .dispatched }
  let dispatchRecorded ← mutate ledger "publish-artifact-dispatch"
    prepared.store.ledger.storedHead.value
    (.advanceExternalOperation prepared.store.ledger.storedHead dispatched)
  let afterDispatch ← load ledger
  let dispatchedState ← match
      (Application.Service.status afterDispatch).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "dispatched external intent was not recoverable"
  expect (dispatchedState.externalOperations == [dispatched] &&
      Domain.ExternalOperation.requiresReconciliation dispatched)
    "interrupted dispatch did not recover as reconciliation-required"

  let exactObservation : Domain.ExternalOperation.RemoteObservation := {
    identity := "immutable-remote-object"
    artifactDigest := some intent.artifactDigest }
  let succeeded := {
    intent with state := .succeeded, observation := some exactObservation }
  let reconciled ← mutate ledger "publish-artifact-reconcile"
    dispatchRecorded.store.ledger.storedHead.value
    (.advanceExternalOperation dispatchRecorded.store.ledger.storedHead succeeded)
  let recovered ← load ledger
  let recoveredState ← match
      (Application.Service.status recovered).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "reconciled external operation was not recoverable"
  expect (recoveredState.externalOperations == [succeeded])
    "matching immutable observation did not durably complete the operation"
  match ← Adapter.SQLite.mutate ledger ⟨"publish-artifact-reconcile"⟩
      dispatchRecorded.store.ledger.storedHead
      (.advanceExternalOperation dispatchRecorded.store.ledger.storedHead succeeded) with
  | .ok retry =>
      expect (retry.exactRetry && retry.receipt == reconciled.receipt)
        "exact reconciliation retry did not return its canonical receipt"
  | .error error =>
      throw <| IO.userError s!"exact reconciliation retry failed: {repr error}"
  let conflictingObservation : Domain.ExternalOperation.RemoteObservation := {
    identity := "immutable-remote-object"
    artifactDigest := some "proof:different-artifact" }
  let conflicting := {
    intent with state := .conflict, observation := some conflictingObservation }
  match ← Adapter.SQLite.mutate ledger ⟨"publish-artifact-reconcile"⟩
      dispatchRecorded.store.ledger.storedHead
      (.advanceExternalOperation dispatchRecorded.store.ledger.storedHead conflicting) with
  | .error .operationConflict => pure ()
  | other =>
      throw <| IO.userError
        s!"changed reconciliation observation reused an attempt identity: {repr other}"
  expect ((← load ledger) == recovered)
    "conflicting reconciliation retry changed the completed ledger"

structure RemoteFixture where
  objects : IO.Ref (List (String × String))
  dispatches : IO.Ref Nat
  loseResponse : IO.Ref Bool
  failBeforeEffect : IO.Ref Bool
  failObservation : IO.Ref Bool

def remoteObservation (fixture : RemoteFixture) (target : String) :
    IO Domain.ExternalOperation.RemoteObservation := do
  if ← fixture.failObservation.get then
    fixture.failObservation.set false
    throw <| IO.userError "injected observation failure"
  return {
    identity := target
    artifactDigest := (← fixture.objects.get).lookup target }

def remotePort (fixture : RemoteFixture) : Adapter.ExternalOperation.Port := {
  observe := remoteObservation fixture
  dispatch := fun _operation target artifactDigest precondition => do
    if ← fixture.failBeforeEffect.get then
      fixture.failBeforeEffect.set false
      throw <| IO.userError "injected failure before remote effect"
    let before ← remoteObservation fixture target
    unless precondition.satisfiedBy before do
      return .observed before
    fixture.dispatches.modify (· + 1)
    fixture.objects.modify fun objects =>
      (target, artifactDigest) :: objects.filter (fun entry => entry.1 != target)
    if ← fixture.loseResponse.get then
      fixture.loseResponse.set false
      return .responseLost
    return .observed {
      identity := target
      artifactDigest := some artifactDigest }
}

def testExternalOperationBoundaryFaults (root : System.FilePath) : IO Unit := do
  let ledger := root / "external-operation-boundary.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let initialized ← bootstrap ledger
  let objects ← IO.mkRef ([] : List (String × String))
  let dispatches ← IO.mkRef 0
  let loseResponse ← IO.mkRef true
  let failBeforeEffect ← IO.mkRef false
  let failObservation ← IO.mkRef false
  let fixture : RemoteFixture := {
    objects, dispatches, loseResponse, failBeforeEffect, failObservation }
  let port := remotePort fixture
  let prepared : Domain.ExternalOperation.Attempt := {
    operation := ⟨"boundary-publication"⟩
    target := .confirmed "immutable:release-v1"
    artifactDigest := "sha256:release-v1"
    remotePrecondition := { expectedArtifactDigest := none }
    state := .prepared }
  let dispatched := { prepared with state := .dispatched }
  let preparedRecord ← mutate ledger "boundary-prepare"
    initialized.store.ledger.storedHead.value
    (.recordExternalOperation initialized.store.ledger.storedHead prepared)
  let dispatchedRecord ← mutate ledger "boundary-dispatch"
    preparedRecord.store.ledger.storedHead.value
    (.advanceExternalOperation preparedRecord.store.ledger.storedHead dispatched)
  let uncertain ← match ← Adapter.ExternalOperation.dispatch ledger port prepared.operation with
    | .ok attempt => pure attempt
    | .error error => throw <| IO.userError s!"response-loss dispatch failed: {repr error}"
  expect (uncertain.state == .uncertain &&
      Domain.ExternalOperation.requiresReconciliation uncertain)
    "acceptance followed by response loss did not become uncertain"
  expect ((← dispatches.get) == 1)
    "the response-loss fixture did not execute exactly one remote effect"
  let uncertainRecord ← mutate ledger "boundary-uncertain"
    dispatchedRecord.store.ledger.storedHead.value
    (.advanceExternalOperation dispatchedRecord.store.ledger.storedHead uncertain)
  match ← Adapter.ExternalOperation.dispatch ledger port prepared.operation with
  | .error .invalidAttempt => pure ()
  | other => throw <| IO.userError s!"retry bypassed reconciliation: {repr other}"
  expect ((← dispatches.get) == 1)
    "a retry before reconciliation duplicated the remote effect"

  failObservation.set true
  let stillUncertain ← match ←
      Adapter.ExternalOperation.reconcile ledger port prepared.operation with
    | .ok attempt => pure attempt
    | .error error => throw <| IO.userError s!"observation failure escaped: {repr error}"
  expect (stillUncertain.state == .uncertain)
    "failure during reconciliation authorized a terminal transition"
  let succeeded ← match ←
      Adapter.ExternalOperation.reconcile ledger port prepared.operation with
    | .ok attempt => pure attempt
    | .error error => throw <| IO.userError s!"matching reconciliation failed: {repr error}"
  expect (succeeded.state == .succeeded &&
      succeeded.observation.any (fun observation =>
        prepared.target.dispatchIdentity? == some observation.identity) &&
      (← dispatches.get) == 1)
    "matching immutable remote state did not complete without duplication"
  let succeededRecord ← mutate ledger "boundary-succeeded"
    uncertainRecord.store.ledger.storedHead.value
    (.advanceExternalOperation uncertainRecord.store.ledger.storedHead succeeded)

  let failurePrepared : Domain.ExternalOperation.Attempt := {
    operation := ⟨"failure-before-effect"⟩
    target := .confirmed "immutable:release-v2"
    artifactDigest := "sha256:release-v2"
    state := .prepared }
  let failurePreparedRecord ← mutate ledger "failure-before-effect-prepare"
    succeededRecord.store.ledger.storedHead.value
    (.recordExternalOperation succeededRecord.store.ledger.storedHead failurePrepared)
  let beforeFailure := { failurePrepared with state := .dispatched }
  let failureDispatchedRecord ← mutate ledger "failure-before-effect-dispatch"
    failurePreparedRecord.store.ledger.storedHead.value
    (.advanceExternalOperation failurePreparedRecord.store.ledger.storedHead beforeFailure)
  failBeforeEffect.set true
  let unknown ← match ←
      Adapter.ExternalOperation.dispatch ledger port beforeFailure.operation with
    | .ok attempt => pure attempt
    | .error error => throw <| IO.userError s!"pre-effect fault escaped: {repr error}"
  expect (unknown.state == .uncertain)
    "a port exception was asserted to be failure without reconciliation"
  let unknownRecord ← mutate ledger "failure-before-effect-uncertain"
    failureDispatchedRecord.store.ledger.storedHead.value
    (.advanceExternalOperation failureDispatchedRecord.store.ledger.storedHead unknown)
  let retryable ← match ←
      Adapter.ExternalOperation.reconcile ledger port beforeFailure.operation with
    | .ok attempt => pure attempt
    | .error error => throw <| IO.userError s!"absence reconciliation failed: {repr error}"
  expect (retryable.state == .retryable)
    "observed absence did not authorize a retry"
  let retryableRecord ← mutate ledger "failure-before-effect-retryable"
    unknownRecord.store.ledger.storedHead.value
    (.advanceExternalOperation unknownRecord.store.ledger.storedHead retryable)

  objects.modify (("immutable:occupied", "sha256:other") :: ·)
  let collisionPrepared : Domain.ExternalOperation.Attempt := {
    operation := ⟨"immutable-collision"⟩
    target := .confirmed "immutable:occupied"
    artifactDigest := "sha256:new"
    state := .prepared }
  let collisionPreparedRecord ← mutate ledger "collision-prepare"
    retryableRecord.store.ledger.storedHead.value
    (.recordExternalOperation retryableRecord.store.ledger.storedHead collisionPrepared)
  let collision := { collisionPrepared with state := .dispatched }
  let collisionDispatchedRecord ← mutate ledger "collision-dispatch"
    collisionPreparedRecord.store.ledger.storedHead.value
    (.advanceExternalOperation collisionPreparedRecord.store.ledger.storedHead collision)
  let conflict ← match ←
      Adapter.ExternalOperation.dispatch ledger port collision.operation with
    | .ok attempt => pure attempt
    | .error error => throw <| IO.userError s!"collision dispatch failed: {repr error}"
  let objectsAfterCollision ← objects.get
  expect (conflict.state == .conflict &&
      collision.target.dispatchIdentity?.bind
        (fun target => objectsAfterCollision.lookup target) == some "sha256:other")
    "pre-existing immutable target was overwritten instead of preserved as conflict"

  objects.modify (("immutable:same-digest", "sha256:same") :: ·)
  let sameDigestPrepared : Domain.ExternalOperation.Attempt := {
    operation := ⟨"immutable-same-digest-precondition-conflict"⟩
    target := .confirmed "immutable:same-digest"
    artifactDigest := "sha256:same"
    remotePrecondition := { expectedArtifactDigest := none }
    state := .prepared }
  let sameDigestPreparedRecord ← mutate ledger "same-digest-conflict-prepare"
    collisionDispatchedRecord.store.ledger.storedHead.value
    (.recordExternalOperation collisionDispatchedRecord.store.ledger.storedHead
      sameDigestPrepared)
  let sameDigestDispatched := { sameDigestPrepared with state := .dispatched }
  let _ ← mutate ledger "same-digest-conflict-dispatch"
    sameDigestPreparedRecord.store.ledger.storedHead.value
    (.advanceExternalOperation sameDigestPreparedRecord.store.ledger.storedHead
      sameDigestDispatched)
  let sameDigestConflict ← match ←
      Adapter.ExternalOperation.dispatch ledger port sameDigestDispatched.operation with
    | .ok attempt => pure attempt
    | .error error =>
        throw <| IO.userError s!"same-digest precondition dispatch failed: {repr error}"
  expect (sameDigestConflict.state == .conflict &&
      sameDigestConflict.observation.any
        (fun observation => observation.artifactDigest == some "sha256:same") &&
      (← dispatches.get) == 1)
    "a same-digest object that violated the prepared precondition became success"

  let fabricated : Domain.ExternalOperation.Attempt := {
    operation := ⟨"never-persisted"⟩
    target := .confirmed "immutable:fabricated"
    artifactDigest := "sha256:fabricated"
    state := .dispatched }
  match ← Adapter.ExternalOperation.dispatch ledger port fabricated.operation with
  | .error .attemptMissing => pure ()
  | other => throw <| IO.userError s!"unpersisted dispatch was authorized: {repr other}"
  let objectsAfterFabricated ← objects.get
  expect (fabricated.target.dispatchIdentity?.bind
      (fun target => objectsAfterFabricated.lookup target) |>.isNone)
    "an unpersisted attempt produced a remote side effect"

def testArtifactBindingsAndRace (root : System.FilePath) : IO Unit := do
  let corruptions := [
    ("deleted", "DELETE FROM artifacts"),
    ("altered", "UPDATE artifacts SET digest='sha3-256:substituted'"),
    ("size", "UPDATE artifacts SET size='999'"),
    ("payload", "UPDATE artifacts SET payload=x'00'"),
    ("extra", "INSERT INTO artifacts (digest,size,payload) VALUES ('sha3-256:extra','1',x'00')")]
  for (name, sql) in corruptions do
    let ledger := root / s!"artifact-table-{name}.sqlite3"
    let artifactRoot := Adapter.SQLite.artifactRoot ledger
    Adapter.SQLite.initializeStore ledger
    let initialized ← bootstrap ledger
    let reference ← Adapter.DurableFilesystem.stage artifactRoot s!"artifact-{name}".toUTF8
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"artifact-{name}"⟩
      target := .confirmed s!"remote:artifact-{name}"
      artifactDigest := reference.digest
      state := .prepared }
    let recorded ← match ← Adapter.SQLite.mutate ledger ⟨s!"record-{name}"⟩
        initialized.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .ok outcome => pure outcome
    | .error error => throw <| IO.userError s!"artifact fixture failed: {repr error}"
    corruptProjection ledger
    let repairPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"artifact repair fixture failed: {repr other}"
    execSql ledger sql
    let corruptBytes ← IO.FS.readBinFile ledger
    match ← Adapter.SQLite.inspectWithArtifacts ledger artifactRoot with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"artifact table corruption {name} was accepted: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted artifact table corruption {name}: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"unrelated-after-{name}"⟩
        recorded.store.ledger.storedHead Application.Service.bootstrapCommand with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"mutation accepted artifact corruption {name}: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"record-{name}"⟩
        recorded.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"exact retry accepted artifact corruption {name}: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == corruptBytes)
      s!"rejected artifact corruption mutation changed ledger bytes: {name}"

  let raceLedger := root / "artifact-race.sqlite3"
  let raceRoot := Adapter.SQLite.artifactRoot raceLedger
  Adapter.SQLite.initializeStore raceLedger
  let initialized ← bootstrap raceLedger
  let reference ← Adapter.DurableFilesystem.stage raceRoot "race artifact".toUTF8
  let attempt : Domain.ExternalOperation.Attempt := {
    operation := ⟨"artifact-race"⟩
    target := .confirmed "remote:artifact-race"
    artifactDigest := reference.digest
    state := .prepared }
  expectFailure
    (Adapter.SQLite.mutateWithHook raceLedger ⟨"artifact-race"⟩
      initialized.store.ledger.storedHead
      (.recordExternalOperation initialized.store.ledger.storedHead attempt)
      [reference] (some raceRoot) (pure ())
      (IO.FS.removeFile (Adapter.DurableFilesystem.objectPath raceRoot reference)))
    "artifact deletion after verification committed"
  expect ((← load raceLedger).ledger.storedHead == initialized.store.ledger.storedHead)
    "artifact race advanced the authoritative ledger"

def testFilesystemArtifactFaults (root : System.FilePath) : IO Unit := do
  for name in ["deleted", "truncated", "replaced"] do
    let ledger := root / s!"artifact-file-{name}.sqlite3"
    let artifactRoot := Adapter.SQLite.artifactRoot ledger
    Adapter.SQLite.initializeStore ledger
    let initialized ← bootstrap ledger
    let reference ← Adapter.DurableFilesystem.stage artifactRoot
      s!"committed artifact {name}".toUTF8
    let attempt : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"artifact-file-{name}"⟩
      target := .confirmed s!"remote:artifact-file-{name}"
      artifactDigest := reference.digest
      state := .prepared }
    let recorded ← match ← Adapter.SQLite.mutate ledger ⟨s!"record-file-{name}"⟩
        initialized.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
      | .ok outcome => pure outcome
      | .error error => throw <| IO.userError s!"file artifact fixture failed: {repr error}"
    corruptProjection ledger
    let repairPlan ← match ← Adapter.SQLite.diagnose ledger with
      | .ok (.projectionRepairRequired plan) => pure plan
      | other => throw <| IO.userError s!"file artifact repair fixture failed: {repr other}"
    let object := Adapter.DurableFilesystem.objectPath artifactRoot reference
    IO.FS.removeFile object
    if name = "truncated" then IO.FS.writeBinFile object ByteArray.empty
    else if name = "replaced" then IO.FS.writeBinFile object "replacement".toUTF8
    let before ← IO.FS.readBinFile ledger
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"normal inspect accepted {name} artifact: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted {name} file artifact: {repr other}"
    let unrelated : Domain.ExternalOperation.Attempt := {
      operation := ⟨s!"unrelated-{name}"⟩
      target := .confirmed s!"remote:unrelated-{name}"
      artifactDigest := "proof:unrelated"
      state := .prepared }
    match ← Adapter.SQLite.mutate ledger ⟨s!"unrelated-{name}"⟩
        recorded.store.ledger.storedHead
        (.recordExternalOperation recorded.store.ledger.storedHead unrelated) with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"unrelated mutation accepted {name} artifact: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"record-file-{name}"⟩
        recorded.store.ledger.storedHead
        (.recordExternalOperation initialized.store.ledger.storedHead attempt)
        [reference] (some artifactRoot) with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"exact retry accepted {name} artifact: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == before)
      s!"artifact file fault changed ledger bytes: {name}"

  let orphanLedger := root / "artifact-orphan.sqlite3"
  Adapter.SQLite.initializeStore orphanLedger
  let _ ← bootstrap orphanLedger
  let _ ← Adapter.DurableFilesystem.stage (Adapter.SQLite.artifactRoot orphanLedger)
    "unreferenced durable object".toUTF8
  let _ ← load orphanLedger

def makeUnsupportedSchema (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.transaction (mode := .immediate) do
    db.exec "UPDATE metadata SET schema_version = '1' WHERE singleton = 1;"

def makeIncompatibleV3 (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.transaction (mode := .immediate) do
    db.exec "UPDATE metadata SET schema_version = '3' WHERE singleton = 1;"

def makePredecessorV2 (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite }
  db.transaction (mode := .immediate) do
    db.exec "
      ALTER TABLE projection_repairs RENAME TO current_projection_repairs;
      CREATE TABLE projection_repairs (
        observed_digest TEXT NOT NULL,
        head_revision TEXT NOT NULL,
        history_digest TEXT NOT NULL,
        adopted_digest TEXT NOT NULL,
        PRIMARY KEY (observed_digest, head_revision, history_digest)
      );
      INSERT INTO projection_repairs
        (observed_digest, head_revision, history_digest, adopted_digest)
      SELECT observed_digest, head_revision, history_digest, adopted_digest
      FROM current_projection_repairs;
      DROP TABLE current_projection_repairs;
      INSERT INTO update_provenance
        (singleton, source_schema, source_digest, backup_digest, backup_size)
      VALUES (1, '1', 'historical-source', 'historical-backup', '0');
      UPDATE metadata SET schema_version = '2' WHERE singleton = 1;"

def removeCoordinator (ledger : System.FilePath) : IO System.FilePath := do
  let coordinator := System.FilePath.mk s!"{ledger}.writer.sqlite3"
  if ← coordinator.pathExists then IO.FS.removeFile coordinator
  return coordinator

def testUpdateInspectionReadOnly (root : System.FilePath) : IO Unit := do
  let check (ledger : System.FilePath) : IO Unit := do
    let coordinator ← removeCoordinator ledger
    let beforeBytes ← IO.FS.readBinFile ledger
    let beforeEntries := (← root.readDir).size
    let _ ← Adapter.Update.inspect ledger
    expect (!(← coordinator.pathExists)) "update inspection created a writer coordinator"
    expect ((← IO.FS.readBinFile ledger) == beforeBytes)
      "update inspection changed ledger bytes"
    expect ((← root.readDir).size == beforeEntries)
      "update inspection changed the directory entry set"
  let current := root / "inspect-current.sqlite3"
  Adapter.SQLite.initializeStore current
  check current
  let predecessor := root / "inspect-predecessor.sqlite3"
  Adapter.SQLite.initializeStore predecessor
  makePredecessorV2 predecessor
  check predecessor
  let unsupported := root / "inspect-unsupported.sqlite3"
  Adapter.SQLite.initializeStore unsupported
  let _ ← bootstrap unsupported
  makeUnsupportedSchema unsupported
  check unsupported

def makeLegacyV5Ledger (ledger : System.FilePath) : IO Unit := do
  Adapter.SQLite.initializeStore ledger
  let artifactPayload := "legacy-v5-publication".toUTF8
  let artifact ← Adapter.DurableFilesystem.stage
    (Adapter.SQLite.artifactRoot ledger) artifactPayload
  let accepted ← match Kernel.Decide.decide Application.Service.bootstrapCommand
      Kernel.Replay.emptyState with
    | .ok accepted => pure accepted
    | .error error => throw <| IO.userError s!"legacy v5 bootstrap failed: {repr error}"
  let (work, activation) ← match accepted.events with
    | [.workInitialized work activation] => pure (work, activation)
    | _ => throw <| IO.userError "legacy v5 bootstrap shape changed"
  let prepared : Adapter.LegacyV5.Attempt := {
    operation := ⟨"legacy-v5-publication"⟩
    artifactDigest := artifact.digest
    state := .prepared }
  let attempt := { prepared with state := .dispatched }
  let events : List Adapter.LegacyV5.Event := [
    .workInitialized work activation,
    .externalOperationRecorded prepared,
    .externalOperationAdvanced attempt
  ]
  let requests : List Adapter.LegacyV5.CanonicalRequest := [
    { command := .initializeWork ⟨0⟩ work activation, artifacts := [] },
    { command := .recordExternalOperation ⟨1⟩ prepared
      artifacts := [(artifact.digest, artifact.size)] },
    { command := .advanceExternalOperation ⟨2⟩ attempt, artifacts := [] }
  ]
  let historyDigest := Adapter.LegacyV5.eventDigest events
  let state : Adapter.LegacyV5.State := {
    revision := ⟨3⟩
    work := [work]
    activations := [activation]
    designs := []
    designApprovals := []
    decompositions := []
    reviewPlans := []
    authorityExceptions := []
    claims := []
    adjudications := []
    reviewFindings := []
    findingVerifications := []
    corrections := []
    authorityTransitions := []
    evidence := []
    externalOperations := [attempt]
    obligations := []
    lifecycle := []
    returnTarget := none }
  let stateDigest := Adapter.LegacyV5.stateDigest state
  let fingerprint : Domain.Projection.ProjectionFingerprint := {
    id := ⟨"projection-3"⟩
    rawDigest := ⟨stateDigest⟩ }
  let projection : Adapter.LegacyV5.ProjectionObservation := {
    fingerprint
    reference := {
      fingerprint
      ledger := ⟨ledger.toString⟩
      revision := ⟨3⟩
      historyDigest := ⟨historyDigest⟩
      stateDigest := ⟨stateDigest⟩ }
    payload := .decoded state }
  let db ← _root_.SQLite.openWith ledger { mode := .readWrite, threading := some .fullmutex }
  db.transaction (mode := .immediate) do
    db.exec "DELETE FROM events; DELETE FROM operations; DELETE FROM projection; DELETE FROM artifacts;"
    let metadata ← db.prepare "
      UPDATE metadata SET schema_version='5', head_revision='3', history_digest=?
      WHERE singleton=1"
    metadata.bindText 1 historyDigest
    metadata.exec
    for (event, index) in events.zipIdx do
      let operationId := s!"legacy-v5-operation-{index + 1}"
      let statement ← db.prepare
        "INSERT INTO events (revision, payload, operation_id) VALUES (?, ?, ?)"
      statement.bindInt64 1 (Int64.ofInt (index + 1))
      statement.bindBlob 2 (toBinary event)
      statement.bindText 3 operationId
      statement.exec
      let request ← match requests[index]? with
        | some request => pure request
        | none => throw <| IO.userError "legacy v5 fixture request is missing"
      let requestPayload := toBinary request
      let payloadDigest ← Adapter.DurableFilesystem.digest requestPayload
      let currentEvents := (events.take (index + 1)).map
        Adapter.LegacyV5.Event.toCurrent
      let currentState ← match Kernel.Replay.replay currentEvents Kernel.Replay.emptyState with
        | .ok verified => pure verified.state
        | .error error =>
            throw (IO.userError s!"legacy v5 fixture replay failed: {repr error}")
      let legacyState ← match
          Adapter.LegacyV5.State.fromCurrent (events.take (index + 1)) currentState with
        | .ok legacy => pure legacy
        | .error error => throw <| IO.userError error
      let resultDigest := Adapter.LegacyV5.stateDigest legacyState
      let receipt : Policy.Update.Receipt := {
        operation := ⟨operationId⟩
        payloadDigest
        resultDigest }
      let operationStatement ← db.prepare "
        INSERT INTO operations
          (operation_id, request_payload, payload_digest, result_digest,
           start_revision, end_revision, history_digest, receipt)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
      operationStatement.bindText 1 operationId
      operationStatement.bindBlob 2 requestPayload
      operationStatement.bindText 3 payloadDigest
      operationStatement.bindText 4 resultDigest
      operationStatement.bindText 5 (toString index)
      operationStatement.bindText 6 (toString (index + 1))
      operationStatement.bindText 7
        (Adapter.LegacyV5.eventDigest (events.take (index + 1)))
      operationStatement.bindBlob 8 (toBinary receipt)
      operationStatement.exec
    let projectionStatement ← db.prepare
      "INSERT INTO projection VALUES (1, '3', ?, ?, ?)"
    projectionStatement.bindText 1 historyDigest
    projectionStatement.bindText 2 stateDigest
    projectionStatement.bindBlob 3 (toBinary projection)
    projectionStatement.exec
    let artifactStatement ← db.prepare
      "INSERT INTO artifacts (digest, size, payload) VALUES (?, ?, ?)"
    artifactStatement.bindText 1 artifact.digest
    artifactStatement.bindText 2 (toString artifact.size)
    artifactStatement.bindBlob 3 artifactPayload
    artifactStatement.exec

def expectLegacyV5Unsupported (ledger : System.FilePath) (reason : String) : IO Unit := do
  match ← Adapter.Update.inspect ledger with
  | .unsupported point =>
      expect (point.schemaVersion == Adapter.SQLite.legacyV5SchemaVersion) reason
  | other => throw <| IO.userError s!"{reason}: {repr other}"

def tamperLegacyV5Projection (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger
    { mode := .readWrite, threading := some .fullmutex }
  db.transaction (mode := .immediate) do
    let query ← db.prepare "SELECT payload FROM projection WHERE singleton=1"
    unless ← query.step do throw <| IO.userError "legacy projection is missing"
    let projection ← match
        fromBinary (α := Adapter.LegacyV5.ProjectionObservation) (← query.columnBlob 0) with
      | .ok projection => pure projection
      | .error error => throw <| IO.userError error
    let state ← match projection.payload with
      | .decoded state => pure state
      | .decodeFailed _ => throw <| IO.userError "legacy projection is not decoded"
    let altered := { state with returnTarget := some ⟨999999⟩ }
    let digest := Adapter.LegacyV5.stateDigest altered
    let fingerprint := { projection.fingerprint with rawDigest := ⟨digest⟩ }
    let alteredProjection : Adapter.LegacyV5.ProjectionObservation := {
      fingerprint
      reference := {
        projection.reference with
        fingerprint
        stateDigest := ⟨digest⟩ }
      payload := .decoded altered }
    let update ← db.prepare
      "UPDATE projection SET state_digest=?, payload=? WHERE singleton=1"
    update.bindText 1 digest
    update.bindBlob 2 (toBinary alteredProjection)
    update.exec

def tamperLegacyV5OperationResult (ledger : System.FilePath) : IO Unit := do
  let db ← _root_.SQLite.openWith ledger
    { mode := .readWrite, threading := some .fullmutex }
  db.transaction (mode := .immediate) do
    let query ← db.prepare
      "SELECT receipt FROM operations WHERE operation_id='legacy-v5-operation-2'"
    unless ← query.step do throw <| IO.userError "legacy operation is missing"
    let receipt ← match
        fromBinary (α := Policy.Update.Receipt) (← query.columnBlob 0) with
      | .ok receipt => pure receipt
      | .error error => throw <| IO.userError error
    let changed := { receipt with resultDigest := "legacy-result-not-derived-from-history" }
    let update ← db.prepare "
      UPDATE operations SET result_digest=?, receipt=?
      WHERE operation_id='legacy-v5-operation-2'"
    update.bindText 1 changed.resultDigest
    update.bindBlob 2 (toBinary changed)
    update.exec

def testLegacyV5CorruptionRejection (root : System.FilePath) : IO Unit := do
  let projection := root / "legacy-v5-projection-tamper.sqlite3"
  makeLegacyV5Ledger projection
  tamperLegacyV5Projection projection
  expectLegacyV5Unsupported projection
    "legacy projection independent of event history was accepted"
  let result := root / "legacy-v5-operation-result-tamper.sqlite3"
  makeLegacyV5Ledger result
  tamperLegacyV5OperationResult result
  expectLegacyV5Unsupported result
    "legacy operation result independent of event history was accepted"
  let artifact := root / "legacy-v5-artifact-missing.sqlite3"
  makeLegacyV5Ledger artifact
  execSql artifact "DELETE FROM artifacts"
  expectLegacyV5Unsupported artifact
    "legacy event artifact authority was accepted without its artifact"
  let durable := root / "legacy-v5-durable-object-missing.sqlite3"
  makeLegacyV5Ledger durable
  let payload := "legacy-v5-publication".toUTF8
  let reference : Adapter.DurableFilesystem.ArtifactRef := {
    digest := ← Adapter.DurableFilesystem.digest payload
    size := payload.size }
  IO.FS.removeFile (Adapter.DurableFilesystem.objectPath
    (Adapter.SQLite.artifactRoot durable) reference)
  expectLegacyV5Unsupported durable
    "legacy durable artifact absence was accepted for update"

def testLegacyV5ExplicitUpdate (root : System.FilePath) : IO Unit := do
  let ledger := root / "legacy-v5.sqlite3"
  let backups := root / "legacy-v5-backups"
  makeLegacyV5Ledger ledger
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan =>
        expect (plan.source.schemaVersion == Adapter.SQLite.legacyV5SchemaVersion &&
          plan.targetVersion == Adapter.SQLite.schemaVersion)
          "legacy v5 update plan changed schema identities"
        pure plan
    | other => throw <| IO.userError s!"legacy v5 update was not planned: {repr other}"
  let receipt ← Adapter.Update.apply ledger backups plan
  expect (receipt.source == plan.source &&
      receipt.target.schemaVersion == Adapter.SQLite.schemaVersion)
    "legacy v5 update receipt changed the planned boundary"
  let store ← load ledger
  let state ← match (Application.Service.status store).value.currentState? with
    | some state => pure state
    | none => throw <| IO.userError "legacy v5 converted state is unavailable"
  let pending ← match state.externalOperations.find?
      (·.operation == ⟨"legacy-v5-publication"⟩) with
    | some attempt => pure attempt
    | none => throw <| IO.userError "legacy v5 external intent was lost"
  expect (pending.target == .unresolved)
    "legacy external intent acquired an invented dispatch target"
  expect (pending.state == .dispatched)
    "legacy response-loss attempt did not preserve its reconciliation-required state"
  let observedLegacy : Adapter.LegacyV5.Attempt := {
    operation := ⟨"legacy-v5-observed-retry"⟩
    artifactDigest := pending.artifactDigest
    state := .retryable
    observation := some {
      identity := "observed-object-is-not-an-authorized-target"
      artifactDigest := none } }
  let observedCurrent := Adapter.LegacyV5.Attempt.toCurrent observedLegacy
  expect (observedCurrent.target == .unresolved &&
      observedCurrent.target.dispatchIdentity?.isNone &&
      observedCurrent.observation.any
        (·.identity == "observed-object-is-not-an-authorized-target") &&
      observedCurrent.wellFormed)
    "legacy observation was promoted into dispatch-target authority"
  let fabricatedSuccess := {
    pending with
    state := .succeeded
    observation := some {
      identity := "fabricated-success-authority"
      artifactDigest := some pending.artifactDigest } }
  match ← Adapter.SQLite.mutate ledger ⟨"unseen-unresolved-success"⟩ ⟨3⟩
      (.advanceExternalOperation ⟨3⟩ fabricatedSuccess) with
  | .error (.rejected _) => pure ()
  | other => throw (IO.userError
      s!"unresolved target authorized fabricated success: {repr other}")
  let fabricatedFailure := {
    pending with
    state := .failed
    observation := none
    disposition := some "fabricated terminal failure" }
  match ← Adapter.SQLite.mutate ledger ⟨"unseen-unresolved-failure"⟩ ⟨3⟩
      (.advanceExternalOperation ⟨3⟩ fabricatedFailure) with
  | .error (.rejected _) => pure ()
  | other => throw (IO.userError
      s!"unresolved target authorized observationless failure: {repr other}")
  match ← Adapter.SQLite.mutate ledger ⟨"legacy-v5-operation-3"⟩ ⟨2⟩
      (.advanceExternalOperation ⟨2⟩ pending) with
  | .ok outcome =>
      expect outcome.exactRetry
        "legacy operation identity did not preserve exact-retry continuity"
  | .error error =>
      throw (IO.userError s!"legacy exact retry was rejected after update: {repr error}")
  match ← Adapter.SQLite.mutate ledger ⟨"legacy-v5-operation-3"⟩ ⟨99⟩
      (.advanceExternalOperation ⟨99⟩ pending) with
  | .error .operationConflict => pure ()
  | other =>
      throw (IO.userError s!"legacy operation identity accepted a changed payload: {repr other}")
  let artifact : Adapter.DurableFilesystem.ArtifactRef := {
    digest := pending.artifactDigest
    size := "legacy-v5-publication".toUTF8.size }
  match ← Adapter.DurableFilesystem.verify (Adapter.SQLite.artifactRoot ledger) artifact with
  | .valid => pure ()
  | other =>
      throw (IO.userError s!"legacy artifact authority was not preserved: {repr other}")
  let remoteObjects ← IO.mkRef ([] : List (String × String))
  let dispatches ← IO.mkRef 0
  let loseResponse ← IO.mkRef false
  let failBeforeEffect ← IO.mkRef false
  let failObservation ← IO.mkRef false
  let port := remotePort {
    objects := remoteObjects
    dispatches
    loseResponse
    failBeforeEffect
    failObservation }
  match ← Adapter.ExternalOperation.dispatch ledger port pending.operation with
  | .error .invalidAttempt => pure ()
  | other => throw <| IO.userError s!"unresolved legacy target was dispatched: {repr other}"
  expect ((← dispatches.get) == 0 && (← remoteObjects.get).isEmpty)
    "legacy v5 conversion produced an external side effect"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source)
    "legacy v5 restore did not recover the exact source point"
  match ← Adapter.Update.inspect ledger with
  | .updateRequired restoredPlan =>
      expect (restoredPlan.source == receipt.source)
        "restored legacy v5 source identity drifted"
  | other => throw <| IO.userError s!"restored legacy v5 state is not recoverable: {repr other}"

def testSchemaFingerprintsAndPredecessorMigration (root : System.FilePath) : IO Unit := do
  let predecessorV2 := root / "predecessor-v2.sqlite3"
  let predecessorV2Backups := root / "predecessor-v2-backups"
  Adapter.SQLite.initializeStore predecessorV2
  let v2Store ← bootstrap predecessorV2
  corruptProjection predecessorV2
  let v2RepairPlan ← match ← Adapter.SQLite.diagnose predecessorV2 with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"predecessor v2 repair fixture failed: {repr other}"
  let v2RepairReceipt ← match ← Adapter.SQLite.repairProjection predecessorV2 v2RepairPlan with
    | .ok receipt => pure receipt
    | .error error => throw <| IO.userError s!"predecessor v2 repair fixture failed: {repr error}"
  makePredecessorV2 predecessorV2
  let v2SourceBytes ← IO.FS.readBinFile predecessorV2
  let v2Plan ← match ← Adapter.Update.inspect predecessorV2 with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"real predecessor v2 was not migratable: {repr other}"
  let v2UpdateReceipt ← Adapter.Update.apply predecessorV2 predecessorV2Backups v2Plan
  expect ((← load predecessorV2) == v2Store.store)
    "predecessor v2 migration changed authoritative state"
  match ← Adapter.SQLite.repairProjection predecessorV2 v2RepairPlan with
  | .ok retry =>
      expect (retry == v2RepairReceipt)
        "predecessor v2 repair receipt changed across v4 migration"
  | .error error =>
      throw <| IO.userError s!"migrated predecessor v2 repair retry failed: {repr error}"
  let v2Restored ← Adapter.Update.restore predecessorV2 predecessorV2Backups v2UpdateReceipt
  expect (v2Restored.restored == v2UpdateReceipt.source)
    "predecessor v2 restore returned the wrong source"
  expect ((← IO.FS.readBinFile predecessorV2) == v2SourceBytes)
    "predecessor v2 restore was not byte-exact"

  let malformedCases := [
    ("extra", "ALTER TABLE events ADD COLUMN unexpected TEXT"),
    ("missing", "DROP TABLE update_provenance"),
    ("forged", "DROP TABLE update_provenance; ALTER TABLE artifacts DROP COLUMN payload"),
    ("constraints", "
      ALTER TABLE events RENAME TO old_events;
      CREATE TABLE events (revision INTEGER, payload BLOB, operation_id TEXT);
      INSERT INTO events SELECT revision, payload, operation_id FROM old_events;
      DROP TABLE old_events;"),
    ("trigger", "CREATE TRIGGER unexpected_trigger BEFORE INSERT ON events
      BEGIN SELECT RAISE(ABORT, 'blocked'); END"),
    ("view", "CREATE VIEW unexpected_view AS SELECT * FROM events"),
    ("index", "CREATE INDEX unexpected_index ON events(operation_id)")]
  for (name, sql) in malformedCases do
    let ledger := root / s!"malformed-v2-{name}.sqlite3"
    Adapter.SQLite.initializeStore ledger
    let healthy ← load ledger
    let repairPlan : Adapter.SQLite.ProjectionRepairPlan := {
      head := {
        ledger := healthy.ledger.id
        revision := healthy.ledger.storedHead
        historyDigest := healthy.ledger.storedHistoryDigest }
      observedDigest := "malformed-schema" }
    execSql ledger sql
    let before ← IO.FS.readBinFile ledger
    match ← Adapter.Update.inspect ledger with
    | .unsupported point =>
        expect (point.schemaVersion == Adapter.SQLite.schemaVersion)
          s!"{name} current identity drifted"
    | other => throw <| IO.userError s!"malformed current schema {name} was accepted: {repr other}"
    match ← Adapter.SQLite.inspect ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"normal inspect accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.inspectWithArtifacts ledger (Adapter.SQLite.artifactRoot ledger) with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"artifact inspect accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.diagnose ledger with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"diagnosis accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.repairProjection ledger repairPlan with
    | .error (.corrupt _) => pure ()
    | other => throw <| IO.userError s!"repair accepted malformed schema {name}: {repr other}"
    match ← Adapter.SQLite.mutate ledger ⟨s!"malformed-{name}"⟩ ⟨0⟩
        Application.Service.bootstrapCommand with
    | .error (.openError (.corrupt _)) => pure ()
    | other => throw <| IO.userError s!"mutation accepted malformed schema {name}: {repr other}"
    expect ((← IO.FS.readBinFile ledger) == before)
      s!"malformed current inspection mutated storage: {name}"

  let predecessorCorruptions := [
    ("constraints", "
      ALTER TABLE events RENAME TO old_events;
      CREATE TABLE events (revision INTEGER, payload BLOB, operation_id TEXT);
      INSERT INTO events SELECT revision, payload, operation_id FROM old_events;
      DROP TABLE old_events;"),
    ("trigger", "CREATE TRIGGER unexpected_trigger BEFORE INSERT ON events
      BEGIN SELECT RAISE(ABORT, 'blocked'); END"),
    ("view", "CREATE VIEW unexpected_view AS SELECT * FROM events"),
    ("index", "CREATE INDEX unexpected_index ON events(operation_id)")]
  for (name, sql) in predecessorCorruptions do
    let malformedPredecessor := root / s!"malformed-predecessor-{name}.sqlite3"
    Adapter.SQLite.initializeStore malformedPredecessor
    makePredecessorV2 malformedPredecessor
    execSql malformedPredecessor sql
    let predecessorBefore ← IO.FS.readBinFile malformedPredecessor
    match ← Adapter.Update.inspect malformedPredecessor with
    | .unsupported point =>
        expect (point.schemaVersion == Adapter.SQLite.predecessorSchemaVersion)
          s!"malformed predecessor identity drifted: {name}"
    | other => throw <| IO.userError s!"malformed predecessor was accepted ({name}): {repr other}"
    expect ((← IO.FS.readBinFile malformedPredecessor) == predecessorBefore)
      s!"malformed predecessor inspection mutated storage: {name}"

def testExplicitUpdateAndRestore (root : System.FilePath) : IO Unit := do
  let ledger := root / "update.sqlite3"
  let backups := root / "backups"
  Adapter.SQLite.initializeStore ledger
  makePredecessorV2 ledger
  let beforeBytes ← IO.FS.readBinFile ledger
  let beforeDigest ← Adapter.DurableFilesystem.digest beforeBytes
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"predecessor schema did not require update: {repr other}"
  let afterInspectDigest ← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile ledger)
  expect (beforeDigest == afterInspectDigest) "update inspection mutated storage"
  let receipt ← Adapter.Update.apply ledger backups plan
  expect (receipt.targetDurability == .confirmed)
    "update adoption durability was not confirmed"
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "published update point drifted"
  | other => throw <| IO.userError s!"update did not publish current storage: {repr other}"
  let targetBytes ← IO.FS.readBinFile ledger
  let arbitrary ← Adapter.DurableFilesystem.stage backups "not a sqlite database".toUTF8
  expectFailure (Adapter.Update.restore ledger backups { receipt with backup := arbitrary })
    "arbitrary restore bytes were adopted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "arbitrary restore bytes changed live storage"
  let unrelatedLedger := root / "unrelated.sqlite3"
  Adapter.SQLite.initializeStore unrelatedLedger
  let unrelated ← Adapter.DurableFilesystem.stage backups (← IO.FS.readBinFile unrelatedLedger)
  expectFailure (Adapter.Update.restore ledger backups { receipt with backup := unrelated })
    "unrelated valid storage was adopted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "unrelated restore changed live storage"
  expectFailure (Adapter.Update.restore ledger backups { receipt with target := receipt.source })
    "stale restore target was accepted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "stale restore changed live storage"
  let forgedLedger := root / "forged-unrelated.sqlite3"
  let unrelatedBackups := root / "forged-unrelated-backups"
  Adapter.SQLite.initializeStore forgedLedger
  makePredecessorV2 forgedLedger
  execSql forgedLedger "INSERT INTO projection_repairs VALUES ('marker','0','marker','marker')"
  let unrelatedPlan ← match ← Adapter.Update.inspect forgedLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"unrelated update fixture failed: {repr other}"
  let unrelatedReceipt ← Adapter.Update.apply forgedLedger unrelatedBackups unrelatedPlan
  let forged : Adapter.Update.Receipt := {
    unrelatedReceipt with target := receipt.target }
  expectFailure (Adapter.Update.restore ledger unrelatedBackups forged)
    "coherently forged restore receipt was accepted"
  expect ((← IO.FS.readBinFile ledger) == targetBytes)
    "forged restore receipt changed live storage"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source && restored.durability == .confirmed)
    "restore did not recover the exact confirmed source image"
  match ← Adapter.Update.inspect ledger with
  | .updateRequired restoredPlan =>
      expect (restoredPlan.source == receipt.source) "restored update plan changed source identity"
  | other => throw <| IO.userError s!"restored old storage was not diagnosed: {repr other}"

  let corruptLedger := root / "corrupt-backup.sqlite3"
  let corruptBackups := root / "corrupt-backups"
  Adapter.SQLite.initializeStore corruptLedger
  makePredecessorV2 corruptLedger
  let corruptPlan ← match ← Adapter.Update.inspect corruptLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"corrupt backup fixture failed: {repr other}"
  let corruptReceipt ← Adapter.Update.apply corruptLedger corruptBackups corruptPlan
  let corruptTarget ← IO.FS.readBinFile corruptLedger
  let corruptBackupPath := Adapter.DurableFilesystem.objectPath corruptBackups corruptReceipt.backup
  IO.FS.removeFile corruptBackupPath
  IO.FS.writeBinFile corruptBackupPath "corrupted backup".toUTF8
  expectFailure (Adapter.Update.restore corruptLedger corruptBackups corruptReceipt)
    "corrupted backup was adopted"
  expect ((← IO.FS.readBinFile corruptLedger) == corruptTarget)
    "corrupted backup changed live storage"

  let unsupportedLedger := root / "unsupported-schema.sqlite3"
  Adapter.SQLite.initializeStore unsupportedLedger
  let _ ← bootstrap unsupportedLedger
  makeUnsupportedSchema unsupportedLedger
  let unsupportedBefore ← IO.FS.readBinFile unsupportedLedger
  match ← Adapter.Update.inspect unsupportedLedger with
  | .unsupported point => expect (point.schemaVersion == 1) "unsupported schema identity drifted"
  | other => throw <| IO.userError s!"unknown schema was not explicit unsupported: {repr other}"
  expect ((← IO.FS.readBinFile unsupportedLedger) == unsupportedBefore)
    "unsupported schema inspection mutated storage"

  let incompatibleLedger := root / "incompatible-v3.sqlite3"
  Adapter.SQLite.initializeStore incompatibleLedger
  let _ ← bootstrap incompatibleLedger
  makeIncompatibleV3 incompatibleLedger
  let incompatibleBefore ← IO.FS.readBinFile incompatibleLedger
  match ← Adapter.Update.inspect incompatibleLedger with
  | .unsupported point =>
      expect (point.schemaVersion == 3)
        "binary-incompatible v3 schema identity drifted"
  | other =>
      throw <| IO.userError
        s!"binary-incompatible v3 ledger was treated as current: {repr other}"
  expect ((← IO.FS.readBinFile incompatibleLedger) == incompatibleBefore)
    "incompatible schema inspection mutated storage"

def postRenameSyncFailureChild (root : System.FilePath) : IO Unit := do
  let ledger := root / "post-rename-uncertain.sqlite3"
  let backups := root / "post-rename-uncertain-backups"
  Adapter.SQLite.initializeStore ledger
  makePredecessorV2 ledger
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"uncertain update fixture failed: {repr other}"
  let receipt ← Adapter.Update.apply ledger backups plan
  expect (receipt.targetDurability == .uncertain)
    "post-rename sync failure was not reported as adopted-but-uncertain"
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "uncertain update was not adopted"
  | other => throw <| IO.userError s!"uncertain update became a rejection: {repr other}"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source && restored.durability == .uncertain)
    "uncertain restore did not report its adopted source"

def testPostRenameSyncFailure (root : System.FilePath) : IO Unit := do
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--post-rename-sync-failure-child", root.toString]
    env := #[("AW_TEST_FAIL_REPLACEMENT_PARENT_FSYNC", some "1")] }
  unless result.exitCode = 0 do
    throw <| IO.userError s!"post-rename sync failure child failed: {result.stderr}"

def freshRootSyncFailureChild (root : System.FilePath) : IO Unit := do
  let payload := "fresh root durability".toUTF8
  let artifactRoot := root / "fresh-artifact-root" / "objects"
  expectFailure (Adapter.DurableFilesystem.stage artifactRoot payload)
    "fresh artifact root parent sync failure was accepted"
  let reference : Adapter.DurableFilesystem.ArtifactRef := {
    digest := ← Adapter.DurableFilesystem.digest payload
    size := payload.size }
  expect (!(← (Adapter.DurableFilesystem.objectPath artifactRoot reference).pathExists))
    "fresh-root sync failure adopted an artifact"

  let ledger := root / "fresh-backup-sync-failure.sqlite3"
  let backups := root / "fresh-backup-root" / "objects"
  Adapter.SQLite.initializeStore ledger
  makePredecessorV2 ledger
  let before ← IO.FS.readBinFile ledger
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired value => pure value
    | other => throw <| IO.userError s!"fresh backup sync fixture failed: {repr other}"
  expectFailure (Adapter.Update.apply ledger backups plan)
    "fresh backup root parent sync failure was accepted"
  expect ((← IO.FS.readBinFile ledger) == before)
    "fresh backup root sync failure changed the authoritative store"

def testFreshRootSyncFailure (root : System.FilePath) : IO Unit := do
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--fresh-root-sync-failure-child", root.toString]
    env := #[("AW_TEST_FAIL_DIRECTORY_PARENT_FSYNC", some "1")] }
  unless result.exitCode = 0 do
    throw <| IO.userError s!"fresh-root sync failure child failed: {result.stderr}"

def updateCrashChild (ledger backups : System.FilePath) : IO Unit := do
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"update crash fixture failed: {repr other}"
  let _ ← Adapter.Update.applyWithHook ledger backups plan (IO.Process.forceExit 86)
  throw <| IO.userError "update replacement crash failpoint returned"

def restoreCrashChild (ledger backups : System.FilePath) (sourceDigest backupDigest : String)
    (backupSize : Nat) (targetDigest : String) : IO Unit := do
  let receipt : Adapter.Update.Receipt := {
    source := { schemaVersion := Adapter.SQLite.predecessorSchemaVersion, digest := sourceDigest }
    backup := { digest := backupDigest, size := backupSize }
    target := { schemaVersion := Adapter.SQLite.schemaVersion, digest := targetDigest }
    targetDurability := .uncertain }
  let _ ← Adapter.Update.restoreWithHook ledger backups receipt (IO.Process.forceExit 86)
  throw <| IO.userError "restore replacement crash failpoint returned"

def testReplacementCrashReconciliation (root : System.FilePath) : IO Unit := do
  let ledger := root / "replacement-crash.sqlite3"
  let backups := root / "replacement-crash-backups"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  makePredecessorV2 ledger
  let plan ← match ← Adapter.Update.inspect ledger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"replacement crash plan failed: {repr other}"
  let applyCrash ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--update-replacement-crash-child", ledger.toString, backups.toString] }
  expect (applyCrash.exitCode == 86) "update replacement crash did not reach failpoint"
  let receipt ← Adapter.Update.apply ledger backups plan
  expect (receipt.targetDurability == .uncertain)
    "post-crash update retry did not recover the durable uncertain receipt"
  match ← Adapter.Update.inspect ledger with
  | .current point => expect (point == receipt.target) "reconciled update target drifted"
  | other => throw <| IO.userError s!"reconciled update is not current: {repr other}"

  let restoreCrash ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--restore-replacement-crash-child", ledger.toString, backups.toString,
      receipt.source.digest, receipt.backup.digest, toString receipt.backup.size,
      receipt.target.digest] }
  expect (restoreCrash.exitCode == 86) "restore replacement crash did not reach failpoint"
  let restored ← Adapter.Update.restore ledger backups receipt
  expect (restored.restored == receipt.source && restored.durability == .uncertain)
    "post-crash restore retry did not recover the durable uncertain receipt"
  expect ((← Adapter.DurableFilesystem.digest (← IO.FS.readBinFile ledger)) =
      receipt.source.digest) "reconciled restore bytes drifted"

partial def awaitWriterBoundary (reached : IO.Ref Bool) (remaining : Nat := 5000) : IO Unit := do
  if ← reached.get then return
  if remaining = 0 then
    throw <| IO.userError "replacement-race writer did not reach the lock boundary"
  IO.sleep 1
  awaitWriterBoundary reached (remaining - 1)

def replacementAttempt (operation : String) : Domain.ExternalOperation.Attempt := {
  operation := ⟨operation⟩
  target := .confirmed s!"remote:{operation}"
  artifactDigest := s!"proof:{operation}"
  state := .prepared }

def assertMutationRaceOutcome (ledger : System.FilePath)
    (task : Task (Except IO.Error
      (Except Adapter.SQLite.MutationError Adapter.SQLite.MutationOutcome))) : IO Unit := do
  match ← IO.ofExcept task.get with
  | .ok outcome =>
      expect ((← load ledger) == outcome.store)
        "acknowledged replacement-race mutation was absent from live path"
  | .error _ => pure ()

def assertRepairRaceOutcome (ledger : System.FilePath)
    (task : Task (Except IO.Error
      (Except Adapter.SQLite.OpenError Adapter.SQLite.ProjectionRepairReceipt))) : IO Unit := do
  match ← IO.ofExcept task.get with
  | .ok _ =>
      match ← Adapter.SQLite.inspect ledger with
      | .ok _ => pure ()
      | other => throw <| IO.userError s!"acknowledged repair was absent from live path: {repr other}"
  | .error _ => pure ()

def testReplacementWriterRaces (root : System.FilePath) : IO Unit := do
  let updateMutationLedger := root / "update-mutation-race.sqlite3"
  let updateMutationBackups := root / "update-mutation-race-backups"
  Adapter.SQLite.initializeStore updateMutationLedger
  let updateMutationStore ← bootstrap updateMutationLedger
  makePredecessorV2 updateMutationLedger
  let updateMutationPlan ← match ← Adapter.Update.inspect updateMutationLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"update/mutation race plan failed: {repr other}"
  let updateMutationReached ← IO.mkRef false
  let updateMutationPending ← IO.mkRef (none : Option
    (Task (Except IO.Error (Except Adapter.SQLite.MutationError Adapter.SQLite.MutationOutcome))))
  let updateMutationName := "writer-after-update"
  let _ ← Adapter.Update.applyWithLockHook updateMutationLedger updateMutationBackups
    updateMutationPlan (do
      let task ← IO.asTask <| Adapter.SQLite.mutateWithLockHook updateMutationLedger
        ⟨updateMutationName⟩ updateMutationStore.store.ledger.storedHead
        (.recordExternalOperation updateMutationStore.store.ledger.storedHead
          (replacementAttempt updateMutationName)) [] none (updateMutationReached.set true)
      updateMutationPending.set (some task)
      awaitWriterBoundary updateMutationReached) (pure ())
  let updateMutationTask ← match ← updateMutationPending.get with
    | some task => pure task
    | none => throw <| IO.userError "update/mutation race task was not started"
  assertMutationRaceOutcome updateMutationLedger updateMutationTask

  let restoreMutationLedger := root / "restore-mutation-race.sqlite3"
  let restoreMutationBackups := root / "restore-mutation-race-backups"
  Adapter.SQLite.initializeStore restoreMutationLedger
  let restoreMutationStore ← bootstrap restoreMutationLedger
  makePredecessorV2 restoreMutationLedger
  let restoreMutationPlan ← match ← Adapter.Update.inspect restoreMutationLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"restore/mutation race plan failed: {repr other}"
  let restoreMutationReceipt ← Adapter.Update.apply restoreMutationLedger
    restoreMutationBackups restoreMutationPlan
  let restoreMutationReached ← IO.mkRef false
  let restoreMutationPending ← IO.mkRef (none : Option
    (Task (Except IO.Error (Except Adapter.SQLite.MutationError Adapter.SQLite.MutationOutcome))))
  let restoreMutationName := "writer-after-restore"
  let _ ← Adapter.Update.restoreWithLockHook restoreMutationLedger restoreMutationBackups
    restoreMutationReceipt (do
      let task ← IO.asTask <| Adapter.SQLite.mutateWithLockHook restoreMutationLedger
        ⟨restoreMutationName⟩ restoreMutationStore.store.ledger.storedHead
        (.recordExternalOperation restoreMutationStore.store.ledger.storedHead
          (replacementAttempt restoreMutationName)) [] none (restoreMutationReached.set true)
      restoreMutationPending.set (some task)
      awaitWriterBoundary restoreMutationReached) (pure ())
  let restoreMutationTask ← match ← restoreMutationPending.get with
    | some task => pure task
    | none => throw <| IO.userError "restore/mutation race task was not started"
  assertMutationRaceOutcome restoreMutationLedger restoreMutationTask

  let updateRepairLedger := root / "update-repair-race.sqlite3"
  let updateRepairBackups := root / "update-repair-race-backups"
  Adapter.SQLite.initializeStore updateRepairLedger
  let _ ← bootstrap updateRepairLedger
  corruptProjection updateRepairLedger
  let updateRepairPlan ← match ← Adapter.SQLite.diagnose updateRepairLedger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"update/repair fixture failed: {repr other}"
  match ← Adapter.SQLite.repairProjection updateRepairLedger updateRepairPlan with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"update/repair fixture repair failed: {repr error}"
  makePredecessorV2 updateRepairLedger
  let updatePlan ← match ← Adapter.Update.inspect updateRepairLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"update/repair race plan failed: {repr other}"
  let updateRepairReached ← IO.mkRef false
  let updateRepairPending ← IO.mkRef (none : Option
    (Task (Except IO.Error
      (Except Adapter.SQLite.OpenError Adapter.SQLite.ProjectionRepairReceipt))))
  let _ ← Adapter.Update.applyWithLockHook updateRepairLedger updateRepairBackups updatePlan (do
    let task ← IO.asTask <| Adapter.SQLite.repairProjectionWithLockHook updateRepairLedger
      updateRepairPlan (updateRepairReached.set true) (pure ())
    updateRepairPending.set (some task)
    awaitWriterBoundary updateRepairReached) (pure ())
  let updateRepairTask ← match ← updateRepairPending.get with
    | some task => pure task
    | none => throw <| IO.userError "update/repair race task was not started"
  assertRepairRaceOutcome updateRepairLedger updateRepairTask

  let restoreRepairLedger := root / "restore-repair-race.sqlite3"
  let restoreRepairBackups := root / "restore-repair-race-backups"
  Adapter.SQLite.initializeStore restoreRepairLedger
  let _ ← bootstrap restoreRepairLedger
  corruptProjection restoreRepairLedger
  let restoreRepairPlan ← match ← Adapter.SQLite.diagnose restoreRepairLedger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"restore/repair fixture failed: {repr other}"
  match ← Adapter.SQLite.repairProjection restoreRepairLedger restoreRepairPlan with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"restore/repair fixture repair failed: {repr error}"
  makePredecessorV2 restoreRepairLedger
  let restoreRepairUpdatePlan ← match ← Adapter.Update.inspect restoreRepairLedger with
    | .updateRequired plan => pure plan
    | other => throw <| IO.userError s!"restore/repair update plan failed: {repr other}"
  let restoreRepairReceipt ← Adapter.Update.apply restoreRepairLedger restoreRepairBackups
    restoreRepairUpdatePlan
  let restoreRepairReached ← IO.mkRef false
  let restoreRepairPending ← IO.mkRef (none : Option
    (Task (Except IO.Error
      (Except Adapter.SQLite.OpenError Adapter.SQLite.ProjectionRepairReceipt))))
  let _ ← Adapter.Update.restoreWithLockHook restoreRepairLedger restoreRepairBackups
    restoreRepairReceipt (do
      let task ← IO.asTask <| Adapter.SQLite.repairProjectionWithLockHook restoreRepairLedger
        restoreRepairPlan (restoreRepairReached.set true) (pure ())
      restoreRepairPending.set (some task)
      awaitWriterBoundary restoreRepairReached) (pure ())
  let restoreRepairTask ← match ← restoreRepairPending.get with
    | some task => pure task
    | none => throw <| IO.userError "restore/repair race task was not started"
  assertRepairRaceOutcome restoreRepairLedger restoreRepairTask

def repairCrashChild (ledger : System.FilePath) (ledgerId : String) (revision : Nat)
    (historyDigest observedDigest : String) : IO Unit := do
  let plan : Adapter.SQLite.ProjectionRepairPlan := {
    head := {
      ledger := ⟨ledgerId⟩
      revision := ⟨revision⟩
      historyDigest := ⟨historyDigest⟩ }
    observedDigest }
  let _ ← Adapter.SQLite.repairProjectionWithHook ledger plan (IO.Process.forceExit 86)
  throw <| IO.userError "projection repair crash failpoint returned"

def testProjectionRepairCrashRetry (root : System.FilePath) : IO Unit := do
  let ledger := root / "projection-repair-crash.sqlite3"
  Adapter.SQLite.initializeStore ledger
  let _ ← bootstrap ledger
  corruptProjection ledger
  let plan ← match ← Adapter.SQLite.diagnose ledger with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"repair crash plan unavailable: {repr other}"
  let result ← IO.Process.output {
    cmd := ".lake/build/bin/storage-laws"
    args := #["--repair-crash-child", ledger.toString, plan.head.ledger.value,
      toString plan.head.revision.value, plan.head.historyDigest.value, plan.observedDigest] }
  expect (result.exitCode == 86) "projection repair crash did not reach failpoint"
  let recovered ← match ← Adapter.SQLite.repairProjection ledger plan with
    | .ok receipt => pure receipt
    | .error error => throw <| IO.userError s!"projection repair retry failed: {repr error}"
  match ← Adapter.SQLite.repairProjection ledger plan with
  | .ok receipt => expect (receipt == recovered) "projection repair canonical receipt drifted"
  | .error error => throw <| IO.userError s!"second projection repair retry failed: {repr error}"
  let changed := { plan with observedDigest := plan.observedDigest ++ "-changed" }
  match ← Adapter.SQLite.repairProjection ledger changed with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"changed projection repair plan was accepted: {repr other}"
  let changedLedger := { plan with head := { plan.head with ledger := ⟨"different-ledger"⟩ } }
  match ← Adapter.SQLite.repairProjection ledger changedLedger with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"changed-ledger projection repair was accepted: {repr other}"
  let unseen := { plan with head := { plan.head with revision := plan.head.revision.next } }
  match ← Adapter.SQLite.repairProjection ledger unseen with
  | .error (.corrupt _) => pure ()
  | other => throw <| IO.userError s!"unseen stale projection repair was accepted: {repr other}"

  let ledgerB := root / "projection-repair-other-ledger.sqlite3"
  Adapter.SQLite.initializeStore ledgerB
  let _ ← bootstrap ledgerB
  corruptProjection ledgerB
  let planB ← match ← Adapter.SQLite.diagnose ledgerB with
    | .ok (.projectionRepairRequired plan) => pure plan
    | other => throw <| IO.userError s!"other-ledger repair plan unavailable: {repr other}"
  execSql ledgerB s!"
    ATTACH DATABASE '{ledger}' AS source;
    INSERT INTO projection_repairs SELECT * FROM source.projection_repairs;
    DETACH DATABASE source;"
  let receiptB ← match ← Adapter.SQLite.repairProjection ledgerB planB with
    | .ok receipt => pure receipt
    | .error error => throw <| IO.userError s!"other-ledger repair rejected: {repr error}"
  expect (receiptB.plan.head.ledger == planB.head.ledger &&
      receiptB.plan.head.ledger != recovered.plan.head.ledger)
    "repair receipt crossed ledger identities"
  match ← Adapter.SQLite.repairProjection ledgerB planB with
  | .ok retry => expect (retry == receiptB) "other-ledger exact retry drifted"
  | .error error => throw <| IO.userError s!"other-ledger exact retry failed: {repr error}"

def main (args : List String) : IO Unit :=
  match args with
  | ["--crash-child", ledger] => crashChild ledger
  | ["--post-rename-sync-failure-child", root] => postRenameSyncFailureChild root
  | ["--fresh-root-sync-failure-child", root] => freshRootSyncFailureChild root
  | ["--update-replacement-crash-child", ledger, backups] =>
      updateCrashChild ledger backups
  | ["--restore-replacement-crash-child", ledger, backups, sourceDigest,
      backupDigest, backupSize, targetDigest] =>
      match backupSize.toNat? with
      | some size => restoreCrashChild ledger backups sourceDigest backupDigest size targetDigest
      | none => throw <| IO.userError "invalid restore crash backup size"
  | ["--repair-crash-child", ledger, ledgerId, revision, historyDigest, observedDigest] =>
      match revision.toNat? with
      | some value => repairCrashChild ledger ledgerId value historyDigest observedDigest
      | none => throw <| IO.userError "invalid repair crash revision"
  | [] => IO.FS.withTempDir fun root => do
    testRecoveryAndRetry root
    testWorkContractPersistence root
    testOperationJournalCorruption root
    testCrashRollback root
    testProcessCrashRecovery root
    testSingleWriter root
    testConcurrentInspection root
    testRelativeReplacement root
    testReadOnlyFaultDetection root
    testArtifactsAndEvidence root
    testExternalOperationReconciliation root
    testExternalOperationBoundaryFaults root
    testCorrectionPersistence root
    testReviewPurposePersistence root
    testFindingAttemptPersistence root
    testArtifactBindingsAndRace root
    testFilesystemArtifactFaults root
    testUpdateInspectionReadOnly root
    testLegacyV5CorruptionRejection root
    testLegacyV5ExplicitUpdate root
    testSchemaFingerprintsAndPredecessorMigration root
    testExplicitUpdateAndRestore root
    testPostRenameSyncFailure root
    testFreshRootSyncFailure root
    testReplacementCrashReconciliation root
    testReplacementWriterRaces root
    testProjectionRepairCrashRetry root
    IO.println "storage laws: pass"
  | _ => throw <| IO.userError "unsupported storage-laws arguments"

end AgentWorkbench.Tests.StorageLaws

def main (args : List String) : IO Unit :=
  AgentWorkbench.Tests.StorageLaws.main args
