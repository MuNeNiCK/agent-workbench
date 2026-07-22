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

def UniqueOperations (attempts : List Attempt) : Prop :=
  (attempts.map (·.operation)).Nodup

def AttemptsWellFormed (attempts : List Attempt) : Prop :=
  (attempts.all fun attempt =>
    !attempt.operation.value.isEmpty && !attempt.artifactDigest.isEmpty) = true

end AgentWorkbench.Domain.ExternalOperation
