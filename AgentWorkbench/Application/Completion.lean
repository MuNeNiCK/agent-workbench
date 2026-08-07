import AgentWorkbench.Application.Common
import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

def completeFocusedWork
    (state : ProjectState) (observations : List TargetObservation)
    (digests : List CurrentClaimDigest) (inputDigest : String) : Except String ProjectState := do
  let workId ← match state.focusedWorkId with
    | some value => pure value
    | none => throw "no focused Work"
  if !completionReady state observations digests then
    throw "focused Work is not ready for completion"
  if inputDigest.isEmpty then throw "completion input digest is empty"
  let input ← completionInput state observations digests
  let completionId := s!"completion-{input.work.id}-{state.revision + 1}"
  if (state.entry? completionId).isSome then throw s!"entry id {completionId} already exists"
  let completion : LedgerEntry := {
    id := completionId, order := nextEntryOrder state, scope := input.work.scope
    workId := some input.work.id, designRevision := some input.design.id
    payload := .workCompletion {
      workId := input.work.id, designRevision := input.design.id, planId := input.plan.id
      inputRevision := state.revision, inputDigest
      completedByRun := input.work.responsibleAgentRun } }
  let works := state.works.map (fun work =>
    if work.id == workId then { work with status := .completed, resumeCondition := none }
    else work)
  validated { state with
    revision := state.revision + 1
    focusedWorkId := none
    works
    ledgerEntries := state.ledgerEntries ++ [completion] }

end AgentWorkbench
