import AgentWorkbench.Domain.Lookup

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
            if current.text != statement.text then
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

def acceptedImplementationFindingIds
    (state : ProjectState) (workId designId : String) : List String :=
  state.ledgerEntries.filterMap fun findingEntry =>
    if findingEntry.workId != some workId || findingEntry.designRevision != some designId then none
    else match findingEntry.payload with
    | .finding finding =>
        let implementationRoot := state.ledgerEntries.any fun reviewEntry =>
          reviewEntry.workId == some workId && reviewEntry.designRevision == some designId &&
          match reviewEntry.payload with
          | .review review => review.reviewId == finding.reviewId &&
              review.context == .fresh && review.purpose == .implementation
          | _ => false
        let accepted := state.ledgerEntries.any fun dispositionEntry =>
          dispositionEntry.workId == some workId && dispositionEntry.designRevision == some designId &&
          match dispositionEntry.payload with
          | .reviewDisposition disposition =>
              disposition.findingEntryId == findingEntry.id && disposition.decision == .accepted
          | _ => false
        if implementationRoot && accepted then some findingEntry.id else none
    | _ => none

end AgentWorkbench
