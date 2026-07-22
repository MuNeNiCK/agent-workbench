import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Work

open AgentWorkbench.Domain

structure WorkUnit where
  id : WorkId
  status : WorkStatus
deriving DecidableEq, Repr

structure Activation where
  id : ActivationId
  work : WorkId
  status : ActivationStatus
  readyToResume : Bool
deriving DecidableEq, Repr

def activeActivations (activations : List Activation) : List Activation :=
  activations.filter (fun activation => activation.status == .active)

def AtMostOneActive (activations : List Activation) : Prop :=
  (activeActivations activations).length ≤ 1

def UniqueWorkIds (work : List WorkUnit) : Prop :=
  (work.map (·.id)).Nodup

def UniqueActivationIds (activations : List Activation) : Prop :=
  (activations.map (·.id)).Nodup

def ActiveReferencesOpenWork (work : List WorkUnit) (activations : List Activation) : Prop :=
  (activations.all fun activation =>
    activation.status != .active ||
      work.any fun unit => unit.id == activation.work && unit.status == .open) = true

def ValidWorkState (work : List WorkUnit) (activations : List Activation) : Prop :=
  UniqueWorkIds work ∧
  UniqueActivationIds activations ∧
  AtMostOneActive activations ∧
  ActiveReferencesOpenWork work activations

theorem single_active_activation {activations : List Activation}
    (valid : AtMostOneActive activations) :
    (activeActivations activations).length ≤ 1 :=
  valid

def noActive (activations : List Activation) : Bool :=
  (activeActivations activations).isEmpty

def resumable (activations : List Activation) (id : ActivationId) : Bool :=
  noActive activations && activations.any fun activation =>
    activation.id == id &&
    activation.status == .suspended &&
    activation.readyToResume

def resume (activations : List Activation) (id : ActivationId) : Option (List Activation) :=
  if resumable activations id then
    some <| activations.map fun activation =>
      if activation.id == id then { activation with status := .active } else activation
  else
    none

theorem resume_requires_readiness {activations : List Activation} {id : ActivationId}
    {resumed : List Activation} (accepted : resume activations id = some resumed) :
    resumable activations id = true := by
  unfold resume at accepted
  split at accepted
  · assumption
  · contradiction

end AgentWorkbench.Domain.Work
