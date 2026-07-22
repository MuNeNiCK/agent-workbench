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
  { expectedRevision := state.revision
    events := [.replaceWork work, .replaceActivations activations]
    eventsNonempty := by simp }

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw <| IO.userError message

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
  let stale := { invalid with expectedRevision := ⟨0⟩ }
  match Application.Service.execute stale first with
  | .error .staleRevision => pure ()
  | _ => throw <| IO.userError "stale command must be rejected"
  expect (Domain.Work.resume [firstActivation] firstActivation.id).isNone
    "an active activation cannot resume"
  let suspended := { firstActivation with status := .suspended, readyToResume := false }
  expect (Domain.Work.resume [suspended] suspended.id).isNone
    "resume must reject an unready activation"
  let ready := { suspended with readyToResume := true }
  expect (Domain.Work.resume [ready] ready.id).isSome
    "resume must accept a ready activation when no activation is active"
  let reviewState : Policy.Authority.ReviewState := { claims := [], adjudications := [] }
  let claim : Domain.Review.Claim := { id := ⟨1⟩, claim := .clean }
  expect (Policy.Authority.authority (Policy.Authority.recordClaim reviewState claim) ==
    Policy.Authority.authority reviewState) "a review claim must not create authority"
  let currentObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := ⟨2⟩, current := true }
  let staleObligation : Domain.Evidence.Obligation :=
    { work := firstWork.id, key := "proof", revision := ⟨2⟩, current := false }
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
  | some action =>
    expect ((Kernel.Resolver.allowedActions first).contains action)
      "next must belong to the allowed action set"
  | none => throw <| IO.userError "resolver must return an allowed action"
  IO.println "kernel laws: pass"

end AgentWorkbench.Tests.KernelLaws

def main : IO Unit :=
  AgentWorkbench.Tests.KernelLaws.main
