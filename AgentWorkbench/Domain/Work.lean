import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Work

open AgentWorkbench.Domain

structure WorkUnit where
  id : WorkId
  status : WorkStatus
deriving DecidableEq, Repr

structure SuspensionContext where
  reason : String
  returnPoint : String
  assumptions : List String
  resumeConditions : List String
deriving DecidableEq, Repr

def SuspensionContext.wellFormed (context : SuspensionContext) : Bool :=
  !context.reason.isEmpty && !context.returnPoint.isEmpty &&
  !context.assumptions.isEmpty && !context.resumeConditions.isEmpty &&
  context.assumptions.all (fun assumption => !assumption.isEmpty) &&
  context.resumeConditions.all (fun condition => !condition.isEmpty)

structure Activation where
  id : ActivationId
  work : WorkId
  status : ActivationStatus
  readyToResume : Bool
  suspension : Option SuspensionContext := none
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

def ActivationsReferenceWork (work : List WorkUnit) (activations : List Activation) : Prop :=
  (activations.all fun activation => work.any (·.id == activation.work)) = true

def NonterminalActivationsReferenceOpenWork (work : List WorkUnit)
    (activations : List Activation) : Prop :=
  (activations.all fun activation =>
    activation.status == .closed ||
      work.any fun unit => unit.id == activation.work && unit.status == .open) = true

def ValidWorkState (work : List WorkUnit) (activations : List Activation) : Prop :=
  UniqueWorkIds work ∧
  UniqueActivationIds activations ∧
  AtMostOneActive activations ∧
  ActiveReferencesOpenWork work activations ∧
  ActivationsReferenceWork work activations ∧
  NonterminalActivationsReferenceOpenWork work activations

theorem single_active_activation {activations : List Activation}
    (valid : AtMostOneActive activations) :
    (activeActivations activations).length ≤ 1 := by
  exact valid

def noActive (activations : List Activation) : Bool :=
  (activeActivations activations).isEmpty

def activeFor (activations : List Activation) (work : WorkId) : Option Activation :=
  activations.find? fun activation =>
    activation.work == work && activation.status == .active

def workIsOpen (work : List WorkUnit) (target : WorkId) : Bool :=
  work.any fun unit => unit.id == target && unit.status == .open

def closeWork (work : List WorkUnit) (target : WorkId) : List WorkUnit :=
  work.map fun unit =>
    if unit.id == target then { unit with status := .closed } else unit

def closeActivation (activations : List Activation) (target : ActivationId) : List Activation :=
  activations.map fun activation =>
    if activation.id == target then { activation with status := .closed } else activation

def resumable (activations : List Activation) (id : ActivationId) : Bool :=
  noActive activations && activations.any fun activation =>
    activation.id == id &&
    activation.status == .suspended &&
    activation.readyToResume &&
    activation.suspension.any SuspensionContext.wellFormed

def suspend (activations : List Activation) (id : ActivationId)
    (context : SuspensionContext) : Option (List Activation) :=
  if context.wellFormed && activations.any (fun activation =>
      activation.id == id && activation.status == .active) then
    some <| activations.map fun activation =>
      if activation.id == id then
        { activation with
          status := .suspended
          readyToResume := false
          suspension := some context }
      else activation
  else
    none

def markResumeReady (activations : List Activation) (id : ActivationId) :
    Option (List Activation) :=
  if noActive activations && activations.any (fun activation =>
      activation.id == id && activation.status == .suspended &&
        activation.suspension.any SuspensionContext.wellFormed) then
    some <| activations.map fun activation =>
      if activation.id == id then { activation with readyToResume := true }
      else activation
  else
    none

def resume (activations : List Activation) (id : ActivationId) : Option (List Activation) :=
  if resumable activations id then
    some <| activations.map fun activation =>
      if activation.id == id then
        { activation with status := .active, readyToResume := false }
      else activation
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
