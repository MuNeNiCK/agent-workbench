import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Lifecycle

open AgentWorkbench AgentWorkbenchTest

private def workLifecycle : IO Unit := do
  let started ← fromExcept <| startWorkRequest ProjectState.empty {
    id := "work-empty", outcome := "define and implement the project", scope := "project"
    responsibleAgentRun := "agent-a" }
  let work ← match started.work? "work-empty" with
    | some value => pure value
    | none => throw (IO.userError "Work was not created")
  expect (work.baselineDesignRevision.isNone && work.designRevision.isNone)
    "Work created before initial Design did not retain the empty baseline"
  let suspended ← fromExcept <| suspendWork started work.id "resume after the Design is accepted"
  expect (suspended.focusedWorkId.isNone && (suspended.work? work.id).any (·.status == .suspended))
    "suspend did not separate focus from Work status"
  let resumed ← fromExcept <| resumeWork suspended work.id
  let handed ← fromExcept <| handoffWork resumed work.id "handoff-1" "agent-b" "continue same outcome"
  expect ((handed.work? work.id).any fun current =>
    current.outcome == work.outcome && current.baselineDesignRevision == work.baselineDesignRevision &&
      current.responsibleAgentRun == "agent-b")
    "handoff changed Work authority beyond responsibility"

private def correctionWithdrawal : IO Unit := do
  let corrected ← fromExcept <| recordCorrection baseState {
    entryId := "correction-withdraw", content := "stop this Work without declaring success" }
  let withdrawn ← fromExcept <| withdrawWork corrected {
    workId := work.id, entryId := "withdrawal-1", correctionEntryId := "correction-withdraw"
    reason := "the user withdrew the requested outcome" }
  expect (withdrawn.focusedWorkId.isNone &&
    (withdrawn.work? work.id).any (·.status == .withdrawn))
    "Correction-authorized withdrawal did not end Work unsuccessfully"
  expectError (withdrawWork baseState {
    workId := work.id, entryId := "invalid-withdrawal", correctionEntryId := "task-open"
    reason := "not authorized" })
    "Work withdrawal accepted a non-Correction authority"

private def successorAdoptionPreservesHistoricalPlan : IO Unit := do
  let predecessor : DesignRevision := { design with status := .superseded }
  let successor : DesignRevision := {
    design with
    id := "design-2"
    parent := some design.id
    status := .accepted
    revisionContentDigest := "blake3:design-2"
    changeRationale := "adopt a strict successor"
  }
  let suspendedWork : Work := {
    work with
    status := .suspended
    resumeCondition := some "adopt the accepted successor"
  }
  let before : ProjectState := { baseState with
    revision := 8, acceptedDesignId := some successor.id, focusedWorkId := none
    designRevisions := [predecessor, successor], works := [suspendedWork] }
  fromExcept (validateState before)
  let impact ← fromExcept <| workAdoptionImpact before work.id
  expect (impact.requiresPlanReplacement && impact.materializedPlanId == some plan.id)
    "adoption impact did not derive the historical Plan replacement"
  let adopted ← fromExcept <| adoptDesignForWork before {
    workId := work.id, entryId := "adoption-1", agentRun := work.responsibleAgentRun }
  let adoptedWork ← match adopted.work? work.id with
    | some value => pure value
    | none => throw (IO.userError "adoption removed the Work")
  expect (adoptedWork.outcome == work.outcome &&
    adoptedWork.baselineDesignRevision == work.baselineDesignRevision &&
    adoptedWork.designRevision == some successor.id)
    "successor adoption changed immutable Work identity"
  expect ((adopted.currentPlanFor? work.id).isNone &&
    (adopted.materializedPlanFor? work.id).any (·.id == plan.id))
    "successor adoption treated the historical Plan as current authority or lost its lineage"
  fromExcept (validateState adopted)

private def completionAuthority : IO Unit := do
  let observations : List TargetObservation :=
    [{ target := criterion.target, snapshot := "blake3:artifact" }]
  let closed ← fromExcept <| closeTask evidencedState observations {
    entryId := "task-closed", taskEntryId := "task-open" }
  let completed ← fromExcept <| completeFocusedWork closed observations []
    "blake3:completion-input"
  let records := completed.ledgerEntries.filter fun entry => match entry.payload with
    | .workCompletion value => value.workId == work.id
    | _ => false
  expect (records.length == 1 && records.head?.any fun entry => match entry.payload with
    | .workCompletion value =>
        value.designRevision == design.id && value.planId == plan.id &&
          value.inputRevision == closed.revision &&
          value.completedByRun == work.responsibleAgentRun
    | _ => false)
    "completion did not atomically persist its exact Work/Design/Plan input authority"
  fromExcept (validateState completed)
  let statusOnly : ProjectState := { closed with
    focusedWorkId := none
    works := closed.works.map fun candidate =>
      if candidate.id == work.id then { candidate with status := .completed } else candidate }
  expectError (validateState statusOnly)
    "completed status was accepted without a WorkCompletion authority"
  expectError (completeFocusedWork closed observations [] "")
    "completion accepted an empty canonical input digest"

def run : IO Unit := do
  fromExcept (validateState baseState)
  workLifecycle
  correctionWithdrawal
  successorAdoptionPreservesHistoricalPlan
  completionAuthority

end AgentWorkbenchTest.Lifecycle
