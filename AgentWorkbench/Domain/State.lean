import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Ledger

namespace AgentWorkbench

def schemaRevision : Nat := 1

structure ProjectState where
  revision : Nat := 0
  acceptedDesignId : Option String := none
  focusedWorkId : Option String := none
  designRevisions : List DesignRevision := []
  works : List Work := []
  ledgerEntries : List LedgerEntry := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def ProjectState.empty : ProjectState := {}

end AgentWorkbench
