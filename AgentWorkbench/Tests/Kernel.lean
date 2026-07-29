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
  expect (currentReviews.length == 1)
    "Review correction left an ambiguous current Review"
  let current ← match currentReviews with
    | [request] => pure request
    | _ => throw <| IO.userError "corrected Review is unavailable"
  expect (current.scope.artifacts == ["src/b"] &&
      current.scope.work.key == corrected.focus.work.key)
    "corrected Review did not select the intended Work and artifact"
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
  let beforeSecond := interrupted
  match Kernel.interrupt interrupted initialState.focus.work
      initialState.focus.task with
  | .callerDecision reason =>
      expect
        (reason ==
          "Return to the saved outcome first, or explicitly replace the return plan.")
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
    { interrupted with
      design :=
        { effects :=
            interrupted.design.effects ++
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
  let history : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects := [{ source := item.source, content := .design item }] }
      formalSpecs := [spec]
      formalResults := [counterexample, corrected] }
  expect (Kernel.formalResultsRequiringVerification history == [corrected])
    "historical formal result displaced the latest verified artifact identity"
  let changedSurface :=
    { spec with implementationSurfaces := ["bin/inventory-v2"] }
  expect (!corrected.currentFor changedSurface [item.ref])
    "changed declared implementation surface retained a stale formal result"

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
  testLocalImpactAndReuse
  testBoundedReuseReview
  IO.println "kernel tests: pass"

end AgentWorkbench.Tests.Kernel
