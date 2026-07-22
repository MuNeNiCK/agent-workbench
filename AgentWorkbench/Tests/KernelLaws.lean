import AgentWorkbench.Application.Service

open AgentWorkbench
open AgentWorkbench.Domain

def firstActivation : Domain.Work.Activation :=
  { id := ⟨1⟩, work := ⟨1⟩, status := .active, readyToResume := false }

def secondActivation : Domain.Work.Activation :=
  { id := ⟨2⟩, work := ⟨2⟩, status := .active, readyToResume := false }

def firstWork : Domain.Work.WorkUnit :=
  { id := ⟨1⟩, status := .open }

def secondWork : Domain.Work.WorkUnit :=
  { id := ⟨2⟩, status := .open }

def replaceWorkAndActivations (state : Kernel.Replay.State)
    (work : List Domain.Work.WorkUnit)
    (activations : List Domain.Work.Activation) : Kernel.Decide.Command :=
  { expectedRevision := state.revision
    events := [.replaceWork work, .replaceActivations activations]
    eventsNonempty := by simp }

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw <| IO.userError message

def main : IO Unit := do
  let initial := Kernel.Replay.emptyState
  expect (Application.Service.queryValidity initial == .pass) "empty state must be valid"
  let first ← match Application.Service.execute
      (replaceWorkAndActivations initial [firstWork] [firstActivation]) initial with
    | .ok transaction => pure transaction.result.state
    | .error error => throw <| IO.userError s!"first activation rejected: {repr error}"
  expect (first.revision == ⟨2⟩) "each accepted event must advance the revision"
  let invalid := replaceWorkAndActivations first [firstWork, secondWork]
    [firstActivation, secondActivation]
  match Application.Service.execute invalid first with
  | .error (.invariantViolation _) => pure ()
  | .error error => throw <| IO.userError s!"wrong rejection: {repr error}"
  | .ok _ => throw <| IO.userError "two active activations must be rejected"
  expect (Kernel.Decide.committedState
    (Application.Service.execute invalid first)
    first == first) "rejection must leave the state unchanged"
  IO.println "kernel laws: pass"
