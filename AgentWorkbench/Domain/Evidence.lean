import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Evidence

open AgentWorkbench.Domain

structure Obligation where
  work : WorkId
  key : String
  revision : Revision
  commandProfile : String
  invocation : String
  repository : String
  snapshot : String
  artifactDigest : String
  current : Bool
  kind : EvidenceKind := .test
  requirements : List String := []
  expectedProducer : String := ""
  expectedObservation : String := ""
  design : DesignId := ⟨0⟩
  designRevision : Revision := ⟨0⟩
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
  kind : EvidenceKind := .test
  requirements : List String := []
  producer : String := ""
  observedAt : String := ""
  design : DesignId := ⟨0⟩
  designRevision : Revision := ⟨0⟩
deriving DecidableEq, Repr

def provenanceExact (item : Evidence) (obligation : Obligation) : Bool :=
  item.work == obligation.work && item.obligation == obligation.key &&
  item.revision == obligation.revision &&
  item.exitCode == 0 && item.commandProfile == obligation.commandProfile &&
  item.invocation == obligation.invocation &&
  item.repository == obligation.repository && item.snapshot == obligation.snapshot &&
  item.artifactDigest == obligation.artifactDigest &&
  item.kind == obligation.kind &&
  item.requirements == obligation.requirements &&
  item.producer == obligation.expectedProducer &&
  item.observedAt == obligation.expectedObservation &&
  item.design == obligation.design &&
  item.designRevision == obligation.designRevision

def exactFor (item : Evidence) (obligation : Obligation) : Bool :=
  item.current && obligation.current && provenanceExact item obligation

def historicalExact (item : Evidence) (obligation : Obligation) : Bool :=
  item.current == obligation.current && provenanceExact item obligation

def traceable (item : Evidence) : Bool :=
  !item.requirements.isEmpty &&
  item.requirements.all (fun requirement => !requirement.isEmpty) &&
  !item.producer.isEmpty && !item.observedAt.isEmpty

def obligationsCurrent (obligations : List Obligation) : Bool :=
  obligations.all (·.current)

def forWork (obligations : List Obligation) (work : WorkId) : List Obligation :=
  obligations.filter fun obligation =>
    obligation.work == work && obligation.current

def UniqueObligations (obligations : List Obligation) : Prop :=
  (obligations.map fun obligation =>
    (obligation.work, obligation.key, obligation.revision)).Nodup

def UniqueEvidenceIds (evidence : List Evidence) : Prop :=
  (evidence.map (·.id)).Nodup

def EvidenceWellFormed (evidence : List Evidence) : Prop :=
  (evidence.all fun item =>
    !item.obligation.isEmpty && !item.commandProfile.isEmpty &&
      !item.invocation.isEmpty && !item.repository.isEmpty &&
      !item.snapshot.isEmpty && !item.artifactDigest.isEmpty &&
      traceable item) = true

def EvidenceCurrentAt (revision : Revision) (evidence : List Evidence) : Prop :=
  (evidence.all fun item => !item.current || item.revision == revision) = true

def ObligationsWellFormed (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation =>
    !obligation.key.isEmpty && !obligation.commandProfile.isEmpty &&
      !obligation.invocation.isEmpty && !obligation.repository.isEmpty &&
      !obligation.snapshot.isEmpty && !obligation.artifactDigest.isEmpty &&
      !obligation.requirements.isEmpty && !obligation.expectedProducer.isEmpty &&
      !obligation.expectedObservation.isEmpty) = true

def ObligationsReferenceWork (work : List WorkId) (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => work.contains obligation.work) = true

def CurrentObligationsReferenceOpenWork (openWork : List WorkId)
    (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation =>
    !obligation.current || openWork.contains obligation.work) = true

def EvidenceReferencesObligations (evidence : List Evidence)
    (obligations : List Obligation) : Prop :=
  (evidence.all fun item => obligations.any fun obligation =>
    historicalExact item obligation) = true

def invalidateEvidence (evidence : List Evidence) : List Evidence :=
  evidence.map fun item => { item with current := false }

def ObligationsCurrentAt (revision : Revision) (obligations : List Obligation) : Prop :=
  (obligations.all fun obligation => !obligation.current || obligation.revision == revision) = true

def invalidate (obligations : List Obligation) : List Obligation :=
  obligations.map fun obligation => { obligation with current := false }

end AgentWorkbench.Domain.Evidence
