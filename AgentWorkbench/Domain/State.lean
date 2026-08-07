import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Ledger
import AgentWorkbench.Domain.Plan

namespace AgentWorkbench

def schemaRevision : Nat := 2

structure ProjectState where
  revision : Nat := 0
  acceptedDesignId : Option String := none
  focusedWorkId : Option String := none
  designRevisions : List DesignRevision := []
  works : List Work := []
  implementationPlans : List ImplementationPlan := []
  ledgerEntries : List LedgerEntry := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def ProjectState.empty : ProjectState := {}

end AgentWorkbench
