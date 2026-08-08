import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource
import AgentWorkbench.Adapter.PlanArchive
import AgentWorkbench.Adapter.Store

namespace AgentWorkbenchTest.PublicRoute

open AgentWorkbench AgentWorkbenchTest

private def executablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench.exe"
  else ".lake/build/bin/agent-workbench"

private def testExecutablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench-tests.exe"
  else ".lake/build/bin/agent-workbench-tests"

private def invoke
    (root : System.FilePath) (command : List String) (input : Option String := none) :
    IO IO.Process.Output :=
  IO.Process.output {
    cmd := executablePath.toString
    args := #["--project", root.toString] ++ command.toArray } input

private def invokeOk
    (root : System.FilePath) (command : List String) (input : Option String := none) : IO String := do
  let output ← invoke root command input
  unless output.exitCode == 0 do
    throw (IO.userError s!"public binary route failed for {command}: {output.stderr}")
  pure output.stdout

private def invokeJson [Lean.ToJson α]
    (root : System.FilePath) (command : List String) (input : α) : IO String :=
  invokeOk root command (some (Lean.toJson input).compress)

private def decodeOutput [Lean.FromJson α] (source : String) : IO α := do
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error message => throw (IO.userError s!"public binary returned invalid JSON: {message}")
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error message => throw (IO.userError s!"public binary returned the wrong JSON shape: {message}")

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let commandCriterion : AcceptanceCriterion := {
      id := "criterion-command-route", statementId := some statement.id
      statement := "the artifact command succeeds", target := criterion.target
      evidenceKind := "command" }
    let routeStep : PlanStep := { step with
      verificationCriterionIds := [criterion.id, commandCriterion.id] }
    let workbenchRoot := root / ".agent-workbench"
    let database := workbenchRoot / "state.db"
    let designDirectory := workbenchRoot / "design" / "product"
    let implementationDirectory := workbenchRoot / "design" / "implementation"
    let planDirectory := workbenchRoot / "design" / "plans" / "work-route"
    IO.FS.createDirAll designDirectory
    IO.FS.createDirAll implementationDirectory
    IO.FS.createDirAll planDirectory
    let _ ← invokeJson root ["work", "start"] ({
      id := "work-route", outcome := "produce a public-route artifact"
      scope := "project", responsibleAgentRun := "agent-route" } : WorkStartRequest)
    let designPath := designDirectory / "design.md"
    IO.FS.writeFile designPath "The artifact exists.\n"
    let designTarget := "file:.agent-workbench/design/product/design.md"
    let capturedDesign ← DesignSource.captureAll root [designTarget]
    let designUnits := capturedDesign.flatMap (·.units)
    expect (!designUnits.isEmpty) "public route produced no Design source unit"
    let designResult ← invokeJson root ["design", "propose"] ({
      producerAgentRun := "agent-route"
      changeRationale := "initial public-route Design"
      sourceDocumentTargets := [designTarget]
      sourceUnitDispositions := designUnits.map fun unit =>
        { unitId := unit.id, role := DesignSourceRole.requirement }
      statements := [statement]
      statementCoverage := [{
        statementId := statement.id, sourceUnitIds := designUnits.map (·.id)
        leanClaims := { noSelectionReason := some "the route has no Design-time logical Claim" }
        acceptanceCriteria := { selectedIds := [criterion.id, commandCriterion.id] }
        implementationRequired := true }]
      acceptanceCriteria := [criterion, commandCriterion] } : DesignProposalRequest)
    let candidate : DesignRevision ← decodeOutput designResult
    let candidateId := candidate.id
    let _ ← invokeJson root ["design", "accept"]
      ({ id := candidateId } : AgentWorkbench.Cli.IdInput)

    let _ ← invokeJson root ["correction", "record"] ({
      entryId := "correction-route-initial"
      content := "clarify how the installed route records its artifact" } : CorrectionRecordRequest)
    let _ ← invokeJson root ["correction", "supersede"] ({
      entryId := "correction-route-current"
      correctionEntryId := "correction-route-initial"
      content := "use the current command route as the clarified action" } : CorrectionSupersedeRequest)
    let _ ← invokeJson root ["work", "suspend"] ({
      workId := "work-route", resumeCondition := "resume after the clarification is recorded" } : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root ["work", "resume"] ({
      workId := "work-route", entryId := "resume-route"
      satisfaction := "the current clarification records the required basis"
      basisEntryIds := ["correction-route-current"], agentRun := "agent-route" } : WorkResumeRequest)
    let _ ← invokeJson root ["work", "handoff"] ({
      workId := "work-route", entryId := "handoff-route"
      successorRun := "agent-route-2", reason := "continue through a distinct responsible run" } : AgentWorkbench.Cli.HandoffInput)
    let _ ← invokeJson root ["kpt", "record"] ({
      entryId := "kpt-route", tryNext := some "run the current Task-bound Command Profile" } : KptRecordRequest)

    let planPath := planDirectory / "plan.md"
    let planBytes := "Create and verify the artifact.\r\n".toUTF8
    IO.FS.writeBinFile planPath planBytes
    let planTarget := "file:.agent-workbench/design/plans/work-route/plan.md"
    let capturedPlan ← PlanSource.captureAll root "work-route" [planTarget]
    let planUnits := capturedPlan.flatMap (·.units)
    expect (!planUnits.isEmpty) "public route produced no Plan source unit"
    let planResult ← invokeJson root ["plan", "propose"] ({
      producerAgentRun := "agent-route"
      reason := "implement the complete initial Design delta"
      sourceDocumentTargets := [planTarget]
      sourceUnitDispositions := planUnits.map fun unit =>
        { unitId := unit.id, stepId := some routeStep.id }
      statementDispositions := [{
        statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := [routeStep.id] }]
      steps := [routeStep] } : PlanProposalRequest)
    let planCandidate : ImplementationPlan ← decodeOutput planResult
    let planId := planCandidate.id
    let archivedPlan ← AgentWorkbench.PlanArchive.source root planId planTarget
    expect (archivedPlan.contentBytes == planBytes.data.toList.map (·.toNat))
      "public Plan proposal did not preserve exact source bytes in SQLite"
    let _ ← invokeJson root ["plan", "materialize"]
      ({ id := planId } : AgentWorkbench.Cli.IdInput)

    let taskId := s!"task-{planId}-{routeStep.id}"
    let artifactPath := root / "artifact.txt"
    IO.FS.writeFile artifactPath "baseline artifact\n"
    IO.FS.writeFile (root / "command-input.txt") "current input\n"
    let testHelper ← IO.FS.realPath testExecutablePath
    let successfulCommand : CommandSpec := {
      executable := testHelper.toString
      arguments := #["write-artifact", artifactPath.toString, "command-output\n"] }
    let helperCheck ← AgentWorkbench.Process.execute root successfulCommand
    unless helperCheck.exitCode == 0 do
      throw (IO.userError s!"native command helper failed: {helperCheck.stderr}")
    IO.FS.writeFile artifactPath "baseline artifact\n"
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-route", purpose := "produce the Task output"
      taskEntryId := taskId, inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target
      criterionIds := [commandCriterion.id], command := successfulCommand } : ProfileDefineRequest)
    let secretProfile := Lean.Json.mkObj [
      ("entryId", "profile-secret-projection"),
      ("purpose", "verify environment disclosure projection"),
      ("taskEntryId", taskId),
      ("inputTargets", Lean.Json.arr #[]),
      ("outputScope", criterion.target),
      ("criterionIds", Lean.Json.arr #[commandCriterion.id]),
      ("command", Lean.Json.mkObj [
        ("executable", testHelper.toString),
        ("arguments", Lean.Json.arr #[]),
        ("workingDirectory", Lean.Json.null),
        ("environment", Lean.Json.arr #[Lean.Json.arr #["API_TOKEN", "super-secret-value"]])])]
    let beforeSecretInput ← Store.loadState (← Store.openReadOnly database)
    let projectedProfile ← invoke root ["profile", "define"] (some secretProfile.compress)
    expect (projectedProfile.exitCode != 0)
      "profile definition accepted a raw environment value"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeSecretInput)
      "rejected raw environment input changed authoritative state"
    let environmentIdentityCommand : CommandSpec := {
      successfulCommand with environment := #["API_TOKEN"] }
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-environment-identity"
      purpose := "record only an environment name"
      taskEntryId := taskId, outputScope := criterion.target
      criterionIds := [commandCriterion.id]
      command := environmentIdentityCommand } : ProfileDefineRequest)
    let projectedEntry ← invoke root ["entry", "get"]
      (some (Lean.toJson ({ id := "profile-environment-identity" } : AgentWorkbench.Cli.IdInput)).compress)
    expect (projectedEntry.exitCode == 0 && projectedEntry.stdout.contains "API_TOKEN" &&
      !projectedEntry.stdout.contains "super-secret-value")
      "persisted Command Profile retained a raw environment value"
    let _ ← invokeJson root ["profile", "replace"] ({
      entryId := "profile-route-current", profileEntryId := "profile-route"
      purpose := "produce the current Task output", taskEntryId := taskId
      inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target, criterionIds := [commandCriterion.id]
      command := successfulCommand } : ProfileReplaceRequest)
    let beforePostCommitFault ← Store.loadState (← Store.openReadOnly database)
    let postCommitRejected ← try
        let _ ← Store.executeMutationWithPostCommitVerification root database
          (.commandRun {
            profileEntryId := "profile-route-current", entryId := "command-route"
            criterionId := some commandCriterion.id })
          (throw (IO.userError "injected post-commit verification fault"))
        pure false
      catch _ => pure true
    expect postCommitRejected
      "post-commit verification fault did not reject the command response"
    let afterPostCommitFault ← Store.loadState (← Store.openReadOnly database)
    expect (afterPostCommitFault.revision == beforePostCommitFault.revision + 1 &&
      afterPostCommitFault.ledgerEntries.any (fun entry => entry.id == "command-route"))
      "post-commit verification fault lost committed command authority"
    expect ((← IO.FS.readFile artifactPath).startsWith "command-output")
      "post-commit verification fault restored the old managed output"
    let recoveryRows ← AgentWorkbench.SQLite.queryTextRows
      (AgentWorkbench.Store.readConnection (← Store.openReadOnly database))
      "SELECT COALESCE(CAST(committed_state_revision AS TEXT), '')
       FROM managed_operations WHERE committed_state_revision IS NOT NULL"
      #[] 1
    expect (recoveryRows.size == 1 && recoveryRows[0]![0]! == toString afterPostCommitFault.revision)
      "post-commit verification fault cleared or misclassified the durable recovery marker"
    let _ ← invokeJson root ["correction", "resolve"] ({
      entryId := "correction-route-resolved"
      correctionEntryId := "correction-route-current", actionEntryId := "command-route"
      reason := "the current Task-bound command applied the clarification" } : CorrectionResolveRequest)
    expect ((← IO.FS.readFile (root / "artifact.txt")).startsWith "command-output")
      "managed-output recovery rolled back the committed command output"
    let remainingRecoveryRows ← AgentWorkbench.SQLite.queryScalar
      (AgentWorkbench.Store.readConnection (← Store.openReadOnly database))
      "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations" #[]
    expect (remainingRecoveryRows == "0")
      "next operation did not recover and clear the committed managed-output marker"
    let _ ← invokeJson root ["kpt", "apply"] ({
      entryId := "kpt-route-applied", kptEntryId := "kpt-route"
      actionEntryId := "command-route", outcome := "the Try produced current command evidence" } : KptApplyRequest)

    let failingCommand : CommandSpec := {
      executable := testHelper.toString
      arguments := #["write-artifact-fail", artifactPath.toString, "partial-output\n"] }
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-failing", purpose := "exercise failed managed output restoration"
      taskEntryId := taskId, outputScope := criterion.target
      criterionIds := [commandCriterion.id], command := failingCommand } : ProfileDefineRequest)
    let beforeFailure ← Store.loadState (← Store.openReadOnly database)
    let beforeFailureOutput ← IO.FS.readFile (root / "artifact.txt")
    let failingRequest : CommandRunRequest := {
      profileEntryId := "profile-failing", entryId := "command-failing"
      criterionId := some commandCriterion.id }
    let failedOutput ← invoke root ["command", "run"]
      (some (Lean.toJson failingRequest).compress)
    let rejected := failedOutput.exitCode != 0
    expect rejected "failed public command was accepted"
    expect ((← IO.FS.readFile (root / "artifact.txt")) == beforeFailureOutput)
      "failed public command did not restore the prior managed output"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeFailure)
      "failed public command changed authoritative state"

    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "evidence-route", taskEntryId := taskId
      criterionId := criterion.id, operation := "inspect artifact"
      result := "artifact exists", successful := true } : ArtifactObserveRequest)
    let beforeInputChange ← Store.loadState (← Store.openReadOnly database)
    IO.FS.writeFile (root / "command-input.txt") "changed after command evidence\n"
    let staleCloseRequest : TaskCloseRequest := {
      entryId := "task-close-stale-input", taskEntryId := taskId }
    let staleClose ← invoke root ["task", "close"]
      (some (Lean.toJson staleCloseRequest).compress)
    expect (staleClose.exitCode != 0)
      "Task close reused command evidence after a declared input changed"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeInputChange)
      "stale command-input rejection changed authoritative state"
    IO.FS.writeFile (root / "command-input.txt") "current input\n"
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-closed-route", taskEntryId := taskId } : TaskCloseRequest)

    let beforeEmptyReviewer ← Store.loadState (← Store.openReadOnly database)
    let emptyReviewer ← invoke root ["review", "start"]
      (some (Lean.toJson ({
        entryId := "review-empty-route", reviewId := "review-empty-route"
        purpose := ReviewPurpose.implementation, reviewerAgentRun := "" } : ReviewStartRequest)).compress)
    expect (emptyReviewer.exitCode != 0)
      "public Review route accepted an empty reviewer identity"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeEmptyReviewer)
      "empty reviewer rejection changed authoritative state"

    let _ ← invokeJson root ["review", "start"] ({
      entryId := "review-route", reviewId := "review-lineage-route"
      purpose := ReviewPurpose.implementation, reviewerAgentRun := "reviewer-route-1" } : ReviewStartRequest)
    let _ ← invokeJson root ["review", "handoff"] ({
      entryId := "review-handoff-route", reviewEntryId := "review-route"
      successorReviewerRun := "reviewer-route-2", reason := "continue the same fixed Review" } : ReviewHandoffRequest)
    let findingSubject : FindingSubject := {
      kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
    let _ ← invokeJson root ["review", "finding"] ({
      entryId := "finding-route", reviewEntryId := "review-route"
      subject := findingSubject
      summary := "the implementation requires one explicit remediation" } : FindingRecordRequest)
    let _ ← invokeJson root ["review", "conclude"] ({
      entryId := "review-conclusion-route", reviewEntryId := "review-route"
      clean := false, summary := "one fixed-target Finding was recorded" } : ReviewConclusionRequest)
    let _ ← invokeJson root ["review", "disposition"] ({
      entryId := "disposition-route", findingEntryId := "finding-route"
      decision := DispositionDecision.accepted
      reason := "materialize the remediation through the Work Plan" } : DispositionRecordRequest)

    IO.FS.writeFile planPath "Remediate and verify the artifact.\n"
    let replacementCapture ← PlanSource.captureAll root "work-route" [planTarget]
    let replacementUnits := replacementCapture.flatMap (·.units)
    let replacementStep : PlanStep := { routeStep with
      description := "remediate and verify the artifact"
      acceptedFindingEntryIds := ["finding-route"] }
    let replacementResult ← invokeJson root ["plan", "replace"] ({
      predecessorPlanId := some planId, producerAgentRun := "agent-route-2"
      reason := "apply the accepted fixed-target Finding"
      changeBasisEntryIds := ["finding-route"]
      sourceDocumentTargets := [planTarget]
      sourceUnitDispositions := replacementUnits.map fun unit =>
        { unitId := unit.id, stepId := some replacementStep.id }
      statementDispositions := [{
        statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := [replacementStep.id] }]
      steps := [replacementStep] } : PlanProposalRequest)
    let replacement : ImplementationPlan ← decodeOutput replacementResult
    let _ ← invokeJson root ["plan", "materialize"]
      ({ id := replacement.id } : AgentWorkbench.Cli.IdInput)
    let replacementTaskId := s!"task-{replacement.id}-{replacementStep.id}"
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-remediation", purpose := "produce the remediated Task output"
      taskEntryId := replacementTaskId, inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target
      criterionIds := [commandCriterion.id], command := successfulCommand } : ProfileDefineRequest)
    let _ ← invokeJson root ["command", "run"] ({
      profileEntryId := "profile-remediation", entryId := "command-remediation"
      criterionId := some commandCriterion.id } : CommandRunRequest)
    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "evidence-remediation", taskEntryId := replacementTaskId
      criterionId := criterion.id, operation := "inspect remediated artifact"
      result := "remediated artifact exists", successful := true } : ArtifactObserveRequest)
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-closed-remediation", taskEntryId := replacementTaskId } : TaskCloseRequest)
    let _ ← invokeJson root ["review", "resume"] ({
      entryId := "review-resume-route", continuesEntryId := "review-route" } : ReviewResumeRequest)
    let _ ← invokeJson root ["review", "verify"] ({
      entryId := "review-verification-route", findingEntryId := "finding-route"
      reviewEntryId := "review-resume-route", evidenceEntryId := "evidence-remediation" } : VerificationRecordRequest)

    let _ ← invokeOk root ["work", "complete"]
    let completed ← Store.loadState (← Store.openReadOnly database)
    expect (completed.focusedWorkId.isNone &&
      completed.works.head?.any (·.status == .completed))
      "public Store route did not complete the same Work"
    let completionEntries := completed.ledgerEntries.filter fun entry => match entry.payload with
      | .workCompletion value => value.workId == "work-route" && !value.inputDigest.isEmpty
      | _ => false
    expect (completionEntries.length == 1)
      "public Store route did not create exactly one completion authority"
    let reloaded ← Store.loadState (← Store.openReadOnly database)
    expect (reloaded == completed)
      "SQLite round trip changed the completed public-route state"

end AgentWorkbenchTest.PublicRoute
