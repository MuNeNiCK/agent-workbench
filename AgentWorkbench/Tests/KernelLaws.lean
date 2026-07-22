import AgentWorkbench.Application.Service

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

def replaceWorkAndActivations (state : Kernel.Replay.State)
    (work : List Domain.Work.WorkUnit)
    (activations : List Domain.Work.Activation) : Kernel.Decide.Command :=
  .replaceWorkState state.revision work activations

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw <| IO.userError message

def expectRejectedNoEffect (command : Kernel.Decide.Command)
    (state : Kernel.Replay.State) (message : String) : IO Unit := do
  let result := Application.Service.execute command state
  match result with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError s!"{message}: command unexpectedly accepted"
  expect (Kernel.Decide.committedEvents result).isEmpty
    s!"{message}: rejected command exposed events"
  expect (Kernel.Decide.committedState result state == state)
    s!"{message}: rejected command changed state or revision"

def completeFacts : Domain.Work.CompletionFacts :=
  { work := firstWork.id
    revision := ⟨2⟩
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
  expect (Application.Service.queryValidity initial == .pass) "empty state must be valid"
  let first ← match Application.Service.execute
      (replaceWorkAndActivations initial [firstWork] [firstActivation]) initial with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"first activation rejected: {repr error}"
  expect (first.revision == ⟨2⟩) "each accepted event must advance the revision"
  let invalid := replaceWorkAndActivations first [firstWork, secondWork]
    [firstActivation, secondActivation]
  match Application.Service.execute invalid first with
  | .error (.invariantViolation _) => pure ()
  | .error error => throw <| IO.userError s!"wrong rejection: {repr error}"
  | .ok _ => throw <| IO.userError "two active activations must be rejected"
  expect (Kernel.Decide.committedState
    (Application.Service.execute invalid first)
    first == first) "rejection must leave the state unchanged"
  expect (Kernel.Decide.committedEvents
    (Application.Service.execute invalid first)).isEmpty
    "rejection must expose no accepted events"
  let stale : Kernel.Decide.Command :=
    .replaceWorkState ⟨0⟩ [firstWork, secondWork] [firstActivation, secondActivation]
  match Application.Service.execute stale first with
  | .error .staleRevision => pure ()
  | _ => throw <| IO.userError "stale command must be rejected"
  expectRejectedNoEffect stale first "stale revision rejection"
  let claim : Domain.Review.Claim := { id := ⟨1⟩, claim := .clean }
  let claimed ← match Application.Service.execute
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
  match Application.Service.execute
      (.recordReviewAdjudication claimed.revision adjudication) claimed with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"valid adjudication rejected: {repr error}"
  let item : Domain.Evidence.Evidence :=
    { id := ⟨1⟩, obligation := "proof", artifactDigest := "sha256:evidence", current := true }
  let evidenced ← match Application.Service.execute (.recordEvidence first.revision item) first with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid evidence rejected: {repr error}"
  expectRejectedNoEffect (.recordEvidence evidenced.revision item) evidenced
    "duplicate evidence identity"
  let malformedItem := { item with id := ⟨2⟩, artifactDigest := "" }
  expectRejectedNoEffect (.recordEvidence first.revision malformedItem) first
    "malformed evidence"
  let attempt : Domain.ExternalOperation.Attempt :=
    { operation := ⟨"publish-1"⟩, artifactDigest := "sha256:artifact", state := .prepared }
  let externalized ← match Application.Service.execute
      (.recordExternalOperation first.revision attempt) first with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid external attempt rejected: {repr error}"
  expectRejectedNoEffect (.recordExternalOperation externalized.revision attempt) externalized
    "duplicate external operation"
  let malformedAttempt := { attempt with operation := ⟨"publish-2"⟩, artifactDigest := "" }
  expectRejectedNoEffect (.recordExternalOperation first.revision malformedAttempt) first
    "malformed external operation"
  expect (Domain.Work.resume [firstActivation] firstActivation.id).isNone
    "an active activation cannot resume"
  let suspended := { firstActivation with status := .suspended, readyToResume := false }
  expect (Domain.Work.resume [suspended] suspended.id).isNone
    "resume must reject an unready activation"
  let ready := { suspended with readyToResume := true }
  expect (Domain.Work.resume [ready] ready.id).isSome
    "resume must accept a ready activation when no activation is active"
  let reviewState : Policy.Authority.ReviewState := { claims := [], adjudications := [] }
  expect (Policy.Authority.authority (Policy.Authority.recordClaim reviewState claim) ==
    Policy.Authority.authority reviewState) "a review claim must not create authority"
  let currentObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := ⟨2⟩, current := true }
  let staleObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := ⟨2⟩, current := false }
  match Application.Service.execute (.recordObligation first.revision currentObligation) first with
  | .ok _ => pure ()
  | .error error => throw <| IO.userError s!"valid obligation rejected: {repr error}"
  let malformedObligation := { currentObligation with key := "" }
  expectRejectedNoEffect (.recordObligation first.revision malformedObligation) first
    "malformed obligation"
  expectRejectedNoEffect
    (.recordCompletionEvidence first.revision completeFacts
      [currentObligation, currentObligation]) first
    "duplicate completion obligations"
  expect (Policy.Completion.closeable firstWork.id first.work first.activations
    [completeFacts] [currentObligation])
    "complete current obligations must allow completion"
  expect (!(Policy.Completion.closeable firstWork.id first.work first.activations
    [completeFacts] [])) "completion must reject missing obligations"
  expect (!(Policy.Completion.closeable firstWork.id first.work first.activations
    [completeFacts] [staleObligation]))
    "completion must reject stale obligations"
  let completable := { first with completionFacts := [completeFacts], obligations := [currentObligation] }
  let completed ← match Application.Service.complete firstWork.id completable with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"valid completion rejected: {repr error}"
  expect (completed.work.any fun unit =>
    unit.id == firstWork.id && unit.status == .closed) "completion must close the target work"
  expect (completed.activations.any fun activation =>
    activation.id == firstActivation.id && activation.status == .closed)
    "completion must atomically close the owning activation"
  expect (completed.revision == completable.revision.next)
    "atomic completion must advance exactly one revision"
  let contradictory := { completable with completionFacts :=
    [completeFacts, { completeFacts with revision := ⟨3⟩, tasksComplete := false }] }
  match Kernel.Replay.verifyState contradictory with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "contradictory completion facts must invalidate state"
  match Application.Service.complete firstWork.id contradictory with
  | .error (.invariantViolation _) => pure ()
  | _ => throw <| IO.userError "invalid contradictory facts must not authorize completion"
  let staleRevisionState := { completable with
    completionFacts := [{ completeFacts with revision := ⟨0⟩ }]
    obligations := [{ currentObligation with revision := ⟨0⟩ }] }
  match Kernel.Replay.verifyState staleRevisionState with
  | .error _ => pure ()
  | .ok _ => throw <| IO.userError "stale-revision current evidence must invalidate state"
  match Application.Service.complete firstWork.id staleRevisionState with
  | .error (.invariantViolation _) => pure ()
  | _ => throw <| IO.userError "stale-revision evidence must not authorize completion"
  let unrelated : Domain.Work.Activation :=
    { id := ⟨2⟩, work := ⟨2⟩, status := .suspended, readyToResume := true }
  let withUnrelated := { completable with
    work := [firstWork, secondWork]
    activations := [firstActivation, unrelated] }
  let afterUnrelated ← match Application.Service.complete firstWork.id withUnrelated with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"completion with unrelated activation failed: {repr error}"
  expect (afterUnrelated.activations.any fun activation => activation == unrelated)
    "completion must preserve unrelated suspended activations"
  match Application.Service.complete secondWork.id withUnrelated with
  | .error (.invalidTransition _) => pure ()
  | _ => throw <| IO.userError "inactive target completion must reject"
  let receipt : Policy.Update.Receipt :=
    { operation := ⟨"operation-1"⟩, payloadDigest := "payload", resultDigest := "result" }
  expect (Policy.Update.resolveRetry receipt.operation "payload" ⟨0⟩ ⟨99⟩ [receipt] ==
    .exact receipt) "an exact retry must return its receipt despite a later revision"
  expect (Policy.Update.resolveRetry receipt.operation "changed" ⟨0⟩ ⟨99⟩ [receipt] ==
    .payloadConflict) "a changed retry payload must conflict"
  let observed := Kernel.Gates.observeGate Kernel.Gates.validStateGate first
  expect (observed.1 == first) "gate observation must preserve state"
  match Application.Service.resolve first with
  | .action action =>
      expect (action.executable first)
        "next action must be executable at its stated revision and target"
      let revised := { first with revision := first.revision.next }
      expect (!action.executable revised)
        "a projected action must become non-executable after revision change"
  | .blocked _ => throw <| IO.userError "active work must resolve to an action"
  let noActivation := { first with activations := [] }
  match Application.Service.resolve noActivation with
  | .blocked blocker@(.noActivation revision) =>
      expect (revision == noActivation.revision)
        "a no-activation blocker must state the current revision"
      expect (decide (blocker.exact noActivation))
        "a no-activation blocker must exactly describe the inspected state"
  | _ => throw <| IO.userError "missing activation must return its exact blocker"
  let notReady := { first with activations := [suspended] }
  match Application.Service.resolve notReady with
  | .blocked blocker@(.noResumableActivation revision candidates) =>
      expect (revision == notReady.revision && candidates == [suspended.id])
        "an unready activation blocker must state revision and candidates"
      expect (decide (blocker.exact notReady))
        "an unready activation blocker must exactly describe the inspected state"
  | _ => throw <| IO.userError "unready activation must not emit resume"
  let readyState := { first with activations := [ready] }
  match Application.Service.resolve readyState with
  | .action action@(.resumeSuspendedWork revision work activation) =>
      expect (revision == readyState.revision && work == firstWork.id && activation == ready.id)
        "resume must bind current revision, target work, and activation"
      expect (action.executable readyState)
        "a returned resume action must execute against the inspected state"
  | _ => throw <| IO.userError "ready suspended activation must emit exact resume"
  match Application.Service.resolve staleRevisionState with
  | .blocked blocker@(.invalidState revision) =>
      expect (revision == staleRevisionState.revision)
        "invalid-state blocker must state the inspected revision"
      expect (decide (blocker.exact staleRevisionState))
        "an invalid-state blocker must exactly describe the inspected state"
  | _ => throw <| IO.userError "invalid state must return an exact blocker"
  IO.println "kernel laws: pass"

end AgentWorkbench.Tests.KernelLaws

def main : IO Unit :=
  AgentWorkbench.Tests.KernelLaws.main
