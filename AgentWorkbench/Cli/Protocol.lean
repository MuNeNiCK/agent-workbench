import AgentWorkbench.Decision.ReviewInput

namespace AgentWorkbench.Cli

structure IdInput where
  id : String
  deriving Lean.ToJson, Lean.FromJson

structure SuspendInput where
  workId : String
  resumeCondition : String
  deriving Lean.ToJson, Lean.FromJson

structure AdoptDesignInput where
  workId : String
  entryId : String
  impactDisposition : String
  agentRun : String
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
  context : Option CurrentContext
  deriving Lean.ToJson

structure ReadinessResult where
  stateRevision : Nat
  ready : Bool
  context : Option CurrentContext
  deriving Lean.ToJson

end AgentWorkbench.Cli
