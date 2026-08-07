import Lean.Data.Json

namespace AgentWorkbench

structure UserCorrectionRecord where
  content : String
  resolvedByEntryId : Option String := none
  resolutionReason : Option String := none
  incorporatedIn : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
