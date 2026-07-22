import AgentWorkbench.Cli.Program

open AgentWorkbench
open AgentWorkbench.Domain

namespace AgentWorkbench.Tests.KernelLaws

def firstActivation : Domain.Work.Activation :=
  { id := ⟨1⟩, work := ⟨1⟩, status := .active, readyToResume := false }

def secondActivation : Domain.Work.Activation :=
  { id := ⟨2⟩, work := ⟨2⟩, status := .active, readyToResume := false }

def firstWork : Domain.Work.WorkUnit :=
  { id := ⟨1⟩, status := .open }

def secondWork : Domain.Work.WorkUnit :=
  { id := ⟨2⟩, status := .open }

def thirdWork : Domain.Work.WorkUnit :=
  { id := ⟨3⟩, status := .open }

def parentWork : Domain.Work.WorkUnit :=
  { id := ⟨4⟩, status := .open }

def completionPlan : Domain.Lifecycle.CompletionPlan :=
  { work := firstWork.id
    relatedWork := [
      { work := secondWork.id, kind := .child },
      { work := thirdWork.id, kind := .dependency }]
    phases := ["phase-1"]
    tasks := ["task-1", "task-after-validation"]
    checklists := ["checklist-1"]
    reviews := [⟨1⟩]
    findings := ["finding-1"]
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

def buildCompletionStore (missing : Option MissingCompletionCondition) :
    IO Kernel.Projection.Store := do
  let store ← executeStore Application.Service.bootstrapCommand
    Application.Service.initialStore "bootstrap rejected"
  let store ← executeStore (.registerWork store.ledger.storedHead secondWork) store
    "child registration rejected"
  let store ← executeStore (.registerWork store.ledger.storedHead thirdWork) store
    "dependency registration rejected"
  let store ← executeStore (.registerWork store.ledger.storedHead parentWork) store
    "parent registration rejected"
  let parentActivation : Domain.Work.Activation :=
    { id := ⟨4⟩, work := parentWork.id, status := .suspended, readyToResume := true }
  let store ← executeStore
    (.registerSuspendedActivation store.ledger.storedHead parentActivation) store
    "parent activation registration rejected"
  let store ← executeStore (.planCompletion store.ledger.storedHead completionPlan) store
    "completion plan rejected"
  let store ← if missing != some .child then
      executeStore (.terminateRelatedWork store.ledger.storedHead
        firstWork.id secondWork.id) store "child completion rejected"
    else pure store
  let store ← if missing != some .dependency then
      executeStore (.terminateRelatedWork store.ledger.storedHead
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
      executeStore (.resolveFinding store.ledger.storedHead firstWork.id "finding-1")
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
        { id := ⟨10⟩, plan := ⟨1⟩, work := firstWork.id, epoch, claim := .clean }
      let claimed ← executeStore
        (.recordReviewClaim store.ledger.storedHead claim) store "review claim rejected"
      executeStore (.recordReviewAdjudication claimed.ledger.storedHead
        { review := claim.id, decision := .accepted }) claimed
        "review adjudication rejected"
    else pure store
  let store ← if missing != some .validation then
      executeStore (.passValidation store.ledger.storedHead firstWork.id
        "validation-1" "artifact:matrix") store "validation observation rejected"
    else pure store
  let obligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "completion-proof",
      revision := store.ledger.storedHead, current := true }
  let store ← executeStore (.recordObligation store.ledger.storedHead obligation) store
    "completion obligation rejected"
  let evidence : Domain.Evidence.Evidence :=
    { id := ⟨100⟩, work := firstWork.id, obligation := obligation.key,
      revision := store.ledger.storedHead, artifactDigest := "proof:matrix", current := true }
  let store ← executeStore (.recordEvidence store.ledger.storedHead evidence) store
    "completion evidence rejected"
  pure store

def expectPublicCompletionRejected (missing : MissingCompletionCondition)
    (label : String) : IO Unit := do
  let store ← buildCompletionStore (some missing)
  let state ← currentState store
  match Application.Service.complete firstWork.id store with
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

set_option maxRecDepth 2048 in
def main : IO Unit := do
  let initial := Kernel.Replay.emptyState
  expect (decide (Kernel.Replay.ValidState initial)) "empty state must be valid"
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
  let currentObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := ⟨1⟩, current := true }
  let obligated ← match Kernel.Decide.decide
      (.recordObligation first.revision currentObligation) first with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid obligation rejected: {repr error}"
  let item : Domain.Evidence.Evidence :=
    { id := ⟨1⟩, work := firstWork.id, obligation := "proof", revision := obligated.revision
      artifactDigest := "sha256:evidence", current := true }
  let evidenced ← match Kernel.Decide.decide
      (.recordEvidence obligated.revision item) obligated with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid evidence rejected: {repr error}"
  expect (evidenced.evidence.any fun recorded =>
    recorded.current && recorded.revision == evidenced.revision)
    "recorded current evidence must bind the resulting revision"
  expect (evidenced.obligations.any fun obligation =>
    obligation.work == firstWork.id && obligation.key == "proof" &&
      obligation.current && obligation.revision == evidenced.revision)
    "current evidence must atomically refresh its referenced obligation"
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
  let suspended := { firstActivation with status := .suspended, readyToResume := false }
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
  let registeredChild ← executeState (.registerWork first.revision secondWork) first
    "child registration rejected"
  let registeredDependency ← executeState
    (.registerWork registeredChild.revision thirdWork) registeredChild
    "dependency registration rejected"
  let registeredParent ← executeState
    (.registerWork registeredDependency.revision parentWork) registeredDependency
    "parent registration rejected"
  let parentActivation : Domain.Work.Activation :=
    { id := ⟨4⟩, work := parentWork.id, status := .suspended, readyToResume := true }
  let withParent ← executeState
    (.registerSuspendedActivation registeredParent.revision parentActivation) registeredParent
    "suspended parent registration rejected"
  let planned ← executeState (.planCompletion withParent.revision completionPlan) withParent
    "completion planning rejected"
  expect (!(Policy.Completion.closeable firstWork.id planned.work planned.activations
    planned.claims planned.adjudications planned.lifecycle
    planned.evidence planned.obligations))
    "an authoritative plan must begin unready instead of self-attested complete"
  match Kernel.Replay.replay
      [.workCompleted firstWork.id firstActivation.id] planned with
  | .error (.invalidTransition _) => pure ()
  | _ => throw <| IO.userError "a raw completion event bypassed authoritative lifecycle derivation"
  let childDone ← executeState
    (.terminateRelatedWork planned.revision firstWork.id secondWork.id) planned
    "child completion rejected"
  let dependencyDone ← executeState
    (.terminateRelatedWork childDone.revision firstWork.id thirdWork.id) childDone
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
    (.resolveFinding checklistDone.revision firstWork.id "finding-1") checklistDone
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
    { id := ⟨1⟩, plan := ⟨1⟩, work := firstWork.id, epoch, claim := .clean }
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
    { review := claim.id, decision := .accepted }
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
    { id := ⟨2⟩, plan := ⟨1⟩, work := firstWork.id,
      epoch := refreshedEpoch, claim := .clean }
  let freshlyClaimed ← executeState
    (.recordReviewClaim repositoryRefreshed.revision freshClaim) repositoryRefreshed
    "fresh scoped review claim rejected"
  let freshAdjudication : Domain.Review.Adjudication :=
    { review := freshClaim.id, decision := .accepted }
  let freshlyAdjudicated ← executeState
    (.recordReviewAdjudication freshlyClaimed.revision freshAdjudication) freshlyClaimed
    "fresh review adjudication rejected"
  let validated ← executeState
    (.passValidation freshlyAdjudicated.revision firstWork.id
      "validation-1" "artifact:2") freshlyAdjudicated
    "current validation observation rejected"
  let completionObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "completion-proof",
      revision := validated.revision, current := true }
  let obligatedCompletion ← executeState
    (.recordObligation validated.revision completionObligation) validated
    "completion obligation rejected"
  let completionEvidence : Domain.Evidence.Evidence :=
    { id := ⟨100⟩, work := firstWork.id, obligation := completionObligation.key,
      revision := obligatedCompletion.revision, artifactDigest := "proof:complete",
      current := true }
  let completable ← executeState
    (.recordEvidence obligatedCompletion.revision completionEvidence) obligatedCompletion
    "completion evidence rejected"
  expect (Policy.Completion.closeable firstWork.id completable.work completable.activations
    completable.claims completable.adjudications completable.lifecycle
    completable.evidence completable.obligations)
    "authoritative current lifecycle records must allow completion"
  let completed ← match Kernel.Decide.closeWork firstWork.id completable with
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
  match Kernel.Decide.closeWork parentWork.id completable with
  | .error (.invalidTransition _) => pure ()
  | _ => throw <| IO.userError "inactive target completion must reject"
  let receipt : Policy.Update.Receipt :=
    { operation := ⟨"operation-1"⟩, payloadDigest := "payload", resultDigest := "result" }
  expect (Policy.Update.resolveRetry receipt.operation "payload" ⟨0⟩ ⟨99⟩ [receipt] ==
    .exact receipt) "an exact retry must return its receipt despite a later revision"
  expect (Policy.Update.resolveRetry receipt.operation "changed" ⟨0⟩ ⟨99⟩ [receipt] ==
    .payloadConflict) "a changed retry payload must conflict"
  let firstStore ← match Application.Service.execute
      Application.Service.bootstrapCommand Application.Service.initialStore with
    | .ok transaction => pure transaction.result
    | .error error => throw <| IO.userError s!"store bootstrap rejected: {repr error}"
  let observed := Kernel.Gates.observeGate Kernel.Gates.validStateGate firstStore
  expect (observed.1 == firstStore) "gate observation must preserve the complete store"
  let firstInspection := Kernel.Projection.inspect firstStore
  match (Application.Service.resolve firstStore).value with
  | .action action =>
      expect (action.executable firstInspection)
        "next action must be executable at its stated ledger point and target"
      let revisedStore := { firstStore with ledger := {
        firstStore.ledger with storedHead := first.revision.next } }
      expect (!action.executable (Kernel.Projection.inspect revisedStore))
        "a projected action must become non-executable after ledger identity changes"
  | .blocked _ => throw <| IO.userError "active work must resolve to an action"
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
  let advancedStore := { staleStore with ledger := {
    staleStore.ledger with storedHead := staleStore.ledger.storedHead.next } }
  expect (!repairAction.executable (Kernel.Projection.inspect advancedStore))
    "repair action must reject after its ledger binding changes"
  match Application.Service.executeRecovery repairAction advancedStore with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "stale repair action unexpectedly executed"

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
  let unmetObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "unmet-proof",
      revision := allReadyStore.ledger.storedHead, current := true }
  let withUnmetObligation ← executeStore
    (.recordObligation allReadyStore.ledger.storedHead unmetObligation) allReadyStore
    "unmet obligation setup rejected"
  let unmetState ← currentState withUnmetObligation
  match Application.Service.complete firstWork.id withUnmetObligation with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "completion erased an unmet current obligation"
  let unmetResult := Kernel.Decide.decide
    (.completeWork unmetState.revision firstWork.id) unmetState
  expect (Kernel.Decide.committedEvents unmetResult).isEmpty
    "unmet obligation rejection exposed an accepted event"
  expect (Kernel.Decide.committedState unmetResult unmetState == unmetState)
    "unmet obligation rejection changed authoritative state"
  let beforeCompletion ← currentState allReadyStore
  let completedTransaction ← match Application.Service.complete firstWork.id allReadyStore with
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
        activation.readyToResume)
    "completion resumed or lost the suspended parent"
  let completedStore := completedTransaction.result
  match (Application.Service.resolve completedStore).value with
  | .action action@(.resumeSuspendedWork _ work activation) =>
      expect (work == parentWork.id && activation == ⟨4⟩)
        "completion exposed the wrong suspended parent"
      expect (action.executable (Kernel.Projection.inspect completedStore))
        "exposed parent resume action is not executable"
  | _ => throw <| IO.userError "completion did not expose the suspended parent"
  IO.println "kernel laws: pass"

end AgentWorkbench.Tests.KernelLaws

def main : IO Unit :=
  AgentWorkbench.Tests.KernelLaws.main
