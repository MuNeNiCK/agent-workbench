import AgentWorkbench.Domain.Design

namespace AgentWorkbench

structure CommandProfileRecord where
  purpose : String
  taskEntryId : Option String := none
  inputTargets : Option (List String) := none
  outputScope : Option String := none
  criterionIds : Option (List String) := none
  taskVerificationIds : Option (List String) := none
  target : Option String := none
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.FromJson

instance : Lean.ToJson CommandProfileRecord where
  toJson value := Lean.Json.mkObj <|
    [("purpose", Lean.toJson value.purpose),
     ("taskEntryId", Lean.toJson value.taskEntryId),
     ("inputTargets", Lean.toJson value.inputTargets),
     ("outputScope", Lean.toJson value.outputScope),
     ("criterionIds", Lean.toJson value.criterionIds)] ++
    (if value.taskVerificationIds.getD [] |>.isEmpty then [] else
      [("taskVerificationIds", Lean.toJson value.taskVerificationIds)]) ++
    [("target", Lean.toJson value.target),
     ("command", Lean.toJson value.command)]

structure InputSnapshot where
  target : String
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CommandExecutionRecord where
  profileEntryId : String
  taskEntryId : Option String := none
  outputScope : Option String := none
  criterionId : Option String := none
  taskVerificationId : Option String := none
  inputSnapshots : Option (List InputSnapshot) := none
  environmentSnapshots : Option (List InputSnapshot) := none
  target : Option String := none
  snapshot : Option String := none
  command : CommandSpec
  exitCode : Nat
  stdoutDigest : String
  stderrDigest : String
  successful : Bool
  producerAgentRun : String
  deriving Repr, DecidableEq

instance : Lean.ToJson CommandExecutionRecord where
  toJson value := Lean.Json.mkObj <|
    [("profileEntryId", Lean.toJson value.profileEntryId),
     ("taskEntryId", Lean.toJson value.taskEntryId),
     ("outputScope", Lean.toJson value.outputScope),
     ("criterionId", Lean.toJson value.criterionId)] ++
    (value.taskVerificationId.toList.map fun id =>
      ("taskVerificationId", Lean.toJson id)) ++
    [("inputSnapshots", Lean.toJson value.inputSnapshots),
     ("environmentSnapshots", Lean.toJson value.environmentSnapshots),
     ("target", Lean.toJson value.target),
     ("snapshot", Lean.toJson value.snapshot),
     ("command", Lean.toJson value.command),
     ("exitCode", Lean.toJson value.exitCode),
     ("stdoutDigest", Lean.toJson value.stdoutDigest),
     ("stderrDigest", Lean.toJson value.stderrDigest),
     ("successful", Lean.toJson value.successful),
     ("producerAgentRun", Lean.toJson value.producerAgentRun)]

private structure PersistedCommandExecutionRecord where
  profileEntryId : String
  taskEntryId : Option String
  outputScope : Option String
  criterionId : Option String
  taskVerificationId : Option String := none
  inputSnapshots : Option (List InputSnapshot)
  environmentSnapshots : Option (List InputSnapshot) := none
  target : Option String
  snapshot : Option String
  command : CommandSpec
  exitCode : Nat
  stdoutDigest : String
  stderrDigest : String
  successful : Bool
  producerAgentRun : String
  deriving Lean.FromJson

private structure LegacyCommandExecutionRecord where
  profileEntryId : String
  taskEntryId : Option String := none
  outputScope : Option String := none
  criterionId : Option String := none
  taskVerificationId : Option String := none
  inputSnapshots : Option (List InputSnapshot) := none
  environmentSnapshots : Option (List InputSnapshot) := none
  target : Option String := none
  snapshot : Option String := none
  command : CommandSpec
  exitCode : Nat
  stdoutDigest : String
  stderrDigest : String
  successful : Bool
  producerAgentRun : String
  deriving Lean.FromJson

instance : Lean.FromJson CommandExecutionRecord where
  fromJson? json :=
    match (Lean.fromJson? json : Except String PersistedCommandExecutionRecord) with
    | .ok value =>
      let environmentSnapshots := value.environmentSnapshots.orElse fun _ => some <|
        value.command.environment.toList.map fun name =>
          ({ target := s!"env:{name}", snapshot := "blake3:legacy-unavailable" } : InputSnapshot)
      pure {
        profileEntryId := value.profileEntryId, taskEntryId := value.taskEntryId
        outputScope := value.outputScope, criterionId := value.criterionId
        taskVerificationId := value.taskVerificationId
        inputSnapshots := value.inputSnapshots, environmentSnapshots
        target := value.target, snapshot := value.snapshot
        command := value.command, exitCode := value.exitCode
        stdoutDigest := value.stdoutDigest, stderrDigest := value.stderrDigest
        successful := value.successful, producerAgentRun := value.producerAgentRun }
    | .error currentError =>
        match (Lean.fromJson? json : Except String LegacyCommandExecutionRecord) with
        | .ok value =>
          let environmentSnapshots := value.environmentSnapshots.orElse fun _ => some <|
            value.command.environment.toList.map fun name =>
              ({ target := s!"env:{name}", snapshot := "blake3:legacy-unavailable" } : InputSnapshot)
          pure {
            profileEntryId := value.profileEntryId, taskEntryId := value.taskEntryId
            outputScope := value.outputScope, criterionId := value.criterionId
            taskVerificationId := value.taskVerificationId
            inputSnapshots := value.inputSnapshots
            environmentSnapshots
            target := value.target, snapshot := value.snapshot
            command := value.command, exitCode := value.exitCode
            stdoutDigest := value.stdoutDigest, stderrDigest := value.stderrDigest
            successful := value.successful, producerAgentRun := value.producerAgentRun }
        | .error legacyError =>
            throw s!"invalid command execution: {currentError}; legacy: {legacyError}"

structure ArtifactObservationRecord where
  taskEntryId : Option String := none
  outputScope : Option String := none
  criterionId : Option String := none
  taskVerificationId : Option String := none
  target : String
  snapshot : String
  operation : String
  result : String
  successful : Bool
  producerAgentRun : String
  deriving Repr, DecidableEq, Lean.FromJson

instance : Lean.ToJson ArtifactObservationRecord where
  toJson value := Lean.Json.mkObj <|
    [("taskEntryId", Lean.toJson value.taskEntryId),
     ("outputScope", Lean.toJson value.outputScope),
     ("criterionId", Lean.toJson value.criterionId)] ++
    (value.taskVerificationId.toList.map fun id =>
      ("taskVerificationId", Lean.toJson id)) ++
    [("target", Lean.toJson value.target),
     ("snapshot", Lean.toJson value.snapshot),
     ("operation", Lean.toJson value.operation),
     ("result", Lean.toJson value.result),
     ("successful", Lean.toJson value.successful),
     ("producerAgentRun", Lean.toJson value.producerAgentRun)]

structure ProofSourceDigest where
  path : String
  digest : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure LeanProofReceiptRecord where
  claimId : String
  claimInput : ClaimInput
  elaboratedPropositionDigest : String := ""
  propositionDependencies : List String := []
  assumptionDependencies : List String := []
  inputDigest : String
  sourceDigests : List ProofSourceDigest
  toolchain : String
  exitCode : Nat
  outputDigest : String
  kernelAccepted : Bool
  deriving Repr, DecidableEq, Lean.ToJson

private structure PersistedLeanProofReceiptRecord where
  claimId : String
  claimInput : ClaimInput
  elaboratedPropositionDigest : String
  propositionDependencies : List String
  assumptionDependencies : List String
  inputDigest : String
  sourceDigests : List ProofSourceDigest
  toolchain : String
  exitCode : Nat
  outputDigest : String
  kernelAccepted : Bool
  deriving Lean.FromJson

private structure LegacyLeanProofReceiptRecord where
  claimId : String
  claimInput : ClaimInput
  inputDigest : String
  sourceDigests : List ProofSourceDigest
  toolchain : String
  exitCode : Nat
  outputDigest : String
  kernelAccepted : Bool
  deriving Lean.FromJson

instance : Lean.FromJson LeanProofReceiptRecord where
  fromJson? json :=
    match (Lean.fromJson? json : Except String PersistedLeanProofReceiptRecord) with
    | .ok value => pure {
        claimId := value.claimId, claimInput := value.claimInput
        elaboratedPropositionDigest := value.elaboratedPropositionDigest
        propositionDependencies := value.propositionDependencies
        assumptionDependencies := value.assumptionDependencies, inputDigest := value.inputDigest
        sourceDigests := value.sourceDigests, toolchain := value.toolchain, exitCode := value.exitCode
        outputDigest := value.outputDigest, kernelAccepted := value.kernelAccepted }
    | .error currentError =>
        match (Lean.fromJson? json : Except String LegacyLeanProofReceiptRecord) with
        | .ok value => pure {
            claimId := value.claimId, claimInput := value.claimInput
            assumptionDependencies := [], inputDigest := value.inputDigest
            sourceDigests := value.sourceDigests, toolchain := value.toolchain
            exitCode := value.exitCode, outputDigest := value.outputDigest
            kernelAccepted := value.kernelAccepted }
        | .error legacyError =>
            throw s!"invalid Lean proof receipt: {currentError}; legacy: {legacyError}"

end AgentWorkbench
