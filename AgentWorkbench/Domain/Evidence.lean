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
  work : WorkId
  obligation : String
  revision : Revision
  commandProfile : String
  invocation : String
  exitCode : Int
  repository : String
  snapshot : String
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
    !item.obligation.isEmpty && !item.commandProfile.isEmpty &&
      !item.invocation.isEmpty && !item.repository.isEmpty &&
      !item.snapshot.isEmpty && !item.artifactDigest.isEmpty) = true

def EvidenceCurrentAt (revision : Revision) (evidence : List Evidence) : Prop :=
  (evidence.all fun item => !item.current || item.revision == revision) = true

def ObligationsWellFormed (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => !obligation.key.isEmpty) = true

def ObligationsReferenceWork (work : List WorkId) (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => work.contains obligation.work) = true

def CurrentObligationsReferenceOpenWork (openWork : List WorkId)
    (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation =>
    !obligation.current || openWork.contains obligation.work) = true

def EvidenceReferencesObligations (evidence : List Evidence)
    (obligations : List Obligation) : Prop :=
  (evidence.all fun item => obligations.any fun obligation =>
    obligation.work == item.work && obligation.key == item.obligation &&
      (!item.current ||
        (obligation.current && obligation.revision == item.revision))) = true

def invalidateEvidence (evidence : List Evidence) : List Evidence :=
  evidence.map fun item => { item with current := false }

def ObligationsCurrentAt (revision : Revision) (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => !obligation.current || obligation.revision == revision) = true

def invalidate (obligations : List Obligation) : List Obligation :=
  obligations.map fun obligation => { obligation with current := false }

end AgentWorkbench.Domain.Evidence
