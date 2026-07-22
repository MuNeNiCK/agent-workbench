import AgentWorkbench.Kernel.Decide
import AgentWorkbench.Kernel.Gates
import AgentWorkbench.Kernel.Resolver

namespace AgentWorkbench.Application.Service

open AgentWorkbench.Domain
open AgentWorkbench.Kernel

def initialStore : Projection.Store :=
  Projection.initialStore

def bootstrapCommand : Decide.Command :=
  .initializeWork ⟨0⟩
    { id := ⟨1⟩, status := .open }
    { id := ⟨1⟩, work := ⟨1⟩, status := .active, readyToResume := false }

structure QueryResult (α : Type) where
  store : Projection.Store
  value : α

structure MutationTransaction where
  accepted : Decide.AcceptedTransaction
  result : Projection.Store

structure CompletionTransaction where
  accepted : Decide.CompletionTransaction
  result : Projection.Store

def projectionFor (ledger : Replay.LedgerImage) (state : Replay.State) :
    Projection.ProjectionObservation :=
  let fingerprint : Domain.Projection.ProjectionFingerprint := {
    id := ⟨s!"projection-{state.revision.value}"⟩
    rawDigest := Replay.stateDigest state }
  { fingerprint
    reference := {
      fingerprint
      ledger := ledger.id
      revision := state.revision
      historyDigest := ledger.storedHistoryDigest
      stateDigest := Replay.stateDigest state }
    payload := .decoded state }

def commitAccepted (store : Projection.Store) (accepted : Decide.AcceptedTransaction) :
    Projection.Store :=
  let events := store.ledger.events ++ accepted.events
  let ledger := {
    store.ledger with
    events
    storedHead := accepted.result.state.revision
    storedHistoryDigest := Replay.eventDigest events }
  { store with
    ledger
    active := some (projectionFor ledger accepted.result.state)
    staged := [] }

def execute (command : Decide.Command) (store : Projection.Store) :
    Except DomainError MutationTransaction :=
  let inspection := Projection.inspect store
  match inspection.currentState? with
  | none => .error (.invalidTransition "projection repair required before mutation")
  | some state => do
      let accepted ← Decide.decide command state
      let result := commitAccepted store accepted
      match Projection.inspect result with
      | .fresh _ _ => .ok { accepted, result }
      | _ => .error (.invariantViolation "atomic ledger/projection commit is not fresh")

def complete (target : WorkId) (store : Projection.Store) :
    Except DomainError CompletionTransaction :=
  let inspection := Projection.inspect store
  match inspection.currentState? with
  | none => .error (.invalidTransition "projection repair required before completion")
  | some state => do
      let accepted ← Decide.closeWork target state
      let result := commitAccepted store accepted.toAcceptedTransaction
      match Projection.inspect result with
      | .fresh _ _ => .ok { accepted, result }
      | _ => .error (.invariantViolation "atomic ledger/projection commit is not fresh")

def status (store : Projection.Store) : QueryResult Projection.Inspection :=
  { store, value := Projection.inspect store }

def queryValidity (store : Projection.Store) : QueryResult GateResult :=
  { store, value := Gates.validStateGate store }

def queryGate (request : Gates.Request) (store : Projection.Store) :
    QueryResult GateResult :=
  { store, value := Gates.run request store }

def resolve (store : Projection.Store) : QueryResult Resolver.Resolution :=
  { store, value := Resolver.next (Projection.inspect store) }

def repairProjection (command : Domain.Projection.RepairCommand)
    (store : Projection.Store) : Except Projection.RepairError Projection.RepairTransaction :=
  Projection.repair command store

def executeRecovery (action : Resolver.Action) (store : Projection.Store) :
    Except Projection.RepairError Projection.RepairTransaction :=
  match action with
  | .repairProjection command => repairProjection command store
  | _ => .error .commandMismatch

structure Response where
  store : Projection.Store
  output : String

def executeAction (action : Resolver.Action) (store : Projection.Store) :
    Except String Response :=
  if action.executable (Projection.inspect store) then
    match action with
    | .repairProjection command =>
        match repairProjection command store with
        | .error error => .error s!"{repr error}"
        | .ok transaction =>
            .ok {
              store := transaction.adopted.result
              output := s!"{repr transaction.adopted.receipt}" }
    | .initializeWork _ =>
        match execute bootstrapCommand store with
        | .error error => .error s!"{repr error}"
        | .ok transaction =>
            .ok { store := transaction.result, output := s!"{repr action}" }
    | .continueActiveWork _ _ _ =>
        .ok { store, output := s!"{repr action}" }
    | .resumeSuspendedWork point work activation =>
        match execute (.resumeWork point.revision work activation) store with
        | .error error => .error s!"{repr error}"
        | .ok transaction =>
            .ok { store := transaction.result, output := s!"{repr action}" }
  else
    .error "resolver action is stale or does not match the authoritative store"

inductive Request
  | status
  | next
  | gate (request : Gates.Request)
  | repairProjection (command : Domain.Projection.RepairCommand)
  | action (action : Resolver.Action)

def executeRequest (request : Request) (store : Projection.Store) :
    Except String Response :=
  match request with
  | .status =>
      let result := status store
      .ok { store := result.store, output := result.value.describe }
  | .next =>
      let result := resolve store
      .ok { store := result.store, output := s!"{repr result.value}" }
  | .gate request =>
      let result := queryGate request store
      .ok { store := result.store, output := s!"{repr result.value}" }
  | .repairProjection command =>
      match repairProjection command store with
      | .error error => .error s!"{repr error}"
      | .ok transaction =>
          .ok {
            store := transaction.adopted.result
            output := s!"{repr transaction.adopted.receipt}" }
  | .action action => executeAction action store

theorem status_is_read_only (store : Projection.Store) :
    (status store).store = store :=
  rfl

theorem next_is_read_only (store : Projection.Store) :
    (resolve store).store = store :=
  rfl

theorem every_gate_is_read_only (request : Gates.Request) (store : Projection.Store) :
    (queryGate request store).store = store :=
  rfl

end AgentWorkbench.Application.Service
