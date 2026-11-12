#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use hoimin_core::{
    METRICS_SCHEMA_VERSION, MetricsValidationError, RunMetrics, StageMetric, WorkerMetric,
};

fn validation_message(metrics: &RunMetrics) -> String {
    metrics.validate().unwrap_err().to_string()
}

#[test]
fn metrics_are_versioned_and_validate_worker_accounting() {
    let metrics = RunMetrics {
        schema_version: METRICS_SCHEMA_VERSION,
        run_id: "run-1".into(),
        elapsed_ms: 20,
        stages: vec![StageMetric {
            name: "mutation_execution".into(),
            elapsed_ms: 5,
        }],
        workers: vec![WorkerMetric {
            worker: 0,
            busy_ms: 10,
            queue_wait_ms: 3,
            processes: 2,
        }],
        discovered: 4,
        executed: 2,
    };

    assert!(metrics.validate().is_ok());
    let value = serde_json::to_value(metrics).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["stages"][0]["name"], "mutation_execution");
    assert_eq!(value["elapsed_ms"], 20);
    assert_eq!(value["stages"][0]["elapsed_ms"], 5);
    assert_eq!(value["workers"][0]["busy_ms"], 10);
    assert_eq!(value["workers"][0]["queue_wait_ms"], 3);
}

#[test]
fn empty_metrics_use_the_current_schema_and_zero_counters() {
    let metrics = RunMetrics::empty("run-1");

    assert_eq!(metrics.schema_version, METRICS_SCHEMA_VERSION);
    assert_eq!(metrics.run_id, "run-1");
    assert_eq!(metrics.elapsed_ms, 0);
    assert_eq!(metrics.stages, Vec::new());
    assert_eq!(metrics.workers, Vec::new());
    assert_eq!(metrics.discovered, 0);
    assert_eq!(metrics.executed, 0);
}

#[test]
fn worker_metrics_start_with_zero_counters() {
    assert_eq!(
        WorkerMetric::new(7),
        WorkerMetric {
            worker: 7,
            busy_ms: 0,
            queue_wait_ms: 0,
            processes: 0,
        }
    );
}

#[test]
fn metrics_reject_duplicate_workers() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.workers = vec![WorkerMetric::new(0), WorkerMetric::new(0)];

    assert_eq!(validation_message(&metrics), "duplicate worker metric 0");
}

#[test]
fn metrics_require_workers_in_ascending_order() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.workers = vec![WorkerMetric::new(1), WorkerMetric::new(0)];

    assert_eq!(
        validation_message(&metrics),
        "worker metrics must be ordered by worker ID"
    );
}

#[test]
fn metrics_reject_busy_time_above_run_elapsed_time() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.elapsed_ms = 10;
    metrics.workers = vec![WorkerMetric {
        worker: 0,
        busy_ms: 11,
        queue_wait_ms: 0,
        processes: 1,
    }];

    assert_eq!(
        validation_message(&metrics),
        "worker 0 busy time exceeds run elapsed time"
    );
}

#[rstest::rstest]
#[case::value_10(10, 6, 5)]
#[case::value_10_2(10, 10, 10)]
#[case::u64_max(u64::MAX, u64::MAX, 1)]
#[case::u64_max_4(u64::MAX, u64::MAX, u64::MAX)]
fn metrics_reject_combined_worker_time_above_run_elapsed_time(
    #[case] elapsed_ms: u64,
    #[case] busy_ms: u64,
    #[case] queue_wait_ms: u64,
) {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.elapsed_ms = elapsed_ms;
    metrics.workers = vec![WorkerMetric {
        busy_ms,
        queue_wait_ms,
        ..WorkerMetric::new(7)
    }];

    assert_eq!(
        validation_message(&metrics),
        "worker 7 combined busy and queue wait time exceeds run elapsed time"
    );
}

#[rstest::rstest]
#[case::value_0(0, 0, 0)]
#[case::value_10(10, 6, 4)]
#[case::value_10_3(10, 0, 10)]
#[case::value_10_4(10, 10, 0)]
#[case::u64_max(u64::MAX, u64::MAX - 1, 1)]
fn metrics_accept_combined_worker_time_at_the_run_boundary(
    #[case] elapsed_ms: u64,
    #[case] busy_ms: u64,
    #[case] queue_wait_ms: u64,
) {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.elapsed_ms = elapsed_ms;
    metrics.workers = vec![WorkerMetric {
        busy_ms,
        queue_wait_ms,
        ..WorkerMetric::new(7)
    }];

    assert_eq!(metrics.validate(), Ok(()));
}

#[test]
fn metrics_allow_parallel_workers_to_share_the_same_run_interval() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.elapsed_ms = 10;
    metrics.workers = vec![
        WorkerMetric {
            busy_ms: 6,
            queue_wait_ms: 4,
            ..WorkerMetric::new(0)
        },
        WorkerMetric {
            busy_ms: 7,
            queue_wait_ms: 3,
            ..WorkerMetric::new(1)
        },
    ];

    assert_eq!(metrics.validate(), Ok(()));
}

#[rstest::rstest]
#[case::unsupported_version(RunMetrics { schema_version: 2, ..RunMetrics::empty("run-1") }, "unsupported metrics schema version 2")]
#[case::empty_identity(RunMetrics::empty(""), "run ID must not be empty")]
#[case::duplicate_stages(RunMetrics { elapsed_ms: 10, stages: vec![StageMetric { name: "analysis".into(), elapsed_ms: 5 }, StageMetric { name: "analysis".into(), elapsed_ms: 6 }], ..RunMetrics::empty("run-1") }, "duplicate stage metric analysis")]
#[case::stage_exceeds_elapsed(RunMetrics { elapsed_ms: 10, stages: vec![StageMetric { name: "analysis".into(), elapsed_ms: 11 }], ..RunMetrics::empty("run-1") }, "stage analysis elapsed time exceeds run elapsed time")]
#[case::queue_exceeds_elapsed(RunMetrics { elapsed_ms: 10, workers: vec![WorkerMetric { queue_wait_ms: 11, ..WorkerMetric::new(0) }], ..RunMetrics::empty("run-1") }, "worker 0 queue wait time exceeds run elapsed time")]
fn metrics_validate_version_identity_names_and_duration_bounds(
    #[case] metrics: RunMetrics,
    #[case] expected: &str,
) {
    let message = validation_message(&metrics);

    assert_eq!(message, expected);
}

proptest::proptest! {
    #[test]
    fn worker_interval_validation_matches_widened_arithmetic(
        elapsed_ms in proptest::prelude::any::<u64>(),
        busy_ms in proptest::prelude::any::<u64>(),
        queue_wait_ms in proptest::prelude::any::<u64>(),
    ) {
        let metrics = RunMetrics {
            elapsed_ms,
            workers: vec![WorkerMetric { busy_ms, queue_wait_ms, ..WorkerMetric::new(0) }],
            ..RunMetrics::empty("run-1")
        };
        let expected = u128::from(busy_ms) + u128::from(queue_wait_ms) <= u128::from(elapsed_ms);

        let result = metrics.validate();

        proptest::prop_assert_eq!(result.is_ok(), expected);
    }
}

#[test]
fn metrics_reject_executing_more_candidates_than_were_discovered() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.discovered = 1;
    metrics.executed = 2;

    assert_eq!(
        metrics.validate(),
        Err(MetricsValidationError::ExecutedExceedsDiscovered {
            discovered: 1,
            executed: 2,
        })
    );
}

#[rstest::rstest]
#[case::value_1(1, 2)]
#[case::value_3(3, 2)]
fn metrics_reject_worker_process_counts_below_or_above_executed(
    #[case] processes: u64,
    #[case] executed: u64,
) {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.discovered = 3;
    metrics.executed = executed;
    metrics.workers = vec![WorkerMetric {
        processes,
        ..WorkerMetric::new(0)
    }];

    assert_eq!(
        metrics.validate(),
        Err(MetricsValidationError::WorkerProcessMismatch {
            worker_processes: processes,
            executed,
        })
    );
}

#[test]
fn metrics_report_worker_process_count_overflow_as_a_typed_error() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.discovered = u64::MAX;
    metrics.executed = u64::MAX;
    metrics.workers = vec![
        WorkerMetric {
            processes: u64::MAX,
            ..WorkerMetric::new(0)
        },
        WorkerMetric {
            processes: 1,
            ..WorkerMetric::new(1)
        },
    ];

    assert_eq!(
        metrics.validate(),
        Err(MetricsValidationError::WorkerProcessOverflow)
    );
}

#[test]
fn metrics_accept_multi_worker_and_zero_work_accounting() {
    assert!(RunMetrics::empty("run-1").validate().is_ok());

    let mut metrics = RunMetrics::empty("run-2");
    metrics.discovered = 5;
    metrics.executed = 3;
    metrics.workers = vec![
        WorkerMetric {
            processes: 1,
            ..WorkerMetric::new(0)
        },
        WorkerMetric {
            processes: 2,
            ..WorkerMetric::new(1)
        },
    ];

    assert!(metrics.validate().is_ok());
}
