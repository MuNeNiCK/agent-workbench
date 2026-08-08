import AgentWorkbench.Application.Mutation
import AgentWorkbenchProof.PersistedFields

namespace AgentWorkbenchProof

open AgentWorkbench

/-- Every persisted payload constructor is assigned without a wildcard. -/
def payloadInvariantCoverage : EntryPayload → InvariantFamily
  | .task _ => .planTask
  | .workDesignAdoption _ | .workHandoff _ | .workWithdrawal _ | .workResume _
  | .workCompletion _ => .workLifecycle
  | .designRejection _ => .designHistory
  | .commandProfile _ | .commandExecution _ | .artifactObservation _
  | .review _ | .finding _ | .reviewDisposition _ | .reviewVerification _
  | .reviewHandoff _ | .reviewConclusion _ | .userCorrection _ | .kpt _
  | .leanProofReceipt _ => .ledgerAuthority

/-- Positional payload-record coverage. A field added to any persisted payload makes this match
fail to compile until its authority meaning is assigned. -/
def payloadFieldCoverage : EntryPayload → List String
  | .task (.mk _ _ _ _ _ _ _ _ _ _ _ _ _ _ _) =>
      ["planId", "planStepId", "lineageId", "dependencyLineageIds", "outputScopes",
       "verificationCriterionIds", "taskVerificationContracts", "verificationEvidenceEntryIds",
       "verificationTaskEntryId", "materializedAtOrder", "retired", "criterionId", "description",
       "required", "closed"]
  | .workDesignAdoption (.mk _ _ _ _) =>
      ["predecessor", "successor", "impactDisposition", "adoptedByRun"]
  | .workHandoff (.mk _ _ _) => ["predecessorRun", "successorRun", "reason"]
  | .workWithdrawal (.mk _ _ _) => ["correctionEntryId", "reason", "withdrawnByRun"]
  | .workResume (.mk _ _ _ _) => ["condition", "satisfaction", "basisEntryIds", "resumedByRun"]
  | .workCompletion (.mk _ _ _ _ _ _) =>
      ["workId", "designRevision", "planId", "inputRevision", "inputDigest", "completedByRun"]
  | .designRejection (.mk _ _ _) => ["designId", "reason", "rejectedByRun"]
  | .commandProfile (.mk _ _ _ _ _ _ _ _) =>
      ["purpose", "taskEntryId", "inputTargets", "outputScope", "criterionIds",
       "taskVerificationIds", "target", "command"]
  | .commandExecution (.mk _ _ _ _ _ _ _ _ _ _ _ _ _ _ _) =>
      ["profileEntryId", "taskEntryId", "outputScope", "criterionId", "taskVerificationId",
       "inputSnapshots", "environmentSnapshots", "target", "snapshot",
       "command", "exitCode", "stdoutDigest", "stderrDigest", "successful", "producerAgentRun"]
  | .artifactObservation (.mk _ _ _ _ _ _ _ _ _ _) =>
      ["taskEntryId", "outputScope", "criterionId", "taskVerificationId", "target", "snapshot",
       "operation", "result", "successful", "producerAgentRun"]
  | .review (.mk _ _ _ _ _ _ _ _ _ _ _) =>
      ["reviewId", "purpose", "context", "continuesEntryId", "targetSourceId", "target",
       "targetSnapshot", "targetManifestVersion", "targetManifest", "producerAgentRuns",
       "reviewerAgentRun"]
  | .finding (.mk _ _ _ _ _ _ _) =>
      ["reviewId", "subject", "targetSourceId", "target", "targetSnapshot", "producerAgentRuns",
       "summary"]
  | .reviewDisposition (.mk _ _ _ _) =>
      ["findingEntryId", "decision", "reason", "decidedByRun"]
  | .reviewVerification (.mk _ _ _ _ _ _ _ _) =>
      ["reviewId", "findingEntryId", "reviewEntryId", "evidenceEntryId", "target", "snapshot",
       "verifiedByRun", "resolved"]
  | .reviewHandoff (.mk _ _ _ _ _) =>
      ["reviewId", "reviewEntryId", "predecessorReviewerRun", "successorReviewerRun", "reason"]
  | .reviewConclusion (.mk _ _ _ _ _) =>
      ["reviewId", "reviewEntryId", "reviewerAgentRun", "clean", "summary"]
  | .userCorrection (.mk _ _ _ _) =>
      ["content", "resolvedByEntryId", "resolutionReason", "incorporatedIn"]
  | .kpt (.mk _ _ _ _ _) =>
      ["keep", "problem", "tryNext", "appliesKptEntryId", "appliedByEntryId"]
  | .leanProofReceipt (.mk _ _ _ _ _ _ _ _ _ _ _) =>
      ["claimId", "claimInput", "elaboratedPropositionDigest", "propositionDependencies",
       "assumptionDependencies", "inputDigest", "sourceDigests", "toolchain", "exitCode",
       "outputDigest", "kernelAccepted"]

/-- Every mutation effect is assigned to the product invariant families it may change. -/
def mutationInvariantCoverage : Mutation → List InvariantFamily
  | .init => [.ledgerAuthority]
  | .designPropose _ | .designAmend _ | .designAccept _ | .designReject _ =>
      [.designHistory, .workLifecycle, .ledgerAuthority]
  | .workStart _ | .workFocus _ | .workSuspend _ _ | .workResume _
  | .workHandoff _ _ _ _ | .workAdoptDesign _ | .workWithdraw _ | .workComplete =>
      [.workLifecycle, .ledgerAuthority]
  | .planPropose _ | .planReplace _ | .planMaterialize _ | .taskClose _ =>
      [.planTask, .ledgerAuthority]
  | .profileDefine _ | .profileReplace _ | .commandRun _ | .artifactObserve _ | .proofRun _
  | .correctionRecord _ | .correctionSupersede _ | .correctionResolve _
  | .correctionIncorporate _ | .kptRecord _ | .kptApply _ | .reviewStart _
  | .reviewResume _ | .reviewHandoff _ | .reviewFinding _ | .reviewDisposition _
  | .reviewConclude _ | .reviewVerify _ => [.ledgerAuthority]

theorem every_mutation_has_invariant_coverage (mutation : Mutation) :
    (mutationInvariantCoverage mutation).isEmpty = false := by
  cases mutation <;> rfl

def stateComponentInvariant : StateComponent → InvariantFamily
  | .acceptedDesign | .designs => .designHistory
  | .focusedWork | .works => .workLifecycle
  | .plans => .planTask
  | .ledger => .ledgerAuthority

/-- The executable effect boundary and the private theorem-family assignment
are connected exhaustively. Adding an operation or state component cannot
leave a permitted effect without an owning invariant family. -/
theorem every_permitted_mutation_effect_has_invariant_coverage (mutation : Mutation) :
    mutation.operation.permittedStateComponents.all (fun component =>
      (mutationInvariantCoverage mutation).contains (stateComponentInvariant component)) = true := by
  cases mutation <;> rfl

end AgentWorkbenchProof
