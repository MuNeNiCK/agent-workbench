import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.StoreSchema

namespace AgentWorkbenchTest.Schema

open AgentWorkbench AgentWorkbenchTest

private def firstColumn (rows : Array (Array String)) : List String :=
  rows.toList.filterMap fun row => row[0]?

def run : IO Unit := do
  IO.FS.withTempDir fun root => do
    let workbenchRoot := root / ".agent-workbench"
    let database := workbenchRoot / "state.db"
    IO.FS.createDirAll workbenchRoot
    let _ ← Store.open database
    let connection ← SQLite.open database
    let actualTables ← SQLite.queryTextRows connection
      "SELECT name FROM sqlite_master
       WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name" #[] 1
    let expectedTables := StoreSchema.persistedTableNames.mergeSort (· < ·)
    expect (firstColumn actualTables == expectedTables)
      "SQLite table inventory differs from the closed production inventory"
    for table in StoreSchema.persistedTableNames do
      let actualColumns ← SQLite.queryTextRows connection
        s!"SELECT name FROM pragma_table_info('{table}') ORDER BY cid" #[] 1
      expect (firstColumn actualColumns == StoreSchema.persistedColumnNames table)
        s!"SQLite columns for {table} differ from the closed production inventory"

end AgentWorkbenchTest.Schema
