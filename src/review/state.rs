use anyhow::{Result, bail};

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
