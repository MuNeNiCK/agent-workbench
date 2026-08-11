import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Application.Current
import AgentWorkbench.Application.Mutation

namespace AgentWorkbench.CompletionPreflight

structure Identity where
  inputRevision : Nat
  inputDigest : String
  prospectiveStateRevision : Nat
  prospectiveStateDigest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Result where
  identity : Identity
  nextState : ProjectState
  deriving Repr, DecidableEq

/-- Construct and completely validate the exact state transition that `work complete` would
commit. This is pure: `ready` may execute it without changing persisted state, while the mutation
path commits the returned state without reconstructing the transition. -/
def prepare
    (state : ProjectState) (inputs : CurrentInputs) : Except String Result :=
  match completionInput state inputs.observations inputs.claimDigests with
  | .error message => .error message
  | .ok input =>
      let inputDigest := input.digest
      let prepared := PreparedMutation.workComplete
        inputs.observations inputs.claimDigests input inputDigest
      match prepared.executeApplicable state with
      | .error message => .error message
      | .ok next => .ok {
          identity := {
            inputRevision := state.revision
            inputDigest
            prospectiveStateRevision := next.revision
            prospectiveStateDigest := ContentDigest.string (Lean.toJson next).compress }
          nextState := next }

end AgentWorkbench.CompletionPreflight
