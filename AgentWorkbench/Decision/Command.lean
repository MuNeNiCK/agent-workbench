import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure ResolvedCommand where
  profileEntryId : String
  target : Option String
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

private def resolveWorkingDirectory
    (projectRoot : System.FilePath) (configured : Option String) : String :=
  match configured with
  | none => projectRoot.toString
  | some path =>
      let value : System.FilePath := path
      if value.isAbsolute then value.toString else (projectRoot / value).toString

def resolveCommandProfile?
    (projectRoot : System.FilePath) (state : ProjectState)
    (profileEntryId : String) : Option ResolvedCommand := do
  let projection ← currentProjection? state
  let entry ← projection.entries.find? (fun candidate => candidate.id == profileEntryId)
  match entry.payload with
  | .commandProfile profile =>
      let command := { profile.command with
        workingDirectory := some (resolveWorkingDirectory projectRoot profile.command.workingDirectory) }
      pure { profileEntryId := entry.id, target := profile.target, command }
  | _ => none

end AgentWorkbench
