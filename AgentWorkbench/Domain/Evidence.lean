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

end AgentWorkbench.Domain.Evidence
