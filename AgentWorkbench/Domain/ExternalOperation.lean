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

structure Attempt where
  operation : OperationId
  work : Option WorkId := none
  kind : OperationKind := .publication
  artifactDigest : String
  state : AttemptState
  observation : Option RemoteObservation := none
  disposition : Option String := none
deriving DecidableEq, Repr, BEq

def RemoteObservation.wellFormed (observation : RemoteObservation) : Bool :=
  !observation.identity.isEmpty &&
    observation.artifactDigest.all (fun digest => !digest.isEmpty)

def RemoteObservation.isAbsent (observation : RemoteObservation) : Bool :=
  observation.artifactDigest.isNone

def RemoteObservation.matches (expected : String)
    (observation : RemoteObservation) : Bool :=
  observation.wellFormed && observation.artifactDigest == some expected

def RemoteObservation.conflicts (expected : String)
    (observation : RemoteObservation) : Bool :=
  observation.wellFormed &&
    observation.artifactDigest.any (fun digest => digest != expected)

def Attempt.wellFormed (attempt : Attempt) : Bool :=
  !attempt.operation.value.isEmpty && !attempt.artifactDigest.isEmpty &&
    (attempt.kind != .release || attempt.work.isSome) &&
    match attempt.state, attempt.observation, attempt.disposition with
    | .prepared, none, none
    | .dispatched, none, none
    | .uncertain, none, none => true
    | .retryable, some observation, none =>
        observation.wellFormed && observation.isAbsent
    | .succeeded, some observation, none =>
        observation.matches attempt.artifactDigest
    | .failed, none, none => true
    | .failed, some observation, some disposition =>
        observation.wellFormed && !disposition.isEmpty
    | .conflict, some observation, none =>
        observation.conflicts attempt.artifactDigest
    | _, _, _ => false

def sameIntent (current next : Attempt) : Bool :=
  current.operation == next.operation &&
    current.work == next.work && current.kind == next.kind &&
    current.artifactDigest == next.artifactDigest

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

def requiresReconciliation (attempt : Attempt) : Bool :=
  attempt.state == .dispatched || attempt.state == .uncertain

inductive PrivateArtifactClass
  | ledger
  | evidence
  | review
  | correction
  | backup
  | design
deriving DecidableEq, Repr, BEq

structure PrivateArtifact where
  kind : PrivateArtifactClass
  digest : String
deriving DecidableEq, Repr, BEq

structure ExportSelection where
  purpose : String
  artifacts : List PrivateArtifact
deriving DecidableEq, Repr

def PrivateArtifact.wellFormed (artifact : PrivateArtifact) : Bool :=
  !artifact.digest.isEmpty

def ExportSelection.wellFormed (selection : ExportSelection) : Bool :=
  !selection.purpose.isEmpty &&
    selection.artifacts.all PrivateArtifact.wellFormed &&
    selection.artifacts.eraseDups.length == selection.artifacts.length

def selectForExport (selection : ExportSelection)
    (available : List PrivateArtifact) : List PrivateArtifact :=
  if selection.wellFormed then
    available.filter selection.artifacts.contains
  else
    []

theorem empty_selection_exports_nothing (available : List PrivateArtifact) :
    selectForExport { purpose := "", artifacts := [] } available = [] := by
  simp [selectForExport, ExportSelection.wellFormed]

def UniqueOperations (attempts : List Attempt) : Prop :=
  (attempts.map (·.operation)).Nodup

def AttemptsWellFormed (attempts : List Attempt) : Prop :=
  (attempts.all Attempt.wellFormed) = true

end AgentWorkbench.Domain.ExternalOperation
