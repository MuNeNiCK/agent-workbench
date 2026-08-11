import AgentWorkbench.Decision.Finding

namespace AgentWorkbench

structure ExpectedStatementDelta where
  statementId : String
  statementText : String
  kind : StatementDeltaKind
  implementationRequired : Bool
  noActionReason : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson

private def coverageChoice?
    (design : DesignRevision) (statementId : String) : Option (Bool × Option String) := do
  let coverage ← uniqueBy? design.statementCoverage (·.statementId) statementId
  some (coverage.implementationRequired, coverage.noImplementationReason)

private def statementAuthorityChanged
    (baseline current : DesignRevision) (statementId : String) : Bool :=
  match baseline.statement? statementId, current.statement? statementId,
      uniqueBy? baseline.statementCoverage (·.statementId) statementId,
      uniqueBy? current.statementCoverage (·.statementId) statementId with
  | some oldStatement, some newStatement, some oldCoverage, some newCoverage =>
      let oldClaims := oldCoverage.leanClaims.selectedIds.filterMap baseline.claim?
      let newClaims := newCoverage.leanClaims.selectedIds.filterMap current.claim?
      let oldCriteria := oldCoverage.acceptanceCriteria.selectedIds.filterMap baseline.criterion?
      let newCriteria := newCoverage.acceptanceCriteria.selectedIds.filterMap current.criterion?
      let oldAssumptions := oldStatement.assumptions.filterMap baseline.assumption?
      let newAssumptions := newStatement.assumptions.filterMap current.assumption?
      oldStatement != newStatement ||
        oldCoverage.leanClaims != newCoverage.leanClaims ||
        oldCoverage.acceptanceCriteria != newCoverage.acceptanceCriteria ||
        oldCoverage.implementationRequired != newCoverage.implementationRequired ||
        oldCoverage.noImplementationReason != newCoverage.noImplementationReason ||
        oldClaims != newClaims || oldCriteria != newCriteria ||
        oldAssumptions != newAssumptions
  | _, _, _, _ => true

private def removalChoice?
    (state : ProjectState) (baseline current : String) (statementId : String) :
    Option RemovedStatementTombstone :=
  state.designRevisions.findSome? fun design =>
    let liesOnPath :=
      (design.id == current || state.designDescendsFrom design.id current) &&
      (design.id == baseline || state.designDescendsFrom baseline design.id)
    if !liesOnPath then none else
      design.removedStatements.find? (·.statementId == statementId)

def expectedStatementDeltas
    (state : ProjectState) (work : Work) (design : DesignRevision) :
    Except String (List ExpectedStatementDelta) := do
  let mut deltas := []
  match work.baselineDesignRevision with
  | none =>
      for statement in design.statements do
        let (required, reason) ← match coverageChoice? design statement.id with
          | some value => pure value
          | none => throw s!"current Statement {statement.id} has no implementation choice"
        deltas := deltas ++ [{
          statementId := statement.id
          statementText := statement.text
          kind := .added
          implementationRequired := required
          noActionReason := reason }]
  | some baselineId =>
      let baseline ← match state.design? baselineId with
        | some value => pure value
        | none => throw s!"Work baseline Design {baselineId} is unavailable"
      for statement in baseline.statements do
        match design.statement? statement.id with
        | none =>
            let removed ← match removalChoice? state baseline.id design.id statement.id with
              | some value => pure value
              | none => throw s!"removed Statement {statement.id} has no immutable tombstone"
            deltas := deltas ++ [{
              statementId := statement.id
              statementText := removed.statementText
              kind := .removed
              implementationRequired := removed.implementationRequired
              noActionReason := removed.noImplementationReason }]
        | some current =>
            if statementAuthorityChanged baseline design statement.id then
              let (required, reason) ← match coverageChoice? design current.id with
                | some value => pure value
                | none => throw s!"modified Statement {current.id} has no implementation choice"
              deltas := deltas ++ [{
                statementId := current.id
                statementText := current.text
                kind := .modified
                implementationRequired := required
                noActionReason := reason }]
      for statement in design.statements do
        if (baseline.statement? statement.id).isNone then
          let (required, reason) ← match coverageChoice? design statement.id with
            | some value => pure value
            | none => throw s!"added Statement {statement.id} has no implementation choice"
          deltas := deltas ++ [{
            statementId := statement.id
            statementText := statement.text
            kind := .added
            implementationRequired := required
            noActionReason := reason }]
  pure deltas

private def directlyAcceptedImplementationFindingIds
    (state : ProjectState) (workId designId : String) : List String :=
  state.ledgerEntries.filterMap fun findingEntry =>
    if findingEntry.workId != some workId then none
    else match findingEntry.payload with
    | .finding finding =>
        let designCurrent := findingEntry.designRevision.any fun findingDesignId =>
          findingDesignId == designId ||
            (state.designDescendsFrom findingDesignId designId &&
              (state.design? designId).any
                (findingCoveredByAssuranceInDesign state findingEntry finding))
        let implementationRoot := state.ledgerEntries.any fun reviewEntry =>
          reviewEntry.workId == some workId &&
            reviewEntry.designRevision == findingEntry.designRevision &&
          match reviewEntry.payload with
          | .review review => review.reviewId == finding.reviewId &&
              review.context == .fresh && review.purpose == .implementation
          | _ => false
        let implementationDefect := (state.findingDisposition? findingEntry.id workId).any fun entry =>
          match entry.payload with
          | .reviewDisposition disposition =>
              disposition.decision == .accepted &&
                disposition.impact == .implementationDefect
          | _ => false
        if designCurrent && implementationRoot && implementationDefect then some findingEntry.id
        else none
    | _ => none

/-- A distinct remediation Work may cover the one accepted postcompletion Finding named by its
immutable causal binding. The Finding remains owned by the completed origin Work; only Plan
coverage authority crosses the binding. -/
def acceptedImplementationFindingIds
    (state : ProjectState) (workId designId : String) : List String :=
  let direct := directlyAcceptedImplementationFindingIds state workId designId
  let causal := state.ledgerEntries.filterMap fun bindingEntry =>
    if bindingEntry.workId != some workId || bindingEntry.designRevision != some designId then none
    else match bindingEntry.payload with
    | .workRemediation binding =>
        if (directlyAcceptedImplementationFindingIds state binding.originWorkId designId).contains
            binding.findingEntryId then some binding.findingEntryId
        else none
    | _ => none
  (direct ++ causal).eraseDups

end AgentWorkbench
