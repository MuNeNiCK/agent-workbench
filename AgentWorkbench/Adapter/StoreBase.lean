import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.StoreSchema

namespace AgentWorkbench.Store

inductive Access where
  | readOnly
  | readWrite

structure Store (access : Access) where private mk ::
  private connection : AgentWorkbench.SQLite.Connection
  migratedFromLegacy : Bool := false

abbrev ReadStore := Store .readOnly
abbrev WriteStore := Store .readWrite

/-- Read capability for either Store. A ReadStore's underlying SQLite handle
is itself opened read-only by the adapter. -/
def readConnection {access : Access} (store : Store access) : AgentWorkbench.SQLite.Connection :=
  store.connection

/-- Write capability is obtainable only from a statically write-enabled Store. -/
def writeConnection (store : WriteStore) : AgentWorkbench.SQLite.Connection :=
  store.connection

def «open» (path : System.FilePath) : IO WriteStore := do
  let connection ← AgentWorkbench.SQLite.open path
  let schemaResult ← AgentWorkbench.StoreSchema.initializeStoreSchema connection
  pure { connection, migratedFromLegacy := schemaResult == .migrated }

/-- Opens existing authoritative state without schema creation or migration capability. -/
def openReadOnly (path : System.FilePath) : IO ReadStore := do
  let connection ← AgentWorkbench.SQLite.openReadOnly path
  pure { connection }

def wasMigratedFromLegacy (store : WriteStore) : Bool :=
  store.migratedFromLegacy

end AgentWorkbench.Store
