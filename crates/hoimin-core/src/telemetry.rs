use std::collections::HashSet;
use thiserror::Error;

pub const METRICS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MetricsValidationError {
    #[error("{0}")]
    Invalid(String),
    #[error("executed process count {executed} exceeds discovered candidate count {discovered}")]
    ExecutedExceedsDiscovered { discovered: u64, executed: u64 },
    #[error("worker process count overflow")]
    WorkerProcessOverflow,
    #[error(
        "worker process count {worker_processes} does not match executed process count {executed}"
    )]
    WorkerProcessMismatch {
        worker_processes: u64,
        executed: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StageMetric {
    pub name: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkerMetric {
    pub worker: u32,
    pub busy_ms: u64,
    pub queue_wait_ms: u64,
    pub processes: u64,
}

impl WorkerMetric {
    #[must_use]
    pub const fn new(worker: u32) -> Self {
        Self {
            worker,
            busy_ms: 0,
            queue_wait_ms: 0,
            processes: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunMetrics {
    pub schema_version: u32,
    pub run_id: String,
    pub elapsed_ms: u64,
    pub stages: Vec<StageMetric>,
    pub workers: Vec<WorkerMetric>,
    pub discovered: u64,
    pub executed: u64,
}

impl RunMetrics {
    #[must_use]
    pub fn empty(run_id: impl Into<String>) -> Self {
        Self {
            schema_version: METRICS_SCHEMA_VERSION,
            run_id: run_id.into(),
            elapsed_ms: 0,
            stages: Vec::new(),
            workers: Vec::new(),
            discovered: 0,
            executed: 0,
        }
    }

    /// # Errors
    ///
    /// Returns an error when the metrics violate the telemetry contract.
    pub fn validate(&self) -> Result<(), MetricsValidationError> {
        if self.schema_version != METRICS_SCHEMA_VERSION {
            return Err(MetricsValidationError::Invalid(format!(
                "unsupported metrics schema version {}",
                self.schema_version
            )));
        }
        if self.run_id.is_empty() {
            return Err(MetricsValidationError::Invalid(
                "run ID must not be empty".into(),
            ));
        }

        let mut stage_names = HashSet::with_capacity(self.stages.len());
        for stage in &self.stages {
            if !stage_names.insert(&stage.name) {
                return Err(MetricsValidationError::Invalid(format!(
                    "duplicate stage metric {}",
                    stage.name
                )));
            }
            if stage.elapsed_ms > self.elapsed_ms {
                return Err(MetricsValidationError::Invalid(format!(
                    "stage {} elapsed time exceeds run elapsed time",
                    stage.name
                )));
            }
        }

        let mut worker_ids = HashSet::with_capacity(self.workers.len());
        for worker in &self.workers {
            if !worker_ids.insert(worker.worker) {
                return Err(MetricsValidationError::Invalid(format!(
                    "duplicate worker metric {}",
                    worker.worker
                )));
            }
            if worker.busy_ms > self.elapsed_ms {
                return Err(MetricsValidationError::Invalid(format!(
                    "worker {} busy time exceeds run elapsed time",
                    worker.worker
                )));
            }
            if worker.queue_wait_ms > self.elapsed_ms {
                return Err(MetricsValidationError::Invalid(format!(
                    "worker {} queue wait time exceeds run elapsed time",
                    worker.worker
                )));
            }
            if worker
                .busy_ms
                .checked_add(worker.queue_wait_ms)
                .is_none_or(|accounted_ms| accounted_ms > self.elapsed_ms)
            {
                return Err(MetricsValidationError::Invalid(format!(
                    "worker {} combined busy and queue wait time exceeds run elapsed time",
                    worker.worker
                )));
            }
        }
        if !self
            .workers
            .windows(2)
            .all(|workers| workers[0].worker < workers[1].worker)
        {
            return Err(MetricsValidationError::Invalid(
                "worker metrics must be ordered by worker ID".into(),
            ));
        }
        if self.executed > self.discovered {
            return Err(MetricsValidationError::ExecutedExceedsDiscovered {
                discovered: self.discovered,
                executed: self.executed,
            });
        }
        let worker_processes = self
            .workers
            .iter()
            .try_fold(0_u64, |total, worker| total.checked_add(worker.processes))
            .ok_or(MetricsValidationError::WorkerProcessOverflow)?;
        if worker_processes != self.executed {
            return Err(MetricsValidationError::WorkerProcessMismatch {
                worker_processes,
                executed: self.executed,
            });
        }

        Ok(())
    }
}
