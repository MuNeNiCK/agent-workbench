import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Projection
import AgentWorkbench.Domain.Validation.OutputScope

namespace AgentWorkbench

structure ProfileDefineRequest where
  entryId : String
  purpose : String
  taskEntryId : String
  inputTargets : List String := []
  outputScope : String
  criterionIds : List String := []
  taskVerificationIds : List String := []
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ProfileReplaceRequest where
  entryId : String
  profileEntryId : String
  purpose : String
  taskEntryId : String
  inputTargets : List String := []
  outputScope : String
  criterionIds : List String := []
  taskVerificationIds : List String := []
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def payload
    (purpose taskEntryId : String) (inputTargets : List String)
    (outputScope : String) (criterionIds taskVerificationIds : List String)
    (command : CommandSpec) : EntryPayload :=
  .commandProfile {
    purpose, taskEntryId := some taskEntryId, inputTargets := some inputTargets
    outputScope := some outputScope, criterionIds := some criterionIds
    taskVerificationIds := if taskVerificationIds.isEmpty then none else some taskVerificationIds
    target := some outputScope, command }

def defineProfile
    (state : ProjectState) (request : ProfileDefineRequest) : Except String ProjectState := do
  Validation.validateManagedOutputScope request.outputScope
  appendCurrentEntry state request.entryId (payload request.purpose request.taskEntryId
    request.inputTargets request.outputScope request.criterionIds request.taskVerificationIds request.command)

def replaceProfile
    (state : ProjectState) (request : ProfileReplaceRequest) : Except String ProjectState := do
  Validation.validateManagedOutputScope request.outputScope
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
    (payload request.purpose request.taskEntryId request.inputTargets request.outputScope
      request.criterionIds request.taskVerificationIds request.command) [prior.id]

end AgentWorkbench
