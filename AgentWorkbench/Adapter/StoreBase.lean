import AgentWorkbench.Adapter.SQLite

namespace AgentWorkbench.Store

/-- Capability needed by normalized Store reads. Mutation modules add their
own instance for `WriteStore`; query roots see only `ReadStore`. -/
class ReadableStore (S : Type) where
  connection : S → AgentWorkbench.SQLite.Connection

structure ReadStore where private mk ::
  private connection : AgentWorkbench.SQLite.Connection

instance : ReadableStore ReadStore where
  connection store := store.connection

def readConnection [ReadableStore S] (store : S) : AgentWorkbench.SQLite.Connection :=
  ReadableStore.connection store

/-- Opens existing authoritative state without schema creation or migration capability. -/
def openReadOnly (path : System.FilePath) : IO ReadStore := do
  let connection ← AgentWorkbench.SQLite.openReadOnly path
  pure { connection }

end AgentWorkbench.Store
