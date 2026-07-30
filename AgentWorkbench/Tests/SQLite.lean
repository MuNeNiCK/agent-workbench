import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Tests.Support

namespace AgentWorkbench.Tests.SQLite

open AgentWorkbench
open AgentWorkbench.Tests

def expectOpen (result : Except Adapter.SQLite.OpenError
    Adapter.SQLite.Snapshot) (message : String) : IO Adapter.SQLite.Snapshot :=
  match result with
  | .ok snapshot => pure snapshot
  | .error error => throw <| IO.userError s!"{message}: {repr error}"

def expectMutation (result : Except Adapter.SQLite.MutationError
    Adapter.SQLite.Snapshot) (message : String) : IO Adapter.SQLite.Snapshot :=
  match result with
  | .ok snapshot => pure snapshot
  | .error error => throw <| IO.userError s!"{message}: {repr error}"

def testPersistence : IO Unit := do
  let nonce ← IO.monoMsNow
  let root : System.FilePath :=
    System.FilePath.mk s!"/tmp/agent-workbench-sqlite-tests-{nonce}"
  IO.FS.createDirAll root
  try
    let path := root / "state.sqlite3"
    let initialized ← expectOpen
      (← Adapter.SQLite.initializeStore path "instance-a" "initialize"
        initialState)
      "initialization failed"
    expect (initialized.revision == 0 && initialized.state == initialState)
      "initial state was not committed exactly"
    let reopened ← expectOpen (← Adapter.SQLite.inspect path)
      "reopen failed"
    expect (reopened == initialized)
      "reopen did not decode the exact committed state"
    let retriedInit ← expectOpen
      (← Adapter.SQLite.initializeStore path "instance-a" "initialize"
        initialState)
      "exact initialization retry failed"
    expect (retriedInit == initialized)
      "exact initialization retry changed durable state"

    let profileDecision :=
      decision "stored-profile" "Persist the exact Command Profile."
    let withProfile ← unwrap
      (AgentWorkbench.Kernel.recordCommandProfile initialized.state
        profileDecision.source (some profileDecision) "stored-check"
        "verify persisted state" .project ["lake", "test"] none .required)
      "stored Command Profile fixture failed"
    let withKPT ← unwrap
      (AgentWorkbench.Kernel.recordKPT withProfile profileDecision.source
        (some profileDecision) "stored-lesson" .keep .project
        "The selected profile survives restart." none)
      "stored KPT fixture failed"
    let memoryCommitted ← expectMutation
      (← Adapter.SQLite.mutate path "project-memory" "project-memory"
        (some initialized.storeId) (some initialized.revision)
        (fun _ => .ok withKPT))
      "Command Profile and KPT persistence failed"
    let memoryReopened ← expectOpen (← Adapter.SQLite.inspect path)
      "Command Profile and KPT reopen failed"
    expect (memoryReopened == memoryCommitted &&
        memoryReopened.state.commandProfiles == withKPT.commandProfiles &&
        memoryReopened.state.kpt == withKPT.kpt)
      "SQLite did not preserve exact Command Profile and KPT facts"

    let committed ← expectMutation
      (← Adapter.SQLite.mutate path "neutral-change" "change"
        (some memoryCommitted.storeId) (some memoryCommitted.revision)
        (fun state =>
          .ok { state with
            design :=
              { effects :=
                  state.design.effects ++
                    [{ source := source "stored-question" .repository
                       content := .nonAuthoritative
                         { kind := .question
                           statement := "Which external value is current?" } }] } }))
      "mutation failed"
    expect (committed.revision == 2)
      "accepted mutation was not committed atomically"
    let exactRetry ← expectMutation
      (← Adapter.SQLite.mutate path "neutral-change" "change"
        (some memoryCommitted.storeId) (some memoryCommitted.revision)
        (fun state => .ok state))
      "exact mutation retry failed"
    expect (exactRetry == committed)
      "exact retry duplicated or changed the committed mutation"

    match ← Adapter.SQLite.mutate path "neutral-change" "different-intent"
        (some memoryCommitted.storeId) (some memoryCommitted.revision)
        (fun state => .ok state) with
    | .error .intentConflict => pure ()
    | other =>
        throw <| IO.userError s!"changed intent reused a receipt: {repr other}"
    match ← Adapter.SQLite.mutate path "stale-operation" "stale"
        (some initialized.storeId) (some 0) (fun state => .ok state) with
    | .error .stale => pure ()
    | other =>
        throw <| IO.userError s!"stale revision was retargeted: {repr other}"
    match ← Adapter.SQLite.mutate path "rejected-operation" "reject"
        (some initialized.storeId) (some committed.revision)
        (fun _ => .error "selected transition rejected") with
    | .error (.rejected _) => pure ()
    | other =>
        throw <| IO.userError s!"rejected transition was committed: {repr other}"
    let afterRejection ← expectOpen (← Adapter.SQLite.inspect path)
      "inspection after rejection failed"
    expect (afterRejection == committed)
      "rejected transition changed durable state"

    let uncertainPath := root / "uncertain.sqlite3"
    let uncertainInitial ← expectOpen
      (← Adapter.SQLite.initializeStore uncertainPath "uncertain-store"
        "initialize-uncertain" initialState)
      "uncertain-result fixture initialization failed"
    let controlled ← _root_.SQLite.openWith uncertainPath
      { mode := .readWriteCreate, threading := some .fullmutex }
      (busyTimeoutMs := 5000)
    controlled.exec "
      CREATE TRIGGER obscure_uncertain_receipt
      AFTER INSERT ON operations
      WHEN NEW.token = 'uncertain-operation'
      BEGIN
        UPDATE operations SET intent = 'temporarily-unavailable'
        WHERE token = NEW.token;
      END;"
    match ← Adapter.SQLite.mutate uncertainPath "uncertain-operation"
        "uncertain-change" (some uncertainInitial.storeId)
        (some uncertainInitial.revision) (fun state => .ok state) with
    | .error .uncertain => pure ()
    | other =>
        throw <| IO.userError
          s!"post-commit verification failure was not uncertain: {repr other}"
    let committedUncertain ← expectOpen
      (← Adapter.SQLite.inspect uncertainPath)
      "uncertain commit inspection failed"
    expect (committedUncertain.revision == uncertainInitial.revision + 1 &&
        committedUncertain.state == uncertainInitial.state)
      "uncertain commit duplicated or lost its durable state"
    controlled.exec "
      DROP TRIGGER obscure_uncertain_receipt;
      UPDATE operations SET intent = 'uncertain-change'
      WHERE token = 'uncertain-operation';"
    let reconciled ← expectMutation
      (← Adapter.SQLite.mutate uncertainPath "uncertain-operation"
        "uncertain-change" (some uncertainInitial.storeId)
        (some uncertainInitial.revision) (fun state => .ok state))
      "uncertain commit retry did not reconcile"
    expect (reconciled == committedUncertain)
      "uncertain commit retry applied the transition twice"

    let replacement := root / "replacement.sqlite3"
    let replacementSnapshot ← expectOpen
      (← Adapter.SQLite.initializeStore replacement "instance-b" "initialize-b"
        initialState)
      "replacement fixture initialization failed"
    IO.FS.removeFile path
    IO.FS.rename replacement path
    match ← Adapter.SQLite.mutate path "after-replacement" "replace"
        (some committed.storeId) (some committed.revision)
        (fun state => .ok state) with
    | .error .stale => pure ()
    | other =>
        throw <| IO.userError
          s!"repository replacement was overwritten: {repr other}"
    let selectedReplacement ← expectOpen (← Adapter.SQLite.inspect path)
      "replacement inspection failed"
    expect (selectedReplacement.storeId == replacementSnapshot.storeId)
      "repository-selected replacement identity was not retained"
  finally
    IO.FS.removeDirAll root

def run : IO Unit := do
  testPersistence
  IO.println "sqlite tests: pass"

end AgentWorkbench.Tests.SQLite
