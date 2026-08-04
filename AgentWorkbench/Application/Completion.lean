import AgentWorkbench.Application.Common
import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

def completeFocusedWork
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) : Except String ProjectState := do
  let workId ← match state.focusedWorkId with
    | some value => pure value
    | none => throw "no focused Work"
  if !completionReady state observations digests then
    throw "focused Work is not ready for completion"
  let works := state.works.map (fun work =>
    if work.id == workId then { work with status := .completed, resumeCondition := none }
    else work)
  validated { state with
    revision := state.revision + 1
    focusedWorkId := none
    works }

end AgentWorkbench
