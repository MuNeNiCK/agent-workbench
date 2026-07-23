import AgentWorkbench.Cli.Program

open AgentWorkbench
open AgentWorkbench.Domain

namespace AgentWorkbench.Tests.KernelLaws

def firstActivation : Domain.Work.Activation :=
  { id := ⟨1⟩, work := ⟨1⟩, status := .active, readyToResume := false }

def secondActivation : Domain.Work.Activation :=
  { id := ⟨2⟩, work := ⟨2⟩, status := .active, readyToResume := false }

def thirdActivation : Domain.Work.Activation :=
  { id := ⟨3⟩, work := ⟨3⟩, status := .active, readyToResume := false }

def firstWork : Domain.Work.WorkUnit :=
  { id := ⟨1⟩, status := .open, owner := "owner" }

def secondWork : Domain.Work.WorkUnit :=
  { id := ⟨2⟩, status := .open, owner := "owner" }

def thirdWork : Domain.Work.WorkUnit :=
  { id := ⟨3⟩, status := .open, owner := "owner" }

def evidenceDesign : Domain.Design.DesignVersion :=
  { id := ⟨1⟩
    revision := ⟨1⟩
    owner := "owner"
    contentDigest := "sha256:kernel-law-design"
    requirements := [{ key := "evidence-integrity", active := true }]
    decisions := ["evidence binds an exact design version"]
    validationGates := ["evidence-matrix"] }

def suspensionContext : Domain.Work.SuspensionContext :=
  { reason := "interrupted"
    returnPoint := "next transition"
    assumptions := ["design and repository remain current"]
    resumeConditions := ["resume readiness confirmed"] }

def parentWork : Domain.Work.WorkUnit :=
  { id := ⟨4⟩, status := .open, owner := "owner" }

def blockedRelatedWork : Domain.Work.WorkUnit :=
  { id := ⟨5⟩, status := .open, owner := "owner" }

def completionPlan : Domain.Lifecycle.CompletionPlan :=
  { work := firstWork.id
    relatedWork := [
      { work := secondWork.id, kind := .child },
      { work := thirdWork.id, kind := .dependency }]
    phases := ["phase-1"]
    tasks := ["task-1", "task-after-validation"]
    checklists := ["checklist-1"]
    reviews := [⟨3001⟩]
    findings := ["blocking-review-finding"]
    validations := ["validation-1"]
    repositories := ["repository-1"]
    corrections := ["correction-1"]
    workRecords := ["record-1"] }

def initializeWork (state : Kernel.Replay.State)
    (work : Domain.Work.WorkUnit)
    (activation : Domain.Work.Activation) : Kernel.Decide.Command :=
  .initializeWork state.revision work activation

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw <| IO.userError message

def testEvidenceScopeSatisfaction : IO Unit := do
  let obligation : Domain.Evidence.Obligation := {
    work := ⟨1⟩, key := "evidence-integrity", revision := ⟨7⟩
    commandProfile := "kernel-laws", invocation := ".lake/build/bin/kernel-laws"
    repository := "main", snapshot := "commit:exact"
    artifactDigest := "sha256:exact", current := true
    requirements := ["evidence-integrity"], expectedProducer := "kernel-law-runner"
    expectedObservation := "kernel-law-observation"
    design := ⟨1⟩, designRevision := ⟨1⟩ }
  let good : Domain.Evidence.Evidence := {
    id := ⟨1⟩, work := ⟨1⟩, obligation := "evidence-integrity", revision := ⟨7⟩
    commandProfile := "kernel-laws", invocation := ".lake/build/bin/kernel-laws"
    exitCode := 0, repository := "main", snapshot := "commit:exact"
    artifactDigest := "sha256:exact", current := true
    requirements := obligation.requirements, producer := obligation.expectedProducer
    observedAt := "kernel-law-observation"
    design := obligation.design, designRevision := obligation.designRevision }
  expect (Policy.Completion.obligationSatisfied [good] obligation)
    "exact successful evidence did not satisfy its obligation"
  let bad := [
    { good with id := ⟨2⟩, exitCode := 1 },
    { good with id := ⟨3⟩, commandProfile := "wrong-profile" },
    { good with id := ⟨4⟩, invocation := "wrong invocation" },
    { good with id := ⟨5⟩, repository := "wrong-repository" },
    { good with id := ⟨6⟩, snapshot := "commit:wrong" },
    { good with id := ⟨7⟩, artifactDigest := "sha256:wrong" },
    { good with id := ⟨8⟩, revision := ⟨8⟩ },
    { good with id := ⟨9⟩, work := ⟨2⟩ },
    { good with id := ⟨10⟩, obligation := "wrong-gate" },
    { good with id := ⟨11⟩, current := false }]
  for item in bad do
    expect (!Policy.Completion.obligationSatisfied [item] obligation)
      s!"mismatched evidence satisfied obligation: {repr item}"

def expectRejectedNoEffect (command : Kernel.Decide.Command)
    (state : Kernel.Replay.State) (message : String) : IO Unit := do
  let result := Kernel.Decide.decide command state
  match result with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError s!"{message}: command unexpectedly accepted"
  expect (Kernel.Decide.committedEvents result).isEmpty
    s!"{message}: rejected command exposed events"
  expect (Kernel.Decide.committedState result state == state)
    s!"{message}: rejected command changed state or revision"

def executeState (command : Kernel.Decide.Command) (state : Kernel.Replay.State)
    (message : String) : IO Kernel.Replay.State :=
  match Kernel.Decide.decide command state with
  | .ok transaction => pure transaction.result.state
  | .error error => throw <| IO.userError s!"{message}: {repr error}"

inductive MissingCompletionCondition
  | child
  | dependency
  | phase
  | task
  | checklist
  | review
  | finding
  | validation
  | repository
  | correction
  | workRecord
deriving DecidableEq, Repr, BEq

def currentState (store : Kernel.Projection.Store) : IO Kernel.Replay.State :=
  match (Kernel.Projection.inspect store).currentState? with
  | some state => pure state
  | none => throw <| IO.userError "public command fixture lost its fresh projection"

def executeStore (command : Kernel.Decide.Command) (store : Kernel.Projection.Store)
    (message : String) : IO Kernel.Projection.Store :=
  match Application.Service.execute command store with
  | .ok transaction => pure transaction.result
  | .error error => throw <| IO.userError s!"{message}: {repr error}"

def contractScope (work : WorkId) (purpose : Domain.Review.Purpose)
    (digest : String) :
    Domain.Review.FrozenScope :=
  { design := some evidenceDesign.id
    work
    repositorySnapshot := s!"snapshot-{work.value}"
    artifactDigest := digest
    purpose }

def installDesignContract (work : WorkId) (store : Kernel.Projection.Store) :
    IO Kernel.Projection.Store := do
  let store ← executeStore (.importDesign store.ledger.storedHead evidenceDesign) store
    "completion design import rejected"
  let plan : Domain.Review.Plan :=
    { id := ⟨1000⟩
      owner := evidenceDesign.owner
      reviewer := "design-reviewer"
      adjudicator := evidenceDesign.owner
      scope := contractScope work .design evidenceDesign.contentDigest }
  let store ← executeStore (.recordReviewPlan store.ledger.storedHead plan) store
    "completion design review plan rejected"
  let claim : Domain.Review.Claim :=
    { id := ⟨1000⟩
      plan := plan.id
      work
      epoch := ⟨0⟩
      claim := .clean
      reviewer := plan.reviewer
      scope := some plan.scope }
  let store ← executeStore (.recordReviewClaim store.ledger.storedHead claim) store
    "completion design review claim rejected"
  let store ← executeStore
    (.recordReviewAdjudication store.ledger.storedHead
      { review := claim.id, decision := .accepted, adjudicator := plan.adjudicator })
    store "completion design review adjudication rejected"
  executeStore (.approveDesign store.ledger.storedHead evidenceDesign.id) store
    "completion design approval rejected"

def bindWorkContract (work : WorkId) (store : Kernel.Projection.Store)
    (includeQuality : Bool := true) :
    IO Kernel.Projection.Store := do
  let decompositionDigest := s!"decomposition-{work.value}"
  let decompositionPlan : Domain.Review.Plan :=
    { id := ⟨2000 + work.value⟩
      owner := evidenceDesign.owner
      reviewer := s!"decomposition-reviewer-{work.value}"
      adjudicator := evidenceDesign.owner
      scope := contractScope work .decomposition decompositionDigest }
  let store ← executeStore
    (.recordReviewPlan store.ledger.storedHead decompositionPlan) store
    "decomposition review plan rejected"
  let decompositionClaim : Domain.Review.Claim :=
    { id := ⟨2000 + work.value⟩
      plan := decompositionPlan.id
      work
      epoch := ⟨0⟩
      claim := .clean
      reviewer := decompositionPlan.reviewer
      scope := some decompositionPlan.scope }
  let store ← executeStore
    (.recordReviewClaim store.ledger.storedHead decompositionClaim) store
    "decomposition review claim rejected"
  let store ← executeStore
    (.recordReviewAdjudication store.ledger.storedHead
      { review := decompositionClaim.id, decision := .accepted
        adjudicator := decompositionPlan.adjudicator })
    store "decomposition review adjudication rejected"
  let decomposition : Domain.Design.Decomposition :=
    { key := decompositionDigest
      design := evidenceDesign.id
      work
      designRevision := evidenceDesign.revision
      contentDigest := decompositionDigest
      items := [{
        key := s!"work-{work.value}"
        requirements := ["evidence-integrity"]
        implementationWork := ["kernel transitions"]
        tasks := ["implement work contract"]
        completionChecks := ["kernel laws"]
        checklists := ["contract reviewed"]
        validationGates := ["evidence-matrix"] }]
      reviewer := decompositionPlan.reviewer
      adjudicator := decompositionPlan.adjudicator
      accepted := true }
  let store ← executeStore
    (.recordDecomposition store.ledger.storedHead decomposition) store
    "work decomposition rejected"
  let implementationPlan : Domain.Review.Plan :=
    { id := ⟨3000 + work.value⟩
      owner := evidenceDesign.owner
      reviewer := s!"implementation-reviewer-{work.value}"
      adjudicator := evidenceDesign.owner
      scope :=
        contractScope work .designConformance s!"implementation-{work.value}" }
  let store ← executeStore
    (.recordReviewPlan store.ledger.storedHead implementationPlan) store
    "implementation review plan rejected"
  let implementationClaim : Domain.Review.Claim :=
    { id := ⟨3000 + work.value⟩
      plan := implementationPlan.id
      work
      epoch := ⟨0⟩
      claim := .clean
      reviewer := implementationPlan.reviewer
      scope := some implementationPlan.scope }
  let store ← executeStore
    (.recordReviewClaim store.ledger.storedHead implementationClaim) store
    "implementation review claim rejected"
  let store ← executeStore
    (.recordReviewAdjudication store.ledger.storedHead
      { review := implementationClaim.id, decision := .accepted
        adjudicator := implementationPlan.adjudicator })
    store "implementation review adjudication rejected"
  if !includeQuality then
    return store
  let qualityPlan : Domain.Review.Plan :=
    { id := ⟨4000 + work.value⟩
      owner := evidenceDesign.owner
      reviewer := s!"quality-reviewer-{work.value}"
      adjudicator := evidenceDesign.owner
      scope :=
        contractScope work .implementationQuality s!"implementation-{work.value}" }
  let store ← executeStore
    (.recordReviewPlan store.ledger.storedHead qualityPlan) store
    "implementation quality review plan rejected"
  let qualityClaim : Domain.Review.Claim :=
    { id := ⟨4000 + work.value⟩
      plan := qualityPlan.id
      work
      epoch := ⟨0⟩
      claim := .clean
      reviewer := qualityPlan.reviewer
      scope := some qualityPlan.scope }
  let store ← executeStore
    (.recordReviewClaim store.ledger.storedHead qualityClaim) store
    "implementation quality review claim rejected"
  executeStore
    (.recordReviewAdjudication store.ledger.storedHead
      { review := qualityClaim.id, decision := .accepted
        adjudicator := qualityPlan.adjudicator })
    store "implementation quality review adjudication rejected"

def recordReadinessEvidence (work : WorkId) (store : Kernel.Projection.Store) :
    IO (Kernel.Projection.Store × Domain.Work.ReadinessBasis) := do
  let obligation : Domain.Evidence.Obligation :=
    { work
      key := s!"readiness-{work.value}"
      revision := store.ledger.storedHead
      commandProfile := "kernel-laws"
      invocation := ".lake/build/bin/kernel-laws"
      repository := "main"
      snapshot := s!"snapshot-{work.value}"
      artifactDigest := s!"sha256:readiness-{work.value}"
      current := true
      requirements := ["evidence-integrity"]
      expectedProducer := "kernel-law-runner"
      expectedObservation := s!"observation-{work.value}"
      design := evidenceDesign.id
      designRevision := evidenceDesign.revision }
  let store ← executeStore (.recordObligation store.ledger.storedHead obligation) store
    "readiness obligation rejected"
  let evidence : Domain.Evidence.Evidence :=
    { id := ⟨1000 + work.value + store.ledger.storedHead.value⟩
      work
      obligation := obligation.key
      revision := obligation.revision
      commandProfile := obligation.commandProfile
      invocation := obligation.invocation
      exitCode := 0
      repository := obligation.repository
      snapshot := obligation.snapshot
      artifactDigest := obligation.artifactDigest
      current := true
      requirements := obligation.requirements
      producer := obligation.expectedProducer
      observedAt := s!"observation-{work.value}"
      design := obligation.design
      designRevision := obligation.designRevision }
  let store ← executeStore (.recordEvidence store.ledger.storedHead evidence) store
    "readiness evidence rejected"
  let basis : Domain.Work.ReadinessBasis :=
    { design := evidenceDesign.id
      designRevision := evidenceDesign.revision
      decompositionKey := s!"decomposition-{work.value}"
      decompositionDigest := s!"decomposition-{work.value}"
      repositorySnapshot := obligation.snapshot
      obligationKeys := [obligation.key]
      evidenceRevision := evidence.revision
      reviewPlan := ⟨3000 + work.value⟩ }
  pure (store, basis)

def executeResolverAction (action : Kernel.Resolver.Action)
    (store : Kernel.Projection.Store) (message : String) : IO Kernel.Projection.Store :=
  match Cli.Program.executeRequest (.action action) store with
  | .ok response => pure response.store
  | .error error => throw <| IO.userError s!"{message}: {error}"

def expectResolverActionRejected (action : Kernel.Resolver.Action)
    (store : Kernel.Projection.Store) (message : String) : IO Unit :=
  match Cli.Program.executeRequest (.action action) store with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError s!"{message}: stale action unexpectedly executed"

def minimalCompletionPlan (work : WorkId) : Domain.Lifecycle.CompletionPlan :=
  { work
    relatedWork := []
    phases := []
    tasks := []
    checklists := []
    reviews := []
    findings := []
    validations := []
    repositories := []
    corrections := []
    workRecords := [] }

def prepareMinimalActiveWork (work : WorkId) (store : Kernel.Projection.Store) :
    IO Kernel.Projection.Store := do
  let store ← executeStore
    (.planCompletion store.ledger.storedHead (minimalCompletionPlan work)) store
    s!"minimal completion plan rejected for {work.value}"
  let key := s!"proof-{work.value}"
  let obligation : Domain.Evidence.Obligation :=
    { work, key, revision := store.ledger.storedHead
      commandProfile := "kernel-laws", invocation := ".lake/build/bin/kernel-laws"
      repository := "main", snapshot := "fixture"
      artifactDigest := s!"sha256:minimal-{work.value}", current := true
      requirements := ["evidence-integrity"]
      expectedProducer := "kernel-law-runner"
      expectedObservation := s!"completion-observation-{work.value}"
      design := evidenceDesign.id
      designRevision := evidenceDesign.revision }
  let store ← executeStore (.recordObligation store.ledger.storedHead obligation) store
    s!"minimal obligation rejected for {work.value}"
  let evidence : Domain.Evidence.Evidence :=
    { id := ⟨work.value⟩
      work
      obligation := key
      revision := obligation.revision
      commandProfile := "kernel-laws"
      invocation := ".lake/build/bin/kernel-laws"
      exitCode := 0
      repository := "main"
      snapshot := "fixture"
      artifactDigest := s!"sha256:minimal-{work.value}"
      current := true
      requirements := obligation.requirements
      producer := obligation.expectedProducer
      observedAt := s!"completion-observation-{work.value}"
      design := obligation.design
      designRevision := obligation.designRevision }
  let store ← executeStore (.recordEvidence store.ledger.storedHead evidence) store
    s!"minimal evidence rejected for {work.value}"
  pure store

def completeMinimalActiveWork (work : WorkId) (store : Kernel.Projection.Store) :
    IO Kernel.Projection.Store := do
  let store ← prepareMinimalActiveWork work store
  match Application.Service.complete store.ledger.storedHead work store with
  | .ok transaction => pure transaction.result
  | .error error => throw <| IO.userError s!"minimal authoritative completion rejected for {work.value}: {repr error}"

def buildPlannedCompletionStore (missing : Option MissingCompletionCondition) :
    IO Kernel.Projection.Store := do
  let store ← executeStore
    (.initializeWork Application.Service.initialStore.ledger.storedHead
      secondWork secondActivation)
    Application.Service.initialStore "child initialization rejected"
  let store ← installDesignContract secondWork.id store
  let store ← bindWorkContract secondWork.id store
  let store ← completeMinimalActiveWork secondWork.id store
  let store ← executeStore (.registerWork store.ledger.storedHead thirdWork) store
    "dependency registration rejected"
  let store ← bindWorkContract thirdWork.id store
  let (store, thirdBasis) ← recordReadinessEvidence thirdWork.id store
  let thirdContext : Domain.Work.SuspensionContext :=
    { suspensionContext with basis := some thirdBasis }
  let suspendedThird : Domain.Work.Activation :=
    { id := thirdActivation.id, work := thirdActivation.work, status := .suspended
      readyToResume := false, suspension := some thirdContext }
  let store ← executeStore
    (.registerSuspendedActivation store.ledger.storedHead suspendedThird) store
    "dependency activation registration rejected"
  let store ← executeStore
    (.confirmResumeReadiness store.ledger.storedHead
      thirdWork.id thirdActivation.id thirdBasis) store
    "dependency readiness confirmation rejected"
  let store ← executeStore
    (.resumeWork store.ledger.storedHead thirdWork.id thirdActivation.id) store
    "dependency resume rejected"
  let store ← completeMinimalActiveWork thirdWork.id store
  let store ← executeStore (.registerWork store.ledger.storedHead parentWork) store
    "parent registration rejected"
  let store ← bindWorkContract parentWork.id store
  let (store, parentBasis) ← recordReadinessEvidence parentWork.id store
  let parentContext : Domain.Work.SuspensionContext :=
    { suspensionContext with basis := some parentBasis }
  let parentActivation : Domain.Work.Activation :=
    { id := ⟨4⟩, work := parentWork.id, status := .suspended
      readyToResume := false, suspension := some parentContext }
  let store ← executeStore
    (.registerSuspendedActivation store.ledger.storedHead parentActivation) store
    "parent activation registration rejected"
  let store ← executeStore
    (.confirmResumeReadiness store.ledger.storedHead
      parentWork.id parentActivation.id parentBasis) store
    "parent readiness confirmation rejected"
  let store ← executeStore
    (.resumeWork store.ledger.storedHead parentWork.id parentActivation.id) store
    "parent resume rejected"
  let store ← executeStore (.registerWork store.ledger.storedHead firstWork) store
    "owner registration rejected"
  let store ← bindWorkContract firstWork.id store
  let (store, firstBasis) ← recordReadinessEvidence firstWork.id store
  let firstContext : Domain.Work.SuspensionContext :=
    { suspensionContext with basis := some firstBasis }
  let suspendedFirst : Domain.Work.Activation :=
    { id := firstActivation.id, work := firstActivation.work, status := .suspended
      readyToResume := false, suspension := some firstContext
      parent := some parentActivation.id }
  let store ← executeStore
    (.registerSuspendedActivation store.ledger.storedHead suspendedFirst) store
    "owner activation registration rejected"
  let store ← executeStore
    (.suspendWork store.ledger.storedHead
      parentWork.id parentActivation.id parentContext)
    store "parent suspension rejected"
  let store ← executeStore
    (.confirmResumeReadiness store.ledger.storedHead
      firstWork.id firstActivation.id firstBasis) store
    "owner readiness confirmation rejected"
  let store ← executeStore
    (.resumeWork store.ledger.storedHead firstWork.id firstActivation.id) store
    "owner resume rejected"
  let store ← if missing == some .child || missing == some .dependency then
      executeStore (.registerWork store.ledger.storedHead blockedRelatedWork) store
        "blocking related-work registration rejected"
    else pure store
  let plan :=
    if missing == some .child then
      { completionPlan with relatedWork := [
          { work := blockedRelatedWork.id, kind := .child },
          { work := thirdWork.id, kind := .dependency }] }
    else if missing == some .dependency then
      { completionPlan with relatedWork := [
          { work := secondWork.id, kind := .child },
          { work := blockedRelatedWork.id, kind := .dependency }] }
    else completionPlan
  executeStore (.planCompletion store.ledger.storedHead plan) store
    "owner completion planning rejected"

def buildCompletionStore (missing : Option MissingCompletionCondition) :
    IO Kernel.Projection.Store := do
  let store ← buildPlannedCompletionStore missing
  let store ← if missing != some .child then
      executeStore (.acknowledgeRelatedWorkTerminal store.ledger.storedHead
        firstWork.id secondWork.id) store "child completion rejected"
    else pure store
  let store ← if missing != some .dependency then
      executeStore (.acknowledgeRelatedWorkTerminal store.ledger.storedHead
        firstWork.id thirdWork.id) store "dependency completion rejected"
    else pure store
  let store ← if missing != some .phase then
      executeStore (.completePhase store.ledger.storedHead firstWork.id "phase-1")
        store "phase completion rejected"
    else pure store
  let store ← if missing != some .task then
      executeStore (.completeTask store.ledger.storedHead firstWork.id "task-1")
        store "task completion rejected"
    else pure store
  let store ← executeStore
    (.completeTask store.ledger.storedHead firstWork.id "task-after-validation")
    store "second task completion rejected"
  let store ← if missing != some .checklist then
      executeStore (.completeChecklist store.ledger.storedHead
        firstWork.id "checklist-1") store "checklist completion rejected"
    else pure store
  let store ← if missing != some .finding then
      executeStore (.resolveFinding store.ledger.storedHead firstWork.id
        "blocking-review-finding")
        store "finding resolution rejected"
    else pure store
  let store ← if missing != some .correction then
      executeStore (.resolveCorrection store.ledger.storedHead
        firstWork.id "correction-1") store "correction resolution rejected"
    else pure store
  let store ← if missing != some .workRecord then
      executeStore (.linkWorkRecord store.ledger.storedHead firstWork.id
        "record-1" "work-record:matrix") store "work-record link rejected"
    else pure store
  let store ← if missing != some .repository then
      executeStore (.classifyRepository store.ledger.storedHead firstWork.id
        "repository-1" "snapshot:matrix") store "repository classification rejected"
    else pure store
  let state ← currentState store
  let epoch ← match Domain.Lifecycle.forWork state.lifecycle firstWork.id with
    | some completion => pure completion.epoch
    | none => throw <| IO.userError "completion lifecycle disappeared"
  let store ← if missing != some .review then
      let claim : Domain.Review.Claim :=
        { id := ⟨10⟩, plan := ⟨3001⟩, work := firstWork.id, epoch
          claim := .clean, reviewer := "implementation-reviewer-1"
          scope := some
            (contractScope firstWork.id .designConformance "implementation-1") }
      let claimed ← executeStore
        (.recordReviewClaim store.ledger.storedHead claim) store "review claim rejected"
      executeStore (.recordReviewAdjudication claimed.ledger.storedHead
        { review := claim.id, decision := .accepted, adjudicator := "owner" }) claimed
        "review adjudication rejected"
    else pure store
  let store ← if missing != some .validation then
      executeStore (.passValidation store.ledger.storedHead firstWork.id
        "validation-1" "artifact:matrix") store "validation observation rejected"
    else pure store
  let obligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "completion-proof",
      revision := store.ledger.storedHead, commandProfile := "kernel-laws"
      invocation := ".lake/build/bin/kernel-laws", repository := "main"
      snapshot := "fixture", artifactDigest := "proof:matrix", current := true
      requirements := ["evidence-integrity"]
      expectedProducer := "kernel-law-runner"
      expectedObservation := "completion-matrix-observation"
      design := evidenceDesign.id
      designRevision := evidenceDesign.revision }
  let store ← executeStore (.recordObligation store.ledger.storedHead obligation) store
    "completion obligation rejected"
  let evidence : Domain.Evidence.Evidence :=
    { id := ⟨100⟩, work := firstWork.id, obligation := obligation.key,
      revision := obligation.revision, commandProfile := "kernel-laws",
      invocation := ".lake/build/bin/kernel-laws", exitCode := 0,
      repository := "main", snapshot := "fixture",
      artifactDigest := "proof:matrix", current := true
      requirements := obligation.requirements
      producer := obligation.expectedProducer
      observedAt := "completion-matrix-observation"
      design := obligation.design
      designRevision := obligation.designRevision }
  let store ← executeStore (.recordEvidence store.ledger.storedHead evidence) store
    "completion evidence rejected"
  pure store

def expectPublicCompletionRejected (missing : MissingCompletionCondition)
    (label : String) : IO Unit := do
  let store ← buildCompletionStore (some missing)
  let state ← currentState store
  match Application.Service.complete store.ledger.storedHead firstWork.id store with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError s!"{label}: public completion unexpectedly accepted"
  let kernelResult := Kernel.Decide.decide
    (.completeWork state.revision firstWork.id) state
  expect (Kernel.Decide.committedEvents kernelResult).isEmpty
    s!"{label}: rejection exposed an accepted event"
  expect (Kernel.Decide.committedState kernelResult state == state)
    s!"{label}: rejection changed state or revision"
  expect ((Application.Service.status store).store == store)
    s!"{label}: rejected public attempt changed the complete store"
  expect (state.work.any fun work => work.id == firstWork.id && work.status == .open)
    s!"{label}: rejected completion did not retain the active target"
  expect (state.activations.any fun activation =>
    activation.work == firstWork.id && activation.status == .active)
    s!"{label}: rejected completion did not retain the owning activation"

def expectMissingRequiredReviewRejected : IO Unit := do
  let store ← executeStore
    (.initializeWork Application.Service.initialStore.ledger.storedHead
      secondWork secondActivation)
    Application.Service.initialStore "required-review fixture initialization rejected"
  let store ← installDesignContract secondWork.id store
  let store ← bindWorkContract secondWork.id store false
  let store ← prepareMinimalActiveWork secondWork.id store
  match Application.Service.complete store.ledger.storedHead secondWork.id store with
  | .error _ => pure ()
  | .ok _ =>
      throw <| IO.userError
        "public completion accepted a missing implementation-quality review"
  let state ← currentState store
  expect (!Kernel.Replay.completionRequiredReviewsReady secondWork.id state)
    "replay accepted a missing implementation-quality review"
  expect (state.work.any fun work =>
    work.id == secondWork.id && work.status == .open)
    "required-review rejection changed the active work"

set_option maxRecDepth 2048 in
def main : IO Unit := do
  testEvidenceScopeSatisfaction
  expectMissingRequiredReviewRejected
  let initial := Kernel.Replay.emptyState
  expect (decide (Kernel.Replay.ValidState initial)) "empty state must be valid"
  expectRejectedNoEffect
    (.initializeWork initial.revision { firstWork with owner := "" } firstActivation)
    initial "work without an authoritative owner"
  let first ← match Kernel.Decide.decide
      (initializeWork initial firstWork firstActivation) initial with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"first activation rejected: {repr error}"
  expect (first.revision == ⟨1⟩) "atomic initialization must advance one revision"
  let invalid := initializeWork first secondWork secondActivation
  match Kernel.Decide.decide invalid first with
  | .error (.invalidTransition _) => pure ()
  | .error error => throw <| IO.userError s!"wrong rejection: {repr error}"
  | .ok _ => throw <| IO.userError "reinitializing work must be rejected"
  expect (Kernel.Decide.committedState
    (Kernel.Decide.decide invalid first)
    first == first) "rejection must leave the state unchanged"
  expect (Kernel.Decide.committedEvents
    (Kernel.Decide.decide invalid first)).isEmpty
    "rejection must expose no accepted events"
  let stale : Kernel.Decide.Command :=
    .initializeWork ⟨0⟩ secondWork secondActivation
  match Kernel.Decide.decide stale first with
  | .error .staleRevision => pure ()
  | _ => throw <| IO.userError "stale command must be rejected"
  expectRejectedNoEffect stale first "stale revision rejection"
  let unboundReviewPlan :=
    { minimalCompletionPlan firstWork.id with reviews := [⟨99⟩] }
  expectRejectedNoEffect
    (.planCompletion first.revision unboundReviewPlan) first
    "completion plan referencing no frozen review plan"
  let unboundObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := first.revision
      commandProfile := "kernel-laws", invocation := ".lake/build/bin/kernel-laws"
      repository := "main", snapshot := "fixture"
      artifactDigest := "sha256:evidence", current := true
      requirements := ["evidence-integrity"], expectedProducer := "kernel-law-runner"
      expectedObservation := "kernel-law-observation"
      design := evidenceDesign.id, designRevision := evidenceDesign.revision }
  expectRejectedNoEffect (.recordObligation first.revision unboundObligation) first
    "obligation without an imported design"
  let designed ← executeState (.importDesign first.revision evidenceDesign) first
    "evidence design import rejected"
  let currentObligation :=
    { unboundObligation with revision := designed.revision }
  let noDesignObligation :=
    { currentObligation with design := ⟨99⟩, designRevision := ⟨99⟩ }
  let noDesignObligationState :=
    { first with obligations := [noDesignObligation] }
  expect (!(decide (Kernel.Replay.ValidState noDesignObligationState)))
    "valid state accepted an obligation for a missing design"
  let obligated ← match Kernel.Decide.decide
      (.recordObligation designed.revision currentObligation) designed with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid obligation rejected: {repr error}"
  let item : Domain.Evidence.Evidence :=
    { id := ⟨1⟩, work := firstWork.id, obligation := "proof"
      revision := currentObligation.revision
      commandProfile := "kernel-laws", invocation := ".lake/build/bin/kernel-laws"
      exitCode := 0, repository := "main", snapshot := "fixture"
      artifactDigest := "sha256:evidence", current := true
      requirements := currentObligation.requirements
      producer := currentObligation.expectedProducer
      observedAt := "kernel-law-observation"
      design := currentObligation.design
      designRevision := currentObligation.designRevision }
  let evidenced ← match Kernel.Decide.decide
      (.recordEvidence obligated.revision item) obligated with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid evidence rejected: {repr error}"
  expect (evidenced.evidence.any fun recorded =>
    recorded.current && recorded.revision == currentObligation.revision)
    "recorded evidence must retain its observed revision"
  expect (evidenced.obligations.any fun obligation =>
    obligation.work == firstWork.id && obligation.key == "proof" &&
      obligation.current && obligation.revision == currentObligation.revision)
    "current evidence must retain its referenced obligation revision"
  expectRejectedNoEffect (.recordEvidence evidenced.revision item) evidenced
    "duplicate evidence identity"
  let malformedItem := { item with id := ⟨2⟩, artifactDigest := "" }
  expectRejectedNoEffect (.recordEvidence obligated.revision malformedItem) obligated
    "malformed evidence"
  expectRejectedNoEffect (.recordEvidence first.revision item) first
    "evidence without a recorded obligation"
  let attempt : Domain.ExternalOperation.Attempt :=
    { operation := ⟨"publish-1"⟩, artifactDigest := "sha256:artifact", state := .prepared }
  let externalized ← match Kernel.Decide.decide
      (.recordExternalOperation first.revision attempt) first with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid external attempt rejected: {repr error}"
  expectRejectedNoEffect (.recordExternalOperation externalized.revision attempt) externalized
    "duplicate external operation"
  let malformedAttempt := { attempt with operation := ⟨"publish-2"⟩, artifactDigest := "" }
  expectRejectedNoEffect (.recordExternalOperation first.revision malformedAttempt) first
    "malformed external operation"
  let emptyOperation := { attempt with operation := ⟨""⟩ }
  expectRejectedNoEffect (.recordExternalOperation first.revision emptyOperation) first
    "empty external operation identity"
  let bypassedOperation := { attempt with operation := ⟨"publish-3"⟩, state := .succeeded }
  expectRejectedNoEffect (.recordExternalOperation first.revision bypassedOperation) first
    "external operation lifecycle bypass"
  expect (Domain.Work.resume [firstActivation] firstActivation.id).isNone
    "an active activation cannot resume"
  let suspended : Domain.Work.Activation :=
    { id := firstActivation.id, work := firstActivation.work, status := .suspended
      readyToResume := false, suspension := some suspensionContext }
  expect (Domain.Work.resume [suspended] suspended.id).isNone
    "resume must reject an unready activation"
  let ready := { suspended with readyToResume := true }
  expect (Domain.Work.resume [ready] ready.id).isSome
    "resume must accept a ready activation when no activation is active"
  let orphanActivation := { ready with work := ⟨99⟩ }
  expectRejectedNoEffect
    (.initializeWork initial.revision firstWork orphanActivation) initial
    "activation referencing missing work"
  let closedWork := { firstWork with status := .closed }
  expectRejectedNoEffect
    (.initializeWork initial.revision closedWork ready) initial
    "ready suspended activation referencing closed work"
  expectRejectedNoEffect
    (.initializeWork initial.revision closedWork suspended) initial
    "unready suspended activation referencing closed work"
  let authorityClaim : Domain.Review.Claim :=
    { id := ⟨99⟩, plan := ⟨99⟩, work := firstWork.id, epoch := ⟨0⟩, claim := .clean }
  let reviewState : Policy.Authority.ReviewState := { claims := [], adjudications := [] }
  expect (Policy.Authority.authority (Policy.Authority.recordClaim reviewState authorityClaim) ==
    Policy.Authority.authority reviewState) "a review claim must not create authority"
  let malformedObligation := { currentObligation with key := "" }
  expectRejectedNoEffect (.recordObligation first.revision malformedObligation) first
    "malformed obligation"
  let orphanObligation := { currentObligation with work := ⟨99⟩ }
  expectRejectedNoEffect (.recordObligation first.revision orphanObligation) first
    "obligation owned by missing work"
  let openRelated ← executeState (.registerWork first.revision secondWork) first
    "open related-work registration rejected"
  let openRelatedPlan : Domain.Lifecycle.CompletionPlan :=
    { minimalCompletionPlan firstWork.id with
      relatedWork := [{ work := secondWork.id, kind := .child }] }
  let openRelated ← executeState
    (.planCompletion openRelated.revision openRelatedPlan) openRelated
    "open related-work completion plan rejected"
  expectRejectedNoEffect
    (.acknowledgeRelatedWorkTerminal openRelated.revision firstWork.id secondWork.id) openRelated
    "owner attempted to close open related work"
  let plannedStore ← buildPlannedCompletionStore none
  let planned ← currentState plannedStore
  let parentActivation ← match planned.activations.find? (·.id == ⟨4⟩) with
    | some activation => pure activation
    | none => throw <| IO.userError "planned parent activation disappeared"
  expect (!(Policy.Completion.closeable firstWork.id planned.work planned.activations
    planned.claims planned.adjudications planned.reviewPlans
    planned.reviewFindings planned.findingVerifications planned.lifecycle
    planned.evidence planned.obligations planned.designs planned.designApprovals
    planned.decompositions planned.corrections))
    "an authoritative plan must begin unready instead of self-attested complete"
  expect (Kernel.Replay.completionApplicable firstWork.id planned ==
    Policy.Completion.closeable firstWork.id planned.work planned.activations
      planned.claims planned.adjudications planned.reviewPlans
      planned.reviewFindings planned.findingVerifications planned.lifecycle
      planned.evidence planned.obligations planned.designs planned.designApprovals
      planned.decompositions planned.corrections)
    "replay completion applicability diverged from authoritative policy"
  match Kernel.Replay.replay
      [.workCompleted firstWork.id firstActivation.id] planned with
  | .error (.invalidTransition _) => pure ()
  | _ => throw <| IO.userError "a raw completion event bypassed authoritative lifecycle derivation"
  let childDone ← executeState
    (.acknowledgeRelatedWorkTerminal planned.revision firstWork.id secondWork.id) planned
    "child completion rejected"
  let dependencyDone ← executeState
    (.acknowledgeRelatedWorkTerminal childDone.revision firstWork.id thirdWork.id) childDone
    "dependency completion rejected"
  let phaseDone ← executeState
    (.completePhase dependencyDone.revision firstWork.id "phase-1") dependencyDone
    "phase completion rejected"
  let taskDone ← executeState
    (.completeTask phaseDone.revision firstWork.id "task-1") phaseDone
    "task completion rejected"
  let checklistDone ← executeState
    (.completeChecklist taskDone.revision firstWork.id "checklist-1") taskDone
    "checklist completion rejected"
  let findingDone ← executeState
    (.resolveFinding checklistDone.revision firstWork.id "blocking-review-finding")
      checklistDone
    "finding resolution rejected"
  let correctionDone ← executeState
    (.resolveCorrection findingDone.revision firstWork.id "correction-1") findingDone
    "correction resolution rejected"
  let recordDone ← executeState
    (.linkWorkRecord correctionDone.revision firstWork.id "record-1" "work-record:1")
      correctionDone "work-record link rejected"
  let repositoryDone ← executeState
    (.classifyRepository recordDone.revision firstWork.id "repository-1" "snapshot:1")
      recordDone "repository classification rejected"
  let epoch ← match Domain.Lifecycle.forWork repositoryDone.lifecycle firstWork.id with
    | some completion => pure completion.epoch
    | none => throw <| IO.userError "completion lifecycle disappeared"
  let claim : Domain.Review.Claim :=
    { id := ⟨1⟩, plan := ⟨3001⟩, work := firstWork.id, epoch
      claim := .clean, reviewer := "implementation-reviewer-1"
      scope := some
        (contractScope firstWork.id .designConformance "implementation-1") }
  let claimed ← executeState
    (.recordReviewClaim repositoryDone.revision claim) repositoryDone
    "current scoped review claim rejected"
  expectRejectedNoEffect (.recordReviewClaim claimed.revision claim) claimed
    "duplicate review claim"
  let unknownAdjudication : Domain.Review.Adjudication :=
    { review := ⟨99⟩, decision := .accepted }
  expectRejectedNoEffect
    (.recordReviewAdjudication claimed.revision unknownAdjudication) claimed
    "adjudication without claim"
  let adjudication : Domain.Review.Adjudication :=
    { review := claim.id, decision := .accepted, adjudicator := "owner" }
  let adjudicated ← executeState
    (.recordReviewAdjudication claimed.revision adjudication) claimed
    "review adjudication rejected"
  let staleable ← executeState
    (.passValidation adjudicated.revision firstWork.id "validation-1" "artifact:1")
      adjudicated "validation observation rejected"
  let staled ← executeState
    (.completeTask staleable.revision firstWork.id "task-after-validation") staleable
    "post-validation task completion rejected"
  expectRejectedNoEffect (.completeWork staled.revision firstWork.id) staled
    "stale completion-context observations"
  let repositoryRefreshed ← executeState
    (.classifyRepository staled.revision firstWork.id "repository-1" "snapshot:2") staled
    "current repository reclassification rejected"
  let refreshedEpoch ← match Domain.Lifecycle.forWork
      repositoryRefreshed.lifecycle firstWork.id with
    | some completion => pure completion.epoch
    | none => throw <| IO.userError "completion lifecycle disappeared after invalidation"
  let freshClaim : Domain.Review.Claim :=
    { id := ⟨2⟩, plan := ⟨3001⟩, work := firstWork.id,
      epoch := refreshedEpoch, claim := .clean
      reviewer := "implementation-reviewer-1"
      scope := some
        (contractScope firstWork.id .designConformance "implementation-1") }
  let freshlyClaimed ← executeState
    (.recordReviewClaim repositoryRefreshed.revision freshClaim) repositoryRefreshed
    "fresh scoped review claim rejected"
  let freshAdjudication : Domain.Review.Adjudication :=
    { review := freshClaim.id, decision := .accepted, adjudicator := "owner" }
  let freshlyAdjudicated ← executeState
    (.recordReviewAdjudication freshlyClaimed.revision freshAdjudication) freshlyClaimed
    "fresh review adjudication rejected"
  let validated ← executeState
    (.passValidation freshlyAdjudicated.revision firstWork.id
      "validation-1" "artifact:2") freshlyAdjudicated
    "current validation observation rejected"
  let completionObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "completion-proof",
      revision := validated.revision, commandProfile := "kernel-laws"
      invocation := ".lake/build/bin/kernel-laws", repository := "main"
      snapshot := "fixture", artifactDigest := "proof:complete", current := true
      requirements := ["evidence-integrity"]
      expectedProducer := "kernel-law-runner"
      expectedObservation := "completion-observation"
      design := evidenceDesign.id
      designRevision := evidenceDesign.revision }
  let obligatedCompletion ← executeState
    (.recordObligation validated.revision completionObligation) validated
    "completion obligation rejected"
  let completionEvidence : Domain.Evidence.Evidence :=
    { id := ⟨100⟩, work := firstWork.id, obligation := completionObligation.key,
      revision := completionObligation.revision, commandProfile := "kernel-laws",
      invocation := ".lake/build/bin/kernel-laws", exitCode := 0,
      repository := "main", snapshot := "fixture", artifactDigest := "proof:complete",
      current := true
      requirements := completionObligation.requirements
      producer := completionObligation.expectedProducer
      observedAt := "completion-observation"
      design := completionObligation.design
      designRevision := completionObligation.designRevision }
  let completable ← executeState
    (.recordEvidence obligatedCompletion.revision completionEvidence) obligatedCompletion
    "completion evidence rejected"
  expect (Policy.Completion.closeable firstWork.id completable.work completable.activations
    completable.claims completable.adjudications completable.reviewPlans
    completable.reviewFindings completable.findingVerifications completable.lifecycle
    completable.evidence completable.obligations completable.designs
    completable.designApprovals completable.decompositions completable.corrections)
    "authoritative current lifecycle records must allow completion"
  let withoutQualityReview :=
    { completable with
      reviewPlans := completable.reviewPlans.filter fun plan =>
        plan.scope.purpose != .implementationQuality }
  expect (!(Policy.Completion.requiredReviewsReady firstWork.id
    withoutQualityReview.reviewPlans withoutQualityReview.decompositions
    withoutQualityReview.claims withoutQualityReview.adjudications
    withoutQualityReview.reviewFindings withoutQualityReview.findingVerifications))
    "completion accepted a missing implementation-quality review"
  let reusedConformanceReview :=
    { completable with
      reviewPlans := completable.reviewPlans.map fun
          (plan : Domain.Review.Plan) =>
        if plan.scope.purpose ==
            Domain.Review.Purpose.implementationQuality then
          { plan with scope :=
              { plan.scope with
                purpose := Domain.Review.Purpose.designConformance } }
        else plan }
  expect (!(Policy.Completion.requiredReviewsReady firstWork.id
    reusedConformanceReview.reviewPlans reusedConformanceReview.decompositions
    reusedConformanceReview.claims reusedConformanceReview.adjudications
    reusedConformanceReview.reviewFindings
    reusedConformanceReview.findingVerifications))
    "one design-conformance review was reused for both required purposes"
  let mismatchedQualityScope :=
    { completable with
      reviewPlans := completable.reviewPlans.map fun
          (plan : Domain.Review.Plan) =>
        if plan.scope.purpose ==
            Domain.Review.Purpose.implementationQuality then
          { plan with scope :=
              { plan.scope with artifactDigest := "sha256:different-artifact" } }
        else plan
      claims := completable.claims.map fun claim =>
        if claim.id == ⟨4000 + firstWork.id.value⟩ then
          { claim with scope := claim.scope.map fun scope =>
              { scope with artifactDigest := "sha256:different-artifact" } }
        else claim }
  expect (Policy.Completion.purposeReviewReady firstWork.id
    (some evidenceDesign.id) .designConformance
    mismatchedQualityScope.reviewPlans mismatchedQualityScope.claims
    mismatchedQualityScope.adjudications mismatchedQualityScope.reviewFindings
    mismatchedQualityScope.findingVerifications)
    "artifact mismatch invalidated the design-conformance review itself"
  expect (Policy.Completion.purposeReviewReady firstWork.id
    (some evidenceDesign.id) .implementationQuality
    mismatchedQualityScope.reviewPlans mismatchedQualityScope.claims
    mismatchedQualityScope.adjudications mismatchedQualityScope.reviewFindings
    mismatchedQualityScope.findingVerifications)
    "artifact mismatch invalidated the implementation-quality review itself"
  expect (!(Policy.Completion.requiredReviewsReady firstWork.id
    mismatchedQualityScope.reviewPlans mismatchedQualityScope.decompositions
    mismatchedQualityScope.claims mismatchedQualityScope.adjudications
    mismatchedQualityScope.reviewFindings mismatchedQualityScope.findingVerifications))
    "completion combined required reviews from different frozen artifact scopes"
  let mismatchedRepositoryScope :=
    { completable with
      reviewPlans := completable.reviewPlans.map fun
          (plan : Domain.Review.Plan) =>
        if plan.scope.purpose ==
            Domain.Review.Purpose.implementationQuality then
          { plan with scope :=
              { plan.scope with repositorySnapshot := "snapshot:different" } }
        else plan
      claims := completable.claims.map fun claim =>
        if claim.id == ⟨4000 + firstWork.id.value⟩ then
          { claim with scope := claim.scope.map fun scope =>
              { scope with repositorySnapshot := "snapshot:different" } }
        else claim }
  expect (Policy.Completion.purposeReviewReady firstWork.id
    (some evidenceDesign.id) .designConformance
    mismatchedRepositoryScope.reviewPlans mismatchedRepositoryScope.claims
    mismatchedRepositoryScope.adjudications
    mismatchedRepositoryScope.reviewFindings
    mismatchedRepositoryScope.findingVerifications)
    "repository mismatch invalidated the design-conformance review itself"
  expect (Policy.Completion.purposeReviewReady firstWork.id
    (some evidenceDesign.id) .implementationQuality
    mismatchedRepositoryScope.reviewPlans mismatchedRepositoryScope.claims
    mismatchedRepositoryScope.adjudications
    mismatchedRepositoryScope.reviewFindings
    mismatchedRepositoryScope.findingVerifications)
    "repository mismatch invalidated the implementation-quality review itself"
  expect (!(Policy.Completion.requiredReviewsReady firstWork.id
    mismatchedRepositoryScope.reviewPlans mismatchedRepositoryScope.decompositions
    mismatchedRepositoryScope.claims mismatchedRepositoryScope.adjudications
    mismatchedRepositoryScope.reviewFindings
    mismatchedRepositoryScope.findingVerifications))
    "completion combined required reviews from different repository snapshots"
  let missingQualityAdjudication :=
    { completable with
      adjudications := completable.adjudications.filter fun decision =>
        decision.review != ⟨4000 + firstWork.id.value⟩ }
  expect (Policy.Completion.purposeReviewReady firstWork.id
    (some evidenceDesign.id) .designConformance
    missingQualityAdjudication.reviewPlans missingQualityAdjudication.claims
    missingQualityAdjudication.adjudications
    missingQualityAdjudication.reviewFindings
    missingQualityAdjudication.findingVerifications)
    "missing quality adjudication invalidated the design-conformance review"
  expect (!(Policy.Completion.purposeReviewReady firstWork.id
    (some evidenceDesign.id) .implementationQuality
    missingQualityAdjudication.reviewPlans missingQualityAdjudication.claims
    missingQualityAdjudication.adjudications
    missingQualityAdjudication.reviewFindings
    missingQualityAdjudication.findingVerifications))
    "implementation-quality review was ready without adjudication"
  expect (!(Policy.Completion.requiredReviewsReady firstWork.id
    missingQualityAdjudication.reviewPlans
    missingQualityAdjudication.decompositions
    missingQualityAdjudication.claims missingQualityAdjudication.adjudications
    missingQualityAdjudication.reviewFindings
    missingQualityAdjudication.findingVerifications))
    "completion accepted an unadjudicated implementation-quality review"
  expect (Kernel.Replay.completionRequiredReviewsReady firstWork.id completable)
    "replay did not recognize both exact required review purposes"
  expect (!Kernel.Replay.completionRequiredReviewsReady
    firstWork.id withoutQualityReview)
    "replay accepted a missing required review purpose"
  expect (!Kernel.Replay.completionRequiredReviewsReady
    firstWork.id mismatchedQualityScope)
    "replay combined reviews from different frozen artifacts"
  expect (!Kernel.Replay.completionRequiredReviewsReady
    firstWork.id mismatchedRepositoryScope)
    "replay combined reviews from different repository snapshots"
  expect (!Kernel.Replay.completionRequiredReviewsReady
    firstWork.id missingQualityAdjudication)
    "replay accepted an unadjudicated implementation-quality review"
  let otherDesign :=
    { evidenceDesign with id := ⟨2⟩, contentDigest := "sha256:other-design" }
  let otherDesignObligation :=
    { completionObligation with
      design := otherDesign.id
      designRevision := otherDesign.revision }
  let otherDesignEvidence :=
    { completionEvidence with
      design := otherDesign.id
      designRevision := otherDesign.revision }
  let wrongDesignEvidenceState :=
    { completable with
      designs := completable.designs ++ [otherDesign]
      obligations := completable.obligations.map fun
          (obligation : Domain.Evidence.Obligation) =>
        if obligation.work == completionObligation.work &&
            obligation.key == completionObligation.key then
          otherDesignObligation
        else obligation
      evidence := completable.evidence.map fun
          (item : Domain.Evidence.Evidence) =>
        if item.id == completionEvidence.id then otherDesignEvidence else item }
  expect (!(Policy.Completion.closeable firstWork.id
    wrongDesignEvidenceState.work wrongDesignEvidenceState.activations
    wrongDesignEvidenceState.claims wrongDesignEvidenceState.adjudications
    wrongDesignEvidenceState.reviewPlans wrongDesignEvidenceState.reviewFindings
    wrongDesignEvidenceState.findingVerifications wrongDesignEvidenceState.lifecycle
    wrongDesignEvidenceState.evidence wrongDesignEvidenceState.obligations
    wrongDesignEvidenceState.designs wrongDesignEvidenceState.designApprovals
    wrongDesignEvidenceState.decompositions wrongDesignEvidenceState.corrections))
    "completion accepted evidence for a different existing design"
  let uncoveredRequirementState :=
    { completable with
      obligations := completable.obligations.map fun
          (obligation : Domain.Evidence.Obligation) =>
        if obligation.work == firstWork.id then
          { obligation with requirements := ["different-requirement"] }
        else
          obligation
      evidence := completable.evidence.map fun
          (item : Domain.Evidence.Evidence) =>
        if item.work == firstWork.id then
          { item with requirements := ["different-requirement"] }
        else
          item }
  expect (!(Policy.Completion.closeable firstWork.id
    uncoveredRequirementState.work uncoveredRequirementState.activations
    uncoveredRequirementState.claims uncoveredRequirementState.adjudications
    uncoveredRequirementState.reviewPlans uncoveredRequirementState.reviewFindings
    uncoveredRequirementState.findingVerifications uncoveredRequirementState.lifecycle
    uncoveredRequirementState.evidence uncoveredRequirementState.obligations
    uncoveredRequirementState.designs uncoveredRequirementState.designApprovals
    uncoveredRequirementState.decompositions uncoveredRequirementState.corrections))
    "completion evidence omitted an active design requirement"
  let supersedingImplementationPlan : Domain.Review.Plan :=
    { id := ⟨3999⟩
      owner := "owner"
      reviewer := "implementation-reviewer-new"
      adjudicator := "owner"
      scope :=
        contractScope firstWork.id .designConformance
          s!"implementation-{firstWork.id.value}" }
  let supersededReviewState :=
    { completable with
      reviewPlans := completable.reviewPlans ++ [supersedingImplementationPlan] }
  expect (!(Policy.Completion.closeable firstWork.id
    supersededReviewState.work supersededReviewState.activations
    supersededReviewState.claims supersededReviewState.adjudications
    supersededReviewState.reviewPlans supersededReviewState.reviewFindings
    supersededReviewState.findingVerifications supersededReviewState.lifecycle
    supersededReviewState.evidence supersededReviewState.obligations
    supersededReviewState.designs supersededReviewState.designApprovals
    supersededReviewState.decompositions supersededReviewState.corrections))
    "an older implementation review remained sufficient after a newer plan"
  expect (!Kernel.Gates.reviewReadyState ⟨3000 + firstWork.id.value⟩
    supersededReviewState)
    "an older implementation review gate remained ready after a newer plan"
  let supersedingFindingsClaim : Domain.Review.Claim :=
    { id := ⟨3998⟩
      plan := supersedingImplementationPlan.id
      work := firstWork.id
      epoch := ⟨0⟩
      claim := .findings
      reviewer := supersedingImplementationPlan.reviewer
      scope := some supersedingImplementationPlan.scope }
  let supersededByFindings :=
    { supersededReviewState with
      claims := supersededReviewState.claims ++ [supersedingFindingsClaim] }
  expect (!(Policy.Completion.closeable firstWork.id
    supersededByFindings.work supersededByFindings.activations
    supersededByFindings.claims supersededByFindings.adjudications
    supersededByFindings.reviewPlans supersededByFindings.reviewFindings
    supersededByFindings.findingVerifications supersededByFindings.lifecycle
    supersededByFindings.evidence supersededByFindings.obligations
    supersededByFindings.designs supersededByFindings.designApprovals
    supersededByFindings.decompositions supersededByFindings.corrections))
    "an older clean implementation review bypassed newer findings"
  let latestCleanClaim : Domain.Review.Claim :=
    { supersedingFindingsClaim with id := ⟨3997⟩, claim := .clean }
  let latestCleanAdjudication : Domain.Review.Adjudication :=
    { review := latestCleanClaim.id
      decision := .accepted
      adjudicator := supersedingImplementationPlan.adjudicator }
  let restoredLatestReview :=
    { supersededByFindings with
      claims := supersededByFindings.claims ++ [latestCleanClaim]
      adjudications :=
        supersededByFindings.adjudications ++ [latestCleanAdjudication] }
  expect (Policy.Completion.closeable firstWork.id
    restoredLatestReview.work restoredLatestReview.activations
    restoredLatestReview.claims restoredLatestReview.adjudications
    restoredLatestReview.reviewPlans restoredLatestReview.reviewFindings
    restoredLatestReview.findingVerifications restoredLatestReview.lifecycle
    restoredLatestReview.evidence restoredLatestReview.obligations
    restoredLatestReview.designs restoredLatestReview.designApprovals
    restoredLatestReview.decompositions restoredLatestReview.corrections)
    "latest clean implementation review did not restore completion authority"
  let currentDecomposition ←
    match completable.decompositions.reverse.find? (·.work == firstWork.id) with
    | some decomposition => pure decomposition
    | none => throw <| IO.userError "completion decomposition disappeared"
  let supersedingDecomposition :=
    { currentDecomposition with
      key := "decomposition-new"
      contentDigest := "decomposition-new"
      items := currentDecomposition.items.map fun item =>
        { item with requirements := ["different-requirement"] } }
  let supersededTraceState :=
    { completable with
      decompositions := completable.decompositions ++ [supersedingDecomposition] }
  expect (!(Policy.Completion.closeable firstWork.id
    supersededTraceState.work supersededTraceState.activations
    supersededTraceState.claims supersededTraceState.adjudications
    supersededTraceState.reviewPlans supersededTraceState.reviewFindings
    supersededTraceState.findingVerifications supersededTraceState.lifecycle
    supersededTraceState.evidence supersededTraceState.obligations
    supersededTraceState.designs supersededTraceState.designApprovals
    supersededTraceState.decompositions supersededTraceState.corrections))
    "an older complete trace remained sufficient after a newer decomposition"
  let completed ← match Kernel.Decide.closeWork completable.revision firstWork.id completable with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid completion rejected: {repr error}"
  expect (completed.work.any fun unit =>
    unit.id == firstWork.id && unit.status == .closed) "completion must close the target work"
  expect (completed.activations.any fun activation =>
    activation.id == firstActivation.id && activation.status == .closed)
    "completion must atomically close the owning activation"
  expect (completed.revision == completable.revision.next)
    "atomic completion must advance exactly one revision"
  expectRejectedNoEffect (.recordObligation completed.revision currentObligation) completed
    "current obligation for closed work"
  expect (completed.activations.any fun activation => activation == parentActivation)
    "completion must preserve unrelated suspended activations"
  match Kernel.Decide.closeWork completable.revision parentWork.id completable with
  | .error (.invalidTransition _) => pure ()
  | _ => throw <| IO.userError "inactive target completion must reject"
  let receipt : Policy.Update.Receipt :=
    { operation := ⟨"operation-1"⟩, payloadDigest := "payload", resultDigest := "result" }
  expect (Policy.Update.resolveRetry receipt.operation "payload" ⟨0⟩ ⟨99⟩ [receipt] ==
    .exact receipt) "an exact retry must return its receipt despite a later revision"
  expect (Policy.Update.resolveRetry receipt.operation "changed" ⟨0⟩ ⟨99⟩ [receipt] ==
    .payloadConflict) "a changed retry payload must conflict"
  let preInitializationAttempt : Domain.ExternalOperation.Attempt :=
    { operation := ⟨"pre-initialization-1"⟩
      artifactDigest := "sha256:pre-initialization-1"
      state := .prepared }
  let nonzeroEmptyStore ← executeStore
    (.recordExternalOperation Application.Service.initialStore.ledger.storedHead
      preInitializationAttempt)
    Application.Service.initialStore "pre-initialization operation rejected"
  expect (nonzeroEmptyStore.ledger.storedHead == ⟨1⟩)
    "pre-initialization mutation must advance the empty ledger"
  let nonzeroInitializeAction ← match
      (Application.Service.resolve nonzeroEmptyStore).value with
    | .action action@(.initializeWork point) =>
        expect (point.revision == nonzeroEmptyStore.ledger.storedHead)
          "initialization action did not preserve its nonzero ledger binding"
        pure action
    | _ => throw <| IO.userError "nonzero-revision empty store did not emit initialization"
  let initializedFromNonzero ← executeResolverAction nonzeroInitializeAction
    nonzeroEmptyStore "nonzero-revision initialization action rejected"
  expect (initializedFromNonzero.ledger.storedHead == ⟨2⟩)
    "nonzero-revision initialization did not advance exactly once"
  let driftAttempt : Domain.ExternalOperation.Attempt :=
    { operation := ⟨"pre-initialization-2"⟩
      artifactDigest := "sha256:pre-initialization-2"
      state := .prepared }
  let driftedEmptyStore ← executeStore
    (.recordExternalOperation nonzeroEmptyStore.ledger.storedHead driftAttempt)
    nonzeroEmptyStore "pre-initialization drift operation rejected"
  expect (!nonzeroInitializeAction.executable
    (Kernel.Projection.inspect driftedEmptyStore))
    "nonzero initialization action remained executable after ledger drift"
  expectResolverActionRejected nonzeroInitializeAction driftedEmptyStore
    "nonzero initialization action did not reject after ledger drift"
  let initializeAction ← match
      (Application.Service.resolve Application.Service.initialStore).value with
    | .action action@(.initializeWork _) => pure action
    | _ => throw <| IO.userError "empty authoritative store did not emit initialization"
  let firstStore ← executeResolverAction initializeAction Application.Service.initialStore
    "CLI initialization action rejected"
  expectResolverActionRejected initializeAction firstStore
    "initialization action did not reject after ledger advancement"
  let observed := Kernel.Gates.observeGate Kernel.Gates.validStateGate firstStore
  expect (observed.1 == firstStore) "gate observation must preserve the complete store"
  let firstInspection := Kernel.Projection.inspect firstStore
  match (Application.Service.resolve firstStore).value with
  | .action action@(.continueActiveWork _ work activation) =>
      expect (work == firstWork.id && activation == firstActivation.id)
        "continue action did not bind the active work and activation"
      expect (action.executable firstInspection)
        "next action must be executable at its stated ledger point and target"
      let continued ← executeResolverAction action firstStore
        "CLI continue action rejected"
      expect (continued == firstStore) "continue action must be observational"
      let revisedStore ← executeStore
        (.registerWork firstStore.ledger.storedHead secondWork) firstStore
        "continue drift fixture rejected"
      expect (!action.executable (Kernel.Projection.inspect revisedStore))
        "a projected action must become non-executable after ledger identity changes"
      expectResolverActionRejected action revisedStore
        "continue action did not reject after ledger drift"
  | _ => throw <| IO.userError "active work must resolve to the exact continue action"
  let forgedLedger := {
    firstStore.ledger with
    events := []
    storedHead := first.revision
    storedHistoryDigest := Kernel.Replay.eventDigest [] }
  let forgedStore := { firstStore with
    ledger := forgedLedger
    active := some (Application.Service.projectionFor forgedLedger first) }
  let forgedInspection := Kernel.Projection.inspect forgedStore
  match (Application.Service.resolve forgedStore).value with
  | .blocked blocker@(.ledgerCorrupt _) =>
      expect (decide (blocker.exact forgedInspection))
        "a noncanonical event stream must return its exact ledger blocker"
  | _ => throw <| IO.userError "raw state without canonical events became authoritative"
  expect forgedInspection.repairCommand?.isNone
    "corrupt canonical ledger must not emit projection repair"
  match (Application.Service.status forgedStore).value with
  | .ledgerCorrupt _ => pure ()
  | _ => throw <| IO.userError "status did not expose canonical-ledger corruption"
  for request in [Kernel.Gates.Request.validState,
      Kernel.Gates.Request.completion firstWork.id] do
    expect ((Application.Service.queryGate request forgedStore).store == forgedStore)
      "every gate must remain observational on a corrupt canonical ledger"
  match Application.Service.execute Application.Service.bootstrapCommand forgedStore with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "mutation accepted a noncanonical authoritative ledger"

  let staleStore := { firstStore with active := some Kernel.Projection.initialProjection }
  let staleInspection := Kernel.Projection.inspect staleStore
  match staleInspection with
  | .stale _ _ _ => pure ()
  | _ => throw <| IO.userError "a correct earlier-prefix projection must classify stale"
  expect ((Application.Service.status staleStore).store == staleStore)
    "status must not write a stale store"
  expect ((Application.Service.resolve staleStore).store == staleStore)
    "next must not write a stale store"
  for request in [Kernel.Gates.Request.validState,
      Kernel.Gates.Request.completion firstWork.id] do
    expect ((Application.Service.queryGate request staleStore).store == staleStore)
      "every gate must preserve a stale store"
  let repairAction ← match (Application.Service.resolve staleStore).value with
    | .action action@(.repairProjection _) => pure action
    | _ => throw <| IO.userError "stale projection must emit an exact repair action"
  expect (repairAction.executable staleInspection)
    "emitted repair action must be executable before ledger advancement"
  let repairCommand ← match repairAction with
    | .repairProjection command => pure command
    | _ => throw <| IO.userError "expected a projection repair command"
  match Kernel.Projection.stageRepair repairCommand forgedStore with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "repair staging accepted a corrupt canonical ledger"
  match Application.Service.executeRecovery repairAction forgedStore with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "emitted repair action executed on a corrupt canonical ledger"
  match Cli.Program.executeRequest (.repairProjection repairCommand) forgedStore with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "CLI repair accepted a corrupt canonical ledger"
  for request in [Application.Service.Request.status,
      Application.Service.Request.next,
      Application.Service.Request.gate Kernel.Gates.Request.validState,
      Application.Service.Request.gate (Kernel.Gates.Request.completion firstWork.id)] do
    match Cli.Program.executeRequest request staleStore with
    | .ok response =>
        expect (response.store == staleStore)
          "CLI inspection request must preserve the complete store"
    | .error error => throw <| IO.userError s!"CLI inspection failed: {error}"
  let staged ← match Kernel.Projection.stageRepair repairCommand staleStore with
    | .ok transaction => pure transaction
    | .error error => throw <| IO.userError s!"repair staging failed: {repr error}"
  expect (staged.result.ledger == staleStore.ledger &&
      staged.result.active == staleStore.active)
    "repair staging must not alter the ledger or live projection"
  let verified ← match Kernel.Projection.verifyStage staged.stage.id staged.result with
    | .ok verified => pure verified
    | .error error => throw <| IO.userError s!"valid staged replay failed verification: {repr error}"
  let poisonedState := { first with revision := first.revision.next }
  let poisonedStage := { staged.stage with candidate := {
    staged.stage.candidate with payload := .decoded poisonedState } }
  let poisonedStore := { staged.result with staged := [poisonedStage] }
  match Kernel.Projection.verifyStage poisonedStage.id poisonedStore with
  | .error .candidateMismatch => pure ()
  | _ => throw <| IO.userError "tampered staged projection passed replay verification"
  let changedAfterVerify := { staged.result with ledger := {
    staged.result.ledger with storedHead := staged.result.ledger.storedHead.next } }
  match Kernel.Projection.adoptVerified verified changedAfterVerify with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "verified candidate adopted after ledger advancement"
  let repaired ← match Application.Service.executeRecovery repairAction staleStore with
    | .ok transaction => pure transaction.adopted.result
    | .error error => throw <| IO.userError s!"emitted repair action failed: {repr error}"
  expect (repaired.ledger == staleStore.ledger)
    "projection repair must not change authoritative ledger events or revision"
  match Kernel.Projection.inspect repaired with
  | .fresh _ _ => pure ()
  | _ => throw <| IO.userError "successful repair must atomically adopt a fresh replay"
  expect (repaired.receipts.length == staleStore.receipts.length + 1)
    "repair adoption and receipt must appear together"
  match Cli.Program.executeRequest
      (.repairProjection repairCommand) staleStore with
  | .ok response =>
      match Kernel.Projection.inspect response.store with
      | .fresh _ _ => pure ()
      | _ => throw <| IO.userError "CLI repair request did not adopt a fresh replay"
  | .error error => throw <| IO.userError s!"CLI repair request failed: {error}"
  let repairedByAction ← executeResolverAction repairAction staleStore
    "CLI resolver repair action failed"
  match Kernel.Projection.inspect repairedByAction with
  | .fresh _ _ => pure ()
  | _ => throw <| IO.userError "CLI resolver repair action did not adopt replayed state"
  let advancedStore ← executeStore
    (.registerWork firstStore.ledger.storedHead secondWork) firstStore
    "repair drift fixture rejected"
  expect (!repairAction.executable (Kernel.Projection.inspect advancedStore))
    "repair action must reject after its ledger binding changes"
  match Application.Service.executeRecovery repairAction advancedStore with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "stale repair action unexpectedly executed"
  expectResolverActionRejected repairAction advancedStore
    "CLI repair action did not reject after ledger drift"

  let missingStore := { firstStore with active := none }
  match (Application.Service.status missingStore).value with
  | .missing _ _ => pure ()
  | _ => throw <| IO.userError "missing projection must remain distinct from stale"
  let currentProjection ← match firstStore.active with
    | some projection => pure projection
    | none => throw <| IO.userError "bootstrap did not create a projection"
  let corruptProjection := { currentProjection with
    reference := { currentProjection.reference with stateDigest := ⟨"wrong"⟩ } }
  let corruptStore := { firstStore with active := some corruptProjection }
  match (Application.Service.status corruptStore).value with
  | .corrupt _ _ _ _ => pure ()
  | _ => throw <| IO.userError "same-revision content mismatch must classify corrupt"
  expect ((Application.Service.status corruptStore).store == corruptStore &&
      (Application.Service.resolve corruptStore).store == corruptStore)
    "corrupt projection inspection must be read-only"
  let corruptAction ← match (Application.Service.resolve corruptStore).value with
    | .action action@(.repairProjection _) => pure action
    | _ => throw <| IO.userError "corrupt projection must emit repair, not normal work"
  let corruptRepaired ← match Application.Service.executeRecovery corruptAction corruptStore with
    | .ok transaction => pure transaction.adopted.result
    | .error error => throw <| IO.userError s!"corrupt projection repair failed: {repr error}"
  match Kernel.Projection.inspect corruptRepaired with
  | .fresh _ _ => pure ()
  | _ => throw <| IO.userError "corrupt projection repair did not adopt replayed state"

  let rejectionCases : List (MissingCompletionCondition × String) := [
    (.child, "child work"),
    (.dependency, "dependency work"),
    (.phase, "phase"),
    (.task, "task"),
    (.checklist, "checklist"),
    (.review, "review"),
    (.finding, "finding"),
    (.validation, "validation"),
    (.repository, "repository classification"),
    (.correction, "correction"),
    (.workRecord, "work-record linkage")]
  for (condition, label) in rejectionCases do
    expectPublicCompletionRejected condition label
  let allReadyStore ← buildCompletionStore none
  let allReadyState ← currentState allReadyStore
  let allReadyEpoch ← match Domain.Lifecycle.forWork
      allReadyState.lifecycle firstWork.id with
    | some completion => pure completion.epoch
    | none => throw <| IO.userError "all-ready lifecycle disappeared"
  let completionProof ← match allReadyState.obligations.find?
      (fun obligation =>
        obligation.work == firstWork.id && obligation.key == "completion-proof") with
    | some obligation => pure obligation
    | none => throw <| IO.userError "all-ready completion obligation disappeared"
  let findingsClaim : Domain.Review.Claim :=
    { id := ⟨11⟩, plan := ⟨3001⟩, work := firstWork.id,
      epoch := allReadyEpoch, claim := .findings
      reviewer := "implementation-reviewer-1"
      scope := some
        (contractScope firstWork.id .designConformance "implementation-1") }
  let findingsClaimed ← executeStore
    (.recordReviewClaim allReadyStore.ledger.storedHead findingsClaim) allReadyStore
    "current findings claim rejected"
  let findingsAdjudicated ← executeStore
    (.recordReviewAdjudication findingsClaimed.ledger.storedHead
      { review := findingsClaim.id, decision := .accepted, adjudicator := "owner" })
    findingsClaimed
    "current findings adjudication rejected"
  let findingsEvidence : Domain.Evidence.Evidence :=
    { id := ⟨101⟩, work := firstWork.id, obligation := "completion-proof",
      revision := completionProof.revision,
      commandProfile := completionProof.commandProfile
      invocation := completionProof.invocation
      exitCode := 0, repository := completionProof.repository
      snapshot := completionProof.snapshot
      artifactDigest := completionProof.artifactDigest, current := true
      requirements := completionProof.requirements
      producer := completionProof.expectedProducer
      observedAt := completionProof.expectedObservation
      design := completionProof.design
      designRevision := completionProof.designRevision }
  let findingsRefreshed ← executeStore
    (.recordEvidence findingsAdjudicated.ledger.storedHead findingsEvidence)
    findingsAdjudicated "findings evidence refresh rejected"
  let findingsState ← currentState findingsRefreshed
  expect (!Policy.Completion.closeable firstWork.id findingsState.work
    findingsState.activations findingsState.claims findingsState.adjudications
    findingsState.reviewPlans findingsState.reviewFindings
    findingsState.findingVerifications findingsState.lifecycle findingsState.evidence
    findingsState.obligations findingsState.designs findingsState.designApprovals
    findingsState.decompositions findingsState.corrections)
    "later accepted findings claim did not dominate an earlier clean claim"
  match Application.Service.complete findingsRefreshed.ledger.storedHead firstWork.id findingsRefreshed with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError <|
      "completion accepted refreshed evidence with a current findings review"
  let recoveryClaim : Domain.Review.Claim :=
    { id := ⟨12⟩, plan := ⟨3001⟩, work := firstWork.id,
      epoch := allReadyEpoch, claim := .clean
      reviewer := "implementation-reviewer-1"
      scope := some
        (contractScope firstWork.id .designConformance "implementation-1") }
  let recoveryClaimed ← executeStore
    (.recordReviewClaim findingsRefreshed.ledger.storedHead recoveryClaim)
    findingsRefreshed "recovery clean claim rejected"
  let recoveryAdjudicated ← executeStore
    (.recordReviewAdjudication recoveryClaimed.ledger.storedHead
      { review := recoveryClaim.id, decision := .accepted, adjudicator := "owner" })
    recoveryClaimed
    "recovery clean adjudication rejected"
  let recoveryEvidence : Domain.Evidence.Evidence :=
    { id := ⟨102⟩, work := firstWork.id, obligation := "completion-proof",
      revision := completionProof.revision
      commandProfile := completionProof.commandProfile
      invocation := completionProof.invocation
      exitCode := 0, repository := completionProof.repository
      snapshot := completionProof.snapshot
      artifactDigest := completionProof.artifactDigest, current := true
      requirements := completionProof.requirements
      producer := completionProof.expectedProducer
      observedAt := completionProof.expectedObservation
      design := completionProof.design
      designRevision := completionProof.designRevision }
  let recoveredStore ← executeStore
    (.recordEvidence recoveryAdjudicated.ledger.storedHead recoveryEvidence)
    recoveryAdjudicated "recovery evidence refresh rejected"
  let recoveredState ← currentState recoveredStore
  expect (Policy.Completion.closeable firstWork.id recoveredState.work
    recoveredState.activations recoveredState.claims recoveredState.adjudications
    recoveredState.reviewPlans recoveredState.reviewFindings
    recoveredState.findingVerifications recoveredState.lifecycle recoveredState.evidence
    recoveredState.obligations recoveredState.designs recoveredState.designApprovals
    recoveredState.decompositions recoveredState.corrections)
    "later accepted clean claim did not restore review readiness"
  match Application.Service.complete recoveredStore.ledger.storedHead firstWork.id recoveredStore with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"clean review recovery did not complete: {repr error}"
  let unmetObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "unmet-proof",
      revision := allReadyStore.ledger.storedHead, commandProfile := "kernel-laws"
      invocation := ".lake/build/bin/kernel-laws", repository := "main"
      snapshot := "fixture", artifactDigest := "proof:unmet", current := true
      requirements := ["evidence-integrity"]
      expectedProducer := "kernel-law-runner"
      expectedObservation := "unmet-observation"
      design := evidenceDesign.id
      designRevision := evidenceDesign.revision }
  let withUnmetObligation ← executeStore
    (.recordObligation allReadyStore.ledger.storedHead unmetObligation) allReadyStore
    "unmet obligation setup rejected"
  let unmetState ← currentState withUnmetObligation
  match Application.Service.complete withUnmetObligation.ledger.storedHead firstWork.id withUnmetObligation with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "completion erased an unmet current obligation"
  let unmetResult := Kernel.Decide.decide
    (.completeWork unmetState.revision firstWork.id) unmetState
  expect (Kernel.Decide.committedEvents unmetResult).isEmpty
    "unmet obligation rejection exposed an accepted event"
  expect (Kernel.Decide.committedState unmetResult unmetState == unmetState)
    "unmet obligation rejection changed authoritative state"
  let staleCompletionRevision := allReadyStore.ledger.storedHead
  let refreshEvidence : Domain.Evidence.Evidence :=
    { id := ⟨101⟩
      work := firstWork.id
      obligation := "completion-proof"
      revision := completionProof.revision
      commandProfile := completionProof.commandProfile
      invocation := completionProof.invocation
      exitCode := 0
      repository := completionProof.repository
      snapshot := completionProof.snapshot
      artifactDigest := completionProof.artifactDigest
      current := true
      requirements := completionProof.requirements
      producer := completionProof.expectedProducer
      observedAt := completionProof.expectedObservation
      design := completionProof.design
      designRevision := completionProof.designRevision }
  let advancedReadyStore ← executeStore
    (.recordEvidence staleCompletionRevision refreshEvidence) allReadyStore
    "stale completion revision fixture rejected"
  match Application.Service.complete staleCompletionRevision firstWork.id advancedReadyStore with
  | .error .staleRevision => pure ()
  | .error error => throw <| IO.userError s!"stale completion returned wrong error: {repr error}"
  | .ok _ => throw <| IO.userError "stale public completion intent was accepted"
  expect ((Application.Service.status advancedReadyStore).store == advancedReadyStore)
    "stale completion intent changed the authoritative store"
  match Application.Service.complete advancedReadyStore.ledger.storedHead
      firstWork.id advancedReadyStore with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"current completion intent rejected: {repr error}"
  let beforeCompletion ← currentState allReadyStore
  let completedTransaction ← match Application.Service.complete allReadyStore.ledger.storedHead firstWork.id allReadyStore with
    | .ok transaction => pure transaction
    | .error error => throw <| IO.userError s!"public all-ready completion rejected: {repr error}"
  expect (completedTransaction.accepted.events ==
      [.workCompleted firstWork.id firstActivation.id])
    "all-ready completion must emit exactly the target close event"
  let afterCompletion := completedTransaction.accepted.result.state
  expect (afterCompletion.revision == beforeCompletion.revision.next)
    "all-ready completion must advance exactly one revision"
  expect (afterCompletion.work.filter (·.id != firstWork.id) ==
      beforeCompletion.work.filter (·.id != firstWork.id))
    "completion changed work other than the exact target"
  expect (afterCompletion.activations.filter (·.id != firstActivation.id) ==
      beforeCompletion.activations.filter (·.id != firstActivation.id))
    "completion changed an activation other than the exact target"
  expect (afterCompletion.work.any fun work =>
      work.id == firstWork.id && work.status == .closed)
    "all-ready completion did not close the target"
  expect (afterCompletion.activations.any fun activation =>
      activation.id == firstActivation.id && activation.status == .closed)
    "all-ready completion did not close the target activation"
  expect (afterCompletion.activations.any fun activation =>
      activation.work == parentWork.id && activation.status == .suspended &&
        !activation.readyToResume)
    "completion resumed or lost the suspended parent"
  expect (afterCompletion.returnTarget == some parentActivation.id)
    "completion did not persist the exact stack-return target"
  let completedStore := completedTransaction.result
  let mismatchStore ← executeStore
    (.registerWork completedStore.ledger.storedHead blockedRelatedWork) completedStore
    "resume mismatch work registration rejected"
  let mismatchInspection := Kernel.Projection.inspect mismatchStore
  let mismatchPoint ← match mismatchInspection.ledgerPoint? with
    | some point => pure point
    | none => throw <| IO.userError "resume mismatch fixture lost ledger binding"
  let mismatchedResume : Kernel.Resolver.Action :=
    .resumeSuspendedWork mismatchPoint blockedRelatedWork.id parentActivation.id
  expect (!mismatchedResume.executable mismatchInspection)
    "resume action accepted an activation belonging to another work"
  expectResolverActionRejected mismatchedResume mismatchStore
    "public resume accepted a mismatched work/activation binding"
  let competitorBound ← bindWorkContract blockedRelatedWork.id mismatchStore
  let (competitorEvidenced, competitorBasis) ←
    recordReadinessEvidence blockedRelatedWork.id competitorBound
  let competitorContext : Domain.Work.SuspensionContext :=
    { suspensionContext with basis := some competitorBasis }
  let competitorActivation : Domain.Work.Activation :=
    { id := ⟨5⟩
      work := blockedRelatedWork.id
      status := .suspended
      readyToResume := false
      suspension := some competitorContext }
  let competitorRegistered ← executeStore
    (.registerSuspendedActivation competitorEvidenced.ledger.storedHead
      competitorActivation)
    competitorEvidenced "competing suspended activation registration rejected"
  let competitorReady ← executeStore
    (.confirmResumeReadiness competitorRegistered.ledger.storedHead
      blockedRelatedWork.id competitorActivation.id competitorBasis)
    competitorRegistered "competing suspended activation readiness rejected"
  let (parentRefreshed, parentBasis) ←
    recordReadinessEvidence parentWork.id competitorReady
  let parentState ← currentState parentRefreshed
  let parentContext ← match parentState.activations.find? (·.id == parentActivation.id) with
    | some activation =>
        match activation.suspension with
        | some context => pure { context with basis := some parentBasis }
        | none => throw <| IO.userError "parent suspension context disappeared"
    | none => throw <| IO.userError "parent activation disappeared"
  let parentRevised ← executeStore
    (.reviseSuspension parentRefreshed.ledger.storedHead
      parentWork.id parentActivation.id parentContext)
    parentRefreshed "parent readiness basis revision rejected"
  let parentReady ← executeStore
    (.confirmResumeReadiness parentRevised.ledger.storedHead
      parentWork.id parentActivation.id parentBasis)
    parentRevised "parent readiness confirmation rejected"
  let returnCandidates ← match
      (Kernel.Projection.inspect parentReady).currentState? with
    | some state => pure (Kernel.Resolver.resumableActivations state)
    | none => throw <| IO.userError "stack-return fixture lost its projection"
  expect (returnCandidates.map (·.id) == [parentActivation.id])
    "resolver exposed a competing suspended activation instead of the exact parent"
  match (Application.Service.resolve parentReady).value with
  | .action action@(.resumeSuspendedWork _ work activation) =>
      expect (work == parentWork.id && activation == ⟨4⟩)
        "completion exposed the wrong suspended parent"
      expect (action.executable (Kernel.Projection.inspect parentReady))
        "exposed parent resume action is not executable"
      let resumedStore ← executeResolverAction action parentReady
        "CLI resume action rejected"
      let resumedState ← currentState resumedStore
      expect (resumedState.activations.any fun candidate =>
        candidate.id == activation && candidate.work == work &&
          candidate.status == .active)
        "resume action did not activate its exact suspended frame"
      expect (resumedStore.ledger.events.reverse.head? ==
        some (Kernel.Replay.Event.workResumed work activation))
        "resume action did not append its exact event"
      expectResolverActionRejected action resumedStore
        "resume action did not reject after ledger advancement"
  | _ => throw <| IO.userError "completion did not expose the suspended parent"
  IO.println "kernel laws: pass"

end AgentWorkbench.Tests.KernelLaws

def main : IO Unit :=
  AgentWorkbench.Tests.KernelLaws.main
