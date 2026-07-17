use super::*;

#[test]
fn invocation_transition_matrix_is_closed() {
    let states = [
        InvocationState::Requested,
        InvocationState::Running,
        InvocationState::Completed,
        InvocationState::Failed,
        InvocationState::Cancelled,
    ];
    for from in states {
        for to in states {
            let allowed = matches!(
                (from, to),
                (
                    InvocationState::Requested,
                    InvocationState::Running
                        | InvocationState::Completed
                        | InvocationState::Failed
                        | InvocationState::Cancelled
                ) | (
                    InvocationState::Running,
                    InvocationState::Completed
                        | InvocationState::Failed
                        | InvocationState::Cancelled
                )
            );
            assert_eq!(
                invocation_transition(from, to).is_ok(),
                allowed,
                "{from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn private_stage_and_finding_lifecycle_matrices_are_closed() {
    let stages = [
        PrivateResultStageState::Staging,
        PrivateResultStageState::Completed,
        PrivateResultStageState::Cancelled,
    ];
    for from in stages {
        for to in stages {
            assert_eq!(
                stage_transition(from, to).is_ok(),
                from == PrivateResultStageState::Staging
                    && matches!(
                        to,
                        PrivateResultStageState::Completed | PrivateResultStageState::Cancelled
                    )
            );
        }
    }
    let states = [
        FindingLifecycle::Open,
        FindingLifecycle::Remediating,
        FindingLifecycle::AwaitingVerification,
        FindingLifecycle::Closed,
    ];
    for from in states {
        for to in states {
            let allowed = matches!(
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
            );
            assert_eq!(finding_lifecycle_transition(from, to).is_ok(), allowed);
        }
    }
}
