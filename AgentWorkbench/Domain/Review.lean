import AgentWorkbench.Domain.Work

namespace AgentWorkbench.Domain.Review

open AgentWorkbench.Domain

abbrev Ref := ReviewRef

inductive Purpose
  | designMeaning
  | implementation
  | reuseDecision
deriving DecidableEq, Repr, BEq

structure Scope where
  work : WorkRef
  design : List DesignRef
  task : Option TaskRef
  purpose : Purpose
  artifacts : List String
deriving DecidableEq, Repr, BEq

def Scope.wellFormed (scope : Scope) : Bool :=
  !scope.work.key.isEmpty &&
    (scope.design.map (·.key)).all (fun key => !key.isEmpty) &&
    scope.design.Nodup &&
    scope.task.all (fun task => !task.key.isEmpty) &&
    !scope.artifacts.isEmpty &&
    scope.artifacts.all (fun artifact => !artifact.isEmpty)

structure Request where
  ref : Ref
  scope : Scope
deriving DecidableEq, Repr, BEq

inductive ObservationKind
  | risk
  | proposal
deriving DecidableEq, Repr, BEq

structure Observation where
  key : String
  kind : ObservationKind
  summary : String
  evidence : String
  addsComplexity : Bool := false
deriving DecidableEq, Repr, BEq

def Observation.wellFormed (observation : Observation) : Bool :=
  !observation.key.isEmpty &&
    !observation.summary.isEmpty &&
    !observation.evidence.isEmpty &&
    (observation.kind == .proposal || !observation.addsComplexity)

structure Result where
  review : Ref
  scope : Scope
  reviewer : String
  observations : List Observation
deriving DecidableEq, Repr, BEq

def Result.exactFor (result : Result) (request : Request) : Bool :=
  result.review == request.ref &&
    result.scope == request.scope &&
    request.scope.wellFormed &&
    !result.reviewer.isEmpty &&
    result.observations.all Observation.wellFormed &&
    (result.observations.map (·.key)).Nodup

inductive Decision
  | accepted
  | rejected
  | rescoped
  | deferred
  | needsEvidence
deriving DecidableEq, Repr, BEq

def Decision.final : Decision → Bool
  | .accepted | .rejected | .rescoped => true
  | .deferred | .needsEvidence => false

abbrev ComplexityRationale := Design.ComplexityRationale

structure Disposition where
  review : Ref
  observation : String
  decision : Decision
  caller : CallerDecision
  successorDesign : Option Design.AcceptedRef := none
  complexity : Option ComplexityRationale := none
deriving DecidableEq, Repr, BEq

def Disposition.wellFormedFor (observation : Observation)
    (disposition : Disposition) : Bool :=
  disposition.observation == observation.key &&
    disposition.caller.wellFormed &&
    match observation.kind, disposition.decision with
    | .risk, .accepted =>
        disposition.successorDesign.isNone && disposition.complexity.isNone
    | .proposal, .accepted =>
        disposition.successorDesign.isSome &&
          if observation.addsComplexity then
            disposition.complexity.any Design.ComplexityRationale.wellFormed
          else
            disposition.complexity.isNone
    | _, _ =>
        disposition.successorDesign.isNone && disposition.complexity.isNone

def latestDisposition? (review : Ref) (observation : String)
    (dispositions : List Disposition) : Option Disposition :=
  dispositions.reverse.find? fun disposition =>
    disposition.review == review &&
      disposition.observation == observation

def Result.resolvedBy (result : Result) (request : Request)
    (dispositions : List Disposition) : Bool :=
  result.exactFor request &&
    result.observations.all fun observation =>
      (latestDisposition? result.review observation.key dispositions).any
        fun disposition =>
          disposition.decision.final &&
            disposition.wellFormedFor observation

end AgentWorkbench.Domain.Review
