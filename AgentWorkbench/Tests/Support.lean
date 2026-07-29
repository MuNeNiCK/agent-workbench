import AgentWorkbench.Kernel.Decide

namespace AgentWorkbench.Tests

open AgentWorkbench
open AgentWorkbench.Domain

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do
    throw <| IO.userError message

def unwrap (result : Except String α) (message : String) : IO α :=
  match result with
  | .ok value => pure value
  | .error reason => throw <| IO.userError s!"{message}: {reason}"

def source (id : String) (kind : SourceKind := .caller) : Source :=
  { id := ⟨id⟩, kind, description := id }

def decision (id reason : String) : CallerDecision :=
  { source := source id, reason }

def initialState : Kernel.State :=
  let workRef : WorkRef := { key := "work", version := 0 }
  let taskRef : TaskRef := { key := "task", version := 0 }
  let work : Work.Unit :=
    { ref := workRef
      outcome := "deliver the selected change"
      completionBoundary :=
        [{ target := .taskSatisfied taskRef
           basis := .workBoundary workRef }]
      authority := decision "start" "Start the selected outcome." }
  let task : Work.Task :=
    { ref := taskRef
      work := workRef
      description := "implement the selected change"
      basis := .workBoundary workRef
      designScope := []
      phase := none
      state := .pending }
  { design := { effects := [] }
    work := [work]
    tasks := [task]
    phases := []
    evidenceSpecs := []
    evidenceResults := []
    formalSpecs := []
    formalResults := []
    reviewRequests := []
    reviewResults := []
    reviewDispositions := []
    focus := { work := workRef, task := some taskRef, returnPoint := none } }

end AgentWorkbench.Tests
