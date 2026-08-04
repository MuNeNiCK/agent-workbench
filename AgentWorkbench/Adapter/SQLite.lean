import SQLite

namespace AgentWorkbench.SQLite

/-- The persistent connection used by a Workbench store operation. -/
abbrev Connection := _root_.SQLite

def «open» (path : System.FilePath) : IO Connection :=
  _root_.SQLite.open path 5000

def openWithTimeout (path : System.FilePath) (busyTimeoutMs : Int32) : IO Connection :=
  _root_.SQLite.open path busyTimeoutMs

private def prepareBound
    (connection : Connection) (sql : String) (params : Array String) : IO _root_.SQLite.Stmt := do
  let statement ← connection.prepare sql
  let expected ← statement.bindParameterCount
  if expected.toNat != params.size then
    throw (IO.userError s!"SQL parameter count mismatch: expected {expected}, got {params.size}")
  for index in [:params.size] do
    statement.bindText (Int.ofNat (index + 1)).toInt32 params[index]!
  pure statement

def runScript (connection : Connection) (script : String) : IO Unit :=
  connection.exec script

def execute (connection : Connection) (sql : String) (params : Array String) : IO Unit := do
  let statement ← prepareBound connection sql params
  statement.exec

def queryScalar (connection : Connection) (sql : String) (params : Array String) : IO String := do
  let statement ← prepareBound connection sql params
  unless ← statement.step do
    throw (IO.userError "SQL scalar query returned no row")
  statement.columnText 0

def queryTextRows
    (connection : Connection) (sql : String) (params : Array String)
    (columns : Nat) : IO (Array (Array String)) := do
  let statement ← prepareBound connection sql params
  let mut rows := #[]
  while ← statement.step do
    let mut row := #[]
    for column in [:columns] do
      row := row.push (← statement.columnText (Int.ofNat column).toInt32)
    rows := rows.push row
  pure rows

def changes (connection : Connection) : IO Int64 :=
  connection.changes

def transaction (connection : Connection) (action : IO α) : IO α :=
  connection.transaction action

def immediateTransaction (connection : Connection) (action : IO α) : IO α :=
  connection.transaction action .immediate

end AgentWorkbench.SQLite
