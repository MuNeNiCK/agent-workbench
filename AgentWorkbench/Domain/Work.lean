import Lean.Data.Json

namespace AgentWorkbench

inductive WorkStatus where
  | active
  | suspended
  | completed
  | withdrawn
  deriving Repr, DecidableEq, Lean.ToJson

instance : Lean.FromJson WorkStatus where
  fromJson? json := do
    match ← json.getStr? with
    | "active" | "focused" => pure .active
    | "suspended" | "blocked" => pure .suspended
    | "completed" => pure .completed
    | "withdrawn" => pure .withdrawn
    | value => throw s!"invalid Work status: {value}"

inductive DispositionDecision where
  | accepted
  | rejected
  | replaced
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Work where
  id : String
  outcome : String
  scope : String
  baselineDesignRevision : Option String := none
  designRevision : Option String := none
  status : WorkStatus
  responsibleAgentRun : String
  resumeCondition : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
