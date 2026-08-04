import AgentWorkbench.Adapter.Store
import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Ledger
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Current
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Profile
import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Guidance
import AgentWorkbench.Application.Review
import AgentWorkbench.Adapter.Runtime
import AgentWorkbench.Adapter.OperationLock
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
  let some contract := operationContract? operation
    | fail s!"missing native contract for {operation}"
  let some sample := contract.inputExample
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

private def writeJson [Lean.ToJson α] (value : α) : IO Unit :=
  IO.println (Lean.toJson value).compress

private structure Invocation where
  projectRoot : System.FilePath
  command : List String

private def parseInvocation (arguments : List String) : IO Invocation := do
  match arguments with
  | "--project" :: root :: command =>
      if command.isEmpty then fail "missing command after --project"
      pure { projectRoot := ← IO.FS.realPath root, command }
  | command =>
      if command.isEmpty then fail "missing command"
      pure { projectRoot := ← IO.currentDir, command }

private def databasePath (root : System.FilePath) : System.FilePath :=
  root / ".agent-workbench" / "state.db"

private def openStore (root : System.FilePath) : IO Store.Store := do
  IO.FS.createDirAll (root / ".agent-workbench")
  Store.open (databasePath root)

private def loadDescribeState (root : System.FilePath) : IO ProjectState := do
  if ← (databasePath root).pathExists then
    Store.loadState (← Store.open (databasePath root))
  else
    pure .empty

private def emitState (state : ProjectState) : IO Unit :=
  writeJson (StateResult.ofState state)

private def emitCurrentState
    (projectRoot : System.FilePath) (state : ProjectState) : IO Unit := do
  let inputs ← evaluateCurrentInputs projectRoot state
  writeJson (ContextResult.mk state.revision
    (currentContext? state inputs.observations inputs.claimDigests))

private def runStateCommandUnlocked (invocation : Invocation) : IO Unit := do
  let store ← openStore invocation.projectRoot
  match invocation.command with
  | ["init"] =>
      Runtime.initializeProject invocation.projectRoot
      emitCurrentState invocation.projectRoot (← Store.loadState store)
  | ["design", "propose"] =>
      let input ← readInput "design propose" ( α := DesignProposalRequest)
      writeJson (← Store.proposeDesignRequest invocation.projectRoot store input)
  | ["design", "accept"] =>
      let input ← readInput "design accept" ( α := IdInput)
      emitState (← Store.acceptDesignRequest invocation.projectRoot store input.id)
  | ["design", "get"] =>
      let input ← readInput "design get" ( α := IdInput)
      let state ← Store.loadState store
      match state.design? input.id with
      | some value => writeJson value
      | none => fail s!"no DesignRevision {input.id}"
  | ["work", "start"] =>
      let input ← readInput "work start" ( α := WorkStartRequest)
      emitCurrentState invocation.projectRoot
        (← Store.startWorkRequest store input)
  | ["work", "get"] =>
      let input ← readInput "work get" ( α := IdInput)
      let state ← Store.loadState store
      match state.work? input.id with
      | some value => writeJson value
      | none => fail s!"no Work {input.id}"
  | ["work", "suspend"] =>
      let input ← readInput "work suspend" ( α := SuspendInput)
      emitState (← Store.suspendWork store input.workId input.resumeCondition)
  | ["work", "focus"] =>
      let input ← readInput "work focus" ( α := IdInput)
      emitCurrentState invocation.projectRoot
        (← Store.focusWork store input.id)
  | ["work", "resume"] =>
      let input ← readInput "work resume" ( α := IdInput)
      emitCurrentState invocation.projectRoot
        (← Store.resumeWork store input.id)
  | ["work", "adopt-design"] =>
      let input ← readInput "work adopt-design" ( α := AdoptDesignInput)
      emitState (← Store.adoptDesignForWork store input.workId input.entryId
        input.impactDisposition input.agentRun)
  | ["work", "handoff"] =>
      let input ← readInput "work handoff" ( α := HandoffInput)
      emitCurrentState invocation.projectRoot (← Store.handoffWork store
        input.workId input.entryId input.successorRun input.reason)
  | ["task", "add"] =>
      let input ← readInput "task add" ( α := TaskAddRequest)
      emitState (← Store.addTask store input)
  | ["task", "close"] =>
      let input ← readInput "task close" ( α := TaskCloseRequest)
      emitState (← Store.closeTask store input)
  | ["profile", "define"] =>
      let input ← readInput "profile define" ( α := ProfileDefineRequest)
      emitState (← Store.defineProfile store input)
  | ["profile", "replace"] =>
      let input ← readInput "profile replace" ( α := ProfileReplaceRequest)
      emitState (← Store.replaceProfile store input)
  | ["artifact", "observe"] =>
      let input ← readInput "artifact observe" ( α := ArtifactObserveRequest)
      emitState (← Store.observeArtifact invocation.projectRoot store input)
  | ["correction", "record"] =>
      let input ← readInput "correction record" ( α := CorrectionRecordRequest)
      emitState (← Store.recordCorrection store input)
  | ["correction", "supersede"] =>
      let input ← readInput "correction supersede" ( α := CorrectionSupersedeRequest)
      emitState (← Store.supersedeCorrection store input)
  | ["correction", "resolve"] =>
      let input ← readInput "correction resolve" ( α := CorrectionResolveRequest)
      emitState (← Store.resolveCorrection store input)
  | ["correction", "incorporate"] =>
      let input ← readInput "correction incorporate" ( α := CorrectionIncorporateRequest)
      emitState (← Store.incorporateCorrection store input)
  | ["kpt", "record"] =>
      let input ← readInput "kpt record" ( α := KptRecordRequest)
      emitState (← Store.recordKpt store input)
  | ["kpt", "apply"] =>
      let input ← readInput "kpt apply" ( α := KptApplyRequest)
      emitState (← Store.applyKpt store input)
  | ["review", "start"] =>
      let input ← readInput "review start" ( α := ReviewStartRequest)
      emitState (← Store.startReview invocation.projectRoot store input)
  | ["review", "resume"] =>
      let input ← readInput "review resume" ( α := ReviewResumeRequest)
      emitState (← Store.resumeReview invocation.projectRoot store input)
  | ["review", "finding"] =>
      let input ← readInput "review finding" ( α := FindingRecordRequest)
      emitState (← Store.recordFinding store input)
  | ["review", "disposition"] =>
      let input ← readInput "review disposition" ( α := DispositionRecordRequest)
      emitState (← Store.recordDisposition store input)
  | ["review", "verify"] =>
      let input ← readInput "review verify" ( α := VerificationRecordRequest)
      emitState (← Store.recordVerification store input)
  | ["entry", "get"] =>
      let input ← readInput "entry get" ( α := IdInput)
      let state ← Store.loadState store
      match state.entry? input.id with
      | some value => writeJson value
      | none => fail s!"no LedgerEntry {input.id}"
  | ["history"] =>
      let input ← readInput "history" ( α := HistoryInput)
      if input.limit == 0 || input.limit > 100 then
        fail "history limit must be between 1 and 100"
      let state ← Store.loadState store
      writeJson (state.ledgerEntries.filter (fun entry => entry.order > input.afterOrder)
        |>.take input.limit)
  | ["review", "context"] =>
      let input ← readInput "review context" ( α := IdInput)
      let state ← Store.loadState store
      match reviewInput? state input.id with
      | some value => writeJson value
      | none => fail s!"no Review entry {input.id}"
  | ["command", "show"] =>
      let input ← readInput "command show" ( α := IdInput)
      let state ← Store.loadState store
      match resolveCommandProfile? invocation.projectRoot state input.id with
      | some resolved => writeJson resolved
      | none => fail s!"no applicable Command Profile {input.id}"
  | ["command", "run"] =>
      let input ← readInput "command run" ( α := CommandRunRequest)
      writeJson (← Store.runCommandProfile invocation.projectRoot store input)
  | ["proof", "digest"] =>
      let input ← readInput "proof digest" ( α := IdInput)
      let state ← Store.loadState store
      writeJson (← currentClaimDigest invocation.projectRoot state input.id)
  | ["proof", "run"] =>
      let input ← readInput "proof run" ( α := ProofRunRequest)
      writeJson (← Store.runProofClaim invocation.projectRoot store input)
  | ["work", "complete"] =>
      emitState (← Store.completeFocusedWork invocation.projectRoot store)
  | ["context"] =>
      let state ← Store.loadState store
      let inputs ← evaluateCurrentInputs invocation.projectRoot state
      writeJson (ContextResult.mk state.revision
        (currentContext? state inputs.observations inputs.claimDigests))
  | ["ready"] =>
      let state ← Store.loadState store
      let inputs ← evaluateCurrentInputs invocation.projectRoot state
      writeJson (ReadinessResult.mk state.revision
        (completionReady state inputs.observations inputs.claimDigests)
        (currentContext? state inputs.observations inputs.claimDigests))
  | command => fail s!"unknown command: {String.intercalate " " command}"

private def isReadOnlyStateCommand : List String → Bool
  | ["design", "get"] | ["work", "get"] | ["entry", "get"] | ["history"]
  | ["review", "context"] | ["command", "show"] | ["proof", "digest"]
  | ["context"] | ["ready"] => true
  | _ => false

private def runStateCommand (invocation : Invocation) : IO Unit :=
  if isReadOnlyStateCommand invocation.command then
    runStateCommandUnlocked invocation
  else
    AgentWorkbench.OperationLock.withProjectMutationLock invocation.projectRoot
      (runStateCommandUnlocked invocation)

private def runCommand (invocation : Invocation) : IO Unit := do
  match invocation.command with
  | ["describe"] => writeJson (operationIndex (← loadDescribeState invocation.projectRoot))
  | "describe" :: operationParts =>
      let operation := String.intercalate " " operationParts
      let state ← loadDescribeState invocation.projectRoot
      match describedOperation? state operation with
      | some value => writeJson value
      | none => fail s!"unknown operation {operation}"
  | _ => runStateCommand invocation

def main (arguments : List String) : IO Unit := do
  runCommand (← parseInvocation arguments)

end AgentWorkbench.Cli
