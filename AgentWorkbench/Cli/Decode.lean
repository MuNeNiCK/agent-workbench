import AgentWorkbench.Application.Mutation
import AgentWorkbench.Application.Query
import AgentWorkbench.Cli.Protocol
import AgentWorkbench.Cli.Describe

namespace AgentWorkbench.Cli

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private partial def unknownJsonFields
    (path : String) (actual sample : Lean.Json) : List String :=
  match actual, sample with
  | .obj actualFields, .obj sampleFields =>
      actualFields.toList.flatMap fun (key, value) =>
        let fieldPath := if path.isEmpty then key else s!"{path}.{key}"
        match sampleFields.get? key with
        | none => [fieldPath]
        | some fieldSample => unknownJsonFields fieldPath value fieldSample
  | .arr actualItems, .arr sampleItems =>
      match sampleItems[0]? with
      | none => []
      | some itemSample =>
          actualItems.toList.zipIdx.flatMap fun (value, index) =>
            unknownJsonFields s!"{path}[{index}]" value itemSample
  | _, _ => []

private def rejectUnknownFields (operation : String) (json : Lean.Json) : IO Unit := do
  if (operationContract? operation).isNone then
    fail s!"missing native contract for {operation}"
  let some sample := operationInputSchema? operation
    | pure ()
  let unknown := unknownJsonFields "" json sample
  unless unknown.isEmpty do
    fail (s!"unknown fields for {operation}: {String.intercalate ", " unknown}; " ++
      s!"run `describe {operation}`")

private def readInput [Lean.FromJson α] (operation : String) : IO α := do
  let stdin ← IO.getStdin
  let source ← stdin.readToEnd
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error error => fail s!"invalid JSON input: {error}"
  rejectUnknownFields operation json
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error error =>
      fail s!"invalid input for {operation}: {error}; run `describe {operation}`"

def decodeMutation? (command : List String) : IO (Option Mutation) :=
  match Operation.parseCommand? command with
  | some .init => pure (some .init)
  | some .designPropose => return some (.designPropose (← readInput "design propose"))
  | some .designAmend => return some (.designAmend (← readInput "design amend"))
  | some .designAccept => do
      let input ← readInput (α := IdInput) "design accept"
      pure (some (.designAccept input.id))
  | some .designReject => return some (.designReject (← readInput "design reject"))
  | some .workStart => return some (.workStart (← readInput "work start"))
  | some .workFocus => do
      let input ← readInput (α := IdInput) "work focus"
      pure (some (.workFocus input.id))
  | some .workSuspend => do
      let input ← readInput (α := SuspendInput) "work suspend"
      pure (some (.workSuspend input.workId input.resumeCondition))
  | some .workResume => do
      pure (some (.workResume (← readInput "work resume")))
  | some .workHandoff => do
      let input ← readInput (α := HandoffInput) "work handoff"
      pure (some (.workHandoff input.workId input.entryId input.successorRun input.reason))
  | some .workAdoptDesign => do
      pure (some (.workAdoptDesign (← readInput "work adopt-design")))
  | some .workWithdraw => return some (.workWithdraw (← readInput "work withdraw"))
  | some .workComplete => pure (some .workComplete)
  | some .planPropose => return some (.planPropose (← readInput "plan propose"))
  | some .planReplace => return some (.planReplace (← readInput "plan replace"))
  | some .planMaterialize => do
      let input ← readInput (α := IdInput) "plan materialize"
      pure (some (.planMaterialize input.id))
  | some .taskClose => return some (.taskClose (← readInput "task close"))
  | some .profileDefine => return some (.profileDefine (← readInput "profile define"))
  | some .profileReplace => return some (.profileReplace (← readInput "profile replace"))
  | some .commandRun => return some (.commandRun (← readInput "command run"))
  | some .artifactObserve => return some (.artifactObserve (← readInput "artifact observe"))
  | some .proofRun => return some (.proofRun (← readInput "proof run"))
  | some .correctionRecord => return some (.correctionRecord (← readInput "correction record"))
  | some .correctionSupersede =>
      return some (.correctionSupersede (← readInput "correction supersede"))
  | some .correctionResolve => return some (.correctionResolve (← readInput "correction resolve"))
  | some .correctionIncorporate =>
      return some (.correctionIncorporate (← readInput "correction incorporate"))
  | some .kptRecord => return some (.kptRecord (← readInput "kpt record"))
  | some .kptApply => return some (.kptApply (← readInput "kpt apply"))
  | some .reviewStart => return some (.reviewStart (← readInput "review start"))
  | some .reviewResume => return some (.reviewResume (← readInput "review resume"))
  | some .reviewHandoff => return some (.reviewHandoff (← readInput "review handoff"))
  | some .reviewFinding => return some (.reviewFinding (← readInput "review finding"))
  | some .reviewDisposition =>
      return some (.reviewDisposition (← readInput "review disposition"))
  | some .reviewConclude => return some (.reviewConclude (← readInput "review conclude"))
  | some .reviewVerify => return some (.reviewVerify (← readInput "review verify"))
  | some .describe | some .designGet | some .designInspectSources | some .designSource
  | some .designDiff | some .designExport | some .workGet | some .workAdoptionImpact
  | some .planGet | some .planInspectSources | some .planSource | some .planDiff
  | some .planExport | some .reviewContext | some .reviewInspect | some .entryGet
  | some .history | some .context | some .ready | some .commandShow | some .proofDigest
  | none => pure none

def decodeQuery? (command : List String) : IO (Option Query) :=
  match command with
  | ["describe"] => pure (some (.describe none))
  | "describe" :: operationParts =>
      pure (some (.describe (some (String.intercalate " " operationParts))))
  | _ => match Operation.parseCommand? command with
  | some .designInspectSources => do
      let input ← readInput (operation := "design inspect-sources") ( α := DesignSourceInspectionInput)
      pure (some (.designInspectSources input.sourceDocumentTargets))
  | some .designGet => do
      let input ← readInput (operation := "design get") ( α := IdInput)
      pure (some (.designGet input.id))
  | some .designSource => do
      let input ← readInput (operation := "design source") ( α := DesignSourceInput)
      pure (some (.designSource input.designId input.target))
  | some .designDiff => do
      let input ← readInput (operation := "design diff") ( α := DesignDiffInput)
      pure (some (.designDiff input.beforeDesignId input.afterDesignId))
  | some .designExport => do
      let input ← readInput (operation := "design export") ( α := IdInput)
      pure (some (.designExport input.id))
  | some .planInspectSources => do
      let input ← readInput (operation := "plan inspect-sources") ( α := PlanSourceInspectionInput)
      pure (some (.planInspectSources input.workId input.sourceDocumentTargets))
  | some .planGet => do
      let input ← readInput (operation := "plan get") ( α := IdInput)
      pure (some (.planGet input.id))
  | some .planSource => do
      let input ← readInput (operation := "plan source") ( α := PlanSourceInput)
      pure (some (.planSource input.planId input.target))
  | some .planDiff => do
      let input ← readInput (operation := "plan diff") ( α := PlanDiffInput)
      pure (some (.planDiff input.beforePlanId input.afterPlanId))
  | some .planExport => do
      let input ← readInput (operation := "plan export") ( α := IdInput)
      pure (some (.planExport input.id))
  | some .workGet => do
      let input ← readInput (operation := "work get") ( α := IdInput)
      pure (some (.workGet input.id))
  | some .workAdoptionImpact => do
      let input ← readInput (operation := "work adoption-impact") ( α := IdInput)
      pure (some (.workAdoptionImpact input.id))
  | some .entryGet => do
      let input ← readInput (operation := "entry get") ( α := IdInput)
      pure (some (.entryGet input.id))
  | some .history => do
      let input ← readInput (operation := "history") ( α := HistoryInput)
      pure (some (.history input.afterOrder input.limit))
  | some .reviewContext => do
      let input ← readInput (operation := "review context") ( α := IdInput)
      pure (some (.reviewContext input.id))
  | some .reviewInspect => do
      let input ← readInput (operation := "review inspect") ( α := IdInput)
      pure (some (.reviewInspect input.id))
  | some .commandShow => do
      let input ← readInput (operation := "command show") ( α := IdInput)
      pure (some (.commandShow input.id))
  | some .proofDigest => do
      let input ← readInput (operation := "proof digest") ( α := IdInput)
      pure (some (.proofDigest input.id))
  | some .context => pure (some .context)
  | some .ready => pure (some .ready)
  | some .describe => pure (some (.describe none))
  | some .init | some .designPropose | some .designAmend | some .designAccept
  | some .designReject | some .workStart | some .workFocus | some .workSuspend
  | some .workResume | some .workHandoff | some .workAdoptDesign | some .workWithdraw
  | some .workComplete | some .planPropose | some .planReplace | some .planMaterialize
  | some .taskClose | some .profileDefine | some .profileReplace | some .artifactObserve
  | some .correctionRecord | some .correctionSupersede | some .correctionResolve
  | some .correctionIncorporate | some .kptRecord | some .kptApply | some .reviewStart
  | some .reviewResume | some .reviewHandoff | some .reviewFinding
  | some .reviewDisposition | some .reviewConclude | some .reviewVerify
  | some .commandRun | some .proofRun
  | none => pure none

end AgentWorkbench.Cli
