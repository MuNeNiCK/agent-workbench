import AgentWorkbench.Decision.ReviewInput
import AgentWorkbench.Adapter.CompletionPreflight

namespace AgentWorkbench.Cli

structure IdInput where
  id : String
  deriving Lean.ToJson, Lean.FromJson

structure DesignSourceInspectionInput where
  sourceDocumentTargets : List String
  deriving Lean.ToJson, Lean.FromJson

structure DesignSourceInput where
  designId : String
  target : String
  deriving Lean.ToJson, Lean.FromJson

structure DesignDiffInput where
  beforeDesignId : String
  afterDesignId : String
  deriving Lean.ToJson, Lean.FromJson

structure PlanSourceInspectionInput where
  workId : String
  sourceDocumentTargets : List String
  deriving Lean.ToJson, Lean.FromJson

structure PlanSourceInput where
  planId : String
  target : String
  deriving Lean.ToJson, Lean.FromJson

structure PlanDiffInput where
  beforePlanId : String
  afterPlanId : String
  deriving Lean.ToJson, Lean.FromJson

structure SuspendInput where
  workId : String
  resumeCondition : String
  deriving Lean.ToJson, Lean.FromJson

structure HandoffInput where
  workId : String
  entryId : String
  successorRun : String
  reason : String
  deriving Lean.ToJson, Lean.FromJson

structure ReadinessInput where
  observations : List TargetObservation := []
  claimDigests : List CurrentClaimDigest := []
  deriving Lean.ToJson, Lean.FromJson

structure HistoryInput where
  afterOrder : Nat := 0
  limit : Nat := 50
  deriving Lean.ToJson, Lean.FromJson

structure StateResult where
  stateRevision : Nat
  acceptedDesignId : Option String
  focusedWorkId : Option String
  deriving Lean.ToJson

def StateResult.ofState (state : ProjectState) : StateResult :=
  { stateRevision := state.revision
    acceptedDesignId := state.acceptedDesignId
    focusedWorkId := state.focusedWorkId }

structure ContextResult where
  stateRevision : Nat
  context : Option ProjectContext
  deriving Lean.ToJson

structure ReadinessResult where
  stateRevision : Nat
  ready : Bool
  preflight : Option CompletionPreflight.Identity
  context : Option ProjectContext
  digest : String
  deriving Lean.ToJson, Lean.FromJson

end AgentWorkbench.Cli
