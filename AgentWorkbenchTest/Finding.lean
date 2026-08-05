import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Finding

open AgentWorkbench AgentWorkbenchTest

private def append (state : ProjectState) (entry : LedgerEntry) : IO ProjectState :=
  fromExcept (appendEntry state entry)

private def startedState : IO ProjectState := do
  let proposed ← fromExcept (proposeDesign .empty design)
  let accepted ← fromExcept (acceptDesign proposed design.id)
  fromExcept (startWork accepted work)

private def addDesignReview (state : ProjectState) : IO ProjectState :=
  append state {
    id := "entry-review", order := nextEntryOrder state, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .review {
      reviewId := "review-1", purpose := .design, context := .fresh
      targetSourceId := design.id, target := s!"design:{design.id}"
      targetSnapshot := "design-snapshot", producerAgentRun := design.producerAgentRun
      reviewerAgentRun := "reviewer-1" } }

def run : IO Unit := do
  let started ← startedState
  expect (!operationApplicable started "review finding")
    "Review Finding was advertised without a current root Review"
  match recordFinding started {
      entryId := "finding-without-review", reviewEntryId := "missing-review"
      subject := { kind := .statement, id := statement.id, exactQuote := statement.text }
      summary := "unreachable finding" } with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "Review Finding succeeded without a root Review")

  let reviewed ← addDesignReview started
  expect (operationApplicable reviewed "review finding")
    "Design Review Finding was not reachable from its fixed Review target"
  match recordFinding reviewed {
      entryId := "finding-invalid-subject", reviewEntryId := "entry-review"
      subject := { kind := .statement, id := statement.id, exactQuote := "changed quote" }
      summary := "invalid subject" } with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "Review Finding accepted a non-exact subject")
  let withFinding ← fromExcept (recordFinding reviewed {
    entryId := "entry-finding", reviewEntryId := "entry-review"
    subject := { kind := .statement, id := statement.id, exactQuote := statement.text }
    summary := "statement mismatch" })
  let finding ← match withFinding.entry? "entry-finding" with
    | some entry => pure entry
    | none => throw (IO.userError "eligible Review Finding was not recorded")
  expect (finding.order == nextEntryOrder reviewed && finding.scope == work.scope &&
      finding.workId == some work.id && finding.designRevision == some design.id)
    "Review Finding did not preserve its current binding"
  match finding.payload with
  | .finding value =>
      expect (value.reviewId == "review-1" && value.targetSourceId == design.id &&
        value.target == s!"design:{design.id}" && value.targetSnapshot == "design-snapshot" &&
        value.producerAgentRun == design.producerAgentRun)
        "Finding did not derive fixed Design Review provenance"
  | _ => throw (IO.userError "Review Finding recorded the wrong payload")

end AgentWorkbenchTest.Finding
