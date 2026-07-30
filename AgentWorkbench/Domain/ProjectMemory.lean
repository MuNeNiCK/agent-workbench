import AgentWorkbench.Domain.Work

namespace AgentWorkbench.Domain

inductive MemoryScope
  | project
  | work (key : String)
deriving DecidableEq, Repr, BEq

def MemoryScope.wellFormed : MemoryScope → Bool
  | .project => true
  | .work key => !key.isEmpty

namespace CommandProfile

inductive Disposition
  | required
  | recommended
  | discouraged
deriving DecidableEq, Repr, BEq

inductive Authority
  | proposed
  | acceptedByCaller (decision : CallerDecision)
deriving DecidableEq, Repr, BEq

structure Profile where
  ref : CommandProfileRef
  predecessor : Option CommandProfileRef
  purpose : String
  scope : MemoryScope
  argv : List String
  cwd : Option String
  disposition : Disposition
  source : Source
  authority : Authority
deriving DecidableEq, Repr, BEq

private def relativePath (path : String) : Bool :=
  !path.isEmpty &&
    !path.startsWith "/" &&
    path != ".." &&
    !path.startsWith "../" &&
    !path.contains "/../"

def Profile.wellFormed (profile : Profile) : Bool :=
  !profile.ref.key.isEmpty &&
    (profile.predecessor.all fun prior =>
      prior.key == profile.ref.key && prior.version < profile.ref.version) &&
    !profile.purpose.isEmpty &&
    profile.scope.wellFormed &&
    !profile.argv.isEmpty &&
    profile.cwd.all relativePath &&
    !profile.source.id.value.isEmpty &&
    match profile.authority with
    | .proposed => profile.source.kind != .caller
    | .acceptedByCaller decision => decision.wellFormed

structure Deviation where
  profile : CommandProfileRef
  evidence : Option EvidenceRef
  actualArgv : List String
  actualCwd : Option String
  reason : String
  source : Source
deriving DecidableEq, Repr, BEq

def Deviation.wellFormed (deviation : Deviation) : Bool :=
  !deviation.profile.key.isEmpty &&
    !deviation.actualArgv.isEmpty &&
    deviation.actualCwd.all relativePath &&
    !deviation.reason.isEmpty &&
    !deviation.source.id.value.isEmpty &&
    deviation.source.kind != .caller

end CommandProfile

namespace KPT

inductive Category
  | keep
  | problem
  | try
deriving DecidableEq, Repr, BEq

inductive Authority
  | nonAuthoritative
  | callerOwned (decision : CallerDecision)
deriving DecidableEq, Repr, BEq

inductive ProfileScopeSelector
  | project
  | focusedWork
deriving DecidableEq, Repr, BEq

inductive DesignAuthoritySelector
  | accepted
  | candidate
deriving DecidableEq, Repr, BEq

inductive EvidenceBasisSelector
  | focusedWork
  | design (key : String)
deriving DecidableEq, Repr, BEq

inductive RelationSelector
  | commandProfile (key : String) (scope : ProfileScopeSelector)
  | design (key : String) (authority : DesignAuthoritySelector)
  | task (description : String)
  | reviewObservation (review observation : String)
  | evidenceResult (key : String) (basis : EvidenceBasisSelector)
      (observedValue : String) (passed : Bool)
deriving DecidableEq, Repr, BEq

inductive Relation
  | commandProfile (ref : CommandProfileRef)
  | design (ref : DesignRef)
  | task (ref : TaskRef)
  | reviewObservation (ref : ReviewObservationRef)
  | evidenceResult (ref : EvidenceResultRef)
deriving DecidableEq, Repr, BEq

def Relation.wellFormed : Relation → Bool
  | .commandProfile ref => !ref.key.isEmpty
  | .design ref => !ref.key.isEmpty
  | .task ref => !ref.key.isEmpty
  | .reviewObservation ref =>
      !ref.review.key.isEmpty && !ref.observation.isEmpty
  | .evidenceResult ref =>
      !ref.evidence.key.isEmpty && !ref.observedValue.isEmpty

structure Entry where
  ref : KPTRef
  predecessor : Option KPTRef
  category : Category
  scope : MemoryScope
  statement : String
  source : Source
  author : String
  relation : Option Relation
  authority : Authority
deriving DecidableEq, Repr, BEq

def Entry.wellFormed (entry : Entry) : Bool :=
  !entry.ref.key.isEmpty &&
    (entry.predecessor.all fun prior =>
      prior.key == entry.ref.key && prior.version < entry.ref.version) &&
    entry.scope.wellFormed &&
    !entry.statement.isEmpty &&
    !entry.source.id.value.isEmpty &&
    !entry.author.isEmpty &&
    entry.relation.all Relation.wellFormed &&
    match entry.authority with
    | .nonAuthoritative => entry.source.kind != .caller
    | .callerOwned decision => decision.wellFormed

end KPT

end AgentWorkbench.Domain
