import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Evidence

open AgentWorkbench.Domain

structure Obligation where
  work : WorkId
  key : String
  revision : Revision
  current : Bool
deriving DecidableEq, Repr

structure Evidence where
  id : EvidenceId
  obligation : String
  artifactDigest : String
  current : Bool
deriving DecidableEq, Repr

def obligationsCurrent (obligations : List Obligation) : Bool :=
  obligations.all (·.current)

def forWork (obligations : List Obligation) (work : WorkId) : List Obligation :=
  obligations.filter (·.work == work)

def UniqueObligations (obligations : List Obligation) : Prop :=
  (obligations.map fun obligation => (obligation.work, obligation.key)).Nodup

def UniqueEvidenceIds (evidence : List Evidence) : Prop :=
  (evidence.map (·.id)).Nodup

def EvidenceWellFormed (evidence : List Evidence) : Prop :=
  (evidence.all fun item =>
    !item.obligation.isEmpty && !item.artifactDigest.isEmpty) = true

def ObligationsWellFormed (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => !obligation.key.isEmpty) = true

def ObligationsReferenceWork (work : List WorkId) (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => work.contains obligation.work) = true

def EvidenceReferencesObligations (evidence : List Evidence)
    (obligations : List Obligation) : Prop :=
  (evidence.all fun item => obligations.any (·.key == item.obligation)) = true

def ObligationsCurrentAt (revision : Revision) (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => !obligation.current || obligation.revision == revision) = true

def invalidate (obligations : List Obligation) : List Obligation :=
  obligations.map fun obligation => { obligation with current := false }

end AgentWorkbench.Domain.Evidence
