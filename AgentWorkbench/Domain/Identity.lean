namespace AgentWorkbench.Domain

structure WorkId where
  value : Nat
deriving DecidableEq, Repr, BEq

structure ActivationId where
  value : Nat
deriving DecidableEq, Repr, BEq

structure DesignId where
  value : Nat
deriving DecidableEq, Repr, BEq

structure ReviewId where
  value : Nat
deriving DecidableEq, Repr, BEq

structure EvidenceId where
  value : Nat
deriving DecidableEq, Repr, BEq

structure OperationId where
  value : String
deriving DecidableEq, Repr, BEq

structure LedgerId where
  value : String
deriving DecidableEq, Repr, BEq

structure ProjectionId where
  value : String
deriving DecidableEq, Repr, BEq

structure Digest where
  value : String
deriving DecidableEq, Repr, BEq

structure StageId where
  value : Nat
deriving DecidableEq, Repr, BEq

structure Revision where
  value : Nat
deriving DecidableEq, Repr, BEq, Ord

def Revision.next (revision : Revision) : Revision :=
  ⟨revision.value + 1⟩

end AgentWorkbench.Domain
