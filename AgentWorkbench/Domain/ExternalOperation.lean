import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.ExternalOperation

open AgentWorkbench.Domain

inductive AttemptState
  | prepared
  | dispatched
  | uncertain
  | retryable
  | succeeded
  | failed
  | conflict
deriving DecidableEq, Repr, BEq

inductive OperationKind
  | release
  | transport
  | publication
deriving DecidableEq, Repr, BEq

structure RemoteObservation where
  identity : String
  artifactDigest : Option String
deriving DecidableEq, Repr, BEq

structure RemotePrecondition where
  expectedArtifactDigest : Option String := none
deriving DecidableEq, Repr, BEq

inductive RemoteTarget
  | confirmed (identity : String)
  | unresolved
deriving DecidableEq, Repr, BEq

structure Attempt where
  operation : OperationId
  work : Option WorkId := none
  kind : OperationKind := .publication
  target : RemoteTarget
  artifactDigest : String
  remotePrecondition : RemotePrecondition := {}
  state : AttemptState
  observation : Option RemoteObservation := none
  disposition : Option String := none
deriving DecidableEq, Repr, BEq

def RemoteObservation.wellFormed (observation : RemoteObservation) : Bool :=
  !observation.identity.isEmpty &&
    observation.artifactDigest.all (fun digest => !digest.isEmpty)

def RemoteObservation.isAbsent (observation : RemoteObservation) : Bool :=
  observation.artifactDigest.isNone

def RemotePrecondition.wellFormed (precondition : RemotePrecondition) : Bool :=
  precondition.expectedArtifactDigest.all (fun digest => !digest.isEmpty)

def RemotePrecondition.satisfiedBy (precondition : RemotePrecondition)
    (observation : RemoteObservation) : Bool :=
  observation.wellFormed &&
    observation.artifactDigest == precondition.expectedArtifactDigest

def RemoteTarget.wellFormed : RemoteTarget → Bool
  | .confirmed identity => !identity.isEmpty
  | .unresolved => true

def RemoteTarget.dispatchIdentity? : RemoteTarget → Option String
  | .confirmed identity => some identity
  | .unresolved => none

def RemoteObservation.forTarget (target : String)
    (observation : RemoteObservation) : Bool :=
  observation.wellFormed && observation.identity == target

def RemoteObservation.matchesAttempt (attempt : Attempt)
    (observation : RemoteObservation) : Bool :=
  observation.wellFormed &&
    (match attempt.target with
    | .confirmed target => observation.identity == target
    | .unresolved => true) &&
    observation.artifactDigest == some attempt.artifactDigest

def RemoteObservation.conflictsWithAttempt (attempt : Attempt)
    (observation : RemoteObservation) : Bool :=
  observation.wellFormed &&
    (match attempt.target with
    | .confirmed target => observation.identity == target
    | .unresolved => true) &&
    (!attempt.remotePrecondition.satisfiedBy observation ||
      observation.artifactDigest.any (fun digest => digest != attempt.artifactDigest))

def Attempt.wellFormed (attempt : Attempt) : Bool :=
  !attempt.operation.value.isEmpty && attempt.target.wellFormed &&
    !attempt.artifactDigest.isEmpty && attempt.remotePrecondition.wellFormed &&
    (attempt.kind != .release || attempt.work.isSome) &&
    match attempt.state, attempt.observation, attempt.disposition with
    | .prepared, none, none
    | .dispatched, none, none
    | .uncertain, none, none => true
    | .retryable, some observation, none =>
        observation.wellFormed &&
          (match attempt.target with
          | .confirmed target => observation.identity == target
          | .unresolved => true) &&
          observation.isAbsent
    | .succeeded, some observation, none =>
        observation.matchesAttempt attempt
    | .failed, some observation, some disposition =>
        observation.wellFormed &&
          (match attempt.target with
          | .confirmed target => observation.identity == target
          | .unresolved => true) &&
          !disposition.isEmpty
    | .failed, none, some disposition =>
        (match attempt.target with
        | .unresolved => true
        | .confirmed _ => false) && !disposition.isEmpty
    | .conflict, some observation, none =>
        observation.conflictsWithAttempt attempt
    | _, _, _ => false

def sameIntent (current next : Attempt) : Bool :=
  current.operation == next.operation &&
    current.work == next.work && current.kind == next.kind &&
    current.target == next.target &&
    current.artifactDigest == next.artifactDigest &&
    current.remotePrecondition == next.remotePrecondition

def transitionAllowed (current next : Attempt) : Bool :=
  sameIntent current next && next.wellFormed &&
    match current.state, next.state with
    | .prepared, .dispatched => true
    | .dispatched, .uncertain
    | .dispatched, .succeeded
    | .dispatched, .failed => true
    | .dispatched, .retryable
    | .uncertain, .retryable
    | .dispatched, .conflict
    | .uncertain, .conflict
    | .uncertain, .succeeded => true
    | .retryable, .dispatched => true
    | .conflict, .failed =>
        next.observation == current.observation &&
          next.disposition.any (fun reason => !reason.isEmpty)
    | _, _ => false

def authorizedTransition (current next : Attempt) : Bool :=
  current.target.dispatchIdentity?.isSome &&
    next.target.dispatchIdentity?.isSome &&
    transitionAllowed current next

def requiresReconciliation (attempt : Attempt) : Bool :=
  attempt.state == .dispatched || attempt.state == .uncertain

def UniqueOperations (attempts : List Attempt) : Prop :=
  (attempts.map (·.operation)).Nodup

def AttemptsWellFormed (attempts : List Attempt) : Prop :=
  (attempts.all Attempt.wellFormed) = true

end AgentWorkbench.Domain.ExternalOperation
