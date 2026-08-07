import Lean.Data.Json

namespace AgentWorkbench

inductive DesignSourceUnitKind where
  | heading
  | paragraph
  | code
  | html
  | table
  | listItem
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignSourceUnit where
  id : String
  target : String
  path : String
  kind : DesignSourceUnitKind
  headingAncestry : List String := []
  text : String
  digest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

inductive DesignSourceRole where
  | requirement
  | assumption
  | rationale
  | example
  | reference
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure SourceUnitDisposition where
  unitId : String
  role : DesignSourceRole
  reason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignAssumption where
  id : String
  text : String
  sourceUnitIds : List String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure SelectionChoice where
  selectedIds : List String := []
  noSelectionReason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure StatementCoverage where
  statementId : String
  sourceUnitIds : List String
  leanClaims : SelectionChoice
  acceptanceCriteria : SelectionChoice
  implementationRequired : Bool
  noImplementationReason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure RemovedStatementTombstone where
  statementId : String
  statementText : String
  implementationRequired : Bool
  noImplementationReason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
