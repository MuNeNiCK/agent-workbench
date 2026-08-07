import AgentWorkbench.Domain.Design

namespace AgentWorkbench

structure ArchivedDesignSource where
  target : String
  mediaKind : String
  digest : String
  contentBytes : List Nat
  deriving Repr, DecidableEq, Lean.ToJson

inductive DesignSourceChange where
  | added
  | deleted
  | changed
  | unchanged
  | binaryChanged
  deriving Repr, DecidableEq, Lean.ToJson

structure DesignSourceLineEdit where
  action : String
  line : String
  deriving Repr, DecidableEq, Lean.ToJson

structure DesignSourceDiff where
  target : String
  change : DesignSourceChange
  beforeDigest : Option String := none
  afterDigest : Option String := none
  lineEdits : List DesignSourceLineEdit := []
  deriving Repr, DecidableEq, Lean.ToJson

structure DesignDiff where
  beforeDesignId : String
  afterDesignId : String
  afterRationale : String
  afterBasisEntryIds : List String
  sources : List DesignSourceDiff
  deriving Repr, DecidableEq, Lean.ToJson

structure PlanDiff where
  beforePlanId : String
  afterPlanId : String
  afterReason : String
  afterBasisEntryIds : List String
  sources : List DesignSourceDiff
  deriving Repr, DecidableEq, Lean.ToJson

end AgentWorkbench
