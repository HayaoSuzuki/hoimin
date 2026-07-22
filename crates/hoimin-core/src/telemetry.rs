use std::collections::HashSet;

pub const METRICS_SCHEMA_VERSION: u32 = 1;

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
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != METRICS_SCHEMA_VERSION {
            return Err(format!(
                "unsupported metrics schema version {}",
                self.schema_version
            ));
        }
        if self.run_id.is_empty() {
            return Err("run ID must not be empty".into());
        }

        let mut stage_names = HashSet::with_capacity(self.stages.len());
        for stage in &self.stages {
            if !stage_names.insert(&stage.name) {
                return Err(format!("duplicate stage metric {}", stage.name));
            }
            if stage.elapsed_ms > self.elapsed_ms {
                return Err(format!(
                    "stage {} elapsed time exceeds run elapsed time",
                    stage.name
                ));
            }
        }

        let mut worker_ids = HashSet::with_capacity(self.workers.len());
        for worker in &self.workers {
            if !worker_ids.insert(worker.worker) {
                return Err(format!("duplicate worker metric {}", worker.worker));
            }
            if worker.busy_ms > self.elapsed_ms {
                return Err(format!(
                    "worker {} busy time exceeds run elapsed time",
                    worker.worker
                ));
            }
            if worker.queue_wait_ms > self.elapsed_ms {
                return Err(format!(
                    "worker {} queue wait time exceeds run elapsed time",
                    worker.worker
                ));
            }
        }
        if !self
            .workers
            .windows(2)
            .all(|workers| workers[0].worker < workers[1].worker)
        {
            return Err("worker metrics must be ordered by worker ID".into());
        }

        Ok(())
    }
}
