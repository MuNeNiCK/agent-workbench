import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.ExternalOperation

open AgentWorkbench.Domain

inductive AttemptState
  | prepared
  | dispatched
  | uncertain
  | reconciled
  | observed
  | retryable
  | succeeded
  | failed
  | conflict
deriving DecidableEq, Repr, BEq

structure Attempt where
  operation : OperationId
  artifactDigest : String
  state : AttemptState
deriving DecidableEq, Repr

end AgentWorkbench.Domain.ExternalOperation
