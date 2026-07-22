use hoimin_core::{METRICS_SCHEMA_VERSION, RunMetrics, StageMetric, WorkerMetric};

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

    assert_eq!(metrics.validate().unwrap_err(), "duplicate worker metric 0");
}

#[test]
fn metrics_require_workers_in_ascending_order() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.workers = vec![WorkerMetric::new(1), WorkerMetric::new(0)];

    assert_eq!(
        metrics.validate().unwrap_err(),
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
        metrics.validate().unwrap_err(),
        "worker 0 busy time exceeds run elapsed time"
    );
}

#[test]
fn metrics_validate_version_identity_names_and_duration_bounds() {
    let mut metrics = RunMetrics::empty("run-1");
    metrics.schema_version = 2;
    assert_eq!(
        metrics.validate().unwrap_err(),
        "unsupported metrics schema version 2"
    );

    metrics.schema_version = METRICS_SCHEMA_VERSION;
    metrics.run_id.clear();
    assert_eq!(metrics.validate().unwrap_err(), "run ID must not be empty");

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
        metrics.validate().unwrap_err(),
        "duplicate stage metric analysis"
    );

    metrics.stages = vec![StageMetric {
        name: "analysis".into(),
        elapsed_ms: 11,
    }];
    assert_eq!(
        metrics.validate().unwrap_err(),
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
        metrics.validate().unwrap_err(),
        "worker 0 queue wait time exceeds run elapsed time"
    );
}
