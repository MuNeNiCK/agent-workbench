import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

structure TaskCloseRequest where
  entryId : String
  taskEntryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def closeTask
    (state : ProjectState) (observations : List TargetObservation)
    (request : TaskCloseRequest) : Except String ProjectState := do
  let (design, work) ← currentBinding state
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw "Task close requires a current Work and Design"
  let prior ← match state.entry? request.taskEntryId with
    | some value => pure value
    | none => throw s!"no Task entry {request.taskEntryId}"
  if entryIsSuperseded state prior then throw s!"Task {request.taskEntryId} is not current"
  if prior.scope != work.scope || prior.workId != some work.id ||
      prior.designRevision != some design.id then
    throw s!"Task {request.taskEntryId} is not bound to current Work and Design"
  let task ← match prior.payload with
    | .task value => pure value
    | _ => throw s!"entry {request.taskEntryId} is not a Task"
  if task.closed then throw s!"Task {request.taskEntryId} is already closed"
  if task.retired then throw s!"Task {request.taskEntryId} is retired"
  let mut verificationEvidenceEntryIds : List String := []
  if task.planId.isSome then
    for dependency in task.dependencyLineageIds do
      let satisfied := state.ledgerEntries.any fun entry =>
        entry.workId == some work.id && entry.designRevision == some design.id &&
        !entryIsSuperseded state entry && match entry.payload with
        | .task candidate =>
            candidate.lineageId == some dependency && candidate.closed && !candidate.retired
        | _ => false
      if !satisfied then throw s!"Task dependency is not closed: {dependency}"
    for criterionId in task.verificationCriterionIds do
      let witness := projection.entries.find? fun entry =>
        entry.order > task.materializedAtOrder && entry.workId == some work.id &&
        entry.designRevision == some design.id && evidenceEntryCurrent projection observations entry &&
        match entry.payload with
        | .artifactObservation evidence =>
            evidence.taskEntryId == some prior.id && evidence.outputScope.isSome &&
            task.outputScopes.contains evidence.outputScope.get! &&
            evidence.criterionId == criterionId && evidence.successful
        | .commandExecution evidence =>
            evidence.taskEntryId == some prior.id && evidence.outputScope.isSome &&
            task.outputScopes.contains evidence.outputScope.get! &&
            evidence.criterionId == some criterionId && evidence.successful
        | _ => false
      let witness ← match witness with
        | some value => pure value
        | none =>
          throw s!"Task {request.taskEntryId} has no post-materialization evidence for {criterionId}"
      verificationEvidenceEntryIds := verificationEvidenceEntryIds ++ [witness.id]
  let closedTask : TaskRecord := { task with
    closed := true
    verificationEvidenceEntryIds := verificationEvidenceEntryIds.eraseDups
    verificationTaskEntryId := some prior.id }
  appendCurrentEntry state request.entryId (.task closedTask) [prior.id]

end AgentWorkbench
