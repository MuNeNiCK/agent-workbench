import Lean.Data.Json

namespace AgentWorkbench

inductive WorkStatus where
  | focused
  | suspended
  | blocked
  | completed
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive DispositionDecision where
  | accepted
  | rejected
  | replaced
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Work where
  id : String
  outcome : String
  scope : String
  designRevision : String
  status : WorkStatus
  responsibleAgentRun : String
  delegatedReviewDecisions : List DispositionDecision := []
  resumeCondition : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
