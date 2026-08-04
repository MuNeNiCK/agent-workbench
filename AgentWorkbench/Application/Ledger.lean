import AgentWorkbench.Application.Common

namespace AgentWorkbench

def appendEntry
    (state : ProjectState) (entry : LedgerEntry) : Except String ProjectState :=
  if (state.entry? entry.id).isSome then
    .error s!"entry id {entry.id} already exists"
  else if entry.order != nextEntryOrder state then
    .error "ledger order is not the next order"
  else
    validated { state with
      revision := state.revision + 1
      ledgerEntries := state.ledgerEntries ++ [entry] }

def appendCurrentEntry
    (state : ProjectState) (id : String) (payload : EntryPayload)
    (supersedes : List String := []) : Except String ProjectState :=
  match currentBinding state with
  | .error message => .error message
  | .ok (design, work) =>
      appendEntry state {
        id, order := nextEntryOrder state, scope := work.scope
        workId := some work.id, designRevision := some design.id
        supersedes, payload }

end AgentWorkbench
