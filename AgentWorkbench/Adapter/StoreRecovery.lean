import AgentWorkbench.Adapter.StoreWrite
import AgentWorkbench.Adapter.StoreCodec
import AgentWorkbench.Adapter.ManagedOutput
import AgentWorkbench.Adapter.ProofBuild

namespace AgentWorkbench.Store

private def fail (message : String) : IO α := throw (IO.userError message)

def recoverManagedOperations (projectRoot : System.FilePath) (store : WriteStore) : IO Unit := do
  let rows ← AgentWorkbench.SQLite.queryTextRows (writeConnection store)
    "SELECT operation_id, CAST(expected_state_revision AS TEXT), recovery_policy, manifest,
       COALESCE(CAST(committed_state_revision AS TEXT), '')
     FROM managed_operations ORDER BY operation_id" #[] 5
  let stateRevision ← AgentWorkbench.SQLite.queryScalar (writeConnection store)
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
      let manifest ← Codec.decode (α := AgentWorkbench.ProofBuild.ManagedOutputManifest)
        "managed operation manifest" row[3]!
      AgentWorkbench.ProofBuild.restoreLayouts manifest.layouts
    else if row[4]!.isEmpty then
      let baseline ← Codec.decode (α := AgentWorkbench.ManagedOutput.Baseline)
        "managed command-output baseline" row[3]!
      AgentWorkbench.ManagedOutput.restore projectRoot baseline
    AgentWorkbench.SQLite.immediateTransaction (writeConnection store) do
      AgentWorkbench.SQLite.execute (writeConnection store)
        "DELETE FROM managed_operations WHERE operation_id = ?1" #[operationId]

end AgentWorkbench.Store
