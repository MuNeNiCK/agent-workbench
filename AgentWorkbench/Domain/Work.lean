import AgentWorkbench.Domain.Design

namespace AgentWorkbench.Domain.Work

open AgentWorkbench.Domain

inductive DerivationBasis
  | design (items : List Design.AcceptedRef)
  | workBoundary (work : WorkRef)
deriving DecidableEq, Repr, BEq

def DerivationBasis.wellFormed (basis : DerivationBasis) : Bool :=
  match basis with
  | .design items =>
      !items.isEmpty &&
        (items.map (·.ref)).Nodup &&
        items.all (fun item => !item.ref.key.isEmpty)
  | .workBoundary work => !work.key.isEmpty

inductive CompletionTarget
  | taskSatisfied (task : TaskRef)
  | assurance (description : String)
  | reviewResolved (review : ReviewRef)
  | externalObservation (evidence : EvidenceRef)
deriving DecidableEq, Repr, BEq

structure CompletionMember where
  target : CompletionTarget
  basis : DerivationBasis
deriving DecidableEq, Repr, BEq

def CompletionTarget.wellFormed : CompletionTarget → Bool
  | .taskSatisfied task => !task.key.isEmpty
  | .assurance key => !key.isEmpty
  | .reviewResolved review => !review.key.isEmpty
  | .externalObservation evidence => !evidence.key.isEmpty

def CompletionMember.wellFormedFor (work : WorkRef)
    (member : CompletionMember) : Bool :=
  member.target.wellFormed && member.basis.wellFormed &&
    match member.basis with
    | .design _ => true
    | .workBoundary selected => selected.key == work.key

structure Unit where
  ref : WorkRef
  outcome : String
  completionBoundary : List CompletionMember
  authority : CallerDecision
deriving DecidableEq, Repr, BEq

def Unit.wellFormed (work : Unit) : Bool :=
  !work.ref.key.isEmpty &&
    !work.outcome.isEmpty &&
    !work.completionBoundary.isEmpty &&
    work.completionBoundary.all (CompletionMember.wellFormedFor work.ref) &&
    work.authority.wellFormed

structure Phase where
  key : String
  name : String
  displayOrder : Nat
deriving DecidableEq, Repr, BEq

inductive TaskState
  | pending
  | satisfied
deriving DecidableEq, Repr, BEq

structure Task where
  ref : TaskRef
  work : WorkRef
  description : String
  basis : DerivationBasis
  designScope : List Design.AcceptedRef
  phase : Option String
  state : TaskState
deriving DecidableEq, Repr, BEq

def Task.wellFormed (task : Task) : Bool :=
  !task.ref.key.isEmpty &&
    !task.work.key.isEmpty &&
    !task.description.isEmpty &&
    task.basis.wellFormed &&
    (task.designScope.map (·.ref)).Nodup &&
    task.designScope.all (fun item => !item.ref.key.isEmpty) &&
    task.phase.all (fun phase => !phase.isEmpty) &&
    match task.basis with
    | .design items => task.designScope == items
    | .workBoundary selectedWork => task.work.key == selectedWork.key

inductive ReturnAssumption
  | design (item : DesignRef)
  | workBoundary (work : WorkRef)
deriving DecidableEq, Repr, BEq

def ReturnAssumption.wellFormed : ReturnAssumption → Bool
  | .design item => !item.key.isEmpty
  | .workBoundary work => !work.key.isEmpty

structure ReturnPoint where
  work : WorkRef
  task : Option TaskRef
  assumptions : List ReturnAssumption
deriving DecidableEq, Repr, BEq

def ReturnPoint.wellFormed (point : ReturnPoint) : Bool :=
  !point.work.key.isEmpty &&
    point.task.all (fun task => !task.key.isEmpty) &&
    !point.assumptions.isEmpty &&
    point.assumptions.Nodup &&
    point.assumptions.all ReturnAssumption.wellFormed

structure Focus where
  work : WorkRef
  task : Option TaskRef
  returnPoint : Option ReturnPoint
deriving DecidableEq, Repr, BEq

def Focus.wellFormed (focus : Focus) : Bool :=
  !focus.work.key.isEmpty &&
    focus.task.all (fun task => !task.key.isEmpty) &&
    focus.returnPoint.all ReturnPoint.wellFormed

end AgentWorkbench.Domain.Work
