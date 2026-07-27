import AgentWorkbench.Application.Service
import SQLite.Blob.Deriving

namespace AgentWorkbench.Adapter.Codec

open SQLite.Blob

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.WorkId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ActivationId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.DesignId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ReviewId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ReviewPlanId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.CompletionEpoch
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.EvidenceId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.OperationId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.LedgerId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ProjectionId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Digest
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.StageId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Revision

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.WorkStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ActivationStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ReviewClaim
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.OwnerDecision
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.EvidenceKind

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.WorkUnit
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.ReadinessBasis
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.SuspensionContext
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.Activation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Requirement
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.DesignVersion
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Approval
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.TraceItem
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Decomposition
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Correction
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.KptEntry
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AuthorityOperation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AuthorityLifetime
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AuthorityKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AuthorityTransition
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Purpose
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.FrozenScope
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Plan
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.AuthorityException
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.ObservationKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Observation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Claim
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.ObservationDecision
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.AdoptionRationale
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.ObservationDisposition
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Adjudication
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.ClosureAttempt
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.VerificationResult
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Finding
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Verification
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Evidence.Obligation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Evidence.Evidence
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ExternalOperation.AttemptState
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ExternalOperation.OperationKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ExternalOperation.RemoteObservation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ExternalOperation.RemotePrecondition
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ExternalOperation.RemoteTarget
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ExternalOperation.Attempt

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.RelatedWorkKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ItemStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.FindingStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ValidationStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.RepositoryStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.CorrectionStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.WorkRecordStatus
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.RelatedWorkRequirement
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.PhaseSpec
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ScopeChangeKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ScopeChangeCause
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ResultingScope
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ScopeChange
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.CompletionPlan
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.PhaseRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.TaskRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ChecklistRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.FindingRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.ValidationRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.RepositoryRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.CorrectionRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.WorkRecordLink
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Lifecycle.CompletionState

deriving instance ToBinary, FromBinary for AgentWorkbench.Kernel.Replay.State
deriving instance ToBinary, FromBinary for AgentWorkbench.Kernel.Replay.Event
deriving instance ToBinary, FromBinary for AgentWorkbench.Kernel.Decide.Command
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Projection.DecodeFault
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Projection.ProjectionFingerprint
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Projection.ProjectionRef
deriving instance ToBinary, FromBinary for AgentWorkbench.Kernel.Projection.ProjectionPayload
deriving instance ToBinary, FromBinary for AgentWorkbench.Kernel.Projection.ProjectionObservation
deriving instance ToBinary, FromBinary for AgentWorkbench.Policy.Update.Receipt

end AgentWorkbench.Adapter.Codec
