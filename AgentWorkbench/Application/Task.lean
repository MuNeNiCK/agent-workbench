import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

structure TaskCloseRequest where
  entryId : String
  taskEntryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def dependentClosure
    (tasks : List LedgerEntry) (seeds : List String) : List String :=
  let rec close (fuel : Nat) (affected : List String) : List String :=
    match fuel with
    | 0 => affected
    | remaining + 1 =>
        let expanded := tasks.foldl (fun found entry => match entry.payload with
          | .task task =>
              if !task.closed || task.lineageId.any found.contains ||
                  !task.dependencyLineageIds.any found.contains then found
              else found ++ task.lineageId.toList
          | _ => found) affected
        if expanded.length == affected.length then affected else close remaining expanded
  close tasks.length seeds

/-- Reopen every current Task whose immutable closing evidence is no longer current, together with
its transitive dependents. This is a Task-state repair, not a Plan change: Plan authority and Task
lineage stay fixed, while inherited closing evidence is discarded atomically. -/
def reopenStaleTasks
    (state : ProjectState) (observations : List TargetObservation) : Except String ProjectState := do
  let projection ← match currentProjection? state with
    | some value => pure value
    | none => throw "Task reopening requires a current Work and Design"
  let tasks := currentPlanTaskEntries state projection
  let stale := staleClosedTaskLineages projection observations tasks
  if stale.isEmpty then throw "no current closed Task has stale verification evidence"
  let affected := dependentClosure tasks stale
  let firstOrder := nextEntryOrder state
  let mut entries := state.ledgerEntries
  let mut created := 0
  for prior in tasks do
    match prior.payload with
    | .task task =>
        if task.lineageId.any affected.contains then
          entries := entries ++ [{
            id := s!"task-reopened-{state.revision + 1}-{created + 1}"
            order := firstOrder + created
            scope := prior.scope
            workId := prior.workId
            designRevision := prior.designRevision
            supersedes := [prior.id]
            payload := .task { task with
              verificationEvidenceEntryIds := []
              verificationTaskEntryId := none
              materializedAtOrder := firstOrder
              closed := false } }]
          created := created + 1
    | _ => pure ()
  validated { state with revision := state.revision + 1, ledgerEntries := entries }

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
            evidence.criterionId == some criterionId && evidence.successful
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
    for contract in task.taskVerificationContracts do
      let witness := projection.entries.find? fun entry =>
        entry.order > task.materializedAtOrder && entry.workId == some work.id &&
        entry.designRevision == some design.id && evidenceEntryCurrent projection observations entry &&
        match entry.payload with
        | .artifactObservation evidence =>
            contract.kind == .artifact && evidence.taskEntryId == some prior.id &&
            evidence.outputScope == some contract.target && evidence.criterionId.isNone &&
            evidence.taskVerificationId == some contract.id && evidence.successful
        | .commandExecution evidence =>
            contract.kind == .command && evidence.taskEntryId == some prior.id &&
            evidence.outputScope == some contract.target && evidence.criterionId.isNone &&
            evidence.taskVerificationId == some contract.id && evidence.successful
        | _ => false
      let witness ← match witness with
        | some value => pure value
        | none => throw s!"Task {request.taskEntryId} has no post-materialization evidence for Task verification {contract.id}"
      verificationEvidenceEntryIds := verificationEvidenceEntryIds ++ [witness.id]
  let closedTask : TaskRecord := { task with
    closed := true
    verificationEvidenceEntryIds := verificationEvidenceEntryIds.eraseDups
    verificationTaskEntryId := some prior.id }
  appendCurrentEntry state request.entryId (.task closedTask) [prior.id]

end AgentWorkbench
