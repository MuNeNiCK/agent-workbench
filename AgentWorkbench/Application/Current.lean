import AgentWorkbench.Decision.Context
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ProofInput
import AgentWorkbench.Adapter.Runtime
import AgentWorkbench.Adapter.ReviewTarget
import AgentWorkbench.Adapter.Process

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
      for criterion in projection.design.acceptanceCriteria do
        if criterionEvidenceRecorded projection criterion then
          try
            if !observations.any (fun prior => prior.target == criterion.target) then
              let snapshot ← Snapshot.target projectRoot criterion.target
              observations := observations ++ [TargetObservation.mk criterion.target snapshot]
          catch _ => pure ()
      for entry in projection.entries do
        match entry.payload with
        | .artifactObservation evidence =>
            if evidenceBoundToCurrentTaskVerification projection entry then
              try
                if !observations.any (fun prior => prior.target == evidence.target) then
                  let snapshot ← Snapshot.target projectRoot evidence.target
                  observations := observations ++ [TargetObservation.mk evidence.target snapshot]
              catch _ => pure ()
        | .commandExecution execution =>
            if evidenceBoundToCurrentTaskVerification projection entry then
              if let some target := execution.target then
                try
                  if !observations.any (fun prior => prior.target == target) then
                    let snapshot ← Snapshot.target projectRoot target
                    observations := observations ++ [TargetObservation.mk target snapshot]
                catch _ => pure ()
            for input in execution.inputSnapshots.getD [] do
              try
                if !observations.any (fun prior => prior.target == input.target) then
                  let snapshot ← Snapshot.target projectRoot input.target
                  observations := observations ++ [TargetObservation.mk input.target snapshot]
              catch _ => pure ()
            for environment in execution.environmentSnapshots.getD [] do
              if environment.target.startsWith "env:" &&
                  !observations.any (fun prior => prior.target == environment.target) then
                let name := environment.target.drop 4 |>.toString
                let identity := Process.environmentIdentity name (← IO.getEnv name)
                observations := observations ++ [TargetObservation.mk identity.1 identity.2]
        | _ => pure ()
      let mut claimDigests := []
      let runtime := Runtime.layout projectRoot
      for claim in projection.design.leanClaims do
        if claimReceiptRecorded projection claim then
          try
            claimDigests := claimDigests ++ [(← ProofInput.evaluate projectRoot runtime claim).1]
          catch _ => pure ()
      for entry in projection.entries do
        match entry.payload with
        | .review review =>
            try
              let snapshot ← ReviewTarget.currentSnapshot projectRoot state review.purpose
                review.target observations claimDigests
              if !observations.any (fun prior => prior.target == review.target) then
                observations := observations ++ [TargetObservation.mk review.target snapshot]
            catch _ => pure ()
        | _ => pure ()
      pure { observations, claimDigests }

end AgentWorkbench
