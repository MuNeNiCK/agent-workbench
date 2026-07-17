use super::*;

#[test]
fn finding_lifecycle_matrix_is_closed() {
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
