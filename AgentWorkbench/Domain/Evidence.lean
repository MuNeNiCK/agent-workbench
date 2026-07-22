import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Evidence

open AgentWorkbench.Domain

structure Obligation where
  key : String
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

end AgentWorkbench.Domain.Evidence
