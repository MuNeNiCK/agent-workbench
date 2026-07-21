use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TaskState {
    Open,
    Blocked,
    Completed,
    OutOfScope,
}

impl TaskState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(Self::Open),
            "blocked" => Ok(Self::Blocked),
            "closed" => Ok(Self::Completed),
            "accepted_out_of_scope" => Ok(Self::OutOfScope),
            _ => bail!("unreadable_source: task status is outside the closed profile"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PhaseState {
    Open,
    Blocked,
    Closed,
    OutOfScope,
    Split,
    Superseded,
}

impl PhaseState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(Self::Open),
            "blocked" => Ok(Self::Blocked),
            "closed" => Ok(Self::Closed),
            "accepted_out_of_scope" => Ok(Self::OutOfScope),
            "split" => Ok(Self::Split),
            "superseded" => Ok(Self::Superseded),
            _ => bail!("unreadable_source: phase status is outside the closed profile"),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Blocked => "blocked",
            Self::Closed => "closed",
            Self::OutOfScope => "out_of_scope",
            Self::Split => "split",
            Self::Superseded => "out_of_scope",
        }
    }

    pub(super) const fn is_live(self) -> bool {
        matches!(self, Self::Open | Self::Blocked)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DependencyState {
    Open,
    Completed,
    OutOfScope,
    Superseded,
}

impl DependencyState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(Self::Open),
            "satisfied" => Ok(Self::Completed),
            "accepted" => Ok(Self::OutOfScope),
            "invalidated" => Ok(Self::Superseded),
            _ => bail!("unreadable_source: dependency status is outside the closed profile"),
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::OutOfScope => "out_of_scope",
            Self::Superseded => "out_of_scope",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChecklistState {
    Open,
    Stale,
    Closed,
}

impl ChecklistState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Open),
            "stale" => Ok(Self::Stale),
            "closed" => Ok(Self::Closed),
            _ => bail!("unreadable_source: checklist status is outside the closed profile"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChecklistItemState {
    Open,
    Blocked,
    Closed,
    OutOfScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DerivationState {
    Active,
    Stale,
    Closed,
}

impl DerivationState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "stale" => Ok(Self::Stale),
            "closed" => Ok(Self::Closed),
            _ => bail!("unreadable_source: derivation status is outside the closed profile"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RequirementState {
    Active,
    Superseded,
    OutOfScope,
}

impl RequirementState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "accepted_out_of_scope" => Ok(Self::OutOfScope),
            _ => bail!("unreadable_source: requirement status is outside the closed profile"),
        }
    }
}

impl ChecklistItemState {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(Self::Open),
            "blocked" => Ok(Self::Blocked),
            "closed" => Ok(Self::Closed),
            "accepted_out_of_scope" => Ok(Self::OutOfScope),
            _ => bail!("unreadable_source: checklist item status is outside the closed profile"),
        }
    }
}
