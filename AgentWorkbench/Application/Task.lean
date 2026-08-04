import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure TaskAddRequest where
  entryId : String
  criterionId : Option String := none
  description : String
  required : Bool := true
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure TaskCloseRequest where
  entryId : String
  taskEntryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def addTask (state : ProjectState) (request : TaskAddRequest) : Except String ProjectState :=
  appendCurrentEntry state request.entryId (.task {
    criterionId := request.criterionId, description := request.description
    required := request.required, closed := false })

def closeTask (state : ProjectState) (request : TaskCloseRequest) : Except String ProjectState := do
  let (design, work) ← currentBinding state
  let prior ← match state.entry? request.taskEntryId with
    | some value => pure value
    | none => throw s!"no Task entry {request.taskEntryId}"
  if entryIsSuperseded state prior then throw s!"Task {request.taskEntryId} is not current"
  if prior.scope != work.scope || prior.workId != some work.id ||
      prior.designRevision != some design.id then
    throw s!"Task {request.taskEntryId} is not bound to current Work and Design"
  let task ← match prior.payload with
    | .task value => pure value
    | _ => throw s!"entry {request.taskEntryId} is not a Task"
  if task.closed then throw s!"Task {request.taskEntryId} is already closed"
  appendCurrentEntry state request.entryId (.task { task with closed := true }) [prior.id]

end AgentWorkbench
