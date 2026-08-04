import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure ProfileDefineRequest where
  entryId : String
  purpose : String
  target : Option String := none
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ProfileReplaceRequest where
  entryId : String
  profileEntryId : String
  purpose : String
  target : Option String := none
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def payload
    (purpose : String) (target : Option String) (command : CommandSpec) : EntryPayload :=
  .commandProfile { purpose, target, command }

def defineProfile
    (state : ProjectState) (request : ProfileDefineRequest) : Except String ProjectState :=
  appendCurrentEntry state request.entryId (payload request.purpose request.target request.command)

def replaceProfile
    (state : ProjectState) (request : ProfileReplaceRequest) : Except String ProjectState := do
  let (design, work) ← currentBinding state
  let prior ← match state.entry? request.profileEntryId with
    | some value => pure value
    | none => throw s!"no Command Profile {request.profileEntryId}"
  if entryIsSuperseded state prior then throw s!"Command Profile {request.profileEntryId} is not current"
  if prior.scope != work.scope || prior.workId != some work.id ||
      prior.designRevision != some design.id then
    throw s!"Command Profile {request.profileEntryId} is not currently applicable"
  match prior.payload with
  | .commandProfile _ => pure ()
  | _ => throw s!"entry {request.profileEntryId} is not a Command Profile"
  appendCurrentEntry state request.entryId
    (payload request.purpose request.target request.command) [prior.id]

end AgentWorkbench
