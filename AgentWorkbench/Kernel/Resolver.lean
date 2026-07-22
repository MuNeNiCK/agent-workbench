import AgentWorkbench.Kernel.Gates
import AgentWorkbench.Domain.Work

namespace AgentWorkbench.Kernel.Resolver

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Action
  | repairProjection (command : Domain.Projection.RepairCommand)
  | initializeWork (point : Domain.Projection.LedgerPoint)
  | continueActiveWork (point : Domain.Projection.LedgerPoint) (work : WorkId)
      (activation : ActivationId)
  | resumeSuspendedWork (point : Domain.Projection.LedgerPoint) (work : WorkId)
      (activation : ActivationId)
deriving DecidableEq, Repr

inductive Blocker
  | ledgerCorrupt (fault : Domain.Projection.LedgerFault)
  | invalidState (point : Domain.Projection.LedgerPoint)
  | noActivation (point : Domain.Projection.LedgerPoint)
  | noResumableActivation (point : Domain.Projection.LedgerPoint)
      (candidates : List ActivationId)
  | malformedInspection
  | nonExecutableAction (action : Action)
deriving DecidableEq, Repr

inductive Resolution
  | action (action : Action)
  | blocked (blocker : Blocker)
deriving DecidableEq, Repr

def resumableActivations (state : State) : List Domain.Work.Activation :=
  state.activations.filter fun activation =>
    Domain.Work.workIsOpen state.work activation.work &&
      Domain.Work.resumable state.activations activation.id

def Action.executable (inspection : Projection.Inspection) : Action → Bool
  | .repairProjection command =>
      decide (inspection.repairCommand? = some command)
  | .initializeWork point =>
      match inspection.currentState?, inspection.ledgerPoint? with
      | some state, some current =>
          decide (point = current) && decide (ValidState state) &&
            state.work.isEmpty && state.activations.isEmpty
      | _, _ => false
  | .continueActiveWork point work activation =>
      match inspection.currentState?, inspection.ledgerPoint? with
      | some state, some current =>
          decide (point = current) && decide (ValidState state) &&
            state.activations.any fun candidate =>
              candidate.id == activation && candidate.work == work &&
                candidate.status == .active
      | _, _ => false
  | .resumeSuspendedWork point work activation =>
      match inspection.currentState?, inspection.ledgerPoint? with
      | some state, some current =>
          decide (point = current) && decide (ValidState state) &&
            Domain.Work.workIsOpen state.work work &&
            state.activations.any (fun candidate =>
              candidate.id == activation && candidate.work == work) &&
            Domain.Work.resumable state.activations activation
      | _, _ => false

def candidateCurrentAction (point : Domain.Projection.LedgerPoint)
    (state : State) : Option Action :=
  if ValidState state then
    if state.work.isEmpty then
      if state.activations.isEmpty then some (.initializeWork point) else none
    else
      match (Domain.Work.activeActivations state.activations).head? with
      | some activation => some (.continueActiveWork point activation.work activation.id)
      | none =>
          match (resumableActivations state).head? with
          | some activation => some (.resumeSuspendedWork point activation.work activation.id)
          | none => none
  else
    none

def candidateAction (inspection : Projection.Inspection) : Option Action :=
  match inspection.repairCommand? with
  | some command => some (.repairProjection command)
  | none =>
      match inspection.currentState?, inspection.ledgerPoint? with
      | some state, some point => candidateCurrentAction point state
      | _, _ => none

def blockerFor (inspection : Projection.Inspection) : Blocker :=
  match inspection with
  | .ledgerCorrupt fault => .ledgerCorrupt fault
  | .fresh ledger projection =>
      match projection.payload with
      | .decodeFailed _ => .malformedInspection
      | .decoded state =>
          if !(decide (ValidState state)) then
            .invalidState ledger.point
          else if state.activations.isEmpty then
            .noActivation ledger.point
          else
            .noResumableActivation ledger.point (state.activations.map (·.id))
  | .missing _ _ | .stale _ _ _ | .corrupt _ _ _ _ => .malformedInspection

def Blocker.exact (inspection : Projection.Inspection) (blocker : Blocker) : Prop :=
  (candidateAction inspection = none ∧ blocker = blockerFor inspection) ∨
  ∃ action, candidateAction inspection = some action ∧
    action.executable inspection = false ∧ blocker = .nonExecutableAction action

instance (inspection : Projection.Inspection) (blocker : Blocker) :
    Decidable (blocker.exact inspection) := by
  unfold Blocker.exact
  infer_instance

def next (inspection : Projection.Inspection) : Resolution :=
  match candidateAction inspection with
  | some action =>
      if action.executable inspection then
        .action action
      else
        .blocked (.nonExecutableAction action)
  | none => .blocked (blockerFor inspection)

theorem next_action_is_executable (inspection : Projection.Inspection) {action : Action}
    (selected : next inspection = .action action) :
    action.executable inspection = true := by
  unfold next at selected
  split at selected
  · split at selected
    · cases selected
      assumption
    · contradiction
  · contradiction

theorem next_blocker_is_exact (inspection : Projection.Inspection) {blocker : Blocker}
    (selected : next inspection = .blocked blocker) :
    blocker.exact inspection := by
  unfold next at selected
  split at selected
  · split at selected
    · contradiction
    · cases selected
      simp_all [Blocker.exact]
  · cases selected
    simp_all [Blocker.exact]

theorem next_is_allowed (inspection : Projection.Inspection) :
    match next inspection with
    | .action action => action.executable inspection = true
    | .blocked blocker => blocker.exact inspection := by
  generalize selected : next inspection = resolution
  cases resolution with
  | action action => exact next_action_is_executable inspection selected
  | blocked blocker => exact next_blocker_is_exact inspection selected

end AgentWorkbench.Kernel.Resolver
