import AgentWorkbench.Adapter.SQLite

namespace AgentWorkbench.OperationLock

private def lockPath (projectRoot : System.FilePath) : System.FilePath :=
  projectRoot / ".agent-workbench" / "mutation-lock.db"

/--
Serializes public semantic mutations before they read authoritative project state.
The dedicated SQLite transaction is process-safe on every supported platform and is
released automatically when the action finishes or the process connection closes.
-/
def withProjectMutationLock (projectRoot : System.FilePath) (action : IO α) : IO α := do
  IO.FS.createDirAll (projectRoot / ".agent-workbench")
  let connection ← AgentWorkbench.SQLite.openWithTimeout (lockPath projectRoot) 86400000
  AgentWorkbench.SQLite.immediateTransaction connection action

end AgentWorkbench.OperationLock
