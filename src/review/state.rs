use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvocationState {
    Requested,
    Running,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateResultStageState {
    Staging,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewClaim {
    Clean,
    Findings,
    Inconclusive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdjudicationDecision {
    Accepted,
    Rejected,
    NeedsEvidence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingDisposition {
    Accepted,
    Rejected,
    NeedsEvidence,
    DesignConflict,
    Deferred,
    AuthorityDisposed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationClaim {
    Verified,
    NotFixed,
    NeedsEvidence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingLifecycle {
    Open,
    Remediating,
    AwaitingVerification,
    Closed,
}

pub fn invocation_transition(from: InvocationState, to: InvocationState) -> Result<()> {
    if matches!(
        (from, to),
        (
            InvocationState::Requested,
            InvocationState::Running
                | InvocationState::Completed
                | InvocationState::Failed
                | InvocationState::Cancelled
        ) | (
            InvocationState::Running,
            InvocationState::Completed | InvocationState::Failed | InvocationState::Cancelled
        )
    ) {
        Ok(())
    } else {
        bail!("review invocation transition is not allowed")
    }
}

pub fn stage_transition(from: PrivateResultStageState, to: PrivateResultStageState) -> Result<()> {
    if from == PrivateResultStageState::Staging
        && matches!(
            to,
            PrivateResultStageState::Completed | PrivateResultStageState::Cancelled
        )
    {
        Ok(())
    } else {
        bail!("private result stage transition is not allowed")
    }
}

pub fn finding_lifecycle_transition(from: FindingLifecycle, to: FindingLifecycle) -> Result<()> {
    if matches!(
        (from, to),
        (
            FindingLifecycle::Open,
            FindingLifecycle::Remediating | FindingLifecycle::Closed
        ) | (
            FindingLifecycle::Remediating,
            FindingLifecycle::AwaitingVerification
        ) | (
            FindingLifecycle::AwaitingVerification,
            FindingLifecycle::Remediating | FindingLifecycle::Closed
        )
    ) {
        Ok(())
    } else {
        bail!("finding lifecycle transition is not allowed")
    }
}

impl InvocationState {
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "requested" => Self::Requested,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => bail!("unknown review invocation state"),
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}
