#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use std::num::NonZeroU64;
use std::time::Duration;

use hoimin_core::disk::{
    DISK_MEASUREMENT_FAILED, DiskCleanupOutcome, DiskCleanupState, DiskDecision, DiskLifecycle,
    DiskLifecycleError, DiskLifecycleEvent, DiskObservation, DiskPolicy, DiskRootEntry, DiskRootId,
    DiskStopReason, FILESYSTEM_RESERVE_REACHED, PROCESS_LIFECYCLE_FAILED, WORKSPACE_SIZE_EXCEEDED,
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

    assert_eq!(failure.code(), FILESYSTEM_RESERVE_REACHED);
    assert_eq!(
        failure.reason.kind(),
        DiskStopReason::FilesystemReserveReached
    );
    assert_eq!(
        failure.reason,
        hoimin_core::DiskFailureReason::FilesystemReserveReached(observation(10, 10))
    );
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
    assert_eq!(failure.code(), DISK_MEASUREMENT_FAILED);
    assert_eq!(failure.reason.kind(), DiskStopReason::MeasurementFailed);
    assert!(matches!(
        failure.reason,
        hoimin_core::DiskFailureReason::MeasurementFailed(_)
    ));
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
    assert_eq!(failure.reason.kind(), DiskStopReason::MeasurementFailed);
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
    assert_eq!(failure.code(), PROCESS_LIFECYCLE_FAILED);
    assert_eq!(failure.reason.kind(), DiskStopReason::ProcessFailed);

    let mut disk_first = DiskLifecycle::new([]).unwrap();
    assert!(disk_first.apply(DiskLifecycleEvent::Observation {
        policy: policy(10, 10),
        value: observation(10, 11),
    }));
    assert!(disk_first.apply(DiskLifecycleEvent::ProcessDrainFailed));
    let failure = disk_first.snapshot().stop.unwrap();
    assert_eq!(failure.reason.kind(), DiskStopReason::WorkspaceSizeExceeded);
    assert_eq!(failure.secondary.len(), 1);
    assert_eq!(failure.secondary[0].code(), PROCESS_LIFECYCLE_FAILED);
}

#[rstest::rstest]
#[case::diskrootid_execution(DiskRootId::Execution)]
#[case::diskrootid_delivery(DiskRootId::Delivery)]
fn duplicate_roots_are_rejected(#[case] root: DiskRootId) {
    // Arrange
    let roots = [root, root];

    // Act
    let result = DiskLifecycle::new(roots);

    // Assert
    assert_eq!(result, Err(DiskLifecycleError::DuplicateRoot(root)));
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
fn deferred_cleanup_retains_its_outcome() {
    // Arrange
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    settle_safety(&mut lifecycle);
    cleanup(
        &mut lifecycle,
        DiskRootId::Execution,
        DiskCleanupOutcome::Deferred("lease remains active".into()),
    );
    let expected = vec![DiskRootEntry {
        root: DiskRootId::Execution,
        cleanup: DiskCleanupState::Completed(DiskCleanupOutcome::Deferred(
            "lease remains active".into(),
        )),
    }];

    // Act
    let actual = lifecycle.snapshot().roots;

    // Assert
    assert_eq!(actual, expected);
}

#[test]
fn deferred_cleanup_records_its_secondary_error() {
    let mut lifecycle = DiskLifecycle::new([DiskRootId::Execution]).unwrap();
    settle_safety(&mut lifecycle);
    let expected = vec![hoimin_core::DiskSecondary::Error {
        code: hoimin_core::WORKSPACE_CLEANUP_DEFERRED.into(),
        message: "lease remains active".into(),
    }];

    cleanup(
        &mut lifecycle,
        DiskRootId::Execution,
        DiskCleanupOutcome::Deferred("lease remains active".into()),
    );

    let actual = lifecycle.snapshot().secondary;

    assert_eq!(actual, expected);
}

#[test]
fn cleanup_request_without_owned_root_is_rejected_without_mutation() {
    // Arrange
    let mut lifecycle = DiskLifecycle::new([]).unwrap();
    settle_safety(&mut lifecycle);
    let before = lifecycle.snapshot();

    // Act
    let accepted = lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
        root: DiskRootId::Execution,
    });

    // Assert
    assert!(!accepted);
    assert_eq!(lifecycle.snapshot(), before);
}

#[rstest::rstest]
#[case::none(0)]
#[case::one(1)]
#[case::two(2)]
fn zero_one_and_two_active_work_items_are_drained_globally(#[case] dispatches: u64) {
    let mut lifecycle = DiskLifecycle::new([]).unwrap();
    for _ in 0..dispatches {
        assert!(lifecycle.apply(DiskLifecycleEvent::DispatchRequested));
    }
    assert!(lifecycle.apply(DiskLifecycleEvent::ProcessDrainSucceeded));
    let snapshot = lifecycle.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.dispatched, dispatches);
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

#[test]
fn legacy_disk_failure_wire_is_rejected() {
    // Arrange: the obsolete independent fields could contradict one another.
    let legacy = serde_json::json!({
        "code": "wrong", "reason": "measurement_failed",
        "observation": null, "message": null, "secondary": []
    });

    // Act.
    let result = serde_json::from_value::<hoimin_core::DiskFailure>(legacy);

    // Assert.
    assert!(result.is_err());
}

macro_rules! typed_failure_evidence_matches_its_variant_case {
    ($name:ident, $index:expr) => {
        proptest::proptest! {

            #[test]
            fn $name(
        owned in proptest::prelude::any::<u64>(),
        available in proptest::prelude::any::<u64>(),
        nanos in proptest::prelude::any::<u64>(),
        message in ".*",
    ) {
        // Arrange.
        use hoimin_core::{DiskFailure, DiskFailureReason, DiskSecondary};
        let value = DiskObservation { owned_bytes: owned, available_bytes: available,
            measured_in: Duration::from_nanos(nanos) };
        let cases = [
            (DiskFailureReason::WorkspaceSizeExceeded(value),
                DiskSecondary::Observation { reason: DiskStopReason::WorkspaceSizeExceeded, value }),
            (DiskFailureReason::FilesystemReserveReached(value),
                DiskSecondary::Observation { reason: DiskStopReason::FilesystemReserveReached, value }),
            (DiskFailureReason::MeasurementFailed(message.clone()),
                DiskSecondary::Error { code: DISK_MEASUREMENT_FAILED.into(), message: message.clone() }),
            (DiskFailureReason::ProcessFailed(message.clone()),
                DiskSecondary::Error { code: PROCESS_LIFECYCLE_FAILED.into(), message }),
        ];

        let (reason, evidence) = cases.into_iter().nth($index).unwrap();

        let failure = DiskFailure { reason, secondary: vec![] };
        // Act.
        let actual = failure.reason.to_secondary();
        // Assert.
        proptest::prop_assert_eq!(actual, evidence);

            }
        }
    };
}

typed_failure_evidence_matches_its_variant_case!(
    typed_failure_evidence_matches_its_variant_workspace,
    0
);
typed_failure_evidence_matches_its_variant_case!(
    typed_failure_evidence_matches_its_variant_reserve,
    1
);
typed_failure_evidence_matches_its_variant_case!(
    typed_failure_evidence_matches_its_variant_measurement,
    2
);
typed_failure_evidence_matches_its_variant_case!(
    typed_failure_evidence_matches_its_variant_process,
    3
);

macro_rules! typed_failure_wire_round_trips_required_payload_case {
    ($name:ident, $index:expr) => {
        proptest::proptest! {

                #[test]
                fn $name(
            owned in proptest::prelude::any::<u64>(),
            available in proptest::prelude::any::<u64>(),
            nanos in proptest::prelude::any::<u64>(),
            message in ".*",
        ) {
            // Arrange.
            use hoimin_core::{DiskFailure, DiskFailureReason};
            let value = DiskObservation { owned_bytes: owned, available_bytes: available,
                measured_in: Duration::from_nanos(nanos) };

            let reason = [DiskFailureReason::WorkspaceSizeExceeded(value),
                DiskFailureReason::FilesystemReserveReached(value),
                DiskFailureReason::MeasurementFailed(message.clone()),
                DiskFailureReason::ProcessFailed(message)].into_iter().nth($index).unwrap();

            let failure = DiskFailure { reason, secondary: vec![] };
            // Act.
            let restored = serde_json::from_value::<DiskFailure>(
                serde_json::to_value(&failure).unwrap()).unwrap();
            // Assert.
            proptest::prop_assert_eq!(restored, failure);

                }
            }
    };
}

typed_failure_wire_round_trips_required_payload_case!(
    typed_failure_wire_round_trips_required_payload_workspace,
    0
);
typed_failure_wire_round_trips_required_payload_case!(
    typed_failure_wire_round_trips_required_payload_reserve,
    1
);
typed_failure_wire_round_trips_required_payload_case!(
    typed_failure_wire_round_trips_required_payload_measurement,
    2
);
typed_failure_wire_round_trips_required_payload_case!(
    typed_failure_wire_round_trips_required_payload_process,
    3
);

#[rstest::rstest]
#[case::serde_json_json_reason_measurement_failed_null_secondary_1(0)]
#[case::serde_json_json_reason_process_failed_null_secondary_2(1)]
#[case::serde_json_json_reason_workspace_size_exceeded_null_secondary_3(2)]
#[case::serde_json_json_reason_filesystem_reserve_reached_null_secondary_4(3)]
fn typed_failure_wire_rejects_missing_required_payloads(#[case] scenario_index: usize) {
    // Arrange: every stop reason requires its associated payload.
    let cases = [
        serde_json::json!({"reason": {"measurement_failed": null}, "secondary": []}),
        serde_json::json!({"reason": {"process_failed": null}, "secondary": []}),
        serde_json::json!({"reason": {"workspace_size_exceeded": null}, "secondary": []}),
        serde_json::json!({"reason": {"filesystem_reserve_reached": null}, "secondary": []}),
    ];

    let wire = cases.into_iter().nth(scenario_index).unwrap();

    // Act.
    let result = serde_json::from_value::<hoimin_core::DiskFailure>(wire);
    // Assert.
    assert!(result.is_err());
}

#[rstest::rstest]
#[case::diskfailurereason_workspacesizeexceeded_value_1(0)]
#[case::diskfailurereason_filesystemreservereached_value_2(1)]
#[case::diskfailurereason_measurementfailed_measure_into_3(2)]
#[case::diskfailurereason_processfailed_drain_into_4(3)]
fn report_projection_preserves_each_variant_payload(#[case] scenario_index: usize) {
    // Arrange.
    use hoimin_core::{DiskFailure, DiskFailureReason, DiskStopReport};
    let value = observation(20, 7);
    let cases = [
        (
            DiskFailureReason::WorkspaceSizeExceeded(value),
            WORKSPACE_SIZE_EXCEEDED,
            Some(20),
            Some(7),
            None,
        ),
        (
            DiskFailureReason::FilesystemReserveReached(value),
            FILESYSTEM_RESERVE_REACHED,
            Some(20),
            Some(7),
            None,
        ),
        (
            DiskFailureReason::MeasurementFailed("measure".into()),
            DISK_MEASUREMENT_FAILED,
            None,
            None,
            Some("measure"),
        ),
        (
            DiskFailureReason::ProcessFailed("drain".into()),
            PROCESS_LIFECYCLE_FAILED,
            None,
            None,
            Some("drain"),
        ),
    ];

    let (reason, code, owned_bytes, available_bytes, message) =
        cases.into_iter().nth(scenario_index).unwrap();

    let secondary = Vec::new();
    let failure = DiskFailure {
        reason,
        secondary: secondary.clone(),
    };
    let expected = DiskStopReport {
        code: code.into(),
        owned_bytes,
        available_bytes,
        message: message.map(str::to_owned),
        secondary,
    };
    // Act.
    let actual = DiskStopReport::from(failure);
    // Assert.
    assert_eq!(actual, expected);
}

proptest::proptest! {
    #[test]
    fn secondary_observation_deduplication_includes_measurement_duration(
        nanos in 0_u64..u64::MAX,
    ) {
        // Arrange: a later observation differs only in measurement duration.
        let mut lifecycle = DiskLifecycle::new([]).unwrap();
        let first = DiskObservation { measured_in: Duration::from_nanos(nanos), ..observation(10, 11) };
        let later = DiskObservation { measured_in: Duration::from_nanos(nanos + 1), ..first };
        lifecycle.apply(DiskLifecycleEvent::Observation { policy: policy(10, 10), value: first });
        // Act: repeat both primary and secondary evidence.
        for value in [first, later, later] {
            lifecycle.apply(DiskLifecycleEvent::Observation { policy: policy(10, 10), value });
        }
        // Assert: only the distinct complete observation survives.
        proptest::prop_assert_eq!(lifecycle.snapshot().secondary, vec![hoimin_core::DiskSecondary::Observation {
            reason: DiskStopReason::WorkspaceSizeExceeded, value: later,
        }]);
    }
}

#[test]
fn typed_failure_wire_rejects_an_independent_code() {
    // Arrange: a valid typed reason cannot also supply an independent code.
    let wire = serde_json::json!({
        "reason": {"measurement_failed": "failed"}, "code": "wrong", "secondary": []
    });
    // Act.
    let result = serde_json::from_value::<hoimin_core::DiskFailure>(wire);
    // Assert.
    assert!(result.is_err());
}

#[rstest::rstest]
#[case::diskfailurereason_workspacesizeexceeded_value_1(0)]
#[case::diskfailurereason_filesystemreservereached_value_2(1)]
#[case::diskfailurereason_measurementfailed_measure_into_3(2)]
#[case::diskfailurereason_processfailed_drain_into_4(3)]
fn failure_code_derives_from_its_variant(#[case] scenario_index: usize) {
    // Arrange.
    use hoimin_core::{DiskFailure, DiskFailureReason};
    let value = observation(20, 7);
    let cases = [
        (
            DiskFailureReason::WorkspaceSizeExceeded(value),
            WORKSPACE_SIZE_EXCEEDED,
        ),
        (
            DiskFailureReason::FilesystemReserveReached(value),
            FILESYSTEM_RESERVE_REACHED,
        ),
        (
            DiskFailureReason::MeasurementFailed("measure".into()),
            DISK_MEASUREMENT_FAILED,
        ),
        (
            DiskFailureReason::ProcessFailed("drain".into()),
            PROCESS_LIFECYCLE_FAILED,
        ),
    ];

    let (reason, expected) = cases.into_iter().nth(scenario_index).unwrap();

    let failure = DiskFailure {
        reason,
        secondary: vec![],
    };
    // Act.
    let actual = failure.code();
    // Assert.
    assert_eq!(actual, expected);
}

#[test]
fn report_projection_retains_independent_secondary_diagnostics() {
    // Arrange.
    use hoimin_core::{DiskFailure, DiskFailureReason, DiskSecondary, DiskStopReport};
    let secondary = vec![
        DiskSecondary::Error {
            code: "cleanup.failed".into(),
            message: "retained".into(),
        },
        DiskSecondary::Error {
            code: "report.failed".into(),
            message: "delivery".into(),
        },
    ];
    let failure = DiskFailure {
        reason: DiskFailureReason::MeasurementFailed("measurement".into()),
        secondary: secondary.clone(),
    };
    // Act.
    let report = DiskStopReport::from(failure);
    // Assert.
    assert_eq!(report.secondary, secondary);
}
