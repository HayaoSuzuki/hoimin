use std::num::NonZeroU64;
use std::time::Duration;

use hoimin_core::disk::{
    DISK_MEASUREMENT_FAILED, DiskCleanupOutcome, DiskDecision, DiskLifecycle, DiskLifecycleError,
    DiskLifecycleEvent, DiskObservation, DiskPolicy, DiskRootId, DiskStopReason,
    FILESYSTEM_RESERVE_REACHED, PROCESS_LIFECYCLE_FAILED, WORKSPACE_SIZE_EXCEEDED,
};

fn policy(max_owned_bytes: u64, min_free_bytes: u64) -> DiskPolicy {
    DiskPolicy {
        max_owned_bytes: NonZeroU64::new(max_owned_bytes).unwrap(),
        min_free_bytes: NonZeroU64::new(min_free_bytes).unwrap(),
    }
}

fn observation(owned_bytes: u64, available_bytes: u64) -> DiskObservation {
    DiskObservation {
        owned_bytes,
        available_bytes,
        measured_in: Duration::from_millis(7),
    }
}

#[test]
fn inclusive_boundaries_prefer_reserve_and_retain_size_as_secondary() {
    let decision = policy(10, 10).evaluate(observation(10, 10));

    let DiskDecision::Stop(failure) = decision else {
        panic!("both inclusive boundaries must stop");
    };
    assert_eq!(failure.code, FILESYSTEM_RESERVE_REACHED);
    assert_eq!(failure.reason, DiskStopReason::FilesystemReserveReached);
    assert_eq!(failure.observation, Some(observation(10, 10)));
    assert_eq!(failure.secondary.len(), 1);
    assert_eq!(failure.secondary[0].code(), WORKSPACE_SIZE_EXCEEDED);
}

#[test]
fn one_byte_below_each_boundary_continues() {
    assert_eq!(
        policy(10, 10).evaluate(observation(9, 11)),
        DiskDecision::Continue
    );
}

#[test]
fn measurement_failure_uses_stable_code_without_an_observation() {
    let mut lifecycle = DiskLifecycle::new([]).unwrap();

    assert!(lifecycle.apply(DiskLifecycleEvent::MeasurementFailed {
        message: "statvfs failed".into(),
    }));

    let failure = lifecycle.snapshot().stop.unwrap();
    assert_eq!(failure.code, DISK_MEASUREMENT_FAILED);
    assert_eq!(failure.reason, DiskStopReason::MeasurementFailed);
    assert_eq!(failure.observation, None);
}

#[test]
fn first_stop_is_sticky_and_only_exact_secondary_evidence_is_deduplicated() {
    let mut lifecycle = DiskLifecycle::new([]).unwrap();
    assert!(lifecycle.apply(DiskLifecycleEvent::MeasurementFailed {
        message: "first".into(),
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::Observation {
        policy: policy(10, 10),
        value: observation(10, 10),
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::MeasurementFailed {
        message: "duplicate".into(),
    }));

    let snapshot = lifecycle.snapshot();
    let failure = snapshot.stop.unwrap();
    assert_eq!(failure.reason, DiskStopReason::MeasurementFailed);
    assert_eq!(
        failure.secondary,
        vec![
            hoimin_core::DiskSecondary::Observation {
                reason: DiskStopReason::FilesystemReserveReached,
                value: observation(10, 10),
            },
            hoimin_core::DiskSecondary::Observation {
                reason: DiskStopReason::WorkspaceSizeExceeded,
                value: observation(10, 10),
            },
            hoimin_core::DiskSecondary::Error {
                code: DISK_MEASUREMENT_FAILED.into(),
                message: "duplicate".into(),
            },
        ]
    );

    assert!(lifecycle.apply(DiskLifecycleEvent::MeasurementFailed {
        message: "duplicate".into(),
    }));
    assert_eq!(lifecycle.snapshot().secondary.len(), 3);
}

#[test]
fn stopped_lifecycle_rejects_dispatch_without_incrementing_counters() {
    let mut lifecycle = DiskLifecycle::new([]).unwrap();
    assert!(lifecycle.apply(DiskLifecycleEvent::Observation {
        policy: policy(10, 10),
        value: observation(10, 11),
    }));

    assert!(!lifecycle.apply(DiskLifecycleEvent::DispatchRequested));
    let snapshot = lifecycle.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.dispatched, 0);
}

#[test]
fn process_drain_failure_is_a_sticky_primary_or_secondary_stop() {
    let mut process_first = DiskLifecycle::new([]).unwrap();
    assert!(process_first.apply(DiskLifecycleEvent::ProcessDrainFailed));
    let failure = process_first.snapshot().stop.unwrap();
    assert_eq!(failure.code, PROCESS_LIFECYCLE_FAILED);
    assert_eq!(failure.reason, DiskStopReason::ProcessFailed);

    let mut disk_first = DiskLifecycle::new([]).unwrap();
    assert!(disk_first.apply(DiskLifecycleEvent::Observation {
        policy: policy(10, 10),
        value: observation(10, 11),
    }));
    assert!(disk_first.apply(DiskLifecycleEvent::ProcessDrainFailed));
    let failure = disk_first.snapshot().stop.unwrap();
    assert_eq!(failure.reason, DiskStopReason::WorkspaceSizeExceeded);
    assert_eq!(failure.secondary.len(), 1);
    assert_eq!(failure.secondary[0].code(), PROCESS_LIFECYCLE_FAILED);
}

#[test]
fn duplicate_roots_and_duplicate_cleanup_requests_are_rejected() {
    assert_eq!(
        DiskLifecycle::new([DiskRootId::Execution, DiskRootId::Execution]),
        Err(DiskLifecycleError::DuplicateRoot(DiskRootId::Execution))
    );

    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    settle_safety(&mut lifecycle);
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    }));
    assert!(!lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    }));
    assert_eq!(
        lifecycle.snapshot().cleanup_requested,
        vec![DiskRootId::Execution]
    );
}

#[test]
fn cleanup_requires_settlement_and_destructive_outcomes_require_success() {
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    assert!(!lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::ProcessDrainFailed));
    assert!(lifecycle.apply(DiskLifecycleEvent::OutputDrainSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::MonitorJoinSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    }));
    assert!(!lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Execution,
        outcome: DiskCleanupOutcome::Clean,
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Execution,
        outcome: DiskCleanupOutcome::Retained,
    }));
}

#[test]
fn finish_requires_report_success_and_clean_delivery_root() {
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution, DiskRootId::Delivery]).unwrap();
    settle_safety(&mut lifecycle);
    cleanup(
        &mut lifecycle,
        DiskRootId::Execution,
        DiskCleanupOutcome::Clean,
    );
    assert!(lifecycle.apply(DiskLifecycleEvent::ReportSucceeded));
    cleanup(
        &mut lifecycle,
        DiskRootId::Delivery,
        DiskCleanupOutcome::Failed("unlink failed".into()),
    );

    assert!(!lifecycle.may_finish());
    assert!(!lifecycle.apply(DiskLifecycleEvent::FinishRequested));
}

#[test]
fn report_failure_blocks_finish_after_all_other_obligations_settle() {
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    settle_safety(&mut lifecycle);
    cleanup(
        &mut lifecycle,
        DiskRootId::Execution,
        DiskCleanupOutcome::Clean,
    );
    assert!(lifecycle.apply(DiskLifecycleEvent::ReportFailed));

    assert!(!lifecycle.may_finish());
    assert!(!lifecycle.apply(DiskLifecycleEvent::FinishRequested));
}

#[test]
fn deferred_cleanup_is_incomplete_and_records_a_stable_secondary_error() {
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    settle_safety(&mut lifecycle);
    cleanup(
        &mut lifecycle,
        DiskRootId::Execution,
        DiskCleanupOutcome::Deferred("lease remains active".into()),
    );
    assert!(lifecycle.apply(DiskLifecycleEvent::ReportSucceeded));

    let snapshot = lifecycle.snapshot();
    assert_eq!(snapshot.cleanup_deferred, vec![DiskRootId::Execution]);
    assert_eq!(
        snapshot.secondary[0].code(),
        hoimin_core::WORKSPACE_CLEANUP_DEFERRED
    );
    assert!(!lifecycle.may_finish());
}

#[test]
fn cleanup_outcome_without_owned_requested_root_is_rejected() {
    let mut lifecycle = DiskLifecycle::new([]).unwrap();
    settle_safety(&mut lifecycle);

    assert!(!lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    }));
    assert!(!lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Execution,
        outcome: DiskCleanupOutcome::Failed("not owned".into()),
    }));
    assert!(lifecycle.snapshot().cleanup_failed.is_empty());
}

#[test]
fn zero_one_and_two_active_work_items_are_drained_globally() {
    for dispatches in 0..=2 {
        let mut lifecycle = DiskLifecycle::new([]).unwrap();
        for _ in 0..dispatches {
            assert!(lifecycle.apply(DiskLifecycleEvent::DispatchRequested));
        }
        assert!(lifecycle.apply(DiskLifecycleEvent::ProcessDrainSucceeded));
        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.active, 0);
        assert_eq!(snapshot.dispatched, dispatches);
    }
}

#[test]
fn settled_process_drain_is_terminal_for_dispatch_and_cleanup_never_races_active_work() {
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    settle_safety(&mut lifecycle);

    assert!(!lifecycle.apply(DiskLifecycleEvent::DispatchRequested));
    assert_eq!(lifecycle.snapshot().active, 0);
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    }));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
        root: DiskRootId::Execution,
        outcome: DiskCleanupOutcome::Clean,
    }));
}

fn settle_safety(lifecycle: &mut DiskLifecycle) {
    assert!(lifecycle.apply(DiskLifecycleEvent::ProcessDrainSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::OutputDrainSucceeded));
    assert!(lifecycle.apply(DiskLifecycleEvent::MonitorJoinSucceeded));
}

fn cleanup(lifecycle: &mut DiskLifecycle, root: DiskRootId, outcome: DiskCleanupOutcome) {
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupRequested { root }));
    assert!(lifecycle.apply(DiskLifecycleEvent::CleanupCompleted { root, outcome }));
}
