import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.StoreRecovery
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbench.Adapter.ManagedOutput

namespace AgentWorkbenchTest.ManagedRecovery

open AgentWorkbench AgentWorkbenchTest

private def rejects (action : IO α) : IO Bool :=
  try
    let _ ← action
    pure false
  catch _ => pure true

private def createDanglingSymlink
    (target link : System.FilePath) : IO Unit := do
  let result ← IO.Process.output { cmd := "ln", args := #["-s", target.toString, link.toString] }
  unless result.exitCode == 0 do
    throw (IO.userError s!"could not create dangling symlink: {result.stderr}")

def run : IO Unit :=
  IO.FS.withTempDir fun sandbox => do
    let root := sandbox / "project"
    let outsideDirectory := sandbox / "outside"
    IO.FS.createDirAll root
    let database := root / "state.db"
    let store ← Store.open database
    let product := root / "product.txt"
    IO.FS.writeFile product "product remains intact"
    let productBefore ← IO.FS.readBinFile product
    let connection ← AgentWorkbench.SQLite.open database
    let stateRevision ← AgentWorkbench.SQLite.queryScalar connection
      "SELECT CAST(state_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
    for identity in [
        "tree:.", "tree:.agent-workbench", "file:.agent-workbench/state.db",
        "tree:.agents", "file:.agents/skills/agent-workbench/SKILL.md",
        "tree:.git", "file:.git/config"] do
      expect (← rejects (ManagedOutput.capture root identity))
        s!"ManagedOutput accepted protected scope {identity}"
      expect ((← IO.FS.readBinFile product) == productBefore)
        s!"protected-scope rejection changed product content for {identity}"
      let currentRevision ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(state_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
      expect (currentRevision == stateRevision)
        s!"protected-scope rejection changed state revision for {identity}"
    let legitimateTree := root / "product-output"
    IO.FS.createDirAll legitimateTree
    IO.FS.writeFile (legitimateTree / "artifact.txt") "ordinary product output"
    let legitimateTreeBaseline ← ManagedOutput.capture root "tree:product-output"
    expect (legitimateTreeBaseline.existed && legitimateTreeBaseline.kind == .tree)
      "ManagedOutput rejected an ordinary product tree"
    let legitimateDotFile := root / ".gitignore"
    IO.FS.writeFile legitimateDotFile "dist/\n"
    let legitimateFileBaseline ← ManagedOutput.capture root "file:.gitignore"
    expect (legitimateFileBaseline.existed && legitimateFileBaseline.kind == .file)
      "ManagedOutput rejected an ordinary dotfile"
    if !System.Platform.isWindows then
      IO.FS.createDirAll outsideDirectory
      let outsideSentinel := outsideDirectory / "sentinel.txt"
      IO.FS.writeFile outsideSentinel "outside remains intact"
      let outsideBefore ← IO.FS.readBinFile outsideSentinel
      let leafLink := root / "dangling-leaf"
      let leafReferent := outsideDirectory / "missing-leaf"
      createDanglingSymlink leafReferent leafLink
      expect (← rejects (ManagedOutput.capture root "file:dangling-leaf"))
        "ManagedOutput accepted a dangling leaf symlink"
      expect (!(← leafReferent.pathExists) &&
          (← IO.FS.readBinFile outsideSentinel) == outsideBefore)
        "dangling leaf rejection changed content outside the project"
      let intermediateLink := root / "dangling-intermediate"
      let intermediateReferent := outsideDirectory / "missing-directory"
      createDanglingSymlink intermediateReferent intermediateLink
      expect (← rejects (ManagedOutput.capture root "file:dangling-intermediate/output.txt"))
        "ManagedOutput accepted a dangling intermediate symlink"
      expect (!(← intermediateReferent.pathExists) &&
          (← IO.FS.readBinFile outsideSentinel) == outsideBefore)
        "dangling intermediate rejection changed content outside the project"
      let afterDanglingCaptureRevision ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(state_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
      expect (afterDanglingCaptureRevision == stateRevision)
        "dangling managed-output rejection changed state revision"

      -- Recovery must retain its journal when the managed leaf has been replaced by a dangling
      -- link. Retrying after the unsafe entry is removed must restore the durable baseline.
      let recoveryOutput := root / "recovery-output.txt"
      IO.FS.writeFile recoveryOutput "recovery baseline"
      let recoveryBaseline ← ManagedOutput.capture root "file:recovery-output.txt"
      IO.FS.removeFile recoveryOutput
      let recoveryReferent := outsideDirectory / "missing-recovery-output.txt"
      createDanglingSymlink recoveryReferent recoveryOutput
      AgentWorkbench.SQLite.execute connection
        "INSERT INTO managed_operations(
           operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
         ) VALUES (?1, ?2, ?3, ?4, NULL)"
        #["dangling-command-crash", stateRevision, "retain-command-output",
          (Lean.toJson recoveryBaseline).compress]
      expect (← rejects (Store.recoverManagedOperations root store))
        "interrupted recovery followed a dangling managed-output symlink"
      expect (!(← recoveryReferent.pathExists) &&
          (← IO.FS.readBinFile outsideSentinel) == outsideBefore)
        "refused dangling recovery changed content outside the project"
      let danglingRows ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations
         WHERE operation_id = 'dangling-command-crash'" #[]
      expect (danglingRows == "1")
        "refused dangling recovery discarded its durable recovery marker"
      let afterDanglingRecoveryRevision ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(state_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
      expect (afterDanglingRecoveryRevision == stateRevision)
        "refused dangling recovery changed state revision"
      IO.FS.removeFile recoveryOutput
      IO.FS.writeFile recoveryOutput "partial local output"
      Store.recoverManagedOperations root store
      expect ((← IO.FS.readFile recoveryOutput) == "recovery baseline")
        "safe retry did not restore the durable managed-output baseline"
      let danglingRowsAfterRetry ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations
         WHERE operation_id = 'dangling-command-crash'" #[]
      expect (danglingRowsAfterRetry == "0")
        "safe recovery retry did not clear its durable recovery marker"
    let protectedBaseline : ManagedOutput.Baseline := {
      identity := "tree:.", kind := .tree, existed := false, digest := "untrusted" }
    AgentWorkbench.SQLite.execute connection
      "INSERT INTO managed_operations(
         operation_id, expected_state_revision, recovery_policy, manifest, committed_state_revision
       ) VALUES (?1, ?2, ?3, ?4, NULL)"
      #["protected-command-crash", stateRevision, "retain-command-output",
        (Lean.toJson protectedBaseline).compress]
    expect (← rejects (Store.recoverManagedOperations root store))
      "interrupted recovery accepted a protected managed-output root"
    expect ((← IO.FS.readBinFile product) == productBefore)
      "interrupted protected-root recovery changed product content"
    let recoveredRevision ← AgentWorkbench.SQLite.queryScalar connection
      "SELECT CAST(state_revision AS TEXT) FROM project_metadata WHERE singleton = 1" #[]
    expect (recoveredRevision == stateRevision)
      "interrupted protected-root recovery changed state revision"
    let protectedRows ← AgentWorkbench.SQLite.queryScalar connection
      "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations
       WHERE operation_id = 'protected-command-crash'" #[]
    expect (protectedRows == "1")
      "refused protected-root recovery discarded its durable recovery marker"
    AgentWorkbench.SQLite.execute connection
      "DELETE FROM managed_operations WHERE operation_id = ?1" #["protected-command-crash"]
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
