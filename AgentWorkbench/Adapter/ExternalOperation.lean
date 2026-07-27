import AgentWorkbench.Adapter.SQLite

namespace AgentWorkbench.Adapter.ExternalOperation

open AgentWorkbench.Domain

inductive DispatchOutcome
  | observed (observation : Domain.ExternalOperation.RemoteObservation)
  | responseLost
  | rejected (observation : Domain.ExternalOperation.RemoteObservation) (reason : String)
deriving DecidableEq, Repr

structure Port where
  observe : String → IO Domain.ExternalOperation.RemoteObservation
  dispatch :
    OperationId →
    String →
    String →
    Domain.ExternalOperation.RemotePrecondition →
    IO DispatchOutcome

inductive BoundaryError
  | storageInvalid
  | attemptMissing
  | invalidAttempt
  | wrongTargetObservation
deriving DecidableEq, Repr

private def classifyObservation
    (attempt : Domain.ExternalOperation.Attempt)
    (target : String)
    (observation : Domain.ExternalOperation.RemoteObservation) :
    Except BoundaryError Domain.ExternalOperation.Attempt :=
  if !observation.forTarget target then
    .error .wrongTargetObservation
  else if observation.matchesAttempt attempt then
    .ok { attempt with state := .succeeded, observation := some observation }
  else if observation.isAbsent then
    .ok { attempt with state := .retryable, observation := some observation }
  else
    .ok { attempt with state := .conflict, observation := some observation }

private def dispatchCurrent (port : Port)
    (attempt : Domain.ExternalOperation.Attempt) :
    IO (Except BoundaryError Domain.ExternalOperation.Attempt) := do
  let target ← match attempt.target.dispatchIdentity? with
    | some target => pure target
    | none => return .error .invalidAttempt
  unless attempt.state == .dispatched && attempt.wellFormed do
    return .error .invalidAttempt
  try
    let before ← port.observe target
    unless before.forTarget target do
      return .error .wrongTargetObservation
    if !attempt.remotePrecondition.satisfiedBy before then
      return .ok { attempt with state := .conflict, observation := some before }
    match ← port.dispatch attempt.operation target attempt.artifactDigest
        attempt.remotePrecondition with
    | .observed observation =>
        return classifyObservation attempt target observation
    | .responseLost =>
        return .ok { attempt with state := .uncertain }
    | .rejected observation reason =>
        if observation.forTarget target && !reason.isEmpty then
          return .ok {
            attempt with
            state := .failed
            observation := some observation
            disposition := some reason }
        return .error .wrongTargetObservation
  catch _ =>
    -- Once a dispatched attempt crosses the port boundary, an exception cannot
    -- prove whether the remote side effect happened.
    return .ok { attempt with state := .uncertain }

private def reconcileCurrent (port : Port)
    (attempt : Domain.ExternalOperation.Attempt) :
    IO (Except BoundaryError Domain.ExternalOperation.Attempt) := do
  let target ← match attempt.target.dispatchIdentity? with
    | some target => pure target
    | none => return .error .invalidAttempt
  unless Domain.ExternalOperation.requiresReconciliation attempt &&
      attempt.wellFormed do
    return .error .invalidAttempt
  try
    return classifyObservation attempt target (← port.observe target)
  catch _ =>
    -- A failed observation changes no knowledge and therefore cannot authorize
    -- retry, success, failure, or conflict.
    return .ok { attempt with state := .uncertain }

private def currentAttempt (path : System.FilePath) (operation : OperationId) :
    IO (Except BoundaryError Domain.ExternalOperation.Attempt) := do
  let store ← match ← SQLite.inspect path with
    | .ok store => pure store
    | .error _ => return .error .storageInvalid
  let state ← match (Application.Service.status store).value.currentState? with
    | some state => pure state
    | none => return .error .storageInvalid
  match state.externalOperations.find? (·.operation == operation) with
  | some attempt => return .ok attempt
  | none => return .error .attemptMissing

def dispatch (path : System.FilePath) (port : Port) (operation : OperationId) :
    IO (Except BoundaryError Domain.ExternalOperation.Attempt) :=
  SQLite.withWriterLock path do
    match ← currentAttempt path operation with
    | .ok attempt => dispatchCurrent port attempt
    | .error error => return .error error

def reconcile (path : System.FilePath) (port : Port) (operation : OperationId) :
    IO (Except BoundaryError Domain.ExternalOperation.Attempt) :=
  SQLite.withWriterLock path do
    match ← currentAttempt path operation with
    | .ok attempt => reconcileCurrent port attempt
    | .error error => return .error error

end AgentWorkbench.Adapter.ExternalOperation
