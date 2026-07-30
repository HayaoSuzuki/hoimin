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
    assert!(metrics.stages.is_empty());
    assert!(metrics.workers.is_empty());
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

#[test]
fn metrics_validate_version_identity_names_and_duration_bounds() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.schema_version = 2;
    assert_eq!(
        validation_message(&metrics),
        "unsupported metrics schema version 2"
    );

    metrics.schema_version = METRICS_SCHEMA_VERSION;
    metrics.run_id.clear();
    assert_eq!(validation_message(&metrics), "run ID must not be empty");

    metrics.run_id = "run-1".into();
    metrics.elapsed_ms = 10;
    metrics.stages = vec![
        StageMetric {
            name: "analysis".into(),
            elapsed_ms: 5,
        },
        StageMetric {
            name: "analysis".into(),
            elapsed_ms: 6,
        },
    ];
    assert_eq!(
        validation_message(&metrics),
        "duplicate stage metric analysis"
    );

    metrics.stages = vec![StageMetric {
        name: "analysis".into(),
        elapsed_ms: 11,
    }];
    assert_eq!(
        validation_message(&metrics),
        "stage analysis elapsed time exceeds run elapsed time"
    );

    metrics.stages.clear();
    metrics.workers = vec![WorkerMetric {
        worker: 0,
        busy_ms: 0,
        queue_wait_ms: 11,
        processes: 0,
    }];
    assert_eq!(
        validation_message(&metrics),
        "worker 0 queue wait time exceeds run elapsed time"
    );
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

#[test]
fn metrics_reject_worker_process_counts_below_or_above_executed() {
    for (processes, executed) in [(1, 2), (3, 2)] {
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
