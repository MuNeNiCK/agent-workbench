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

def completeFacts : Domain.Work.CompletionFacts :=
  { work := firstWork.id
    revision := ⟨1⟩
    current := true
    dependentWorkTerminal := true
    phasesTerminal := true
    tasksComplete := true
    checklistsComplete := true
    reviewsClean := true
    findingsResolved := true
    repositoryClassified := true
    workRecordsLinked := true
    correctionsResolved := true }

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
  let claim : Domain.Review.Claim := { id := ⟨1⟩, claim := .clean }
  let claimed ← match Kernel.Decide.decide
      (.recordReviewClaim first.revision claim) first with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid review claim rejected: {repr error}"
  expectRejectedNoEffect (.recordReviewClaim claimed.revision claim) claimed
    "duplicate review claim"
  let unknownAdjudication : Domain.Review.Adjudication :=
    { review := ⟨99⟩, decision := .accepted }
  expectRejectedNoEffect (.recordReviewAdjudication first.revision unknownAdjudication) first
    "adjudication without claim"
  let adjudication : Domain.Review.Adjudication :=
    { review := claim.id, decision := .accepted }
  match Kernel.Decide.decide
      (.recordReviewAdjudication claimed.revision adjudication) claimed with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"valid adjudication rejected: {repr error}"
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
  let reviewState : Policy.Authority.ReviewState := { claims := [], adjudications := [] }
  expect (Policy.Authority.authority (Policy.Authority.recordClaim reviewState claim) ==
    Policy.Authority.authority reviewState) "a review claim must not create authority"
  let staleObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := ⟨1⟩, current := false }
  let malformedObligation := { currentObligation with key := "" }
  expectRejectedNoEffect (.recordObligation first.revision malformedObligation) first
    "malformed obligation"
  expectRejectedNoEffect
    (.recordCompletionEvidence first.revision completeFacts
      [currentObligation, currentObligation]) first
    "duplicate completion obligations"
  let orphanObligation := { currentObligation with work := ⟨99⟩ }
  expectRejectedNoEffect (.recordObligation first.revision orphanObligation) first
    "obligation owned by missing work"
  let orphanFacts := { completeFacts with work := ⟨99⟩ }
  expectRejectedNoEffect
    (.recordCompletionEvidence first.revision orphanFacts [orphanObligation]) first
    "completion evidence owned by missing work"
  expect (Policy.Completion.closeable firstWork.id first.work first.activations
    [completeFacts] [currentObligation])
    "complete current obligations must allow completion"
  expect (!(Policy.Completion.closeable firstWork.id first.work first.activations
    [completeFacts] [])) "completion must reject missing obligations"
  expect (!(Policy.Completion.closeable firstWork.id first.work first.activations
    [completeFacts] [staleObligation]))
    "completion must reject stale obligations"
  let completable := { first with completionFacts := [completeFacts], obligations := [currentObligation] }
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
  expectRejectedNoEffect
    (.recordCompletionEvidence completed.revision completeFacts [currentObligation]) completed
    "current completion evidence for closed work"
  expectRejectedNoEffect (.recordObligation completed.revision currentObligation) completed
    "current obligation for closed work"
  let contradictory := { completable with completionFacts :=
    [completeFacts, { completeFacts with revision := ⟨3⟩, tasksComplete := false }] }
  match Kernel.Replay.verifyState contradictory with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "contradictory completion facts must invalidate state"
  match Kernel.Decide.closeWork firstWork.id contradictory with
  | .error (.invariantViolation _) => pure ()
  | _ => throw <| IO.userError "invalid contradictory facts must not authorize completion"
  let staleRevisionState := { completable with
    completionFacts := [{ completeFacts with revision := ⟨0⟩ }]
    obligations := [{ currentObligation with revision := ⟨0⟩ }] }
  match Kernel.Replay.verifyState staleRevisionState with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "stale-revision current evidence must invalidate state"
  match Kernel.Decide.closeWork firstWork.id staleRevisionState with
  | .error (.invariantViolation _) => pure ()
  | _ => throw <| IO.userError "stale-revision evidence must not authorize completion"
  let unrelated : Domain.Work.Activation :=
    { id := ⟨2⟩, work := ⟨2⟩, status := .suspended, readyToResume := true }
  let withUnrelated := { completable with
    work := [firstWork, secondWork]
    activations := [firstActivation, unrelated] }
  let afterUnrelated ← match Kernel.Decide.closeWork firstWork.id withUnrelated with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"completion with unrelated activation failed: {repr error}"
  expect (afterUnrelated.activations.any fun activation => activation == unrelated)
    "completion must preserve unrelated suspended activations"
  match Kernel.Decide.closeWork secondWork.id withUnrelated with
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
  IO.println "kernel laws: pass"

end AgentWorkbench.Tests.KernelLaws

def main : IO Unit :=
  AgentWorkbench.Tests.KernelLaws.main
