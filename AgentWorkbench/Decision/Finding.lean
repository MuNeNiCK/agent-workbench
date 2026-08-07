import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

def isFindingRootReview (entry : LedgerEntry) : Bool :=
  match entry.payload with
  | .review review => review.context == .fresh
  | _ => false

def findingSubjectCurrent (design : DesignRevision) (subject : FindingSubject) : Bool :=
  match subject.kind with
  | .criterion => design.acceptanceCriteria.any (fun criterion =>
      criterion.id == subject.id && criterion.statement == subject.exactQuote)
  | .statement => design.statements.any (fun statement =>
      statement.id == subject.id && statement.text == subject.exactQuote)
  | .assumption => design.statements.any (fun statement =>
      statement.id == subject.id && statement.assumptions.contains subject.exactQuote)
  | .implementationComponent => false

def designFindingSubject? (design : DesignRevision) : Option FindingSubject :=
  match design.statements, design.acceptanceCriteria with
  | statement :: _, _ => some {
      kind := .statement, id := statement.id, exactQuote := statement.text }
  | [], criterion :: _ => some {
      kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
  | [], [] => none

theorem designFindingSubject_current
    (design : DesignRevision) (subject : FindingSubject)
    (selected : designFindingSubject? design = some subject) :
    findingSubjectCurrent design subject = true := by
  cases statementEq : design.statements with
  | nil =>
      cases criterionEq : design.acceptanceCriteria with
      | nil => simp [designFindingSubject?, statementEq, criterionEq] at selected
      | cons criterion criteria =>
          simp [designFindingSubject?, statementEq, criterionEq] at selected
          cases selected
          simp [findingSubjectCurrent, criterionEq]
  | cons statement statements =>
      simp [designFindingSubject?, statementEq] at selected
      cases selected
      simp [findingSubjectCurrent, statementEq]

def findingInputsEligible
    (projection : CurrentProjection) (reviewEntryId : String) (subject : FindingSubject) : Bool :=
  projection.entries.any (fun entry => entry.id == reviewEntryId && isFindingRootReview entry) &&
    findingSubjectCurrent projection.design subject

def reviewFindingApplicable (state : ProjectState) : Bool :=
  state.ledgerEntries.any fun entry =>
    match entry.payload, entry.designRevision with
    | .review review, some designId =>
        review.context == .fresh &&
          (review.purpose == .implementation && !review.targetManifest.isEmpty ||
            (state.design? designId).any (fun design => (designFindingSubject? design).isSome))
    | _, _ => false

private def remediationTaskEntryId? (entry : LedgerEntry) : Option String :=
  match entry.payload with
  | .artifactObservation value => value.taskEntryId
  | .commandExecution value => value.taskEntryId
  | _ => none

/-- Exact files only cover themselves. A tree observation also covers files and
subtrees below that tree, but never a sibling or parent target. -/
def implementationTargetCovers (evidenceTarget findingTarget : String) : Bool :=
  if evidenceTarget == findingTarget then true
  else if evidenceTarget.startsWith "tree:" then
    let root := evidenceTarget.drop 5
    if root == "." then
      findingTarget.startsWith "tree:" || findingTarget.startsWith "file:"
    else
      findingTarget.startsWith s!"tree:{root}/" ||
        findingTarget.startsWith s!"file:{root}/"
  else false

private def findingTargetMatches
    (state : ProjectState) (findingEntry : LedgerEntry) (finding : FindingRecord)
    (target : String) : Bool :=
  match finding.subject.kind with
  | .implementationComponent => implementationTargetCovers target finding.subject.id
  | .criterion =>
      findingEntry.designRevision.bind state.design? |>.bind (·.criterion? finding.subject.id)
        |>.any (·.target == target)
  | .statement | .assumption => true

/-- A remediation receipt is causal only when it was produced by the current
replacement-Plan Task that explicitly owns this accepted Finding. -/
def findingRemediationBindingCurrent
    (state : ProjectState) (findingEntry evidenceEntry : LedgerEntry)
    (finding : FindingRecord) (target : String) : Bool :=
  let accepted := findingEntry.workId.any fun workId =>
    state.findingAccepted findingEntry.id workId
  match findingEntry.workId, remediationTaskEntryId? evidenceEntry with
  | some workId, some sourceTaskEntryId =>
      let sourceTaskEntry? := state.entry? sourceTaskEntryId
      let currentPlan? := state.currentPlanFor? workId
      accepted && findingTargetMatches state findingEntry finding target &&
        sourceTaskEntry?.any (fun sourceTaskEntry =>
          sourceTaskEntry.order > findingEntry.order &&
          sourceTaskEntry.order < evidenceEntry.order &&
          match sourceTaskEntry.payload, currentPlan? with
          | .task sourceTask, some plan =>
              let currentTaskEntry? := state.ledgerEntries.find? fun candidate =>
                !entryIsSuperseded state candidate && candidate.workId == some workId &&
                candidate.designRevision == findingEntry.designRevision &&
                match candidate.payload with
                | .task task => task.lineageId == sourceTask.lineageId && !task.retired
                | _ => false
              currentTaskEntry?.any fun currentTaskEntry =>
                match currentTaskEntry.payload with
                | .task currentTask =>
                    currentTask.planId == some plan.id && currentTask.closed &&
                    currentTask.outputScopes.contains target &&
                    currentTask.materializedAtOrder > findingEntry.order &&
                    currentTask.verificationTaskEntryId == some sourceTaskEntry.id &&
                    currentTask.verificationEvidenceEntryIds.contains evidenceEntry.id &&
                    (currentTask.planStepId.bind (fun stepId =>
                      uniqueBy? plan.steps (·.id) stepId)
                      |>.any (·.acceptedFindingEntryIds.contains findingEntry.id))
                | _ => false
          | _, _ => false)
  | _, _ => false

/-- Validate the causal remediation chain as it existed when a verification was recorded. Later
Plan replacement may make that verification non-current, but cannot make its history malformed. -/
def findingRemediationBindingBefore
    (state : ProjectState) (findingEntry evidenceEntry : LedgerEntry)
    (finding : FindingRecord) (target : String) (beforeOrder : Nat) : Bool :=
  let priorEntries := state.ledgerEntries.filter (·.order < beforeOrder)
  let supersededBefore (candidate : LedgerEntry) := priorEntries.any fun replacement =>
    replacement.order > candidate.order && replacement.supersedes.contains candidate.id
  match findingEntry.workId, remediationTaskEntryId? evidenceEntry with
  | some workId, some sourceTaskEntryId =>
      let acceptedDisposition? := findingDispositionIn? priorEntries findingEntry.id workId
      let sourceTaskEntry? := priorEntries.find? (·.id == sourceTaskEntryId)
      acceptedDisposition?.any (fun dispositionEntry =>
        sourceTaskEntry?.any (fun sourceTaskEntry => dispositionEntry.order < sourceTaskEntry.order) &&
          match dispositionEntry.payload with
          | .reviewDisposition disposition => disposition.decision == .accepted
          | _ => false) &&
      findingTargetMatches state findingEntry finding target &&
      sourceTaskEntry?.any fun sourceTaskEntry =>
        let sourceBinding := match sourceTaskEntry.payload with
          | .task sourceTask =>
              (sourceTask.planId.bind state.plan?).any (fun (plan : ImplementationPlan) =>
                plan.workId == workId && some plan.designRevision == findingEntry.designRevision &&
                (sourceTask.planStepId.bind (fun stepId => uniqueBy? plan.steps (·.id) stepId)
                  |>.any (·.acceptedFindingEntryIds.contains findingEntry.id)) &&
                priorEntries.any (fun closedTaskEntry =>
                  let taskMatches := match closedTaskEntry.payload with
                    | .task closedTask =>
                        closedTask.planId == sourceTask.planId &&
                        closedTask.lineageId == sourceTask.lineageId && closedTask.closed &&
                        !closedTask.retired && closedTask.outputScopes.contains target &&
                        closedTask.verificationTaskEntryId == some sourceTaskEntry.id &&
                        closedTask.verificationEvidenceEntryIds.contains evidenceEntry.id
                    | _ => false
                  !supersededBefore closedTaskEntry &&
                    closedTaskEntry.workId == some workId &&
                    closedTaskEntry.designRevision == findingEntry.designRevision &&
                    closedTaskEntry.order > evidenceEntry.order && taskMatches))
          | _ => false
        sourceTaskEntry.order > findingEntry.order &&
          sourceTaskEntry.order < evidenceEntry.order && sourceBinding
  | _, _ => false

end AgentWorkbench
