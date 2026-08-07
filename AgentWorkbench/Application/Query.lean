import AgentWorkbench.Domain.Operation

namespace AgentWorkbench

/-- Every read-only public request. Arguments are already decoded from the CLI transport. -/
inductive Query where
  | describe (operation : Option String)
  | designInspectSources (targets : List String)
  | designGet (designId : String)
  | designSource (designId target : String)
  | designDiff (beforeDesignId afterDesignId : String)
  | designExport (designId : String)
  | planInspectSources (workId : String) (targets : List String)
  | planGet (planId : String)
  | planSource (planId target : String)
  | planDiff (beforePlanId afterPlanId : String)
  | planExport (planId : String)
  | workGet (workId : String)
  | workAdoptionImpact (workId : String)
  | entryGet (entryId : String)
  | history (afterOrder limit : Nat)
  | reviewContext (reviewEntryId : String)
  | reviewInspect (reviewEntryId : String)
  | commandShow (profileEntryId : String)
  | proofDigest (claimId : String)
  | context
  | ready
  deriving Repr, DecidableEq

def Query.operation : Query → Operation
  | .describe _ => .describe
  | .designInspectSources _ => .designInspectSources
  | .designGet _ => .designGet
  | .designSource _ _ => .designSource
  | .designDiff _ _ => .designDiff
  | .designExport _ => .designExport
  | .planInspectSources _ _ => .planInspectSources
  | .planGet _ => .planGet
  | .planSource _ _ => .planSource
  | .planDiff _ _ => .planDiff
  | .planExport _ => .planExport
  | .workGet _ => .workGet
  | .workAdoptionImpact _ => .workAdoptionImpact
  | .entryGet _ => .entryGet
  | .history _ _ => .history
  | .reviewContext _ => .reviewContext
  | .reviewInspect _ => .reviewInspect
  | .commandShow _ => .commandShow
  | .proofDigest _ => .proofDigest
  | .context => .context
  | .ready => .ready

end AgentWorkbench
