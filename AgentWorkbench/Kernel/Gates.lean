import AgentWorkbench.Kernel.Replay
import AgentWorkbench.Policy.Completion
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority

namespace AgentWorkbench.Kernel.Gates

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Request
  | validState
  | completion (target : WorkId)
  | designReady (design : DesignId)
  | traceReady (design : DesignId) (work : WorkId)
  | resumeReady (work : WorkId) (activation : ActivationId)
  | reviewReady (plan : ReviewPlanId)
  | evidenceExact (work : WorkId) (obligation : String)
  | correctionsReady (scope : String)
deriving DecidableEq, Repr

def validStateGate (store : Projection.Store) : GateResult :=
  let inspection := Projection.inspect store
  match inspection.currentState? with
  | some state =>
      if ValidState state then .pass else .blocked "state invariant violation"
  | none => .blocked s!"projection unavailable: {repr inspection.repairCommand?}"

def completionGate (target : WorkId) (store : Projection.Store) : GateResult :=
  let inspection := Projection.inspect store
  match inspection.currentState? with
  | some state =>
      if Policy.Completion.closeable target state.work state.activations
          state.claims state.adjudications state.reviewPlans
          state.reviewFindings state.findingVerifications state.lifecycle
          state.evidence state.obligations state.designs state.designApprovals
          state.decompositions state.corrections then
        .pass
      else
        .blocked "completion obligations remain"
  | none => .blocked s!"projection unavailable: {repr inspection.repairCommand?}"

private def inspectState (store : Projection.Store)
    (check : State → Bool) (blocked : String) : GateResult :=
  let inspection := Projection.inspect store
  match inspection.currentState? with
  | some state => if check state then .pass else .blocked blocked
  | none => .blocked s!"projection unavailable: {repr inspection.repairCommand?}"

def designReadyState (design : DesignId) (state : State) : Bool :=
  state.designs.any (·.id == design) &&
  state.designApprovals.any (·.design == design) &&
  !state.corrections.any (fun correction =>
    !correction.resolved &&
    (correction.design == some design ||
      (correction.design.isNone && correction.work.isNone)))

def traceReadyState (design : DesignId) (work : WorkId) (state : State) : Bool :=
  match state.decompositions.reverse.find? (·.work == work) with
  | none => false
  | some decomposition =>
      Replay.traceReadyFor design work decomposition.key
        decomposition.contentDigest state

def resumeReadyState (work : WorkId) (activation : ActivationId)
    (state : State) : Bool :=
  Work.workIsOpen state.work work &&
  Replay.workCorrectionsCurrent state work &&
  state.activations.any (fun current =>
    current.id == activation && current.work == work) &&
  Work.resumable state.activations activation &&
  Replay.resumeCurrent work activation state

def reviewReadyState (target : ReviewPlanId) (state : State) : Bool :=
  state.reviewPlans.any fun plan =>
    plan.id == target && Review.isLatestPlan plan state.reviewPlans &&
      Review.scopeReady plan state.claims
      state.adjudications state.reviewFindings state.findingVerifications

def evidenceExactState (work : WorkId) (key : String) (state : State) : Bool :=
  state.obligations.any fun obligation =>
    obligation.work == work && obligation.key == key && obligation.current &&
    state.evidence.any fun item =>
      Evidence.exactFor item obligation && Evidence.traceable item

def correctionsReadyState (scope : String) (state : State) : Bool :=
  !state.corrections.any fun correction =>
    !correction.resolved &&
      (correction.scope == scope || correction.scope == "global")

def run : Request → Projection.Store → GateResult
  | .validState, store => validStateGate store
  | .completion target, store => completionGate target store
  | .designReady design, store =>
      inspectState store (designReadyState design)
        "design is not independently reviewed, approved, and correction-current"
  | .traceReady design work, store =>
      inspectState store (traceReadyState design work)
        "reviewed decomposition does not cover every active requirement"
  | .resumeReady work activation, store =>
      inspectState store (resumeReadyState work activation)
        "suspended activation assumptions or corrections are not current"
  | .reviewReady plan, store =>
      inspectState store (reviewReadyState plan)
        "review lacks exact clean adjudication or verified finding closure"
  | .evidenceExact work obligation, store =>
      inspectState store (evidenceExactState work obligation)
        "evidence kind, scope, revision, provenance, or freshness does not match"
  | .correctionsReady scope, store =>
      inspectState store (correctionsReadyState scope)
        "an applicable durable user correction remains unresolved"

def observeGate (gate : Projection.Store → GateResult) (store : Projection.Store) :
    Projection.Store × GateResult :=
  (store, gate store)

theorem gate_is_read_only (gate : Projection.Store → GateResult)
    (store : Projection.Store) :
    (observeGate gate store).1 = store :=
  rfl

theorem all_gates_are_read_only (request : Request) (store : Projection.Store) :
    (observeGate (run request) store).1 = store :=
  rfl

end AgentWorkbench.Kernel.Gates
