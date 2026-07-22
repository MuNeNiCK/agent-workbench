import AgentWorkbench.Domain.Work
import AgentWorkbench.Domain.Design
import AgentWorkbench.Domain.Review
import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.ExternalOperation

namespace AgentWorkbench.Kernel.Replay

open AgentWorkbench.Domain

structure State where
  revision : Revision
  work : List Work.WorkUnit
  activations : List Work.Activation
  claims : List Review.Claim
  adjudications : List Review.Adjudication
  evidence : List Evidence.Evidence
  externalOperations : List ExternalOperation.Attempt
  obligations : List Evidence.Obligation
  completionFacts : List Work.CompletionFacts
deriving DecidableEq, Repr

def ValidState (state : State) : Prop :=
  Work.ValidWorkState state.work state.activations

instance (state : State) : Decidable (ValidState state) := by
  unfold ValidState Work.ValidWorkState Work.UniqueWorkIds
    Work.UniqueActivationIds Work.AtMostOneActive Work.ActiveReferencesOpenWork
  infer_instance

structure VerifiedState where
  state : State
  valid : ValidState state

inductive Event
  | replaceWork (work : List Work.WorkUnit)
  | replaceActivations (activations : List Work.Activation)
  | reviewClaimed (claim : Review.Claim)
  | reviewAdjudicated (decision : Review.Adjudication)
  | evidenceRecorded (item : Evidence.Evidence)
  | externalOperationRecorded (attempt : ExternalOperation.Attempt)
  | obligationRecorded (obligation : Evidence.Obligation)
  | workCompleted (work : WorkId) (activation : ActivationId)
deriving DecidableEq, Repr

def applyUnchecked (event : Event) (state : State) : State :=
  let revised := state.revision.next
  match event with
  | .replaceWork work => { state with revision := revised, work }
  | .replaceActivations activations => { state with revision := revised, activations }
  | .reviewClaimed claim => { state with revision := revised, claims := state.claims ++ [claim] }
  | .reviewAdjudicated decision =>
      { state with revision := revised, adjudications := state.adjudications ++ [decision] }
  | .evidenceRecorded item => { state with revision := revised, evidence := state.evidence ++ [item] }
  | .externalOperationRecorded attempt =>
      { state with revision := revised, externalOperations := state.externalOperations ++ [attempt] }
  | .obligationRecorded obligation =>
      { state with revision := revised, obligations := state.obligations ++ [obligation] }
  | .workCompleted work activation =>
      { state with
        revision := revised
        work := Work.closeWork state.work work
        activations := Work.closeActivation state.activations activation }

def verifyState (state : State) : Except DomainError VerifiedState :=
  if valid : ValidState state then
    .ok ⟨state, valid⟩
  else
    .error (.invariantViolation "more than one active activation")

def applyEvent (event : Event) (verified : VerifiedState) : Except DomainError VerifiedState :=
  verifyState (applyUnchecked event verified.state)

def replayFrom : List Event → VerifiedState → Except DomainError VerifiedState
  | [], state => .ok state
  | event :: rest, state => do
      replayFrom rest (← applyEvent event state)

def replay (events : List Event) (initial : State) : Except DomainError VerifiedState := do
  replayFrom events (← verifyState initial)

theorem replay_deterministic (events : List Event) (initial : State)
    {left right : VerifiedState}
    (leftResult : replay events initial = .ok left)
    (rightResult : replay events initial = .ok right) :
    left.state = right.state := by
  rw [leftResult] at rightResult
  simp only [Except.ok.injEq] at rightResult
  exact congrArg VerifiedState.state rightResult

theorem replay_preserves_valid (events : List Event) (initial : State)
    {result : VerifiedState} (_accepted : replay events initial = .ok result) :
    ValidState result.state :=
  result.valid

def emptyState : State :=
  { revision := ⟨0⟩
    work := []
    activations := []
    claims := []
    adjudications := []
    evidence := []
    externalOperations := []
    obligations := []
    completionFacts := [] }

theorem emptyState_valid : ValidState emptyState := by
  simp [ValidState, Work.ValidWorkState, Work.UniqueWorkIds,
    Work.UniqueActivationIds, Work.AtMostOneActive,
    Work.ActiveReferencesOpenWork, Work.activeActivations, emptyState]

end AgentWorkbench.Kernel.Replay
