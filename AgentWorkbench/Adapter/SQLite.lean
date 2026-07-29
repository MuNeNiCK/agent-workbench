import AgentWorkbench.Adapter.Codec
import SQLite

namespace AgentWorkbench.Adapter.SQLite

open SQLite.Blob

structure Snapshot where
  storeId : String
  revision : Nat
  state : AgentWorkbench.Kernel.State
deriving DecidableEq, Repr

inductive OpenError
  | uninitialized
  | corrupt (reason : String)
deriving DecidableEq, Repr

inductive MutationError
  | openError (error : OpenError)
  | intentConflict
  | stale
  | wait
  | rejected (reason : String)
  | uncertain
deriving DecidableEq, Repr

private def openDatabase (path : System.FilePath) : IO _root_.SQLite :=
  _root_.SQLite.openWith path
    { mode := .readWriteCreate, threading := some .fullmutex }
    (busyTimeoutMs := 5000)

private def decodeState (bytes : ByteArray) :
    Except OpenError AgentWorkbench.Kernel.State :=
  match fromBinary (α := AgentWorkbench.Kernel.State) bytes with
  | .ok state =>
      if state.wellFormed then .ok state
      else .error (.corrupt "The stored project state is invalid.")
  | .error reason => .error (.corrupt s!"The stored project state cannot be decoded: {reason}")

private def readSnapshotFrom (db : _root_.SQLite) :
    IO (Except OpenError Snapshot) := do
  let statement ← db.prepare
    "SELECT instance, revision, payload FROM current_state WHERE singleton = 1"
  unless ← statement.step do
    return .error .uninitialized
  let storeId ← statement.columnText 0
  if storeId.isEmpty then
    return .error (.corrupt "The stored project identity is invalid.")
  let revision := (← statement.columnInt64 1).toInt.toNat
  match decodeState (← statement.columnBlob 2) with
  | .error error => return .error error
  | .ok state => return .ok { storeId, revision, state }

def inspect (path : System.FilePath) : IO (Except OpenError Snapshot) := do
  if !(← path.pathExists) then
    return .error .uninitialized
  let db ← openDatabase path
  db.transaction (readSnapshotFrom db)

private def createSchema (db : _root_.SQLite) : IO Unit :=
  db.exec "
    CREATE TABLE current_state (
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
      instance TEXT NOT NULL,
      revision INTEGER NOT NULL CHECK (revision >= 0),
      payload BLOB NOT NULL
    );
    CREATE TABLE operations (
      token TEXT PRIMARY KEY,
      intent TEXT NOT NULL,
      revision INTEGER NOT NULL CHECK (revision >= 0)
    );"

private def lookupOperation (db : _root_.SQLite) (token : String) :
    IO (Except OpenError (Option (String × Nat))) := do
  let statement ← db.prepare
    "SELECT intent, revision FROM operations WHERE token = ?"
  statement.bindText 1 token
  unless ← statement.step do
    return .ok none
  let intent ← statement.columnText 0
  let revision := (← statement.columnInt64 1).toInt.toNat
  return .ok (some (intent, revision))

def initializeStore (path : System.FilePath) (token intent : String)
    (initial : AgentWorkbench.Kernel.State) :
    IO (Except OpenError Snapshot) := do
  if !initial.wellFormed then
    return .error (.corrupt "The initial project state is invalid.")
  if token.isEmpty || intent.isEmpty then
    return .error (.corrupt "The private initialization context is incomplete.")
  if ← path.pathExists then
    let db ← openDatabase path
    match ← db.transaction (lookupOperation db token) with
    | .ok (some (storedIntent, revision)) =>
        match ← db.transaction (readSnapshotFrom db) with
        | .ok snapshot =>
            if storedIntent == intent && revision == snapshot.revision &&
                snapshot.state == initial then
              return .ok snapshot
            return .error (.corrupt
              "Project state already exists for another intention.")
        | .error error => return .error error
    | .ok none =>
        return .error (.corrupt "Project state already exists.")
    | .error error => return .error error
  IO.FS.createDirAll (path.parent.getD ".")
  let db ← openDatabase path
  db.exec "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;"
  db.transaction (mode := .immediate) do
    createSchema db
    let insert ← db.prepare
      "INSERT INTO current_state VALUES (1, ?, 0, ?)"
    insert.bindText 1 token
    insert.bindBlob 2 (toBinary initial)
    insert.exec
    let operation ← db.prepare
      "INSERT INTO operations VALUES (?, ?, 0)"
    operation.bindText 1 token
    operation.bindText 2 intent
    operation.exec
    return .ok { storeId := token, revision := 0, state := initial }

private def verifyCommitted (path : System.FilePath) (token intent : String)
    (expected : Snapshot) : IO Bool := do
  if !(← path.pathExists) then
    return false
  try
    let db ← openDatabase path
    match ← db.transaction do
        let receipt ← lookupOperation db token
        let current ← readSnapshotFrom db
        pure (receipt, current) with
    | (.ok (some (storedIntent, revision)), .ok snapshot) =>
        return storedIntent == intent && revision == expected.revision &&
          snapshot == expected
    | _ => return false
  catch _ =>
    return false

private def mutateRaw (path : System.FilePath) (token intent : String)
    (expectedInstance : Option String) (expectedRevision : Option Nat)
    (transition :
      AgentWorkbench.Kernel.State →
        Except String AgentWorkbench.Kernel.State) :
    IO (Except MutationError Snapshot) := do
  if token.isEmpty || intent.isEmpty then
    return .error (.rejected "The private operation context is incomplete.")
  if !(← path.pathExists) then
    return .error (.openError .uninitialized)
  let db ← openDatabase path
  db.exec "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;"
  let applied : Except MutationError Snapshot ←
    db.transaction (mode := .immediate) do
    match ← lookupOperation db token with
    | .error error =>
        return .error (.openError error)
    | .ok (some (storedIntent, revision)) =>
        if storedIntent == intent then
          match ← readSnapshotFrom db with
          | .ok snapshot =>
              if snapshot.revision == revision then
                return .ok snapshot
              return .error .stale
          | .error error => return .error (.openError error)
        else
          return .error .intentConflict
    | .ok none =>
        let current ← match ← readSnapshotFrom db with
          | .ok snapshot => pure snapshot
          | .error error => return .error (.openError error)
        if expectedInstance.any (· != current.storeId) ||
            expectedRevision.any (· != current.revision) then
          return .error .stale
        let next ← match transition current.state with
          | .ok state => pure state
          | .error reason => return .error (.rejected reason)
        unless next.wellFormed do
          return .error (.rejected "The transition produced invalid project state.")
        let snapshot : Snapshot :=
          { storeId := current.storeId
            revision := current.revision + 1
            state := next }
        let update ← db.prepare
          "UPDATE current_state SET revision = ?, payload = ? WHERE singleton = 1"
        update.bindInt64 1 (Int.ofNat snapshot.revision).toInt64
        update.bindBlob 2 (toBinary snapshot.state)
        update.exec
        db.exec "DELETE FROM operations"
        let operation ← db.prepare
          "INSERT INTO operations VALUES (?, ?, ?)"
        operation.bindText 1 token
        operation.bindText 2 intent
        operation.bindInt64 3 (Int.ofNat snapshot.revision).toInt64
        operation.exec
        return .ok snapshot
  match applied with
  | .error error => return .error error
  | .ok snapshot =>
      if ← verifyCommitted path token intent snapshot then
        return .ok snapshot
      else
        return .error .uncertain

def mutate (path : System.FilePath) (token intent : String)
    (expectedInstance : Option String) (expectedRevision : Option Nat)
    (transition :
      AgentWorkbench.Kernel.State →
        Except String AgentWorkbench.Kernel.State) :
    IO (Except MutationError Snapshot) := do
  try
    mutateRaw path token intent expectedInstance expectedRevision transition
  catch error =>
    let message := toString error
    if message.contains "database is locked" ||
        message.contains "database table is locked" ||
        message.contains "database is busy" then
      return .error .wait
    throw error

end AgentWorkbench.Adapter.SQLite
