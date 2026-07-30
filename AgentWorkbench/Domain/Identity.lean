namespace AgentWorkbench.Domain

structure SourceId where
  value : String
deriving DecidableEq, Repr, BEq

inductive SourceKind
  | caller
  | agent
  | reviewer
  | repository
  | document
deriving DecidableEq, Repr, BEq

structure Source where
  id : SourceId
  kind : SourceKind
  description : String
deriving DecidableEq, Repr, BEq

structure CallerDecision where
  source : Source
  reason : String
deriving DecidableEq, Repr, BEq

def CallerDecision.wellFormed (decision : CallerDecision) : Bool :=
  decision.source.kind == .caller &&
    !decision.source.id.value.isEmpty &&
    !decision.reason.isEmpty

structure DesignRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure WorkRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure TaskRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure ReviewRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure EvidenceRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure EvidenceResultRef where
  evidence : EvidenceRef
  observedValue : String
  passed : Bool
deriving DecidableEq, Repr, BEq

structure CommandProfileRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure KPTRef where
  key : String
  version : Nat
deriving DecidableEq, Repr, BEq

structure ReviewObservationRef where
  review : ReviewRef
  observation : String
deriving DecidableEq, Repr, BEq

end AgentWorkbench.Domain
