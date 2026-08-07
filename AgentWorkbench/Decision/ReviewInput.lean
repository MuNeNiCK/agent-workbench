import AgentWorkbench.Decision.Context

namespace AgentWorkbench

structure ReviewInput where
  designId : String
  workId : Option String
  review : LedgerEntry
  lineage : List EntryReference
  lineageTruncated : Bool
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ReviewInspection where
  designId : String
  workId : Option String
  review : LedgerEntry
  lineage : List LedgerEntry
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def belongsToReview (reviewId : String) (findingIds : List String)
    (entry : LedgerEntry) : Bool :=
  match entry.payload with
  | .review value => value.reviewId == reviewId
  | .finding value => value.reviewId == reviewId
  | .reviewDisposition value => findingIds.contains value.findingEntryId
  | .reviewVerification value => value.reviewId == reviewId
  | .reviewHandoff value => value.reviewId == reviewId
  | .reviewConclusion value => value.reviewId == reviewId
  | _ => false

def reviewInput? (state : ProjectState) (reviewEntryId : String) : Option ReviewInput := do
  let reviewEntry ← state.entry? reviewEntryId
  let review ← match reviewEntry.payload with
    | .review value => some value
    | _ => none
  let designId ← reviewEntry.designRevision
  let lineage :=
    match review.context with
    | .fresh => []
    | .resume =>
        let findingIds := state.ledgerEntries.filterMap (fun (entry : LedgerEntry) =>
          match entry.payload with
          | .finding value => if value.reviewId == review.reviewId then some entry.id else none
          | _ => none)
        state.ledgerEntries.filter (fun (entry : LedgerEntry) =>
          entry.order < reviewEntry.order && entry.scope == reviewEntry.scope &&
          entry.workId == reviewEntry.workId && entry.designRevision == reviewEntry.designRevision &&
          belongsToReview review.reviewId findingIds entry)
  pure {
    designId
    workId := reviewEntry.workId
    review := reviewEntry
    lineage := (lineage.take currentContextLimit).map (fun entry => entry.reference)
    lineageTruncated := lineage.length > currentContextLimit }

def reviewInspection? (state : ProjectState) (reviewEntryId : String) : Option ReviewInspection := do
  let reviewEntry ← state.entry? reviewEntryId
  let review ← match reviewEntry.payload with
    | .review value => some value
    | _ => none
  let designId ← reviewEntry.designRevision
  let findingIds := state.ledgerEntries.filterMap (fun entry =>
    match entry.payload with
    | .finding value => if value.reviewId == review.reviewId then some entry.id else none
    | _ => none)
  let lineage := state.ledgerEntries.filter fun entry =>
    entry.order != reviewEntry.order && entry.scope == reviewEntry.scope &&
      entry.workId == reviewEntry.workId && belongsToReview review.reviewId findingIds entry
  pure { designId, workId := reviewEntry.workId, review := reviewEntry, lineage }

end AgentWorkbench
