import AgentWorkbench.Domain.Design

namespace AgentWorkbench

structure CommandProfileRecord where
  purpose : String
  taskEntryId : Option String := none
  inputTargets : Option (List String) := none
  outputScope : Option String := none
  criterionIds : Option (List String) := none
  target : Option String := none
  command : CommandSpec
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure InputSnapshot where
  target : String
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CommandExecutionRecord where
  profileEntryId : String
  taskEntryId : Option String := none
  outputScope : Option String := none
  criterionId : Option String := none
  inputSnapshots : Option (List InputSnapshot) := none
  target : Option String := none
  snapshot : Option String := none
  command : CommandSpec
  exitCode : Nat
  stdoutDigest : String
  stderrDigest : String
  successful : Bool
  producerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson

private structure PersistedCommandExecutionRecord where
  profileEntryId : String
  taskEntryId : Option String
  outputScope : Option String
  criterionId : Option String
  inputSnapshots : Option (List InputSnapshot)
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
    | .ok value => pure {
        profileEntryId := value.profileEntryId, taskEntryId := value.taskEntryId
        outputScope := value.outputScope, criterionId := value.criterionId
        inputSnapshots := value.inputSnapshots, target := value.target, snapshot := value.snapshot
        command := value.command, exitCode := value.exitCode
        stdoutDigest := value.stdoutDigest, stderrDigest := value.stderrDigest
        successful := value.successful, producerAgentRun := value.producerAgentRun }
    | .error currentError =>
        match (Lean.fromJson? json : Except String LegacyCommandExecutionRecord) with
        | .ok value => pure {
            profileEntryId := value.profileEntryId, taskEntryId := value.taskEntryId
            outputScope := value.outputScope, criterionId := value.criterionId
            inputSnapshots := none, target := value.target, snapshot := value.snapshot
            command := value.command, exitCode := value.exitCode
            stdoutDigest := value.stdoutDigest, stderrDigest := value.stderrDigest
            successful := value.successful, producerAgentRun := value.producerAgentRun }
        | .error legacyError =>
            throw s!"invalid command execution: {currentError}; legacy: {legacyError}"

structure ArtifactObservationRecord where
  taskEntryId : Option String := none
  outputScope : Option String := none
  criterionId : String
  target : String
  snapshot : String
  operation : String
  result : String
  successful : Bool
  producerAgentRun : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

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
