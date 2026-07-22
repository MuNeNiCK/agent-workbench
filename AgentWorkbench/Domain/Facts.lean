namespace AgentWorkbench.Domain

inductive WorkStatus
  | open
  | closed
  | blocked
  | abandoned
deriving DecidableEq, Repr, BEq

inductive ActivationStatus
  | active
  | suspended
  | closed
deriving DecidableEq, Repr, BEq

inductive ReviewClaim
  | clean
  | findings
deriving DecidableEq, Repr, BEq

inductive OwnerDecision
  | pending
  | accepted
  | rejected
deriving DecidableEq, Repr, BEq

inductive GateResult
  | pass
  | blocked (reason : String)
deriving DecidableEq, Repr, BEq

inductive DomainError
  | staleRevision
  | invalidTransition (reason : String)
  | invariantViolation (reason : String)
deriving DecidableEq, Repr, BEq

end AgentWorkbench.Domain
