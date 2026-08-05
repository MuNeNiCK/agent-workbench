import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.FindingStore

open AgentWorkbench AgentWorkbenchTest

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let store ← Store.open (root / "state.db")
    let candidate ← Store.proposeDesignRequest root store {
      producerAgentRun := design.producerAgentRun
      statements := design.statements
      acceptanceCriteria := design.acceptanceCriteria
      leanClaims := design.leanClaims }
    let _ ← Store.acceptDesignRequest root store candidate.id
    let started ← Store.startWorkRequest store {
      id := work.id, outcome := work.outcome, scope := work.scope
      responsibleAgentRun := work.responsibleAgentRun
      delegatedReviewDecisions := work.delegatedReviewDecisions }
    expect (!operationApplicable started "review finding")
      "Store advertised Review Finding without a current root Review"
    let revisionBeforeRejectedFinding := started.revision
    let rejected ← try
        let _ ← Store.recordFinding store {
          entryId := "finding-without-review", reviewEntryId := "missing-review"
          subject := { kind := .statement, id := statement.id, exactQuote := statement.text }
          summary := "unreachable finding" }
        pure false
      catch _ => pure true
    expect rejected "Store accepted Review Finding without a root Review"
    expect ((← Store.loadState store).revision == revisionBeforeRejectedFinding)
      "rejected Review Finding advanced the authoritative revision"

    let reviewed ← Store.startReview root store {
      entryId := "entry-review", reviewId := "review-1", purpose := .design
      targetSourceId := candidate.id, reviewerAgentRun := "reviewer-1" }
    expect (operationApplicable reviewed "review finding")
      "Store did not advertise a fixed-target Design Review Finding"
    let withFinding ← Store.recordFinding store {
      entryId := "entry-finding", reviewEntryId := "entry-review"
      subject := { kind := .statement, id := statement.id, exactQuote := statement.text }
      summary := "statement mismatch" }
    let finding ← match withFinding.entry? "entry-finding" with
      | some entry => pure entry
      | none => throw (IO.userError "Store did not persist eligible Review Finding")
    expect (finding.order == nextEntryOrder reviewed &&
      finding.scope == work.scope && finding.workId == some work.id &&
      finding.designRevision == some candidate.id)
      "Store persisted Review Finding with incorrect authoritative binding"
    match finding.payload with
    | .finding value =>
        expect (value.reviewId == "review-1" && value.targetSourceId == candidate.id &&
          value.target == s!"design:{candidate.id}" &&
          value.producerAgentRun == candidate.producerAgentRun)
          "Store did not derive Finding provenance from its fixed Review"
    | _ => throw (IO.userError "Store persisted the wrong Finding payload")

end AgentWorkbenchTest.FindingStore
