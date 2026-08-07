import Lean.Data.Json

namespace AgentWorkbench

structure KPTRecord where
  keep : Option String := none
  problem : Option String := none
  tryNext : Option String := none
  appliesKptEntryId : Option String := none
  appliedByEntryId : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
