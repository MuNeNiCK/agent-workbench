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
  Work.ValidWorkState state.work state.activations ∧
  Review.ValidReviewState state.claims state.adjudications ∧
  Evidence.UniqueEvidenceIds state.evidence ∧
  Evidence.EvidenceWellFormed state.evidence ∧
  Evidence.EvidenceCurrentAt state.revision state.evidence ∧
  Evidence.EvidenceReferencesObligations state.evidence state.obligations ∧
  ExternalOperation.UniqueOperations state.externalOperations ∧
  ExternalOperation.AttemptsWellFormed state.externalOperations ∧
  Work.UniqueCompletionFacts state.completionFacts ∧
  Work.CompletionFactsReferenceWork state.work state.completionFacts ∧
  Work.CurrentCompletionFactsReferenceOpenWork state.work state.completionFacts ∧
  Work.CompletionFactsCurrent state.revision state.completionFacts ∧
  Evidence.UniqueObligations state.obligations ∧
  Evidence.ObligationsWellFormed state.obligations ∧
  Evidence.ObligationsReferenceWork (state.work.map (·.id)) state.obligations ∧
  Evidence.CurrentObligationsReferenceOpenWork
    ((state.work.filter (·.status == .open)).map (·.id)) state.obligations ∧
  Evidence.ObligationsCurrentAt state.revision state.obligations

instance (state : State) : Decidable (ValidState state) := by
  unfold ValidState Work.ValidWorkState Work.UniqueWorkIds
    Work.UniqueActivationIds Work.AtMostOneActive Work.ActiveReferencesOpenWork
    Work.ActivationsReferenceWork Work.NonterminalActivationsReferenceOpenWork
    Review.ValidReviewState Review.UniqueClaimIds Review.UniqueAdjudications
    Review.AdjudicationsReferenceClaims
    Evidence.UniqueEvidenceIds Evidence.EvidenceWellFormed
    Evidence.EvidenceCurrentAt
    Evidence.EvidenceReferencesObligations
    ExternalOperation.UniqueOperations ExternalOperation.AttemptsWellFormed
    Work.UniqueCompletionFacts Work.CompletionFactsReferenceWork
    Work.CurrentCompletionFactsReferenceOpenWork
    Work.CompletionFactsCurrent
    Evidence.UniqueObligations Evidence.ObligationsWellFormed
    Evidence.ObligationsReferenceWork Evidence.CurrentObligationsReferenceOpenWork
    Evidence.ObligationsCurrentAt
  infer_instance

structure VerifiedState where
  state : State
  valid : ValidState state

inductive Event
  | workInitialized (work : Work.WorkUnit) (activation : Work.Activation)
  | reviewClaimed (claim : Review.Claim)
  | reviewAdjudicated (decision : Review.Adjudication)
  | evidenceRecorded (item : Evidence.Evidence)
  | externalOperationRecorded (attempt : ExternalOperation.Attempt)
  | obligationRecorded (obligation : Evidence.Obligation)
  | completionEvidenceRecorded (facts : Work.CompletionFacts)
      (obligations : List Evidence.Obligation)
  | workCompleted (work : WorkId) (activation : ActivationId)
deriving DecidableEq, Repr

def applyUnchecked (event : Event) (state : State) : State :=
  let revised := state.revision.next
  let invalidated := {
    state with
    revision := revised
    evidence := Evidence.invalidateEvidence state.evidence
    completionFacts := Work.invalidateCompletionFacts state.completionFacts
    obligations := Evidence.invalidate state.obligations }
  match event with
  | .workInitialized work activation =>
      { invalidated with work := [work], activations := [activation] }
  | .reviewClaimed claim => { invalidated with claims := state.claims ++ [claim] }
  | .reviewAdjudicated decision =>
      { invalidated with adjudications := state.adjudications ++ [decision] }
  | .evidenceRecorded item =>
      let obligations := (Evidence.invalidate state.obligations).map fun obligation =>
        if obligation.work == item.work && obligation.key == item.obligation then
          { obligation with revision := revised, current := true }
        else obligation
      { invalidated with
        evidence := Evidence.invalidateEvidence state.evidence ++
          [{ item with revision := revised, current := true }]
        obligations }
  | .externalOperationRecorded attempt =>
      { invalidated with externalOperations := state.externalOperations ++ [attempt] }
  | .obligationRecorded obligation =>
      let retained := (Evidence.invalidate state.obligations).filter fun existing =>
        existing.work != obligation.work || existing.key != obligation.key
      { invalidated with obligations := retained ++ [{ obligation with revision := revised, current := true }] }
  | .completionEvidenceRecorded facts obligations =>
      let retainedFacts := (Work.invalidateCompletionFacts state.completionFacts).filter
        (·.work != facts.work)
      let retainedObligations := (Evidence.invalidate state.obligations).filter fun existing =>
        existing.work != facts.work ||
          !(obligations.any fun replacement => replacement.key == existing.key)
      { invalidated with
        completionFacts := retainedFacts ++ [{ facts with revision := revised, current := true }]
        obligations := retainedObligations ++ obligations.map fun obligation =>
          { obligation with work := facts.work, revision := revised, current := true } }
  | .workCompleted work activation =>
      { invalidated with
        work := Work.closeWork state.work work
        activations := Work.closeActivation state.activations activation }

def verifyState (state : State) : Except DomainError VerifiedState :=
  if valid : ValidState state then
    .ok ⟨state, valid⟩
  else
    .error (.invariantViolation "state invariant violation")

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

theorem work_completed_event_exact (state : State) (work : WorkId) (activation : ActivationId) :
    let completed := applyUnchecked (.workCompleted work activation) state
    completed.work = Work.closeWork state.work work ∧
    completed.activations = Work.closeActivation state.activations activation ∧
    completed.revision = state.revision.next := by
  simp [applyUnchecked]

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
    Work.ActiveReferencesOpenWork, Work.ActivationsReferenceWork,
    Work.NonterminalActivationsReferenceOpenWork,
    Review.ValidReviewState,
    Review.UniqueClaimIds, Review.UniqueAdjudications,
    Review.AdjudicationsReferenceClaims, Evidence.UniqueEvidenceIds,
    Evidence.EvidenceWellFormed, Evidence.EvidenceCurrentAt,
    Evidence.EvidenceReferencesObligations,
    ExternalOperation.UniqueOperations,
    ExternalOperation.AttemptsWellFormed, Work.UniqueCompletionFacts,
    Work.CompletionFactsReferenceWork, Work.CurrentCompletionFactsReferenceOpenWork,
    Work.CompletionFactsCurrent,
    Evidence.UniqueObligations, Evidence.ObligationsWellFormed,
    Evidence.ObligationsReferenceWork, Evidence.CurrentObligationsReferenceOpenWork,
    Evidence.ObligationsCurrentAt,
    Work.activeActivations, emptyState]

end AgentWorkbench.Kernel.Replay
