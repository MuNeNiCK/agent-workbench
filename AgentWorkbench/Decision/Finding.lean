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
          (state.design? designId).any (fun design => (designFindingSubject? design).isSome)
    | _, _ => false

end AgentWorkbench
