import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.StoreRecovery
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbench.Adapter.ManagedOutput

namespace AgentWorkbenchTest.ManagedRecovery

open AgentWorkbench AgentWorkbenchTest

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let database := root / "state.db"
    let store ← Store.open database
    let output := root / "proof" / ".lake" / "build"
    IO.FS.createDirAll output
    IO.FS.writeFile (output / "baseline") "original"
    let layouts ← ProofBuild.outputLayouts
      [{ directory := output, existed := true, parentExisted := true }] "crash"
    let layout ← match layouts.head? with
      | some value => pure value
      | none => throw (IO.userError "managed output layout was not created")
    IO.FS.rename layout.original layout.backup
    IO.FS.createDirAll layout.isolated
    IO.FS.writeFile (layout.isolated / "partial") "interrupted"
    let manifest : ProofBuild.ManagedOutputManifest := { layouts }
    let connection ← AgentWorkbench.SQLite.open database
    AgentWorkbench.SQLite.execute connection
      "INSERT INTO managed_operations(
         operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
       ) VALUES (?1, ?2, ?3, ?4, NULL)"
      #["proof-crash", "0", "restore-proof-outputs", (Lean.toJson manifest).compress]
    Store.recoverManagedOperations root store
    expect ((← IO.FS.readFile (output / "baseline")) == "original")
      "managed operation recovery did not restore the pre-operation output"
    expect (!(← layout.backup.pathExists) && !(← layout.isolated.pathExists))
      "managed operation recovery left backup or isolated output"
    let rows ← AgentWorkbench.SQLite.queryScalar connection
      "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations" #[]
    expect (rows == "0") "managed operation recovery did not clear its durable journal"
    Store.recoverManagedOperations root store
    expect ((← IO.FS.readFile (output / "baseline")) == "original")
      "managed operation recovery was not idempotent"
    let commandOutput := root / "artifact.txt"
    IO.FS.writeFile commandOutput "old"
    let baseline ← ManagedOutput.capture root "file:artifact.txt"
    IO.FS.writeFile commandOutput "partial"
    AgentWorkbench.SQLite.execute connection
      "INSERT INTO managed_operations(
         operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
       ) VALUES (?1, ?2, ?3, ?4, NULL)"
      #["command-crash", "0", "retain-command-output", (Lean.toJson baseline).compress]
    Store.recoverManagedOperations root store
    expect ((← IO.FS.readFile commandOutput) == "old")
      "uncommitted command output was not restored"
    IO.FS.writeFile commandOutput "committed"
    AgentWorkbench.SQLite.execute connection
      "INSERT INTO managed_operations(
         operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
       ) VALUES (?1, ?2, ?3, ?4, 0)"
      #["command-committed", "0", "retain-command-output", (Lean.toJson baseline).compress]
    Store.recoverManagedOperations root store
    expect ((← IO.FS.readFile commandOutput) == "committed")
      "committed command output was rolled back"

end AgentWorkbenchTest.ManagedRecovery
