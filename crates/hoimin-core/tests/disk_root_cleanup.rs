use hoimin_core::{
    DiskCleanupOutcome, DiskCleanupState, DiskLifecycle, DiskLifecycleEvent, DiskRootEntry,
    DiskRootId,
};
use proptest::prelude::*;

fn settled(root: DiskRootId) -> DiskLifecycle {
    let mut lifecycle = DiskLifecycle::new([root]).unwrap();
    for event in [
        DiskLifecycleEvent::ProcessDrainSucceeded,
        DiskLifecycleEvent::OutputDrainSucceeded,
        DiskLifecycleEvent::MonitorJoinSucceeded,
    ] {
        assert!(lifecycle.apply(event));
    }
    lifecycle
}

proptest! {
    #[test]
    fn snapshot_preserves_registration_order(delivery_first in any::<bool>()) {
        // Arrange
        let roots = if delivery_first { [DiskRootId::Delivery, DiskRootId::Execution] }
            else { [DiskRootId::Execution, DiskRootId::Delivery] };
        let lifecycle = DiskLifecycle::new(roots).unwrap();
        let expected = roots.map(|root| DiskRootEntry {root, cleanup: DiskCleanupState::NotRequested}).to_vec();

        // Act
        let actual = lifecycle.snapshot().roots;

        // Assert
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn completion_keeps_failure_message(message in ".*") {
        // Arrange
        let mut lifecycle = settled(DiskRootId::Execution);
        assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {root: DiskRootId::Execution}));
        let outcome = DiskCleanupOutcome::Failed(message);
        let expected = vec![DiskRootEntry {root: DiskRootId::Execution,
            cleanup: DiskCleanupState::Completed(outcome.clone())}];

        // Act
        let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {root: DiskRootId::Execution, outcome});
        let actual = lifecycle.snapshot().roots;

        // Assert
        prop_assert!(accepted);
        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn cleanup_request_replaces_not_requested_state() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Execution);
    let expected = vec![DiskRootEntry {
        root: DiskRootId::Execution,
        cleanup: DiskCleanupState::Requested,
    }];

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    });
    let actual = lifecycle.snapshot().roots;

    // Assert
    assert!(accepted);
    assert_eq!(actual, expected);
}

#[test]
fn completion_without_request_is_rejected_without_mutation() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Execution);
    let before = lifecycle.snapshot();

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Execution,
        outcome: DiskCleanupOutcome::Clean,
    });

    // Assert
    assert!(!accepted);
    assert_eq!(lifecycle.snapshot(), before);
}

#[test]
fn completion_for_unowned_root_is_rejected_without_mutation() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Execution);
    let before = lifecycle.snapshot();

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Delivery,
        outcome: DiskCleanupOutcome::Retained,
    });

    // Assert
    assert!(!accepted);
    assert_eq!(lifecycle.snapshot(), before);
}

#[test]
fn duplicate_request_is_rejected_without_mutation() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Execution);
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution
    }));
    let before = lifecycle.snapshot();

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    });

    // Assert
    assert!(!accepted);
    assert_eq!(lifecycle.snapshot(), before);
}

#[test]
fn active_work_blocks_cleanup_request_without_mutation() {
    // Arrange
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    assert!(lifecycle.apply(DiskLifecycleEvent::DispatchRequested));
    assert!(lifecycle.apply(DiskLifecycleEvent::OutputDrainSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::MonitorJoinSucceeded));
    let before = lifecycle.snapshot();

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    });

    // Assert
    assert!(!accepted);
    assert_eq!(lifecycle.snapshot(), before);
}

proptest! {
    #[test]
    fn each_pending_safety_component_blocks_cleanup_request(pending in 0usize..3) {
        // Arrange
        let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
        for (index, event) in [DiskLifecycleEvent::ProcessDrainSucceeded,
            DiskLifecycleEvent::OutputDrainSucceeded, DiskLifecycleEvent::MonitorJoinSucceeded].into_iter().enumerate() {
            if index != pending { assert!(lifecycle.apply(event)); }
        }
        let before = lifecycle.snapshot();

        // Act
        let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {root: DiskRootId::Execution});

        // Assert
        prop_assert!(!accepted);
        prop_assert_eq!(lifecycle.snapshot(), before);
    }

    #[test]
    fn each_failed_safety_component_blocks_destructive_outcome(failed in 0usize..3, failed_outcome in any::<bool>()) {
        // Arrange
        let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
        let events = [
            (DiskLifecycleEvent::ProcessDrainSucceeded, DiskLifecycleEvent::ProcessDrainFailed),
            (DiskLifecycleEvent::OutputDrainSucceeded, DiskLifecycleEvent::OutputDrainFailed),
            (DiskLifecycleEvent::MonitorJoinSucceeded, DiskLifecycleEvent::MonitorJoinFailed),
        ];
        for (index, (success, failure)) in events.into_iter().enumerate() {
            assert!(lifecycle.apply(if index == failed {failure} else {success}));
        }
        assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {root: DiskRootId::Execution}));
        let before = lifecycle.snapshot();
        let outcome = if failed_outcome {DiskCleanupOutcome::Failed("cannot unlink".into())} else {DiskCleanupOutcome::Clean};

        // Act
        let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {root: DiskRootId::Execution, outcome});

        // Assert
        prop_assert!(!accepted);
        prop_assert_eq!(lifecycle.snapshot(), before);
    }

    #[test]
    fn duplicate_completion_is_rejected_without_mutation(kind in 0u8..4, message in ".*") {
        // Arrange
        let mut lifecycle = settled(DiskRootId::Execution);
        assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {root: DiskRootId::Execution}));
        let outcome = match kind {
            0 => DiskCleanupOutcome::Clean,
            1 => DiskCleanupOutcome::Failed(message),
            2 => DiskCleanupOutcome::Deferred(message),
            _ => DiskCleanupOutcome::Retained,
        };
        assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {root: DiskRootId::Execution, outcome}));
        let before = lifecycle.snapshot();

        // Act
        let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {root: DiskRootId::Execution, outcome: DiskCleanupOutcome::Clean});

        // Assert
        prop_assert!(!accepted);
        prop_assert_eq!(lifecycle.snapshot(), before);
    }

    #[test]
    fn deferred_outcome_blocks_finish(message in ".*") {
        // Arrange
        let mut lifecycle = settled(DiskRootId::Execution);
        assert!(lifecycle.apply(DiskLifecycleEvent::ReportSucceeded));
        assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {root: DiskRootId::Execution}));
        assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {root: DiskRootId::Execution, outcome: DiskCleanupOutcome::Deferred(message)}));

        // Act
        let may_finish = lifecycle.may_finish();

        // Assert
        prop_assert!(!may_finish);
    }

    #[test]
    fn updates_preserve_registration_order(delivery_first in any::<bool>()) {
        // Arrange
        let roots = if delivery_first {[DiskRootId::Delivery, DiskRootId::Execution]}
            else {[DiskRootId::Execution, DiskRootId::Delivery]};
        let mut lifecycle = DiskLifecycle::new(roots).unwrap();
        for event in [DiskLifecycleEvent::ProcessDrainSucceeded, DiskLifecycleEvent::OutputDrainSucceeded,
            DiskLifecycleEvent::MonitorJoinSucceeded, DiskLifecycleEvent::ReportSucceeded] {
            assert!(lifecycle.apply(event));
        }
        for root in roots.into_iter().rev() {
            assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {root}));
        }
        let expected = roots.map(|root| DiskRootEntry {root, cleanup: DiskCleanupState::Requested}).to_vec();

        // Act
        let actual = lifecycle.snapshot().roots;

        // Assert
        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn retained_execution_allows_finish() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Execution);
    assert!(lifecycle.apply(DiskLifecycleEvent::ReportSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Execution,
        outcome: DiskCleanupOutcome::Retained
    }));

    // Act
    let may_finish = lifecycle.may_finish();

    // Assert
    assert!(may_finish);
}

#[test]
fn retained_delivery_blocks_finish() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Delivery);
    assert!(lifecycle.apply(DiskLifecycleEvent::ReportSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Delivery
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Delivery,
        outcome: DiskCleanupOutcome::Retained
    }));

    // Act
    let may_finish = lifecycle.may_finish();

    // Assert
    assert!(!may_finish);
}

#[test]
fn pending_report_blocks_delivery_request_without_mutation() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Delivery);
    let before = lifecycle.snapshot();

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Delivery,
    });

    // Assert
    assert!(!accepted);
    assert_eq!(lifecycle.snapshot(), before);
}

#[test]
fn failed_report_allows_delivery_cleanup_request() {
    // Arrange
    let mut lifecycle = settled(DiskRootId::Delivery);
    assert!(lifecycle.apply(DiskLifecycleEvent::ReportFailed));

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Delivery,
    });

    // Assert
    assert!(accepted);
}
