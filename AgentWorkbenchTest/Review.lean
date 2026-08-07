import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Review

open AgentWorkbench AgentWorkbenchTest

private def startedReview (root : System.FilePath) : IO ProjectState :=
  startReview root baseState {
    entryId := "review-1", reviewId := "review-lineage-1", purpose := .implementation
    reviewerAgentRun := "reviewer-1" }

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    IO.FS.writeFile (root / "artifact.txt") "candidate"
    let reviewed ← startedReview root
    let reviewEntry ← match reviewed.entry? "review-1" with
      | some value => pure value
      | none => throw (IO.userError "Review was not recorded")
    let review ← match reviewEntry.payload with
      | .review value => pure value
      | _ => throw (IO.userError "Review entry has wrong payload")
    expect (review.targetSourceId == work.id && review.target == s!"work:{work.id}" &&
      review.targetManifest.any (fun value => value.kind == "design" && value.id == design.id) &&
      review.targetManifest.any (fun value => value.kind == "plan" && value.id == plan.id) &&
      review.targetManifest.any (fun value => value.kind == "task" && value.id == "task-open"))
      "Implementation Review did not freeze Design, Plan, and complete Task graph"
    expect (review.producerAgentRuns.contains design.producerAgentRun &&
      review.producerAgentRuns.contains plan.producerAgentRun &&
      review.producerAgentRuns.contains work.responsibleAgentRun)
      "Implementation Review did not derive complete producer provenance"
    let artifactComponent ← match review.targetManifest.find? (fun value =>
        value.kind == "implementation_target" && value.id == criterion.target) with
      | some value => pure value
      | none => throw (IO.userError "Implementation Review omitted the planned output component")
    let implementationSubject : FindingSubject := {
      kind := .implementationComponent
      id := artifactComponent.id
      exactQuote := artifactComponent.snapshot }
    let implementationFound ← fromExcept <| recordFinding reviewed {
      entryId := "finding-implementation-component", reviewEntryId := "review-1"
      subject := implementationSubject
      summary := "the fixed implementation output is incomplete" }
    fromExcept (validateState implementationFound)
    let mut selfReviewRejected := false
    try
      let _ ← startReview root baseState {
        entryId := "review-self", reviewId := "review-self", purpose := .implementation
        reviewerAgentRun := design.producerAgentRun }
    catch _ => selfReviewRejected := true
    expect selfReviewRejected "a target producer was allowed to review its own manifest"
    expectError (handoffReview reviewed {
      entryId := "handoff-invalid", reviewEntryId := "review-1"
      successorReviewerRun := plan.producerAgentRun, reason := "invalid" })
      "Review handoff accepted a target producer"
    let handed ← fromExcept <| handoffReview reviewed {
      entryId := "review-handoff", reviewEntryId := "review-1"
      successorReviewerRun := "reviewer-2", reason := "continue fixed Review" }
    let concluded ← fromExcept <| concludeReview handed {
      entryId := "review-clean", reviewEntryId := "review-1"
      clean := true, summary := "no findings" }
    let inspection ← match reviewInspection? concluded "review-1" with
      | some value => pure value
      | none => throw (IO.userError "Review inspection is unavailable")
    expect (inspection.lineage.any (·.id == "review-handoff") &&
      inspection.lineage.any (·.id == "review-clean"))
      "Review inspection omitted persisted handoff or conclusion"
    let subject : FindingSubject := {
      kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
    let found ← fromExcept <| recordFinding reviewed {
      entryId := "finding-1", reviewEntryId := "review-1", subject
      summary := "artifact does not satisfy the Criterion" }
    expectError (concludeReview found {
      entryId := "invalid-clean", reviewEntryId := "review-1"
      clean := true, summary := "incorrectly clean" })
      "clean Review conclusion ignored a recorded Finding"
    let nonclean ← fromExcept <| concludeReview found {
      entryId := "review-findings", reviewEntryId := "review-1"
      clean := false, summary := "one Finding was recorded" }
    let disposed ← fromExcept <| recordDisposition nonclean {
      entryId := "disposition-1", findingEntryId := "finding-1"
      decision := .accepted, reason := "the fixed target confirms the mismatch" }
    let omittedPlan : ImplementationPlan := {
      plan with
      id := "plan-finding-omitted"
      predecessorPlanId := some plan.id
      status := .candidate, contentDigest := "blake3:finding-omitted" }
    let omittedState := { disposed with
      implementationPlans := disposed.implementationPlans ++ [omittedPlan] }
    fromExcept (validateState omittedState)
    expectError (materializePlan omittedState omittedPlan.id [])
      "Plan materialization omitted an accepted Implementation Review Finding"
    let coveredStep := { step with acceptedFindingEntryIds := ["finding-1"] }
    let coveredPlan : ImplementationPlan := {
      omittedPlan with
      id := "plan-finding-covered"
      contentDigest := "blake3:finding-covered"
      steps := [coveredStep] }
    let coveredState := { disposed with
      implementationPlans := disposed.implementationPlans ++ [coveredPlan] }
    fromExcept (validateState coveredState)
    let covered ← fromExcept <| materializePlan coveredState coveredPlan.id []
    expect ((covered.currentPlanFor? work.id).any (·.id == coveredPlan.id))
      "accepted Implementation Review Finding could not become an explicit Plan obligation"
    IO.FS.writeFile (root / "artifact.txt") "remediated"
    let remediated ← observeArtifact root disposed {
      entryId := "evidence-remediation", taskEntryId := "task-open"
      criterionId := criterion.id, operation := "inspect remediated artifact"
      result := "artifact now satisfies the Criterion", successful := true }
    let resumed ← resumeReview root remediated {
      entryId := "review-resume", continuesEntryId := "review-1" }
    let tamperedEntries := resumed.ledgerEntries.map fun entry =>
      if entry.id == "review-resume" then match entry.payload with
        | .review value => { entry with payload := .review {
            value with targetSnapshot := "blake3:replaced-resume-target" } }
        | _ => entry
      else entry
    expectError (validateState { resumed with ledgerEntries := tamperedEntries })
      "resumed Review accepted replacement of the immutable root target"
    let verified ← fromExcept <| recordVerification resumed {
      entryId := "verification-1", findingEntryId := "finding-1"
      reviewEntryId := "review-resume", evidenceEntryId := "evidence-remediation" }
    fromExcept (validateState verified)

end AgentWorkbenchTest.Review
