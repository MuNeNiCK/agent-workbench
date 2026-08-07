namespace AgentWorkbenchProof

inductive InvariantFamily where
  | designHistory
  | workLifecycle
  | planTask
  | ledgerAuthority
  deriving Repr, DecidableEq

structure FieldCoverage where
  field : String
  owner : InvariantFamily
  deriving Repr, DecidableEq

end AgentWorkbenchProof
