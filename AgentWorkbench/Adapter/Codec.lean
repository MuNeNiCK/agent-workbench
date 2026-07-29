import AgentWorkbench.Kernel.Decide
import SQLite.Blob.Deriving

namespace AgentWorkbench.Adapter.Codec

open SQLite.Blob

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.SourceId
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.SourceKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Source
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.CallerDecision
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.DesignRef
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.WorkRef
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.TaskRef
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.ReviewRef
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.EvidenceRef

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Role
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AssuranceKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AssuranceMethod
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AssuranceObligation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AssuranceSelection
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Authority
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.ComplexityRationale
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Item
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.AcceptedRef
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.OperatingInstruction
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.NonAuthoritativeKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.NonAuthoritativeRecord
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.EffectContent
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Effect
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Design.Package

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.DerivationBasis
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.CompletionTarget
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.CompletionMember
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.Unit
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.Phase
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.TaskState
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.Task
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.ReturnAssumption
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.ReturnPoint
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Work.Focus

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Evidence.Spec
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Evidence.Result
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Evidence.FormalSpec
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Evidence.FormalResult

deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Purpose
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Scope
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Request
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.ObservationKind
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Observation
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Result
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Decision
deriving instance ToBinary, FromBinary for AgentWorkbench.Domain.Review.Disposition

deriving instance ToBinary, FromBinary for AgentWorkbench.Kernel.State

end AgentWorkbench.Adapter.Codec
