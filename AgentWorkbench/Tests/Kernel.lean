import AgentWorkbench.Tests.Support

namespace AgentWorkbench.Tests.Kernel

open AgentWorkbench
open AgentWorkbench.Domain
open AgentWorkbench.Tests

def testFlatCompletionAndPhase : IO Unit := do
  expect (!Kernel.currentlyComplete initialState initialState.focus.work)
    "pending flat Work reported completion"
  let phased ← unwrap
    (Kernel.assignPhase initialState "implement the selected change" "Delivery" 2)
    "Phase assignment failed"
  let renamed ← unwrap (Kernel.renamePhase phased "Delivery" "Release")
    "Phase rename failed"
  let ordered ← unwrap (Kernel.orderPhase renamed "Release" 1)
    "Phase reorder failed"
  let originalTask ← match initialState.tasks with
    | [task] => pure task
    | _ => throw <| IO.userError "flat fixture has no unique task"
  let orderedTask ← match ordered.tasks with
    | [task] => pure task
    | _ => throw <| IO.userError "Phase operation changed task cardinality"
  expect (orderedTask.ref == originalTask.ref &&
      orderedTask.designScope == originalTask.designScope &&
      orderedTask.state == originalTask.state)
    "Phase presentation changed Task identity, design scope, or satisfaction"
  let finished ← unwrap (Kernel.finishCurrentTask ordered)
    "flat Task completion failed"
  expect (Kernel.currentlyComplete finished finished.focus.work)
    "flat Work did not complete without another record family"

def testPhaseGroupsTasksWithoutSemanticEffect : IO Unit := do
  let withSecond ← unwrap (Kernel.addTask initialState "verify the selected change")
    "second Task creation failed"
  let semanticTasks :=
    withSecond.tasks.map fun task =>
      (task.ref, task.work, task.designScope, task.state)
  let boundary := withSecond.work.map fun work =>
    (work.ref, work.completionBoundary)
  let completion := Kernel.currentlyComplete withSecond withSecond.focus.work
  let phasedFirst ← unwrap
    (Kernel.assignPhase withSecond "implement the selected change" "Delivery" 2)
    "first Task Phase assignment failed"
  let phasedBoth ← unwrap
    (Kernel.assignPhase phasedFirst "verify the selected change" "Delivery" 2)
    "second Task Phase assignment failed"
  let renamed ← unwrap (Kernel.renamePhase phasedBoth "Delivery" "Release")
    "shared Phase rename failed"
  let ordered ← unwrap (Kernel.orderPhase renamed "Release" 1)
    "shared Phase reorder failed"
  expect (ordered.tasks.all (·.phase == some "phase-1"))
    "larger Work did not group both Tasks under one Phase"
  expect (ordered.tasks.map (fun task =>
      (task.ref, task.work, task.designScope, task.state)) == semanticTasks)
    "Phase operations changed Task semantic facts"
  expect (ordered.work.map (fun work =>
      (work.ref, work.completionBoundary)) == boundary &&
      Kernel.currentlyComplete ordered ordered.focus.work == completion &&
      ordered.evidenceSpecs == withSecond.evidenceSpecs &&
      ordered.reviewRequests == withSecond.reviewRequests)
    "Phase operations changed completion, assurance, or Review semantics"

def testEvidenceCurrentness : IO Unit := do
  let selected ← unwrap
    (Kernel.addEvidence initialState "latency"
      "The selected command completes within 100 ms."
      "measure the selected command" "supported release host"
      ["command=check"] "elapsed <= 100 ms" "monotonic clock"
      "sha256:release-a")
    "Evidence selection failed"
  let recorded ← unwrap (Kernel.recordEvidence selected "latency" "42 ms" true)
    "Evidence recording failed"
  let first ← match recorded.evidenceResults with
    | [result] => pure result
    | _ => throw <| IO.userError "Evidence result was not uniquely recorded"
  expect (Kernel.evidenceResultCurrent recorded first)
    "exact Evidence result was not current"
  let changed ← unwrap
    (Kernel.addEvidence recorded "latency"
      "The selected command completes within 100 ms."
      "measure the selected command" "supported release host"
      ["command=check"] "elapsed <= 100 ms" "monotonic clock"
      "sha256:release-b")
    "Evidence correction failed"
  expect (!Kernel.evidenceResultCurrent changed first)
    "Evidence result survived a changed artifact identity"

def testSelectedEvidenceControlsCompletion : IO Unit := do
  let selectedSource := source "latency-design"
  let item : Design.Item :=
    { ref := { key := "latency", version := 0 }
      predecessor := none
      statement := "The selected command completes within 100 ms."
      role := .nonFunctionalRequirement
      source := selectedSource
      dependencies := []
      assurance :=
        { kind := .evidence
          obligations :=
            [{ key := "latency"
               method := .evidence
               description := "Observe selected command latency." }] }
      authority :=
        .acceptedByCaller
          { source := selectedSource, reason := "Caller selected the latency boundary." } }
  let accepted ← match item.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "accepted Evidence design is invalid"
  let taskRef ← match initialState.focus.task with
    | some task => pure task
    | none => throw <| IO.userError "selected Task is unavailable"
  let baseTask ← match initialState.tasks with
    | [task] => pure task
    | _ => throw <| IO.userError "selected Task is not unique"
  let task := { baseTask with
    basis := Work.DerivationBasis.design [accepted]
    designScope := [accepted]
    state := .satisfied }
  let baseWork ← match initialState.work with
    | [work] => pure work
    | _ => throw <| IO.userError "selected Work is not unique"
  let boundary : List Work.CompletionMember :=
    [{ target := .taskSatisfied taskRef
       basis := .design [accepted] },
     { target := .assurance "latency"
       basis := .design [accepted] }]
  let work := { baseWork with
    completionBoundary :=
      boundary }
  let base : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects :=
            [{ source := selectedSource, content := .design item }] }
      work := [work]
      tasks := [task] }
  expect (!Kernel.currentlyComplete base work.ref)
    "selected Evidence was bypassed by Task satisfaction"
  let withSpec ← unwrap
    (Kernel.addEvidence base "latency" "Observe selected command latency."
      "measure selected command" "supported release host" ["command=check"]
      "elapsed <= 100 ms" "monotonic clock" "sha256:release")
    "selected Evidence specification failed"
  let withResult ← unwrap
    (Kernel.recordEvidence withSpec "latency" "42 ms" true)
    "selected Evidence result failed"
  expect (Kernel.currentlyComplete withResult withResult.focus.work)
    "exact selected Evidence did not satisfy completion"
  expect (Evidence.selectedAssurance (Kernel.currentDesignItems withResult)
      |>.all (·.method == .evidence))
    "non-formal Evidence created an unselected formal obligation"

def testCompositeSourceEffects : IO Unit := do
  let common := source "composite-message"
  let recorded ← unwrap
    (Kernel.recordSourceEffects initialState common
      (some "project-layout") (some "Keep contracts under Inventory.")
      .projectStructure { kind := .none, obligations := [] } []
      (some "Keep Workbench vocabulary outside project modules.")
      (some "Which deployment host supplies measurements?")
      (some ("deliver the package", "classify the package")))
    "composite source recording failed"
  let effects := recorded.design.effects
  expect (effects.length == 3 && effects.all (·.source == common))
    "composite source did not preserve common provenance"
  expect (recorded.work.any (·.outcome == "deliver the package"))
    "work request was duplicated into design instead of routing to Work"
  expect (Kernel.currentDesignItems recorded).isEmpty
    "unaccepted composite design changed current authority"
  let rejected ← unwrap
    (Kernel.recordNonAuthoritative recorded common .rejection
      "The proposed cache is not selected." (some "cache"))
    "proposal rejection recording failed"
  expect (Kernel.currentDesignItems rejected).isEmpty
    "proposal rejection selected a completion obligation"
  let firstFinished ← unwrap (Kernel.finishCurrentTask rejected)
    "composite Work classification Task failed"
  let requested ← unwrap
    (Kernel.requestDesignReview firstFinished "layout-review" "project-layout")
    "positive successor design Review request failed"
  let reviewed ← unwrap
    (Kernel.recordCleanReview requested "layout-review" "layout-reviewer")
    "positive successor design Review failed"
  let accepted ← unwrap
    (Kernel.acceptDesign reviewed "project-layout"
      (decision "accept-layout" "Caller selected the reviewed layout."))
    "positive successor acceptance failed"
  let tasked ← unwrap
    (Kernel.addTaskForDesign accepted "apply selected project layout"
      ["project-layout"])
    "positive successor Task creation failed"
  match Kernel.nextAction tasked with
  | some (.satisfy member) =>
      expect (match member.target with
        | .taskSatisfied selected =>
            tasked.tasks.any fun task =>
              task.ref == selected &&
                task.description == "apply selected project layout"
        | _ => false)
        "accepted positive successor did not become the exact next Task"
  | _ =>
      throw <| IO.userError
        "accepted positive successor did not produce a bounded next action"
  let completed ← unwrap (Kernel.finishCurrentTask tasked)
    "positive successor Task completion failed"
  expect (Kernel.currentlyComplete completed completed.focus.work)
    "accepted positive successor did not produce the exact completion"

def testReviewAuthority : IO Unit := do
  let requested ← unwrap
    (Kernel.requestReview initialState "implementation-review" "src/change"
      .implementation)
    "Review request failed"
  let observation : Review.Observation :=
    { key := "bounded-risk"
      kind := .risk
      summary := "Confirm the selected boundary."
      evidence := "The selected artifact was inspected." }
  let reviewed ← unwrap
    (Kernel.recordReviewResult requested "implementation-review" "reviewer"
      observation)
    "Review result failed"
  let reviewRef ← match reviewed.reviewRequests with
    | [request] => pure request.ref
    | _ => throw <| IO.userError "Review request was not uniquely retained"
  expect (!Kernel.reviewResolved reviewed reviewRef)
    "reviewer observation acquired caller authority"
  let deferred ← unwrap
    (Kernel.recordReviewDisposition reviewed "implementation-review"
      "bounded-risk" .deferred (decision "defer" "Obtain more evidence."))
    "non-final Review disposition failed"
  expect (!Kernel.reviewResolved deferred reviewRef)
    "non-final Review disposition resolved the Review"
  let accepted ← unwrap
    (Kernel.recordReviewDisposition deferred "implementation-review"
      "bounded-risk" .accepted (decision "accept" "The risk is satisfied."))
    "final Review disposition failed"
  expect (Kernel.reviewResolved accepted reviewRef)
    "caller disposition did not resolve the exact Review"

def testReviewProposalAuthority : IO Unit := do
  let proposed ← unwrap
    (Kernel.recordDesign initialState (source "review-proposed-design" .reviewer)
      "review-successor" "Use the reviewed bounded successor."
      .functionalRequirement { kind := .none, obligations := [] })
    "review successor proposal failed"
  let designRequested ← unwrap
    (Kernel.requestDesignReview proposed "review-successor-meaning"
      "review-successor")
    "review successor meaning request failed"
  let designReviewed ← unwrap
    (Kernel.recordCleanReview designRequested "review-successor-meaning"
      "meaning-reviewer")
    "review successor meaning result failed"
  let implementationRequested ← unwrap
    (Kernel.requestReview designReviewed "review-proposal" "src/change"
      .implementation)
    "review proposal request failed"
  let observation : Review.Observation :=
    { key := "bounded-successor"
      kind := .proposal
      summary := "Adopt the reviewed bounded successor."
      evidence := "The proposed successor matches the selected scope." }
  let reviewed ← unwrap
    (Kernel.recordReviewResult implementationRequested "review-proposal"
      "implementation-reviewer" observation)
    "review proposal result failed"
  let rejected ← unwrap
    (Kernel.recordReviewDisposition reviewed "review-proposal"
      "bounded-successor" .rejected
      (decision "reject-review-proposal"
        "Caller did not select the reviewer proposal."))
    "review proposal rejection failed"
  expect (Kernel.currentDesignItems rejected).isEmpty
    "rejected reviewer proposal acquired design authority"
  let adopted ← unwrap
    (Kernel.adoptReviewProposal reviewed "review-proposal"
      "bounded-successor" "review-successor"
      (decision "adopt-review-proposal"
        "Caller selected the reviewed successor."))
    "review proposal adoption failed"
  let selected := Kernel.currentDesignItems adopted
  expect (selected.length == 1 &&
      selected.any (·.ref.key == "review-successor"))
    "review proposal adoption selected more than the accepted successor"
  let selectedRef ← match selected with
    | [item] => pure item.ref
    | _ => throw <| IO.userError "adopted successor is not unique"
  expect (adopted.reviewDispositions.any fun disposition =>
      disposition.observation == "bounded-successor" &&
        disposition.decision == .accepted &&
        disposition.successorDesign.any
          (·.ref == selectedRef))
    "review proposal adoption did not retain caller-owned successor authority"

def testReviewTargetCorrection : IO Unit := do
  let requested ← unwrap
    (Kernel.requestReview initialState "target-review" "src/a" .implementation)
    "mistaken Review request failed"
  let reviewed ← unwrap
    (Kernel.recordCleanReview requested "target-review" "reviewer-a")
    "mistaken Review result failed"
  let second ← unwrap
    (Kernel.startWork reviewed "deliver feature B" "implement feature B"
      (decision "feature-b" "Start feature B."))
    "second Work creation failed"
  let corrected ← unwrap
    (Kernel.correctReviewByOutcome second "target-review" "deliver feature B"
      "implement feature B" "src/b"
      (decision "correct-review" "The Review belongs to feature B."))
    "Review target correction failed"
  expect (corrected.reviewRequests.length == 2)
    "Review correction edited history instead of creating a successor"
  let currentReviews :=
    corrected.reviewRequests.filter (Kernel.reviewRequestCurrent corrected)
  expect (currentReviews.length == 2)
    "Review correction globally invalidated the other Work's Review lineage"
  let selectedReviews := Kernel.selectedReviewRequests corrected "target-review"
  let current ← match selectedReviews with
    | [request] => pure request
    | _ => throw <| IO.userError "corrected Review is unavailable"
  expect (current.scope.artifacts == ["src/b"] &&
      current.scope.work.key == corrected.focus.work.key)
    "corrected Review did not select the intended Work and artifact"
  let correctedWork ← match
      corrected.work.find? (·.ref == corrected.focus.work) with
    | some work => pure work
    | none => throw <| IO.userError "corrected Work is unavailable"
  expect
    (correctedWork.authority.reason == "The Review belongs to feature B.")
    "Review correction discarded the caller decision"
  let erroneousCurrent :=
    corrected.work.find? fun work =>
      work.ref.key == initialState.focus.work.key &&
        Kernel.workCurrent corrected work.ref
  expect (erroneousCurrent.all fun work =>
      !work.completionBoundary.any fun member =>
        match member.target with
        | .reviewResolved review => review.version == 0
        | _ => false)
    "mistaken Review remained selected by the erroneous Work"
  let beforeReview ← unwrap (Kernel.finishCurrentTask corrected)
    "intended Task completion failed"
  expect (!Kernel.currentlyComplete beforeReview beforeReview.focus.work)
    "intended Work completed before Review B"
  match Kernel.nextAction beforeReview with
  | some (.satisfy member) =>
      expect (match member.target with
        | .reviewResolved review => review == current.ref
        | _ => false)
        "next did not identify Review B"
  | _ => throw <| IO.userError "Review B was not the exact next action"
  let afterReview ← unwrap
    (Kernel.recordCleanReview beforeReview "target-review" "reviewer-b")
    "Review B result failed"
  expect (Kernel.currentlyComplete afterReview afterReview.focus.work)
    "Work B did not require and accept its own Review result"

def testInterruptionAndReturn : IO Unit := do
  let interrupted ← unwrap
    (Kernel.startInterruption initialState "repair security issue"
      "apply security fix" (decision "interrupt" "Handle the urgent issue."))
    "interruption failed"
  expect interrupted.focus.returnPoint.isSome
    "interruption lost the saved return point"
  match Kernel.returnFromInterruption interrupted with
  | .invalid _ => pure ()
  | _ =>
      throw <| IO.userError
        "unfinished interrupting Work returned to the saved outcome"
  let earlyReplanned ← unwrap
    (Kernel.replanReturnByOutcome interrupted
      "deliver the selected change"
      (decision "early-replan"
        "Caller explicitly replaced the unfinished urgent return plan."))
    "explicit unfinished return-plan replacement failed"
  let earlyReplannedWork ← match
      earlyReplanned.work.find? (·.ref == earlyReplanned.focus.work) with
    | some work => pure work
    | none => throw <| IO.userError "explicitly replanned Work is unavailable"
  expect
    (earlyReplanned.focus.returnPoint.isNone &&
      earlyReplannedWork.authority.reason ==
        "Caller explicitly replaced the unfinished urgent return plan.")
    "explicit unfinished return-plan replacement lost caller authority"
  let beforeSecond := interrupted
  match Kernel.interrupt interrupted initialState.focus.work
      initialState.focus.task with
  | .callerDecision reason =>
      expect
        (reason ==
          "Return after finishing the interrupting outcome, or use replan-return with the caller's selected outcome and reason to replace the return plan.")
        "second interruption did not report the exact caller decision"
      expect (interrupted == beforeSecond)
        "second interruption changed the existing return plan"
  | _ =>
      throw <| IO.userError "second interruption replaced the saved return point"
  let urgentFinished ← unwrap (Kernel.finishCurrentTask interrupted)
    "urgent Task completion failed"
  expect (Kernel.nextAction urgentFinished == some .returnToSavedWork)
    "completed interruption did not expose the exact return action"
  match Kernel.returnFromInterruption urgentFinished with
  | .accepted returned =>
      expect (returned.focus.work == initialState.focus.work &&
          returned.focus.task == initialState.focus.task &&
          returned.focus.returnPoint.isNone)
        "return did not restore the exact saved Work and Task"
  | _ => throw <| IO.userError "unchanged return assumptions required replan"

def testChangedInterruptionAssumptionRequiresReplan : IO Unit := do
  let selectedSource := source "return-design"
  let first : Design.Item :=
    { ref := { key := "return-rule", version := 0 }
      predecessor := none
      statement := "Return to the selected implementation Task."
      role := .functionalRequirement
      source := selectedSource
      dependencies := []
      assurance := { kind := .none, obligations := [] }
      authority :=
        .acceptedByCaller
          { source := selectedSource, reason := "Caller selected the return rule." } }
  let accepted ← match first.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "accepted return design is invalid"
  let baseTask ← match initialState.tasks with
    | [task] => pure task
    | _ => throw <| IO.userError "return Task is not unique"
  let selectedTask := { baseTask with
    basis := Work.DerivationBasis.design [accepted]
    designScope := [accepted] }
  let selected : AgentWorkbench.Kernel.State :=
    { initialState with
      design := { effects := [{ source := selectedSource, content := .design first }] }
      tasks := [selectedTask] }
  let interrupted ← unwrap
    (Kernel.startInterruption selected "repair urgent issue" "apply urgent fix"
      (decision "urgent" "Handle urgent work."))
    "design-scoped interruption failed"
  let urgentFinished ← unwrap (Kernel.finishCurrentTask interrupted)
    "changed-assumption urgent Task completion failed"
  let successorSource := source "return-design-v2"
  let successor : Design.Item :=
    { first with
      ref := { key := "return-rule", version := 1 }
      predecessor := some first.ref
      statement := "Return after selecting the corrected implementation Task."
      source := successorSource
      authority :=
        .acceptedByCaller
          { source := successorSource, reason := "Caller corrected the return rule." } }
  let changed :=
    { urgentFinished with
      design :=
        { effects :=
            urgentFinished.design.effects ++
              [{ source := successorSource, content := .design successor }] } }
  match Kernel.returnFromInterruption changed with
  | .replanRequired assumptions =>
      expect (assumptions.contains (.design first.ref))
        "replan did not identify the changed saved design assumption"
  | _ => throw <| IO.userError "changed saved assumption returned silently"
  let replanned ← unwrap
    (Kernel.replanReturnByOutcome changed "deliver the selected change"
      (decision "replan" "Use the caller-selected corrected outcome."))
    "bounded return replan failed"
  expect replanned.focus.returnPoint.isNone
    "bounded replan left a stale return point"
  let replannedWork ← match replanned.work.find? (·.ref == replanned.focus.work) with
    | some work => pure work
    | none => throw <| IO.userError "replanned Work was not retained"
  expect
    (replannedWork.authority.reason ==
      "Use the caller-selected corrected outcome.")
    "bounded replan discarded the caller decision"

def acceptedFormalItem : Design.Item :=
  let selectedSource := source "formal-design"
  { ref := { key := "inventory", version := 0 }
    predecessor := none
    statement := "A reservation cannot exceed available stock."
    role := .functionalRequirement
    source := selectedSource
    dependencies := []
    assurance :=
      { kind := .formal
        obligations :=
          [{ key := "inventory"
             method := .formal
             description := "Reservation respects stock." }] }
    authority :=
      .acceptedByCaller
        { source := selectedSource, reason := "Caller accepted the rule." } }

def testFormalMeaningAndConformance : IO Unit := do
  let item := acceptedFormalItem
  let spec : Evidence.FormalSpec :=
    { key := "inventory"
      design := item.ref
      modules := ["Inventory.Proof"]
      oracle := some "Inventory.Oracle"
      implementationSurfaces := ["bin/inventory"]
      cases := ["case-equal"]
      adapter := some "test/observe-inventory" }
  let counterexample : Evidence.FormalResult :=
    { spec
      toolIdentity := "lean:v4.30.0"
      checkedClosure := ["Inventory.Rule", "Inventory.Proof"]
      checkedArtifacts := ["sha256:rule", "sha256:proof", "sha256:product-a"]
      oracleArtifact := some "sha256:oracle"
      conformancePassed := some false
      semanticPreview := "available=3, requested=3 => rejected"
      previewIdentity := "sha256:preview" }
  expect (counterexample.currentFor spec [item.ref])
    "checked formal meaning was lost when product conformance failed"
  expect (!counterexample.conformsFor spec [item.ref])
    "counterexample incorrectly satisfied external conformance"
  let corrected := { counterexample with conformancePassed := some true }
  expect (corrected.conformsFor spec [item.ref])
    "corrected product did not restore conformance"
  let executionFailure := { counterexample with conformancePassed := none }
  expect
    (executionFailure.conformanceOutcome == .executionFailure &&
      executionFailure.currentFor spec [item.ref] &&
      !executionFailure.conformsFor spec [item.ref])
    "external execution failure became conformance or a counterexample"
  let history : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects := [{ source := item.source, content := .design item }] }
      formalSpecs := [spec]
      formalResults := [counterexample, corrected] }
  expect (Kernel.formalResultsRequiringVerification history == [corrected])
    "historical formal result displaced the latest verified artifact identity"
  let basis : Work.DerivationBasis := .design [{ ref := item.ref }]
  expect (Kernel.assuranceSatisfiedForBasis history spec.key basis)
    "latest passing formal result did not satisfy its exact Design basis"
  let counterexampleHistory :=
    { history with formalResults := [corrected, counterexample] }
  expect
    (!Kernel.assuranceSatisfiedForBasis counterexampleHistory spec.key basis)
    "older pass satisfied completion after a newer counterexample"
  let failedHistory :=
    { history with formalResults := [corrected, executionFailure] }
  expect
    (!Kernel.assuranceSatisfiedForBasis failedHistory spec.key basis)
    "older pass satisfied completion after a newer execution failure"
  let refreshed ← unwrap
    (Kernel.recordFormalResult failedHistory spec.key corrected.toolIdentity
      corrected.oracleArtifact corrected.checkedClosure
      corrected.checkedArtifacts corrected.conformancePassed
      corrected.semanticPreview corrected.previewIdentity)
    "repeated formal verification failed"
  expect
    (Kernel.formalResultsRequiringVerification refreshed == [corrected])
    "repeating an identical pass left a newer execution failure authoritative"
  expect (Kernel.assuranceSatisfiedForBasis refreshed spec.key basis)
    "repeating an identical pass did not restore exact-basis completion"
  let changedSurface :=
    { spec with implementationSurfaces := ["bin/inventory-v2"] }
  expect (!corrected.currentFor changedSurface [item.ref])
    "changed declared implementation surface retained a stale formal result"

  let evidenceSource := source "same-key-evidence"
  let evidenceItem : Design.Item :=
    { ref := { key := "latency", version := 0 }
      predecessor := none
      statement := "Latency satisfies the selected threshold."
      role := .nonFunctionalRequirement
      source := evidenceSource
      dependencies := []
      assurance :=
        { kind := .evidence
          obligations :=
            [{ key := "inventory"
               method := .evidence
               description := "Observe latency." }] }
      authority :=
        .acceptedByCaller
          { source := evidenceSource, reason := "Caller accepted latency." } }
  let formalAccepted ← match item.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "formal item is not accepted"
  let evidenceAccepted ← match evidenceItem.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "evidence item is not accepted"
  let evidenceSpec : Evidence.Spec :=
    { ref := { key := "inventory", version := 0 }
      observation := "Latency remains below the threshold."
      method := "measure latency"
      environment := "test"
      inputs := []
      acceptanceCondition := "below threshold"
      trustedBoundary := "monotonic clock"
      artifactIdentity := "latency-artifact"
      basis := .design [evidenceAccepted] }
  let sameKeyState : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects :=
            [{ source := item.source, content := .design item },
             { source := evidenceItem.source, content := .design evidenceItem }] }
      evidenceSpecs := [evidenceSpec]
      evidenceResults :=
        [{ spec := evidenceSpec
           observedValue := "below threshold"
           passed := true }] }
  let formalMember : Work.CompletionMember :=
    { target := .assurance "inventory"
      basis := .design [formalAccepted] }
  expect
    (!Kernel.completionMemberSatisfied sameKeyState
      initialState.focus.work formalMember)
    "same-key Evidence from another Design satisfied a formal completion member"

  let summarySource := source "same-key-formal"
  let summaryItem : Design.Item :=
    { ref := { key := "inventory-summary", version := 0 }
      predecessor := none
      statement := "Inventory summaries preserve reservation availability."
      role := .functionalRequirement
      source := summarySource
      dependencies := []
      assurance :=
        { kind := .formal
          obligations :=
            [{ key := "inventory"
               method := .formal
               description := "Check inventory summary availability." }] }
      authority :=
        .acceptedByCaller
          { source := summarySource, reason := "Caller accepted summary." } }
  let summaryAccepted ← match summaryItem.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "summary item is not accepted"
  let summarySpec :=
    { spec with
      design := summaryItem.ref
      modules := ["Inventory.Summary"]
      oracle := some "Inventory.SummaryOracle"
      implementationSurfaces := ["bin/inventory-summary"]
      adapter := some "test/observe-inventory-summary" }
  let formalResult :=
    { corrected with previewIdentity := "formal-result:inventory" }
  let summaryResult : Evidence.FormalResult :=
    { formalResult with
      spec := summarySpec
      checkedClosure := ["Inventory.Summary"]
      previewIdentity := "formal-result:inventory-summary" }
  let isolatedState : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects :=
            [{ source := item.source, content := .design item },
             { source := summaryItem.source, content := .design summaryItem }] }
      formalSpecs := [spec, summarySpec]
      formalResults := [formalResult, summaryResult] }
  let summaryBasis : Work.DerivationBasis := .design [summaryAccepted]
  expect
    (!Kernel.assuranceSatisfiedForBasis isolatedState "inventory" basis
      [formalResult.identity] &&
      Kernel.assuranceSatisfiedForBasis isolatedState "inventory"
        summaryBasis [formalResult.identity])
    "stale identity for one same-key Design invalidated another Design binding"
  expect
    ((Kernel.selectedFormalSpecs isolatedState "inventory").length == 2 &&
      Kernel.selectedFormalSpecsForDesign isolatedState "inventory"
        summaryItem.ref.key == [summarySpec])
    "same-key formal selection could not resolve an exact Design"

def testFormalApprovalAndCompletion : IO Unit := do
  let base ← unwrap (Kernel.finishCurrentTask initialState)
    "initial Task completion failed"
  let proposed ← unwrap
    (Kernel.recordDesign base (source "formal-candidate") "inventory"
      "A reservation cannot exceed available stock."
      .functionalRequirement
      { kind := .formal
        obligations :=
          [{ key := "inventory"
             method := .formal
             description := "Reservation respects stock." }] })
    "formal design recording failed"
  let selected ← unwrap
    (Kernel.selectFormal proposed "inventory" "inventory"
      (some "Inventory.Oracle") ["Inventory.Rule", "Inventory.Proof"] [] [] none)
    "formal selection failed"
  let verified ← unwrap
    (Kernel.recordFormalResult selected "inventory" "lean:v4.30.0"
      (some "sha256:oracle") ["Inventory.Rule", "Inventory.Proof"]
      ["sha256:rule", "sha256:proof"] none
      "stock=3, quantity=3 => available" "sha256:preview")
    "formal meaning verification failed"
  match Kernel.acceptDesign verified "inventory"
      (decision "early-acceptance" "Attempt acceptance before Review.") with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError "formal design was accepted before exact fresh Review"
  match Kernel.requestDesignReview verified "stale-design-inventory"
      "inventory"
      [{ key := "inventory"
         design := { key := "inventory", version := 0 }
         previewIdentity := "sha256:preview" }] with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "stale formal artifacts entered a design meaning Review"
  let requested ← unwrap
    (Kernel.requestDesignReview verified "design-inventory" "inventory")
    "formal design Review request failed"
  let reviewed ← unwrap
    (Kernel.recordCleanReview requested "design-inventory" "reviewer")
    "formal design Review result failed"
  let accepted ← unwrap
    (Kernel.acceptDesign reviewed "inventory"
      (decision "accept-inventory" "Caller accepted the reviewed oracle meaning."))
    "reviewed formal design acceptance failed"
  let tasked ← unwrap
    (Kernel.addTaskForDesign accepted "implement inventory" ["inventory"])
    "formal implementation Task failed"
  let finished ← unwrap (Kernel.finishCurrentTask tasked)
    "formal implementation Task completion failed"
  expect (Kernel.assuranceSatisfied finished "inventory" &&
      Kernel.currentlyComplete finished finished.focus.work)
    "exact verified formal result did not satisfy accepted completion"

def acceptedItem (key statement : String) (version : Nat := 0)
    (predecessor : Option DesignRef := none)
    (dependencies : List DesignRef := [])
    (assurance : Design.AssuranceSelection :=
      { kind := .none, obligations := [] }) : Design.Item :=
  let selectedSource := source s!"{key}-{version}"
  { ref := { key, version }
    predecessor
    statement
    role := .functionalRequirement
    source := selectedSource
    dependencies
    assurance
    authority :=
      .acceptedByCaller
        { source := selectedSource, reason := "Caller accepted this exact meaning." } }

def testSameKeyFormalSuccessorSelection : IO Unit := do
  let assurance : Design.AssuranceSelection :=
    { kind := .formal
      obligations :=
        [{ key := "inventory"
           method := .formal
           description := "Check the selected inventory meaning." }] }
  let predecessor :=
    acceptedItem "inventory" "Inventory meaning v1."
      (assurance := assurance)
  let successor : Design.Item :=
    { predecessor with
      ref := { key := "inventory", version := 1 }
      predecessor := some predecessor.ref
      statement := "Inventory meaning v2."
      source := source "inventory-successor" .caller
      authority := .unaccepted }
  let predecessorSpec : Evidence.FormalSpec :=
    { key := "inventory"
      design := predecessor.ref
      modules := ["Inventory.V1"]
      oracle := some "Inventory.V1Oracle"
      implementationSurfaces := [] }
  let successorSpec : Evidence.FormalSpec :=
    { predecessorSpec with
      design := successor.ref
      modules := ["Inventory.V2"]
      oracle := some "Inventory.V2Oracle" }
  let state : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects :=
            [{ source := predecessor.source, content := .design predecessor },
             { source := successor.source, content := .design successor }] }
      formalSpecs := [predecessorSpec, successorSpec] }
  expect
    (Kernel.selectedFormalSpecsForPreview state "inventory" "inventory" ==
      [successorSpec])
    "same-key formal successor did not displace its accepted predecessor during preview"
  expect
    (Kernel.selectedFormalSpecsForDesign state "inventory" "inventory" ==
      [predecessorSpec])
    "accepted formal completion was retargeted to a same-key proposal"

def testSameKeyEvidenceBasisSelection : IO Unit := do
  let evidenceAssurance : Design.AssuranceSelection :=
    { kind := .evidence
      obligations :=
        [{ key := "checkout-evidence"
           method := .evidence
           description := "Observe the exact checkout rule." }] }
  let checkout :=
    acceptedItem "checkout" "Checkout preserves the selected invariant."
      (assurance := evidenceAssurance)
  let namedLikeEvidence :=
    acceptedItem "checkout-evidence" "Checkout evidence is retained."
      (assurance := evidenceAssurance)
  let finished ← unwrap (Kernel.finishCurrentTask initialState)
    "initial Evidence fixture Task completion failed"
  let based : AgentWorkbench.Kernel.State :=
    { finished with
      design :=
        { effects :=
            [{ source := checkout.source, content := .design checkout },
             { source := namedLikeEvidence.source,
               content := .design namedLikeEvidence }] } }
  let tasked ← unwrap
    (Kernel.addTaskForDesign based "implement both checkout rules"
      ["checkout", "checkout-evidence"])
    "same-key Evidence Task selection failed"
  match Kernel.addEvidence tasked "checkout-evidence" "Observe checkout."
      "run checkout observation" "supported host" []
      "observation passes" "ordinary process" "sha256:ambiguous" with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "same-key Evidence selection silently chose the first Design basis"
  let firstSelected ← unwrap
    (Kernel.addEvidence tasked "checkout-evidence" "Observe checkout."
      "run checkout observation" "supported host" []
      "observation passes" "ordinary process" "sha256:checkout"
      (some "checkout"))
    "first exact Evidence selection failed"
  let secondSelected ← unwrap
    (Kernel.addEvidence firstSelected "checkout-evidence"
      "Observe checkout evidence." "run evidence observation" "supported host"
      [] "observation passes" "ordinary process" "sha256:checkout-evidence"
      (some "checkout-evidence"))
    "second exact Evidence selection failed"
  let firstRecorded ← unwrap
    (Kernel.recordEvidence secondSelected "checkout-evidence"
      "checkout passed" true (some "checkout"))
    "first exact Evidence result failed"
  let bothRecorded ← unwrap
    (Kernel.recordEvidence firstRecorded "checkout-evidence"
      "checkout evidence passed" true (some "checkout-evidence"))
    "second exact Evidence result failed"
  let checkoutAccepted ← match checkout.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "checkout acceptance is unavailable"
  let evidenceAccepted ← match namedLikeEvidence.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "Evidence acceptance is unavailable"
  expect
    (Kernel.assuranceSatisfiedForBasis bothRecorded "checkout-evidence"
        (.design [checkoutAccepted]) &&
      Kernel.assuranceSatisfiedForBasis bothRecorded "checkout-evidence"
        (.design [evidenceAccepted]))
    "same-key Evidence results did not satisfy both exact Design bases"
  let laterTask ← unwrap
    (Kernel.addTaskForDesign bothRecorded "extend only checkout" ["checkout"])
    "later exact-basis Task selection failed"
  let selectedWork ← match laterTask.work.find? (·.ref == laterTask.focus.work) with
    | some work => pure work
    | none => throw <| IO.userError "later Task Work is unavailable"
  let firstMember : Work.CompletionMember :=
    { target := .assurance "checkout-evidence"
      basis := .design [checkoutAccepted] }
  let secondMember : Work.CompletionMember :=
    { target := .assurance "checkout-evidence"
      basis := .design [evidenceAccepted] }
  expect
    (selectedWork.completionBoundary.contains firstMember &&
      selectedWork.completionBoundary.contains secondMember)
    "later Task erased an unrelated same-key assurance basis"
  let laterFinished ← unwrap (Kernel.finishCurrentTask laterTask)
    "focused later Task completion failed"
  let earlierFinished ← unwrap (Kernel.finishCurrentTask laterFinished)
    "unique earlier pending Task was not recoverable after the focus completed"
  expect
    ((earlierFinished.tasks.filter (·.state == .pending)).isEmpty)
    "sequential Task additions left an unreachable pending Task"

def testGeneralLineageAndPublicRecovery : IO Unit := do
  let evidenceAssurance : Design.AssuranceSelection :=
    { kind := .evidence
      obligations :=
        [{ key := "shared"
           method := .evidence
           description := "Observe the selected lineage." }] }
  let predecessor :=
    acceptedItem "lineage" "Lineage v1." (assurance := evidenceAssurance)
  let successorAssurance : Design.AssuranceSelection :=
    { kind := .evidence
      obligations :=
        [{ key := "replacement"
           method := .evidence
           description := "Observe the successor lineage." }] }
  let successor :=
    acceptedItem "lineage" "Lineage v2." 1 (some predecessor.ref)
      (assurance := successorAssurance)
  let predecessorAccepted ← match predecessor.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "predecessor acceptance is unavailable"
  let successorAccepted ← match successor.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "successor acceptance is unavailable"
  let baseWork ← match initialState.work with
    | [work] => pure work
    | _ => throw <| IO.userError "lineage Work is unavailable"
  let staleMember : Work.CompletionMember :=
    { target := .assurance "shared", basis := .design [predecessorAccepted] }
  let based : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects :=
            [{ source := predecessor.source, content := .design predecessor },
             { source := successor.source, content := .design successor }] }
      work :=
        [{ baseWork with
            completionBoundary := staleMember :: baseWork.completionBoundary }] }
  let tasked ← unwrap
    (Kernel.addTaskForDesign based "implement the successor lineage" ["lineage"])
    "successor lineage Task selection failed"
  let selectedWork ← match tasked.work.find? (·.ref == tasked.focus.work) with
    | some work => pure work
    | none => throw <| IO.userError "successor lineage Work is unavailable"
  let successorMember : Work.CompletionMember :=
    { target := .assurance "replacement", basis := .design [successorAccepted] }
  expect
    (!selectedWork.completionBoundary.contains staleMember &&
      selectedWork.completionBoundary.contains successorMember)
    "successor Task retained an impossible predecessor assurance"

  let withSecond ← unwrap (Kernel.addTask initialState "second pending task")
    "second pending Task creation failed"
  let withThird ← unwrap (Kernel.addTask withSecond "third pending task")
    "third pending Task creation failed"
  let thirdDone ← unwrap (Kernel.finishCurrentTask withThird)
    "third pending Task was not publicly finishable"
  let secondDone ← unwrap (Kernel.finishCurrentTask thirdDone)
    "completion-selected second Task was not publicly finishable"
  let allDone ← unwrap (Kernel.finishCurrentTask secondDone)
    "completion-selected first Task was not publicly finishable"
  expect ((allDone.tasks.filter (·.state == .pending)).isEmpty)
    "three pending Tasks left an unreachable completion member"

def testEvidenceAndReviewStayOnSelectedWork : IO Unit := do
  let workRequiredDecision :=
    decision "work-required-profile" "Accept the required Work route."
  let workRequired ← unwrap
    (Kernel.recordCommandProfile initialState workRequiredDecision.source
      (some workRequiredDecision) "work-required-check" "observe work"
      .project ["tool", "required"] none .required)
    "Work-basis required profile failed"
  let workBound ← unwrap
    (Kernel.addEvidence workRequired "required-work-evidence"
      "Observe the required Work route." "run exact argv" "host" []
      "passes" "process" "sha256:required-work" none
      (some "work-required-check") (some .project)
      (some (decision "select-work-required"
        "Select the exact required Work route.")))
    "Work-basis required Evidence binding failed"
  let workCorrectionDecision :=
    decision "correct-work-required" "Correct the required Work route."
  let workCorrected ← unwrap
    (Kernel.recordCommandProfile workBound workCorrectionDecision.source
      (some workCorrectionDecision) "work-required-check" "observe work"
      .project ["tool", "corrected"] none .required)
    "Work-basis required profile correction failed"
  match Kernel.addEvidence workCorrected "required-work-evidence"
      "Observe an unbound replacement." "run another route" "host" []
      "passes" "process" "sha256:unbound-work" with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "an unbound Work-basis Evidence superseded a required profile binding"
  let workAlternateDecision :=
    decision "work-alternate-profile" "Accept the exact alternate Work route."
  let workAlternate ← unwrap
    (Kernel.recordCommandProfile workCorrected workAlternateDecision.source
      (some workAlternateDecision) "work-alternate-check" "observe work"
      .project ["tool", "alternate"] none .recommended)
    "Work-basis alternate profile failed"
  let workRebound ← unwrap
    (Kernel.addEvidence workAlternate "required-work-evidence"
      "Observe the accepted alternate." "run exact argv" "host" []
      "passes" "process" "sha256:alternate-work" none
      (some "work-alternate-check") (some .project)
      (some (decision "select-work-alternate"
        "Select the exact caller-accepted alternate.")))
    "caller-selected Work-basis alternate was rejected"
  expect
    (workRebound.evidenceSpecs.reverse.head?.any fun spec =>
      spec.commandProfile ==
          some ({ key := "work-alternate-check", version := 0 } :
            CommandProfileRef) &&
        spec.commandProfileDecision.isSome)
    "Work-basis alternate lost its exact binding or caller selection"

  let workEvidence ← unwrap
    (Kernel.addEvidence initialState "shared"
      "Observe the Work boundary." "observe work" "host" []
      "work passes" "process" "sha256:work")
    "Work-bound Evidence selection failed"
  let design :=
    acceptedItem "design-evidence" "Observe the Design basis."
      (assurance :=
        { kind := .evidence
          obligations :=
            [{ key := "shared"
               method := .evidence
               description := "Observe the Design basis." }] })
  let withDesign : AgentWorkbench.Kernel.State :=
    { workEvidence with
      design :=
        { effects := [{ source := design.source, content := .design design }] } }
  let tasked ← unwrap
    (Kernel.addTaskForDesign withDesign "implement evidence design"
      ["design-evidence"])
    "Design Evidence Task selection failed"
  let designRequiredDecision :=
    decision "design-required-profile" "Accept the required Design route."
  let designRequired ← unwrap
    (Kernel.recordCommandProfile tasked designRequiredDecision.source
      (some designRequiredDecision) "design-required-check" "observe design"
      .project ["tool", "design-required"] none .required)
    "Design-basis required profile failed"
  let designBound ← unwrap
    (Kernel.addEvidence designRequired "shared" "Observe the Design basis."
      "run exact argv" "host" [] "passes" "process"
      "sha256:required-design" (some "design-evidence")
      (some "design-required-check") (some .project)
      (some (decision "select-design-required"
        "Select the exact required Design route.")))
    "Design-basis required Evidence binding failed"
  let designCorrectionDecision :=
    decision "correct-design-required" "Correct the required Design route."
  let designCorrected ← unwrap
    (Kernel.recordCommandProfile designBound designCorrectionDecision.source
      (some designCorrectionDecision) "design-required-check" "observe design"
      .project ["tool", "design-corrected"] none .required)
    "Design-basis required profile correction failed"
  match Kernel.addEvidence designCorrected "shared"
      "Observe an unbound Design replacement." "run another route" "host" []
      "passes" "process" "sha256:unbound-design"
      (some "design-evidence") with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "an unbound Design-basis Evidence superseded a required profile binding"
  let selected ← unwrap
    (Kernel.addEvidence tasked "shared" "Observe the Design basis."
      "observe design" "host" [] "design passes" "process" "sha256:design"
      (some "design-evidence"))
    "Design-bound Evidence selection failed"
  expect (selected.evidenceSpecs.all (Kernel.evidenceSpecCurrent selected))
    "same-key Evidence crossed its Work/Design basis kind"
  let workRecorded ← unwrap
    (Kernel.recordEvidence selected "shared" "work passed" true)
    "exact Work-bound Evidence result was retargeted"
  let bothRecorded ← unwrap
    (Kernel.recordEvidence workRecorded "shared" "design passed" true
      (some "design-evidence"))
    "exact Design-bound Evidence result was retargeted"
  expect (bothRecorded.evidenceResults.length == 2 &&
      (bothRecorded.evidenceResults.map (·.spec.basis)).eraseDups.length == 2)
    "same-key Evidence did not preserve both exact basis kinds"

  let firstRequested ← unwrap
    (Kernel.requestReview initialState "shared-review" "work-a" .implementation)
    "first Work Review request failed"
  let firstReviewed ← unwrap
    (Kernel.recordCleanReview firstRequested "shared-review" "reviewer-a")
    "first Work Review result failed"
  let secondWork ← unwrap
    (Kernel.startWork firstReviewed "second outcome" "second task"
      (decision "second-work" "Start the second Work."))
    "second Work creation failed"
  let secondRequested ← unwrap
    (Kernel.requestReview secondWork "shared-review" "work-b" .implementation)
    "same-key second Work Review request failed"
  let secondReviewed ← unwrap
    (Kernel.recordCleanReview secondRequested "shared-review" "reviewer-b")
    "same-key second Work Review result was retargeted"
  expect
    (secondReviewed.reviewRequests.all
        (Kernel.reviewRequestCurrent secondReviewed) &&
      (Kernel.selectedReviewRequests secondReviewed "shared-review").length == 1 &&
      secondReviewed.reviewResults.length == 2)
    "same Review key crossed Work lineage or became ambiguous"

def testLocalImpactAndReuse : IO Unit := do
  let inventory0 := acceptedItem "inventory" "Inventory rule v1."
  let reservation0 :=
    acceptedItem "reservation" "Reservation depends on inventory." 0 none
      [inventory0.ref]
  let healthAssurance : Design.AssuranceSelection :=
    { kind := .formal
      obligations :=
        [{ key := "health"
           method := .formal
           description := "Prove the selected health rule." }] }
  let health0 :=
    acceptedItem "health" "Health remains ready."
      (assurance := healthAssurance)
  let health1 : Design.Item :=
    { health0 with
      ref := { key := "health", version := 1 }
      predecessor := some health0.ref
      statement := "Proposed successor health rule."
      source := source "health-1" .agent
      authority := .unaccepted }
  let inventory1 :=
    acceptedItem "inventory" "Inventory rule v2." 1 (some inventory0.ref)
  let inventoryAccepted ← match inventory0.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "inventory design is invalid"
  let reservationAccepted ← match reservation0.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "reservation design is invalid"
  let baseTask ← match initialState.tasks with
    | [task] => pure task
    | _ => throw <| IO.userError "reuse Task is not unique"
  let task := { baseTask with
    basis := Work.DerivationBasis.design
      [inventoryAccepted, reservationAccepted]
    designScope := [inventoryAccepted, reservationAccepted] }
  let healthSpec : Evidence.FormalSpec :=
    { key := "health"
      design := health0.ref
      modules := ["Health.Proof"]
      oracle := some "Health.Oracle"
      implementationSurfaces := []
      cases := []
      adapter := none }
  let healthResult : Evidence.FormalResult :=
    { spec := healthSpec
      toolIdentity := "lean:v4.30.0"
      checkedClosure := ["Health.Rule", "Health.Proof"]
      checkedArtifacts := ["sha256:health-rule", "sha256:health-proof"]
      oracleArtifact := some "sha256:health-oracle"
      conformancePassed := none
      semanticPreview := "ready=true"
      previewIdentity := "sha256:health-preview" }
  let healthReviewRef : ReviewRef := { key := "health-review", version := 0 }
  let healthScope : Review.Scope :=
    { work := initialState.focus.work
      design := [health0.ref]
      task := none
      purpose := .designMeaning
      artifacts := [healthResult.previewIdentity] }
  let healthRequest : Review.Request := { ref := healthReviewRef, scope := healthScope }
  let healthReview : Review.Result :=
    { review := healthReviewRef
      scope := healthScope
      reviewer := "health-reviewer"
      observations := [] }
  let state : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects :=
            [inventory0, reservation0, health0, inventory1, health1].map fun item =>
              { source := item.source, content := Design.EffectContent.design item } }
      tasks := [task]
      formalSpecs := [healthSpec]
      formalResults := [healthResult]
      reviewRequests := [healthRequest]
      reviewResults := [healthReview] }
  expect state.wellFormed
    "local formal reuse fixture is not a valid project state"
  expect (Kernel.selectedFormalSpecs state "health" == [healthSpec])
    "an unselected proposal displaced the accepted formal selection"
  expect (Kernel.formalResultsRequiringVerification state == [healthResult])
    "an unselected proposal hid accepted formal artifacts from verification"
  let affected := Kernel.affectedDesigns state
  expect (affected.any (·.key == "inventory") &&
      affected.any (·.key == "reservation") &&
      !affected.any (·.key == "health"))
    "local correction did not isolate its declared affected closure"
  expect (healthResult.currentFor healthSpec (Kernel.currentDesignRefs state))
    "unrelated formal result identity was not reusable"
  expect (Kernel.reviewRequestCurrent state healthRequest &&
      Kernel.reviewResolved state healthReviewRef)
    "unrelated Review identity was not reusable"

def testBoundedReuseReview : IO Unit := do
  let item := acceptedFormalItem
  let accepted ← match item.acceptedRef? with
    | some selected => pure selected
    | none => throw <| IO.userError "bounded reuse design is not accepted"
  let spec : Evidence.FormalSpec :=
    { key := "inventory"
      design := item.ref
      modules := ["Inventory.Rule", "Inventory.Proof"]
      oracle := some "Inventory.Oracle"
      implementationSurfaces := ["bin/inventory"]
      cases := ["case-equal"]
      adapter := some "test/observe-inventory" }
  let result : Evidence.FormalResult :=
    { spec
      toolIdentity := "lean:v4.30.0"
      checkedClosure := ["Inventory.Rule", "Inventory.Proof"]
      checkedArtifacts := ["sha256:rule", "sha256:proof", "sha256:surface"]
      oracleArtifact := some "sha256:oracle"
      conformancePassed := some true
      semanticPreview := "stock=3, quantity=3 => available"
      previewIdentity := "sha256:reuse-preview" }
  let baseTask ← match initialState.tasks with
    | [task] => pure task
    | _ => throw <| IO.userError "bounded reuse Task is not unique"
  let task :=
    { baseTask with
      basis := Work.DerivationBasis.design [accepted]
      designScope := [accepted]
      state := .satisfied }
  let baseWork ← match initialState.work with
    | [work] => pure work
    | _ => throw <| IO.userError "bounded reuse Work is not unique"
  let basis := Work.DerivationBasis.design [accepted]
  let work :=
    { baseWork with
      completionBoundary :=
        [{ target := .taskSatisfied task.ref, basis },
         { target := .assurance "inventory", basis }] }
  let base : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects := [{ source := item.source, content := .design item }] }
      work := [work]
      tasks := [task]
      formalSpecs := [spec]
      formalResults := [result] }
  expect (base.wellFormed && Kernel.currentlyComplete base base.focus.work)
    "bounded reuse fixture is not complete before an unrelated change"
  expect (!Kernel.currentlyComplete base base.focus.work
    [result.identity])
    "stale formal assurance remained usable by Kernel completion"
  let saved : Work.ReturnPoint :=
    { work := base.focus.work
      task := base.focus.task
      assumptions := [.workBoundary base.focus.work] }
  let staleInterrupt :=
    { base with
      focus := { base.focus with returnPoint := some saved } }
  match Kernel.returnFromInterruption staleInterrupt
      [result.identity] with
  | .invalid _ => pure ()
  | _ =>
      throw <| IO.userError
        "stale selected assurance allowed return from interrupting Work"
  let requested ← unwrap
    (Kernel.requestReview base "inventory-reuse" "notes.txt" .reuseDecision)
    "bounded reuse Review request failed"
  expect (!Kernel.currentlyComplete requested requested.focus.work)
    "unresolved bounded reuse Review did not hold completion"
  expect (Kernel.formalResultsRequiringVerification requested == [result])
    "bounded reuse Review replaced the unchanged FormalResult identity"
  let reviewed ← unwrap
    (Kernel.recordCleanReview requested "inventory-reuse" "reuse-reviewer")
    "bounded reuse Review result failed"
  expect (Kernel.currentlyComplete reviewed reviewed.focus.work)
    "clean bounded reuse Review did not restore completion"
  expect (Kernel.formalResultsRequiringVerification reviewed == [result])
    "clean bounded reuse Review replaced the reusable FormalResult identity"

def testCommandProfileAndKPTInvariants : IO Unit := do
  let globalDecision := decision "profile-global" "Use the accepted global check."
  let withGlobal ← unwrap
    (Kernel.recordCommandProfile initialState globalDecision.source
      (some globalDecision) "global-check" "verify the implementation"
      .project ["lake", "test"] none .required)
    "global Command Profile recording failed"
  let workDecision := decision "profile-work" "Use the accepted Work check."
  let withWork ← unwrap
    (Kernel.recordCommandProfile withGlobal workDecision.source
      (some workDecision) "work-check" "verify the implementation"
      (.work withGlobal.focus.work.key) ["lake", "build"] (some ".")
      .recommended)
    "Work Command Profile recording failed"
  expect
    ((Kernel.applicableCommandProfiles withWork "verify the implementation"
      |>.map (·.ref.key)) == ["global-check", "work-check"])
    "project and exact-Work Command Profiles acquired silent precedence"
  let proposedGlobalCorrection ← unwrap
    (Kernel.recordCommandProfile withGlobal
      (source "proposed-global-correction" .agent) none "global-check"
      "verify the implementation" .project ["lake", "test", "--", "all"]
      none .required)
    "proposed global Command Profile correction failed"
  let acceptedGlobalCorrectionDecision :=
    decision "accept-global-correction" "Accept the exact proposed correction."
  let acceptedGlobalCorrection ← unwrap
    (Kernel.acceptCommandProfile proposedGlobalCorrection "global-check"
      .project acceptedGlobalCorrectionDecision)
    "proposed global Command Profile correction acceptance failed"
  expect
    (acceptedGlobalCorrection.commandProfiles.reverse.head?.any fun profile =>
      profile.ref ==
          ({ key := "global-check", version := 2 } : CommandProfileRef) &&
        profile.predecessor ==
          some ({ key := "global-check", version := 0 } : CommandProfileRef) &&
        profile.argv == ["lake", "test", "--", "all"] &&
        profile.source.kind == .agent &&
        profile.authority ==
          .acceptedByCaller acceptedGlobalCorrectionDecision)
    "accepted Command Profile correction lost exact payload or predecessor"
  let otherWorkDecision :=
    decision "other-work" "Start the independent other Work."
  let otherWork ← unwrap
    (Kernel.startWork withWork "deliver another change" "implement another change"
      otherWorkDecision)
    "second Work creation failed"
  expect
    ((Kernel.applicableCommandProfiles otherWork "verify the implementation"
      |>.map (·.ref.key)) == ["global-check"])
    "an exact-Work Command Profile leaked into another Work"
  let revisedBoundary ← unwrap
    (Kernel.addTask withWork "revise the same Work boundary")
    "same-Work boundary revision failed"
  let workCorrectionDecision :=
    decision "profile-work-correction" "Correct the same exact Work profile."
  let correctedWorkScope ← unwrap
    (Kernel.recordCommandProfile revisedBoundary
      workCorrectionDecision.source (some workCorrectionDecision)
      "work-check" "verify the implementation"
      (.work revisedBoundary.focus.work.key) ["lake", "build", "AgentWorkbench"]
      (some ".") .recommended)
    "same-Work Command Profile correction failed"
  expect
    ((correctedWorkScope.commandProfiles.filter fun profile =>
      profile.ref.key == "work-check" &&
        Kernel.commandProfileCurrent correctedWorkScope profile).map
          (·.ref.version) == [1])
    "a Work boundary revision split one semantic Command Profile scope"
  let sharedProjectDecision :=
    decision "shared-project-profile" "Select the project-scoped shared route."
  let sharedProject ← unwrap
    (Kernel.recordCommandProfile withWork sharedProjectDecision.source
      (some sharedProjectDecision) "shared-check" "verify shared scope"
      .project ["lake", "test"] none .recommended)
    "same-key project profile failed"
  let sharedWorkDecision :=
    decision "shared-work-profile" "Select the Work-scoped shared route."
  let sharedScopes ← unwrap
    (Kernel.recordCommandProfile sharedProject sharedWorkDecision.source
      (some sharedWorkDecision) "shared-check" "verify shared scope"
      (.work sharedProject.focus.work.key) ["lake", "build"] none
      .recommended)
    "same-key Work profile failed"
  match Kernel.addEvidence sharedScopes "ambiguous-shared"
      "Observe a shared route." "run exact argv" "supported host" []
      "passes" "ordinary process" "sha256:ambiguous" none
      (some "shared-check") none
      (some (decision "ambiguous-selection" "Select one exact shared route.")) with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "same-key cross-scope profiles were selected without a bounded scope"
  let selectedSharedWork ← unwrap
    (Kernel.addEvidence sharedScopes "selected-shared"
      "Observe the Work shared route." "run exact argv" "supported host" []
      "passes" "ordinary process" "sha256:shared-work" none
      (some "shared-check") (some (.work sharedScopes.focus.work.key))
      (some (decision "shared-selection" "Select the exact Work route.")))
    "same-key Work profile selection failed"
  expect
    (selectedSharedWork.evidenceSpecs.reverse.head?.bind
      (·.commandProfile) ==
        some ({ key := "shared-check", version := 1 } : CommandProfileRef))
    "bounded same-key profile selection did not freeze the exact Work version"
  let proposed ← unwrap
    (Kernel.recordCommandProfile withWork
      (source "profile-proposal" .agent) none "agent-check"
      "verify the implementation" .project ["lake", "env", "lean"]
      none .recommended)
    "agent Command Profile proposal failed"
  match Kernel.addEvidence proposed "proposal-evidence" "Observe proposal."
      "run selected argv" "supported host" [] "passes"
      "ordinary process" "sha256:proposal" none (some "agent-check") none
      (some (decision "proposal-selection" "Select the proposed route.")) with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "an unaccepted Command Profile was selected by Evidence"
  let selectedGlobal ← unwrap
    (Kernel.addEvidence proposed "global-evidence" "Observe global command."
      "run selected argv" "supported host" [] "passes"
      "ordinary process" "sha256:global" none (some "global-check") none
      (some (decision "global-selection" "Select the exact global route.")))
    "global Command Profile Evidence selection failed"
  let globalRecorded ← unwrap
    (Kernel.recordEvidence selectedGlobal "global-evidence" "passed" true)
    "global Command Profile Evidence recording failed"
  let globalResult ← match globalRecorded.evidenceResults.reverse.head? with
    | some result => pure result
    | none => throw <| IO.userError "global Command Profile result is missing"
  expect
    (globalResult.spec.commandProfile ==
      some ({ key := "global-check", version := 0 } : CommandProfileRef))
    "Evidence did not retain the exact accepted Command Profile version"
  let selectedWork ← unwrap
    (Kernel.addEvidence globalRecorded "work-evidence" "Observe Work command."
      "run selected argv" "supported host" [] "passes"
      "ordinary process" "sha256:work" none (some "work-check") none
      (some (decision "work-selection" "Select the exact Work route.")))
    "Work Command Profile Evidence selection failed"
  let bothRecorded ← unwrap
    (Kernel.recordEvidence selectedWork "work-evidence" "passed" true)
    "Work Command Profile Evidence recording failed"
  let workResult ← match bothRecorded.evidenceResults.reverse.head? with
    | some result => pure result
    | none => throw <| IO.userError "Work Command Profile result is missing"
  let correctedDecision :=
    decision "profile-global-correction" "Correct only the global check."
  let corrected ← unwrap
    (Kernel.recordCommandProfile bothRecorded correctedDecision.source
      (some correctedDecision) "global-check" "verify the implementation"
      .project ["lake", "test", "--", "all"] none .required)
    "Command Profile correction failed"
  expect (!Kernel.evidenceResultCurrent corrected globalResult)
    "a corrected Command Profile left its exact Evidence consumer current"
  expect (Kernel.evidenceResultCurrent corrected workResult)
    "a Command Profile correction invalidated an unrelated Evidence consumer"
  match Kernel.recordCommandDeviation corrected "global-check"
      ["lake", "build"] none "Use a faster route."
      (source "required-deviation" .agent) with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "an agent reason bypassed a required Command Profile"
  match Kernel.recordKPT sharedScopes (source "dangling-relation" .agent)
      "codex" none "dangling-relation" .problem .project
      "Do not retain a dangling relation."
      (some (.commandProfile "missing-profile")) with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError "a dangling KPT relation committed"
  match Kernel.recordKPT sharedScopes (source "ambiguous-relation" .agent)
      "codex" none "ambiguous-relation" .problem .project
      "Do not guess between scoped profiles."
      (some (.commandProfile "shared-check")) with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError "an ambiguous KPT relation committed"
  let relatedKPT ← unwrap
    (Kernel.recordKPT corrected (source "resolved-relation" .agent) "codex"
      none "resolved-relation" .keep .project
      "Retain the exact current Task relation."
      (some (.task "implement the selected change")))
    "an exact KPT Task relation failed"
  expect
    (relatedKPT.kpt.reverse.head?.bind (·.relation) ==
      some (.task ({ key := "task", version := 0 } : TaskRef)))
    "a KPT relation did not freeze the exact resolved Task"
  let relatedProfile ← unwrap
    (Kernel.resolveKPTRelation corrected (.commandProfile "global-check"))
    "current Command Profile relation failed"
  expect
    (relatedProfile ==
      .commandProfile
        ({ key := "global-check", version := 1 } : CommandProfileRef))
    "Command Profile relation did not freeze the exact current version"
  let relatedEvidence ← unwrap
    (Kernel.resolveKPTRelation corrected (.evidenceResult "work-evidence"))
    "current Evidence result relation failed"
  expect
    (relatedEvidence ==
      .evidenceResult
        { evidence := { key := "work-evidence", version := 0 }
          observedValue := "passed"
          passed := true })
    "Evidence relation did not freeze the exact current result payload"
  let relationReviewRequested ← unwrap
    (Kernel.requestReview initialState "relation-review" "artifact"
      .implementation)
    "KPT relation Review request failed"
  let relationObservation : Review.Observation :=
    { key := "relation-risk"
      kind := .risk
      summary := "Retain the reviewed relation."
      evidence := "The exact observation exists." }
  let relationReviewed ← unwrap
    (Kernel.recordReviewResult relationReviewRequested "relation-review"
      "relation-reviewer" relationObservation)
    "KPT relation Review observation failed"
  let relatedReview ← unwrap
    (Kernel.resolveKPTRelation relationReviewed
      (.reviewObservation "relation-review" "relation-risk"))
    "current Review observation relation failed"
  expect
    (relatedReview ==
      .reviewObservation
        { review := { key := "relation-review", version := 0 }
          observation := "relation-risk" })
    "Review relation did not freeze the exact Review and observation"
  let relationDesign ← unwrap
    (Kernel.recordDesign initialState (source "relation-design" .agent)
      "relation-design" "Retain the exact Design relation." .decision
      { kind := .none, obligations := [] })
    "KPT relation Design candidate failed"
  let relatedDesign ← unwrap
    (Kernel.resolveKPTRelation relationDesign (.design "relation-design"))
    "current Design relation failed"
  expect
    (relatedDesign ==
      .design ({ key := "relation-design", version := 0 } : DesignRef))
    "Design relation did not freeze the exact current candidate"
  let deviationPending ← unwrap
    (Kernel.addEvidence corrected "deviated-evidence"
      "Observe the actual recommended route." "run exact argv"
      "supported host" [] "passes" "ordinary process"
      "sha256:deviated" none (some "work-check") none
      (some (decision "deviation-selection"
        "Select the exact recommended route.")))
    "recommended deviation Evidence selection failed"
  let deviated ← unwrap
    (Kernel.recordCommandDeviation deviationPending "work-check"
      ["lake", "build", "AgentWorkbench"] none "Narrow diagnostic."
      (source "recommended-deviation" .agent))
    "recommended Command Profile deviation failed"
  expect (deviated.commandDeviations.length == 1 &&
      deviated.commandDeviations.head?.bind (·.evidence) ==
        some ({ key := "deviated-evidence", version := 0 } : EvidenceRef))
    "recommended deviation was promoted into another authority mechanism"
  match Kernel.recordEvidence deviated "deviated-evidence" "passed" true with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "deviated actual argv was recorded as if the recommended profile ran"
  let completionBefore := Kernel.currentlyComplete deviated deviated.focus.work
  let missingBefore := Kernel.missingCompletion deviated deviated.focus.work
  let agentAuthor := source "standalone-agent-kpt" .agent
  let standaloneKPT ← unwrap
    (Kernel.recordKPT deviated agentAuthor "codex" none "standalone-lesson" .try
      .project "Try the accepted project route." none)
    "standalone agent KPT failed"
  expect
    ((Kernel.currentKPT standaloneKPT).any
      (·.statement == "Try the accepted project route."))
    "standalone agent-authored KPT was hidden as a caller correction"
  let correctedStandalone ← unwrap
    (Kernel.recordKPT standaloneKPT (source "new-action-token" .agent)
      "codex" none "standalone-lesson"
      .keep .project "Keep using the accepted project route."
      none)
    "agent KPT self-correction failed"
  expect
    (((Kernel.currentKPT correctedStandalone).filter
      (·.ref.key == "standalone-lesson") |>.map (·.statement)) ==
        ["Keep using the accepted project route."])
    "an author could not supersede its own KPT entry"
  let callerSupersessionDecision :=
    decision "caller-supersedes-agent-kpt"
      "Caller supersedes the exact current agent-only entry."
  let parallelStandalone ← unwrap
    (Kernel.recordKPT correctedStandalone
      (source "parallel-standalone-author" .agent) "other-agent" none
      "standalone-lesson" .try .project
      "Try the other author's independent route." none)
    "parallel standalone KPT author failed"
  match Kernel.recordKPT parallelStandalone callerSupersessionDecision.source
      "caller" (some callerSupersessionDecision) "standalone-lesson" .keep
      .project "Use the caller-selected accepted project route." none with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "caller KPT guessed among parallel standalone authors"
  match Kernel.acceptKPT parallelStandalone "standalone-lesson" .project
      "codex" callerSupersessionDecision with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "standalone KPT was incorrectly exposed through adoption"
  let callerSupersededStandalone ← unwrap
    (Kernel.recordKPT parallelStandalone callerSupersessionDecision.source
      "caller" (some callerSupersessionDecision) "standalone-lesson" .keep
      .project "Use the caller-selected accepted project route." none
      (some "codex"))
    "caller could not supersede an agent-only current KPT entry"
  expect
    (callerSupersededStandalone.kpt.reverse.head?.any fun entry =>
      entry.predecessor ==
          some ({ key := "standalone-lesson", version := 1 } : KPTRef) &&
        entry.authority == .callerOwned callerSupersessionDecision)
    "caller KPT supersession lost the exact agent-only predecessor"
  let kptDecision := decision "caller-kpt" "Retain the caller's Problem."
  let callerKPT ← unwrap
    (Kernel.recordKPT correctedStandalone kptDecision.source "caller"
      (some kptDecision) "review-bias" .problem .project
      "A resumed reviewer carries implementation context." none)
    "caller KPT recording failed"
  let agentKPT ← unwrap
    (Kernel.recordKPT callerKPT (source "agent-kpt" .agent) "codex" none
      "review-bias" .try .project
      "Reuse the reviewer context anyway." none)
    "agent KPT correction proposal failed"
  expect
    (agentKPT.kpt.reverse.head?.any fun candidate =>
      candidate.predecessor ==
        some ({ key := "review-bias", version := 0 } : KPTRef))
    "an agent KPT correction did not bind the exact current caller entry"
  let parallelKPT ← unwrap
    (Kernel.recordKPT agentKPT (source "other-agent-kpt" .agent) "other-agent"
      none "review-bias" .try .project "Use another fresh reviewer."
      none)
    "parallel author KPT proposal failed"
  expect
    (((Kernel.pendingKPTCandidates parallelKPT).map (·.author)
      |>.eraseDups) == ["codex", "other-agent"])
    "parallel KPT authors were not exposed as exact adoption alternatives"
  let currentCallerKPT :=
    (Kernel.currentKPT agentKPT).find? (·.ref.key == "review-bias")
  expect
    (currentCallerKPT.any
      (·.statement == "A resumed reviewer carries implementation context."))
    "an agent KPT correction hid the caller-owned KPT"
  let adoptionDecision :=
    decision "kpt-adoption" "Adopt the exact agent-authored correction."
  let adoptedKPT ← unwrap
    (Kernel.acceptKPT parallelKPT "review-bias" .project "codex"
      adoptionDecision)
    "agent KPT adoption failed"
  expect
    (!(Kernel.pendingKPTCandidates adoptedKPT).any
        (·.ref.key == "review-bias") &&
      (Kernel.currentKPT adoptedKPT).any
        (·.statement == "Reuse the reviewer context anyway."))
    "an adopted KPT correction remained pending or failed to become current"
  match Kernel.acceptKPT adoptedKPT "review-bias" .project "codex"
      adoptionDecision with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError "the same KPT candidate was adopted repeatedly"
  expect
    (Kernel.currentlyComplete agentKPT agentKPT.focus.work == completionBefore &&
      Kernel.missingCompletion agentKPT agentKPT.focus.work == missingBefore &&
      agentKPT.work == deviated.work &&
      agentKPT.tasks == deviated.tasks &&
      agentKPT.evidenceSpecs == deviated.evidenceSpecs &&
      agentKPT.reviewRequests == deviated.reviewRequests)
    "KPT changed assurance, Review, next-action, or completion facts"
  let atomicDecision :=
    decision "atomic-kpt-profile" "Record one source and both conclusions."
  let atomic ← unwrap
    (Kernel.recordKPTWithCommandProfile adoptedKPT atomicDecision.source
      "caller" (some atomicDecision) "stable-check" .keep .project
      "The release check is stable." none "release-check"
      "verify the release" ["lake", "test"] none .recommended)
    "atomic KPT and Command Profile recording failed"
  expect
    (atomic.kpt.length == adoptedKPT.kpt.length + 1 &&
      atomic.commandProfiles.length == adoptedKPT.commandProfiles.length + 1)
    "atomic KPT and Command Profile did not commit both facts"
  let instructionDecision :=
    decision "atomic-kpt-instruction" "Record the lesson and instruction."
  let instructed ← unwrap
    (Kernel.recordKPTWithInstruction atomic instructionDecision "caller"
      "instruction-lesson" .try .project
      "Consult the accepted profile before validation." none
      "Consult the accepted Command Profile before validation.")
    "atomic KPT and instruction recording failed"
  expect
    (instructed.kpt.length == atomic.kpt.length + 1 &&
      instructed.design.instructions.reverse.head?.any fun instruction =>
        instruction.statement ==
            "Consult the accepted Command Profile before validation." &&
          instruction.authority == instructionDecision)
    "atomic KPT and existing instruction route did not commit both facts"
  let correctionDecision :=
    decision "atomic-kpt-profile-correction"
      "Correct the lesson and exact command route together."
  let correctedAtomic ← unwrap
    (Kernel.recordKPTWithCommandProfile instructed correctionDecision.source
      "caller" (some correctionDecision) "stable-check" .try .project
      "Use the corrected exact release route."
      (some (.commandProfile "release-check"))
      "release-check" "verify the release"
      ["lake", "test", "--", "all"] (some "validation")
      .required)
    "atomic KPT and successor Command Profile correction failed"
  let correctedKPT ← match correctedAtomic.kpt.reverse.head? with
    | some entry => pure entry
    | none => throw <| IO.userError "corrected atomic KPT is missing"
  let correctedProfile ← match correctedAtomic.commandProfiles.reverse.head? with
    | some profile => pure profile
    | none => throw <| IO.userError "corrected atomic profile is missing"
  expect
    (correctedKPT.predecessor ==
        some ({ key := "stable-check", version := 0 } : KPTRef) &&
      correctedKPT.relation ==
        some
          (.commandProfile
            ({ key := "release-check", version := 1 } :
              CommandProfileRef)) &&
      correctedKPT.scope == .project &&
      correctedKPT.source == correctionDecision.source &&
      correctedKPT.authority == .callerOwned correctionDecision &&
      correctedProfile.predecessor ==
        some ({ key := "release-check", version := 0 } : CommandProfileRef) &&
      correctedProfile.scope == .project &&
      correctedProfile.argv == ["lake", "test", "--", "all"] &&
      correctedProfile.cwd == some "validation" &&
      correctedProfile.disposition == .required &&
      correctedProfile.source == correctionDecision.source &&
      correctedProfile.authority == .acceptedByCaller correctionDecision)
    "atomic correction lost exact predecessor, payload, source, or authority"
  match Kernel.recordKPTWithCommandProfile atomic atomicDecision.source
      "caller" (some atomicDecision) "invalid-atomic" .keep .project "Invalid." none
      "invalid-profile" "invalid" [] none .recommended with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "an invalid atomic Command Profile committed its KPT half"
  let designAtomic ← unwrap
    (Kernel.recordKPTWithDesignCandidate atomic atomicDecision.source
      "caller" (some atomicDecision) "design-lesson" .try .project
      "Consider a bounded follow-up." none "follow-up-design"
      "Add the bounded follow-up." .decision
      { kind := .none, obligations := [] })
    "atomic KPT and Design candidate recording failed"
  expect
    (designAtomic.design.designItems.reverse.find?
      (·.ref.key == "follow-up-design") |>.any
        (·.authority == .unaccepted))
    "KPT created a caller-accepted Design without the ordinary review flow"
  match Kernel.acceptDesignWithKPT designAtomic "follow-up-design"
      atomicDecision "caller" "premature-lesson" .keep .project
      "Do not bypass Design Review." none with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "KPT accompanied Design acceptance before exact Review prerequisites"
  let reviewedCandidate ← unwrap
    (Kernel.recordDesign initialState (source "accepted-kpt-design")
      "accepted-kpt-design" "Accept the reviewed design with one KPT."
      .decision { kind := .none, obligations := [] })
    "KPT acceptance Design candidate failed"
  let reviewRequested ← unwrap
    (Kernel.requestDesignReview reviewedCandidate "accepted-kpt-review"
      "accepted-kpt-design")
    "KPT acceptance Design Review request failed"
  let reviewClean ← unwrap
    (Kernel.recordCleanReview reviewRequested "accepted-kpt-review"
      "fresh-design-reviewer")
    "KPT acceptance clean Design Review failed"
  let acceptanceDecision :=
    decision "accepted-kpt-decision" "Accept the reviewed design and lesson."
  let acceptedWithKPT ← unwrap
    (Kernel.acceptDesignWithKPT reviewClean "accepted-kpt-design"
      acceptanceDecision "caller" "accepted-design-lesson" .keep .project
      "The exact reviewed meaning was sufficient." none)
    "atomic Design acceptance and KPT recording failed"
  expect
    ((Kernel.currentDesignRefs acceptedWithKPT).any
      (·.key == "accepted-kpt-design") &&
      (Kernel.currentKPT acceptedWithKPT).any
        (·.ref.key == "accepted-design-lesson"))
    "KPT could not accompany ordinary successful Design acceptance"

def run : IO Unit := do
  testFlatCompletionAndPhase
  testPhaseGroupsTasksWithoutSemanticEffect
  testEvidenceCurrentness
  testSelectedEvidenceControlsCompletion
  testCompositeSourceEffects
  testReviewAuthority
  testReviewProposalAuthority
  testReviewTargetCorrection
  testInterruptionAndReturn
  testChangedInterruptionAssumptionRequiresReplan
  testFormalMeaningAndConformance
  testFormalApprovalAndCompletion
  testSameKeyFormalSuccessorSelection
  testSameKeyEvidenceBasisSelection
  testGeneralLineageAndPublicRecovery
  testEvidenceAndReviewStayOnSelectedWork
  testLocalImpactAndReuse
  testBoundedReuseReview
  testCommandProfileAndKPTInvariants
  IO.println "kernel tests: pass"

end AgentWorkbench.Tests.Kernel
