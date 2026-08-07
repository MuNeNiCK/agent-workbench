import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Review

open AgentWorkbench AgentWorkbenchTest

private def startedReview (root : System.FilePath) : IO ProjectState :=
  let predecessor := { plan with
    id := "plan-0", status := .superseded, contentDigest := "blake3:superseded-plan" }
  startReview root { baseState with implementationPlans := [predecessor, plan] } {
    entryId := "review-1", reviewId := "review-lineage-1", purpose := .implementation
    reviewerAgentRun := "reviewer-1" }

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    IO.FS.writeFile (root / "artifact.txt") "candidate"
    let projectSnapshot ← Snapshot.target root "tree:."
    IO.FS.createDirAll (root / ".agent-workbench" / "toolchains")
    IO.FS.writeFile (root / ".agent-workbench" / "toolchains" / "private-state") "ignored"
    expect ((← Snapshot.target root "tree:.") == projectSnapshot)
      "project snapshot included private Agent Workbench state"
    let reviewed ← startedReview root
    let reviewEntry ← match reviewed.entry? "review-1" with
      | some value => pure value
      | none => throw (IO.userError "Review was not recorded")
    let review ← match reviewEntry.payload with
      | .review value => pure value
      | _ => throw (IO.userError "Review entry has wrong payload")
    let encodedReview := (Lean.toJson review).compress
    let persistedWithoutVersion := encodedReview.replace "\"targetManifestVersion\":1," ""
    expect (persistedWithoutVersion != encodedReview)
      "Review fixture did not contain the current manifest version"
    let decodedV0 ← match Lean.Json.parse persistedWithoutVersion with
      | .error message => throw (IO.userError message)
      | .ok json => match (Lean.fromJson? json : Except String ReviewRecord) with
        | .error message => throw (IO.userError message)
        | .ok value => pure value
    expect (decodedV0.targetManifestVersion == 0)
      "Review decoder did not retain a pre-version manifest as historical format"
    expect (review.targetSourceId == work.id && review.target == s!"work:{work.id}" &&
      review.targetManifestVersion == 1 &&
      review.targetManifest.any (fun value => value.kind == "design" && value.id == design.id) &&
      review.targetManifest.any (fun value => value.kind == "plan" && value.id == plan.id) &&
      review.targetManifest.any (fun value => value.kind == "task" && value.id == "task-open"))
      "Implementation Review did not freeze Design, Plan, and complete Task graph"
    expect (review.producerAgentRuns.contains design.producerAgentRun &&
      review.producerAgentRuns.contains plan.producerAgentRun &&
      review.producerAgentRuns.contains work.responsibleAgentRun)
      "Implementation Review did not derive complete producer provenance"
    let injectedComponent : ReviewTargetComponent := {
      kind := "finding", id := "historical-finding"
      snapshot := "blake3:historical-finding"
      producerAgentRuns := [work.responsibleAgentRun] }
    let injectedEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value => { entry with payload := .review {
            value with targetManifest := value.targetManifest ++ [injectedComponent] } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := injectedEntries })
      "Implementation Review accepted an extra historical ledger component"
    let withoutWorkEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let manifest := value.targetManifest.filter (·.kind != "work")
            { entry with payload := .review { value with
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := withoutWorkEntries })
      "Implementation Review accepted a manifest with no fixed Work"
    let duplicateTargetEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            match value.targetManifest.find? (·.kind == "implementation_target") with
            | some target =>
                let manifest := value.targetManifest ++
                  [{ target with snapshot := "blake3:conflicting-target" }]
                { entry with payload := .review { value with
                    targetManifest := manifest
                    targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
            | none => entry
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := duplicateTargetEntries })
      "Implementation Review accepted conflicting duplicate implementation targets"
    let reorderedEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let manifest := value.targetManifest.reverse
            { entry with payload := .review { value with
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := reorderedEntries })
      "Implementation Review accepted noncanonical manifest ordering"
    let foreignDesignEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let foreign : ReviewTargetComponent := {
              kind := "design", id := "design-foreign", snapshot := "blake3:foreign"
              producerAgentRuns := [design.producerAgentRun] }
            let manifest := normalizeReviewTargetComponents (value.targetManifest ++ [foreign])
            { entry with payload := .review { value with
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := foreignDesignEntries })
      "Implementation Review accepted an unrelated Design component"
    let supersededPlanEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let manifest := value.targetManifest.map fun component =>
              if component.kind == "plan" then { component with
                id := "plan-0", snapshot := "blake3:superseded-plan" }
              else component
            { entry with payload := .review { value with
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := supersededPlanEntries })
      "Implementation Review accepted a superseded same-Design Plan"
    let forgedWorkEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let manifest := value.targetManifest.map fun component =>
              if component.kind == "work" then { component with snapshot := "blake3:forged-work" }
              else component
            { entry with payload := .review { value with
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := forgedWorkEntries })
      "Implementation Review accepted a forged Work identity snapshot"
    let forgedPlanProducerEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let manifest := value.targetManifest.map fun component =>
              if component.kind == "plan" then
                { component with producerAgentRuns := ["forged-planner"] }
              else component
            let manifestProducers := manifest.foldl (fun found component =>
              component.producerAgentRuns.foldl (fun runs producer =>
                if producer.isEmpty || runs.contains producer then runs else runs ++ [producer]) found) []
            { entry with payload := .review { value with
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress
                producerAgentRuns := manifestProducers } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := forgedPlanProducerEntries })
      "Implementation Review accepted forged structural producer provenance"
    let legacyEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value =>
            let manifest := value.targetManifest.map fun component =>
              if component.kind == "work" then { component with
                snapshot := ContentDigest.string (Lean.toJson work).compress }
              else component
            { entry with payload := .review { value with
                targetManifestVersion := 0
                targetManifest := manifest
                targetSnapshot := ContentDigest.string (Lean.toJson manifest).compress } }
        | _ => entry
      else entry
    fromExcept (validateState { reviewed with ledgerEntries := legacyEntries })
    let extraProducerEntries := reviewed.ledgerEntries.map fun entry =>
      if entry.id == "review-1" then match entry.payload with
        | .review value => { entry with payload := .review {
            value with producerAgentRuns := value.producerAgentRuns ++ ["unrelated-producer"] } }
        | _ => entry
      else entry
    expectError (validateState { reviewed with ledgerEntries := extraProducerEntries })
      "Implementation Review accepted an unrelated top-level producer"
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
    let replacedDisposition ← fromExcept <| recordDisposition disposed {
      entryId := "disposition-2", findingEntryId := "finding-1"
      decision := .replaced, reason := "the later responsible-agent decision replaces acceptance" }
    expect (!replacedDisposition.findingAccepted "finding-1" work.id &&
      !(acceptedImplementationFindingIds replacedDisposition work.id design.id).contains "finding-1")
      "a replaced Finding disposition left the earlier acceptance authoritative"
    let replacementEntry ← match replacedDisposition.entry? "disposition-2" with
      | some value => pure value
      | none => throw (IO.userError "replacement disposition was not recorded")
    expect (replacementEntry.supersedes == ["disposition-1"])
      "replacement disposition did not supersede prior disposition history"
    fromExcept (validateState replacedDisposition)
    let omittedPlan : ImplementationPlan := {
      plan with
      id := "plan-finding-omitted"
      predecessorPlanId := some plan.id
      status := .candidate, contentDigest := "blake3:finding-omitted" }
    let omittedState := { disposed with
      implementationPlans := disposed.implementationPlans ++ [omittedPlan] }
    fromExcept (validateState omittedState)
    expectError (materializePlan omittedState omittedPlan.id [] [])
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
    let covered ← fromExcept <| materializePlan coveredState coveredPlan.id [] []
    expect ((covered.currentPlanFor? work.id).any (·.id == coveredPlan.id))
      "accepted Implementation Review Finding could not become an explicit Plan obligation"
    IO.FS.writeFile (root / "artifact.txt") "remediated"
    let bypassEvidence ← observeArtifact root disposed {
      entryId := "evidence-bypass", taskEntryId := "task-open"
      criterionId := criterion.id, operation := "inspect remediated artifact"
      result := "artifact now satisfies the Criterion", successful := true }
    let bypassResume ← resumeReview root bypassEvidence {
      entryId := "review-resume-bypass", continuesEntryId := "review-1" }
    expectError (recordVerification bypassResume {
      entryId := "verification-bypass", findingEntryId := "finding-1"
      reviewEntryId := "review-resume-bypass", evidenceEntryId := "evidence-bypass" })
      "Review verification bypassed the Finding-bound replacement Plan Task"
    let remediationTaskId := s!"task-{coveredPlan.id}-{coveredStep.id}"
    let remediated ← observeArtifact root covered {
      entryId := "evidence-remediation", taskEntryId := remediationTaskId
      criterionId := criterion.id, operation := "inspect remediated artifact"
      result := "artifact now satisfies the Criterion", successful := true }
    let remediationSnapshot ← match remediated.entry? "evidence-remediation" with
      | some entry => match entry.payload with
        | .artifactObservation value => pure value.snapshot
        | _ => throw (IO.userError "remediation evidence has the wrong kind")
      | none => throw (IO.userError "remediation evidence was not recorded")
    let closedRemediation ← fromExcept <| closeTask remediated
      [{ target := criterion.target, snapshot := remediationSnapshot }]
      { entryId := "task-closed-remediation", taskEntryId := remediationTaskId }
    let resumed ← resumeReview root closedRemediation {
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
