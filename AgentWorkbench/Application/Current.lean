import AgentWorkbench.Decision.Context
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ProofInput
import AgentWorkbench.Adapter.Runtime
import AgentWorkbench.Adapter.ReviewTarget

namespace AgentWorkbench

structure CurrentInputs where
  observations : List TargetObservation
  claimDigests : List CurrentClaimDigest

def evaluateCurrentInputs
    (projectRoot : System.FilePath) (state : ProjectState) : IO CurrentInputs := do
  match currentProjection? state with
  | none => pure { observations := [], claimDigests := [] }
  | some projection =>
      let mut observations := []
      for source in projection.design.sourceDocuments do
        try
          let snapshot ← Snapshot.target projectRoot source.target
          if !observations.any (fun prior => prior.target == source.target) then
            observations := observations ++ [TargetObservation.mk source.target snapshot]
        catch _ => pure ()
      for criterion in projection.design.acceptanceCriteria do
        try
          let snapshot ← Snapshot.target projectRoot criterion.target
          if !observations.any (fun prior => prior.target == criterion.target) then
            observations := observations ++ [TargetObservation.mk criterion.target snapshot]
        catch _ => pure ()
      for entry in projection.entries do
        match entry.payload with
        | .review review =>
            try
              let snapshot ← ReviewTarget.currentSnapshot projectRoot state review.purpose review.target
              if !observations.any (fun prior => prior.target == review.target) then
                observations := observations ++ [TargetObservation.mk review.target snapshot]
            catch _ => pure ()
        | _ => pure ()
      let mut claimDigests := []
      let runtime := Runtime.layout projectRoot
      for claim in projection.design.leanClaims do
        try
          claimDigests := claimDigests ++ [(← ProofInput.evaluate projectRoot runtime claim).1]
        catch _ => pure ()
      pure { observations, claimDigests }

end AgentWorkbench
