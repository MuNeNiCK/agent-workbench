import AgentWorkbench.Kernel.Replay
import AgentWorkbench.Policy.Completion

namespace AgentWorkbench.Kernel.Gates

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

def validStateGate (state : State) : GateResult :=
  if ValidState state then .pass else .blocked "state invariant violation"

def completionGate (context : Policy.Completion.CompletionContext) : GateResult :=
  if Policy.Completion.closeable context then .pass else .blocked "completion obligations remain"

def observeGate (gate : State → GateResult) (state : State) : State × GateResult :=
  (state, gate state)

theorem gate_is_read_only (gate : State → GateResult) (state : State) :
    (observeGate gate state).1 = state :=
  rfl

end AgentWorkbench.Kernel.Gates
