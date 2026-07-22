import AgentWorkbench.Kernel.Replay

namespace AgentWorkbench.Kernel.Projection

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive ProjectionPayload
  | decoded (state : State)
  | decodeFailed (fault : Domain.Projection.DecodeFault)
deriving DecidableEq, Repr, BEq

structure ProjectionObservation where
  fingerprint : Domain.Projection.ProjectionFingerprint
  reference : Domain.Projection.ProjectionRef
  payload : ProjectionPayload
deriving DecidableEq, Repr

structure StagedProjection where
  id : StageId
  binding : Domain.Projection.RepairBinding
  candidate : ProjectionObservation
deriving DecidableEq, Repr

structure RepairReceipt where
  stage : StageId
  before : Option Domain.Projection.ProjectionFingerprint
  adopted : Domain.Projection.ProjectionFingerprint
  head : Domain.Projection.LedgerPoint
deriving DecidableEq, Repr

structure Store where
  ledger : LedgerImage
  active : Option ProjectionObservation
  staged : List StagedProjection
  receipts : List RepairReceipt
  nextStage : StageId
deriving DecidableEq, Repr

inductive Inspection
  | ledgerCorrupt (fault : Domain.Projection.LedgerFault)
  | fresh (ledger : VerifiedLedger) (projection : ProjectionObservation)
  | missing (ledger : VerifiedLedger) (repair : Domain.Projection.RepairCommand)
  | stale (ledger : VerifiedLedger) (projection : ProjectionObservation)
      (repair : Domain.Projection.RepairCommand)
  | corrupt (ledger : VerifiedLedger) (projection : Option ProjectionObservation)
      (fault : Domain.Projection.ProjectionFault)
      (repair : Domain.Projection.RepairCommand)

def observedFingerprint (store : Store) :
    Option Domain.Projection.ProjectionFingerprint :=
  store.active.map (·.fingerprint)

def repairCommand (ledger : VerifiedLedger) (store : Store) :
    Domain.Projection.RepairCommand :=
  { binding := { head := ledger.point, observed := observedFingerprint store } }

def projectionMatchesHead (ledger : VerifiedLedger)
    (projection : ProjectionObservation) : Bool :=
  projection.reference.fingerprint == projection.fingerprint &&
  projection.reference.ledger == ledger.image.id &&
  projection.reference.revision == ledger.head.state.revision &&
  projection.reference.historyDigest == eventDigest ledger.image.events &&
  projection.reference.stateDigest == stateDigest ledger.head.state &&
  projection.fingerprint.rawDigest == stateDigest ledger.head.state &&
  projection.payload == .decoded ledger.head.state

def classifyProjection (ledger : VerifiedLedger) (store : Store) : Inspection :=
  let repair := repairCommand ledger store
  match store.active with
  | none => .missing ledger repair
  | some projection =>
      match projection.payload with
      | .decodeFailed fault => .corrupt ledger (some projection) (.undecodable fault) repair
      | .decoded state =>
          if projection.reference.ledger != ledger.image.id then
            .corrupt ledger (some projection)
              (.wrongLedger projection.reference.ledger ledger.image.id) repair
          else if ledger.head.state.revision.value < projection.reference.revision.value then
            .corrupt ledger (some projection)
              (.aheadOfLedger projection.reference.revision ledger.head.state.revision) repair
          else if projection.reference.fingerprint != projection.fingerprint then
            .corrupt ledger (some projection) .stateDigestMismatch repair
          else if projection.reference.revision = ledger.head.state.revision then
            if projectionMatchesHead ledger projection then
              .fresh ledger projection
            else
              .corrupt ledger (some projection) .replayMismatch repair
          else
            match replayAt ledger projection.reference.revision with
            | .error _ => .corrupt ledger (some projection) .replayMismatch repair
            | .ok prefixState =>
                if projection.reference.historyDigest ==
                    eventDigest (ledger.image.events.take projection.reference.revision.value) &&
                    projection.reference.stateDigest == stateDigest prefixState.state &&
                    projection.fingerprint.rawDigest == stateDigest prefixState.state &&
                    state == prefixState.state then
                  .stale ledger projection repair
                else
                  .corrupt ledger (some projection) .replayMismatch repair

def inspect (store : Store) : Inspection :=
  match verifyLedger store.ledger with
  | .error fault => .ledgerCorrupt fault
  | .ok ledger => classifyProjection ledger store

def Inspection.repairCommand? : Inspection → Option Domain.Projection.RepairCommand
  | .missing _ repair | .stale _ _ repair | .corrupt _ _ _ repair => some repair
  | .fresh _ _ | .ledgerCorrupt _ => none

def Inspection.currentState? : Inspection → Option State
  | .fresh _ projection =>
      match projection.payload with
      | .decoded state => some state
      | .decodeFailed _ => none
  | _ => none

def Inspection.ledgerPoint? : Inspection → Option Domain.Projection.LedgerPoint
  | .fresh ledger _ | .missing ledger _ | .stale ledger _ _ | .corrupt ledger _ _ _ =>
      some ledger.point
  | .ledgerCorrupt _ => none

def Inspection.describe : Inspection → String
  | .ledgerCorrupt fault => s!"ledger-corrupt {repr fault}"
  | .fresh ledger _ => s!"fresh {repr ledger.point}"
  | .missing ledger repair => s!"missing {repr ledger.point} repair={repr repair}"
  | .stale ledger projection repair =>
      s!"stale projected={repr projection.reference.revision} head={repr ledger.point} repair={repr repair}"
  | .corrupt ledger _ fault repair =>
      s!"projection-corrupt head={repr ledger.point} fault={repr fault} repair={repr repair}"

inductive RepairError
  | ledgerCorrupt (fault : Domain.Projection.LedgerFault)
  | commandMismatch
  | stageMissing (stage : StageId)
  | candidateMismatch
  | candidateNotVerified
deriving DecidableEq, Repr

def candidateObservation (ledger : VerifiedLedger) (stage : StageId) :
    ProjectionObservation :=
  let fingerprint : Domain.Projection.ProjectionFingerprint :=
    { id := ⟨s!"repair-{stage.value}"⟩, rawDigest := stateDigest ledger.head.state }
  { fingerprint
    reference := {
      fingerprint
      ledger := ledger.image.id
      revision := ledger.head.state.revision
      historyDigest := eventDigest ledger.image.events
      stateDigest := stateDigest ledger.head.state }
    payload := .decoded ledger.head.state }

structure StageTransaction where
  stage : StagedProjection
  result : Store

def stageRepair (command : Domain.Projection.RepairCommand) (store : Store) :
    Except RepairError StageTransaction :=
  match inspect store with
  | .ledgerCorrupt fault => .error (.ledgerCorrupt fault)
  | .fresh _ _ => .error .commandMismatch
  | .missing ledger expected | .stale ledger _ expected | .corrupt ledger _ _ expected =>
      if command = expected then
        let staged : StagedProjection := {
          id := store.nextStage
          binding := command.binding
          candidate := candidateObservation ledger store.nextStage }
        .ok {
          stage := staged
          result := { store with
            staged := store.staged ++ [staged]
            nextStage := ⟨store.nextStage.value + 1⟩ } }
      else
        .error .commandMismatch

structure VerifiedStage where
  stage : StagedProjection
  ledger : VerifiedLedger
  candidateState : State
  candidateExact : stage.candidate.payload = .decoded candidateState
  replayExact : candidateState = ledger.head.state
  candidateMatches : projectionMatchesHead ledger stage.candidate = true

def verifyStage (stageId : StageId) (store : Store) : Except RepairError VerifiedStage := do
  let stage ← match store.staged.find? (·.id == stageId) with
    | some stage => .ok stage
    | none => .error (.stageMissing stageId)
  let ledger ← match verifyLedger store.ledger with
    | .ok ledger => .ok ledger
    | .error fault => .error (.ledgerCorrupt fault)
  unless stage.binding.head = ledger.point &&
      stage.binding.observed = observedFingerprint store do
    throw .commandMismatch
  match candidateState : stage.candidate.payload with
  | .decodeFailed _ => .error .candidateMismatch
  | .decoded state =>
      if replayExact : state = ledger.head.state then
        if candidateMatches : projectionMatchesHead ledger stage.candidate then
          .ok ⟨stage, ledger, state, candidateState, replayExact, candidateMatches⟩
        else
          .error .candidateMismatch
      else
        .error .candidateMismatch

structure AdoptionTransaction where
  receipt : RepairReceipt
  candidate : ProjectionObservation
  sourceLedger : LedgerImage
  result : Store
  ledgerUnchanged : result.ledger = sourceLedger
  activeAdopted : result.active = some candidate

def adoptVerified (verified : VerifiedStage) (store : Store) :
    Except RepairError AdoptionTransaction := do
  let current ← match store.staged.find? (·.id == verified.stage.id) with
    | some stage => .ok stage
    | none => .error (.stageMissing verified.stage.id)
  unless current = verified.stage do throw .candidateMismatch
  let ledger ← match verifyLedger store.ledger with
    | .ok ledger => .ok ledger
    | .error fault => .error (.ledgerCorrupt fault)
  unless verified.stage.binding.head = ledger.point &&
      verified.stage.binding.observed = observedFingerprint store &&
      projectionMatchesHead ledger verified.stage.candidate do
    throw .commandMismatch
  let receipt : RepairReceipt := {
    stage := verified.stage.id
    before := observedFingerprint store
    adopted := verified.stage.candidate.fingerprint
    head := ledger.point }
  return {
    receipt
    candidate := verified.stage.candidate
    sourceLedger := store.ledger
    result := { store with
      active := some verified.stage.candidate
      staged := store.staged.filter (·.id != verified.stage.id)
      receipts := store.receipts ++ [receipt] }
    ledgerUnchanged := rfl
    activeAdopted := rfl }

structure RepairTransaction where
  staged : StageTransaction
  verified : VerifiedStage
  adopted : AdoptionTransaction

def repair (command : Domain.Projection.RepairCommand) (store : Store) :
    Except RepairError RepairTransaction := do
  let staged ← stageRepair command store
  let verified ← verifyStage staged.stage.id staged.result
  let adopted ← adoptVerified verified staged.result
  return { staged, verified, adopted }

def status (store : Store) : Store × Inspection :=
  (store, inspect store)

theorem status_is_read_only (store : Store) :
    (status store).1 = store :=
  rfl

theorem stage_preserves_ledger_and_active (command : Domain.Projection.RepairCommand)
    (store : Store) {transaction : StageTransaction}
    (accepted : stageRepair command store = .ok transaction) :
    transaction.result.ledger = store.ledger ∧
    transaction.result.active = store.active := by
  unfold stageRepair at accepted
  split at accepted <;> try contradiction
  all_goals
    split at accepted
    · cases accepted
      exact ⟨rfl, rfl⟩
    · contradiction

theorem verified_stage_matches_replay (verified : VerifiedStage) :
    verified.candidateState = verified.ledger.head.state ∧
    projectionMatchesHead verified.ledger verified.stage.candidate = true :=
  ⟨verified.replayExact, verified.candidateMatches⟩

theorem adoption_is_atomic (transaction : AdoptionTransaction) :
    transaction.result.ledger = transaction.sourceLedger ∧
    transaction.result.active = some transaction.candidate :=
  ⟨transaction.ledgerUnchanged, transaction.activeAdopted⟩

def initialLedger : LedgerImage :=
  { id := ⟨"agent-workbench"⟩
    initial := emptyState
    events := []
    storedHead := emptyState.revision
    storedHistoryDigest := eventDigest [] }

def initialProjection : ProjectionObservation :=
  let fingerprint : Domain.Projection.ProjectionFingerprint :=
    { id := ⟨"projection-0"⟩, rawDigest := stateDigest emptyState }
  { fingerprint
    reference := {
      fingerprint
      ledger := initialLedger.id
      revision := emptyState.revision
      historyDigest := eventDigest []
      stateDigest := stateDigest emptyState }
    payload := .decoded emptyState }

def initialStore : Store :=
  { ledger := initialLedger
    active := some initialProjection
    staged := []
    receipts := []
    nextStage := ⟨1⟩ }

end AgentWorkbench.Kernel.Projection
