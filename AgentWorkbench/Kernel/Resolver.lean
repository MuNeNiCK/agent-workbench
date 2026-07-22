import AgentWorkbench.Kernel.Gates
import AgentWorkbench.Domain.Work

namespace AgentWorkbench.Kernel.Resolver

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Action
  | initializeWork (expectedRevision : Revision)
  | continueActiveWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId)
  | resumeSuspendedWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId)
deriving DecidableEq, Repr

inductive Blocker
  | invalidState (revision : Revision)
  | noActivation (revision : Revision)
  | noResumableActivation (revision : Revision) (candidates : List ActivationId)
  | nonExecutableAction (revision : Revision) (action : Action)
deriving DecidableEq, Repr

inductive Resolution
  | action (action : Action)
  | blocked (blocker : Blocker)
deriving DecidableEq, Repr

def resumableActivations (state : State) : List Domain.Work.Activation :=
  state.activations.filter fun activation =>
    Domain.Work.workIsOpen state.work activation.work &&
      Domain.Work.resumable state.activations activation.id

def Action.executable (state : State) : Action → Bool
  | .initializeWork revision =>
      revision == state.revision && state.work.isEmpty && state.activations.isEmpty
  | .continueActiveWork revision work activation =>
      revision == state.revision &&
        state.activations.any fun candidate =>
          candidate.id == activation && candidate.work == work && candidate.status == .active
  | .resumeSuspendedWork revision work activation =>
      revision == state.revision &&
        Domain.Work.workIsOpen state.work work &&
        Domain.Work.resumable state.activations activation

def blockerFor (state : State) : Blocker :=
  if !(decide (ValidState state)) then
    .invalidState state.revision
  else if state.activations.isEmpty then
    .noActivation state.revision
  else
    .noResumableActivation state.revision (state.activations.map (·.id))

def candidateAction (state : State) : Option Action :=
  if ValidState state then
    if state.work.isEmpty then
      if state.activations.isEmpty then
        some (.initializeWork state.revision)
      else
        none
    else
      match (Domain.Work.activeActivations state.activations).head? with
      | some activation =>
          some (.continueActiveWork state.revision activation.work activation.id)
      | none =>
          match (resumableActivations state).head? with
          | some activation =>
              some (.resumeSuspendedWork state.revision activation.work activation.id)
          | none => none
  else
    none

def Blocker.exact (state : State) : Blocker → Prop
  | .invalidState revision =>
      revision = state.revision ∧ ¬ValidState state
  | .noActivation revision =>
      revision = state.revision ∧
        ValidState state ∧
        candidateAction state = none ∧
        state.activations = []
  | .noResumableActivation revision candidates =>
      revision = state.revision ∧
        ValidState state ∧
        candidateAction state = none ∧
        state.activations ≠ [] ∧
        candidates = state.activations.map (·.id)
  | .nonExecutableAction revision action =>
      revision = state.revision ∧
        candidateAction state = some action ∧
        action.executable state = false

instance (state : State) (blocker : Blocker) : Decidable (blocker.exact state) := by
  cases blocker <;> unfold Blocker.exact <;> infer_instance

def next (state : State) : Resolution :=
  match candidateAction state with
  | some action =>
      if action.executable state then
        .action action
      else
        .blocked (.nonExecutableAction state.revision action)
  | none => .blocked (blockerFor state)

theorem next_action_is_executable (state : State) {action : Action}
    (selected : next state = .action action) :
    action.executable state = true := by
  unfold next at selected
  split at selected
  · split at selected
    · cases selected
      assumption
    · contradiction
  · contradiction

theorem next_blocker_is_exact (state : State) {blocker : Blocker}
    (selected : next state = .blocked blocker) :
    blocker.exact state := by
  unfold next at selected
  split at selected
  · split at selected
    · contradiction
    · cases selected
      simp_all [Blocker.exact]
  · cases selected
    unfold blockerFor
    split
    · simp_all [Blocker.exact]
    · split <;> simp_all [Blocker.exact]

theorem next_is_allowed (state : State) :
    match next state with
    | .action action => action.executable state = true
    | .blocked blocker => blocker.exact state := by
  generalize selected : next state = resolution
  cases resolution with
  | action action => exact next_action_is_executable state selected
  | blocked blocker => exact next_blocker_is_exact state selected

end AgentWorkbench.Kernel.Resolver
