import AgentWorkbench.Domain.Validation

namespace AgentWorkbench

def validated (state : ProjectState) : Except String ProjectState :=
  match validateState state with
  | .error message => .error message
  | .ok _ => .ok state

def nextEntryOrder (state : ProjectState) : Nat :=
  state.ledgerEntries.foldl (fun maximum entry => max maximum entry.order) 0 + 1

def currentBinding (state : ProjectState) : Except String (DesignRevision × Work) := do
  let design ← match state.currentDesign? with
    | some value => pure value
    | none => throw "no current accepted Design"
  let work ← match state.currentWork? with
    | some value => pure value
    | none => throw "no focused Work"
  if work.designRevision != design.id then
    throw "focused Work is not bound to the current accepted Design"
  pure (design, work)

end AgentWorkbench
