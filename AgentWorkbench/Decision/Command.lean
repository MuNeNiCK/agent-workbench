import AgentWorkbench.Decision.Completion

namespace AgentWorkbench

structure ResolvedCommand where
  profileEntryId : String
  taskEntryId : Option String
  inputTargets : List String
  outputScope : Option String
  criterionIds : List String
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
      pure {
        profileEntryId := entry.id
        taskEntryId := profile.taskEntryId
        inputTargets := profile.inputTargets.getD []
        outputScope := profile.outputScope
        criterionIds := profile.criterionIds.getD []
        target := profile.target
        command }
  | _ => none

def commandAuthorized
    (state : ProjectState) (digests : List CurrentClaimDigest)
    (resolved : ResolvedCommand) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection =>
      let plan := state.currentPlanFor? projection.work.id
      let taskReady := resolved.taskEntryId.any fun taskId =>
        projection.entries.any fun entry =>
          entry.id == taskId && match entry.payload, plan with
          | .task task, some currentPlan =>
              task.planId == some currentPlan.id && task.required && !task.closed && !task.retired &&
              task.dependencyLineageIds.all fun dependency =>
                projection.entries.any fun candidate => match candidate.payload with
                | .task predecessor => predecessor.lineageId == some dependency &&
                    predecessor.closed && !predecessor.retired
                | _ => false
          | _, _ => false
      taskReady && projection.design.leanClaims.all (claimHasReceipt projection digests)

end AgentWorkbench
