import AgentWorkbench.Kernel.Decide
import AgentWorkbench.Kernel.Gates
import AgentWorkbench.Kernel.Resolver

namespace AgentWorkbench.Application.Service

open AgentWorkbench.Domain
open AgentWorkbench.Kernel

def initialState : Replay.State :=
  Replay.emptyState

def execute (command : Decide.Command) (state : Replay.State) :
    Except DomainError Decide.AcceptedTransaction :=
  Decide.decide command state

def queryValidity (state : Replay.State) : GateResult :=
  Gates.validStateGate state

def resolve (state : Replay.State) : Option Resolver.Action :=
  Resolver.next state

end AgentWorkbench.Application.Service
