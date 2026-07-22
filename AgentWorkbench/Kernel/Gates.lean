import AgentWorkbench.Kernel.Projection
import AgentWorkbench.Policy.Completion

namespace AgentWorkbench.Kernel.Gates

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Request
  | validState
  | completion (target : WorkId)
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
          state.claims state.adjudications state.lifecycle then
        .pass
      else
        .blocked "completion obligations remain"
  | none => .blocked s!"projection unavailable: {repr inspection.repairCommand?}"

def run : Request → Projection.Store → GateResult
  | .validState, store => validStateGate store
  | .completion target, store => completionGate target store

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
