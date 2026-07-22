import AgentWorkbench.Kernel.Decide
import AgentWorkbench.Kernel.Gates
import AgentWorkbench.Kernel.Resolver

namespace AgentWorkbench.Application.Service

open AgentWorkbench.Domain
open AgentWorkbench.Kernel

def initialState : Replay.State :=
  Replay.emptyState

def bootstrapCommand : Decide.Command :=
  .initializeWork ⟨0⟩
    { id := ⟨1⟩, status := .open }
    { id := ⟨1⟩, work := ⟨1⟩, status := .active, readyToResume := false }

def execute (command : Decide.Command) (state : Replay.State) :
    Except DomainError Decide.AcceptedTransaction :=
  Decide.decide command state

def complete (target : WorkId) (state : Replay.State) :
    Except DomainError Decide.CompletionTransaction :=
  Decide.closeWork target state

def queryValidity (state : Replay.State) : GateResult :=
  Gates.validStateGate state

def resolve (state : Replay.State) : Resolver.Resolution :=
  Resolver.next state

end AgentWorkbench.Application.Service
