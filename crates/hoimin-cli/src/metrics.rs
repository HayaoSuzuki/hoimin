use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hoimin_core::{RunMetrics, StageMetric, WorkerMetric};
use tempfile::NamedTempFile;
use thiserror::Error;

pub(crate) trait Clock {
    fn elapsed(&self) -> Duration;
}

pub(crate) struct MonotonicClock(Instant);

impl MonotonicClock {
    fn new() -> Self {
        Self(Instant::now())
    }
}

impl Clock for MonotonicClock {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

#[derive(Debug, Error)]
pub(crate) enum MetricsError {
    #[error("stage {name} has already started")]
    StageAlreadyStarted { name: String },
    #[error("stage {name} has not started")]
    StageNotStarted { name: String },
    #[error("stage {name} has already finished")]
    StageAlreadyFinished { name: String },
    #[error("worker {worker} already has a queued process")]
    ProcessAlreadyQueued { worker: u32 },
    #[error("worker {worker} has no queued process")]
    ProcessNotQueued { worker: u32 },
    #[error("worker {worker} already has a running process")]
    ProcessAlreadyStarted { worker: u32 },
    #[error("worker {worker} has no running process")]
    ProcessNotStarted { worker: u32 },
    #[error("worker {worker} still has an outstanding process")]
    ProcessOutstanding { worker: u32 },
    #[error("metrics are invalid: {0}")]
    Validation(String),
    #[error("failed to serialize metrics: {0}")]
    Serialization(serde_json::Error),
    #[error("failed to create temporary metrics file in {parent}: {source}")]
    TemporaryFileCreation {
        parent: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write temporary metrics file: {0}")]
    Write(std::io::Error),
    #[error("failed to flush temporary metrics file: {0}")]
    Flush(std::io::Error),
    #[error("failed to sync temporary metrics file: {0}")]
    Sync(std::io::Error),
    #[error("failed to persist metrics to {path}: {source}")]
    Persist {
        path: PathBuf,
        source: std::io::Error,
    },
}

struct StageState {
    started: Duration,
    elapsed_ms: Option<u64>,
}

#[derive(Default)]
struct WorkerState {
    metric: Option<WorkerMetric>,
    queued: Option<Duration>,
    started: Option<Duration>,
}

pub(crate) struct MetricsCollector<C = MonotonicClock> {
    run_id: String,
    clock: C,
    stages: BTreeMap<String, StageState>,
    workers: BTreeMap<u32, WorkerState>,
    discovered: u64,
}

impl MetricsCollector<MonotonicClock> {
    pub(crate) fn new(run_id: impl Into<String>) -> Self {
        Self::with_clock(run_id, MonotonicClock::new())
    }
}

impl<C: Clock> MetricsCollector<C> {
    pub(crate) fn with_clock(run_id: impl Into<String>, clock: C) -> Self {
        Self {
            run_id: run_id.into(),
            clock,
            stages: BTreeMap::new(),
            workers: BTreeMap::new(),
            discovered: 0,
        }
    }

    pub(crate) fn begin_stage(&mut self, name: impl Into<String>) -> Result<(), MetricsError> {
        let name = name.into();
        if self.stages.contains_key(&name) {
            return Err(MetricsError::StageAlreadyStarted { name });
        }
        self.stages.insert(
            name,
            StageState {
                started: self.clock.elapsed(),
                elapsed_ms: None,
            },
        );
        Ok(())
    }

    pub(crate) fn finish_stage(&mut self, name: &str) -> Result<(), MetricsError> {
        let now = self.clock.elapsed();
        let stage = self
            .stages
            .get_mut(name)
            .ok_or_else(|| MetricsError::StageNotStarted { name: name.into() })?;
        if stage.elapsed_ms.is_some() {
            return Err(MetricsError::StageAlreadyFinished { name: name.into() });
        }
        stage.elapsed_ms = Some(duration_ms(now.saturating_sub(stage.started)));
        Ok(())
    }

    pub(crate) fn queued(&mut self, worker: u32) -> Result<(), MetricsError> {
        let state = self.workers.entry(worker).or_default();
        if state.queued.is_some() {
            return Err(MetricsError::ProcessAlreadyQueued { worker });
        }
        if state.started.is_some() {
            return Err(MetricsError::ProcessAlreadyStarted { worker });
        }
        state
            .metric
            .get_or_insert_with(|| WorkerMetric::new(worker));
        state.queued = Some(self.clock.elapsed());
        Ok(())
    }

    pub(crate) fn process_started(&mut self, worker: u32) -> Result<(), MetricsError> {
        let now = self.clock.elapsed();
        let state = self.workers.entry(worker).or_default();
        if state.started.is_some() {
            return Err(MetricsError::ProcessAlreadyStarted { worker });
        }
        let queued = state
            .queued
            .take()
            .ok_or(MetricsError::ProcessNotQueued { worker })?;
        let metric = state
            .metric
            .get_or_insert_with(|| WorkerMetric::new(worker));
        metric.queue_wait_ms = metric
            .queue_wait_ms
            .saturating_add(duration_ms(now.saturating_sub(queued)));
        state.started = Some(now);
        Ok(())
    }

    pub(crate) fn cancel_queued(&mut self, worker: u32) -> Result<(), MetricsError> {
        let state = self.workers.entry(worker).or_default();
        state
            .queued
            .take()
            .ok_or(MetricsError::ProcessNotQueued { worker })?;
        Ok(())
    }

    pub(crate) fn process_finished(
        &mut self,
        worker: u32,
        executed: bool,
    ) -> Result<(), MetricsError> {
        let now = self.clock.elapsed();
        let state = self.workers.entry(worker).or_default();
        let started = state
            .started
            .take()
            .ok_or(MetricsError::ProcessNotStarted { worker })?;
        let metric = state
            .metric
            .get_or_insert_with(|| WorkerMetric::new(worker));
        metric.busy_ms = metric
            .busy_ms
            .saturating_add(duration_ms(now.saturating_sub(started)));
        if executed {
            metric.processes = metric.processes.saturating_add(1);
        }
        Ok(())
    }

    pub(crate) fn discovered(&mut self, count: u64) {
        self.discovered = count;
    }

    pub(crate) fn set_run_id(&mut self, run_id: impl Into<String>) {
        self.run_id = run_id.into();
    }

    pub(crate) fn finish(self, discovered: u64, executed: u64) -> Result<RunMetrics, MetricsError> {
        if let Some((&worker, _)) = self
            .workers
            .iter()
            .find(|(_, state)| state.queued.is_some() || state.started.is_some())
        {
            return Err(MetricsError::ProcessOutstanding { worker });
        }
        let mut metrics = RunMetrics::empty(self.run_id);
        metrics.elapsed_ms = duration_ms(self.clock.elapsed());
        metrics.stages = self
            .stages
            .into_iter()
            .filter_map(|(name, state)| {
                state
                    .elapsed_ms
                    .map(|elapsed_ms| StageMetric { name, elapsed_ms })
            })
            .collect();
        metrics.workers = self
            .workers
            .into_values()
            .filter_map(|state| state.metric)
            .filter(|metric| metric.processes > 0)
            .collect();
        metrics.discovered = discovered.max(self.discovered);
        metrics.executed = executed;
        metrics
            .validate()
            .map_err(|error| MetricsError::Validation(error.to_string()))?;
        Ok(metrics)
    }
}

pub(crate) fn write_metrics(path: &Path, metrics: &RunMetrics) -> Result<(), MetricsError> {
    metrics
        .validate()
        .map_err(|error| MetricsError::Validation(error.to_string()))?;
    let parent = destination_parent(path);
    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|source| MetricsError::TemporaryFileCreation {
            parent: parent.to_path_buf(),
            source,
        })?;
    serde_json::to_writer(temporary.as_file_mut(), metrics).map_err(MetricsError::Serialization)?;
    temporary
        .as_file_mut()
        .write_all(b"\n")
        .map_err(MetricsError::Write)?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(MetricsError::Flush)?;
    temporary.as_file().sync_all().map_err(MetricsError::Sync)?;
    temporary
        .persist(path)
        .map_err(|error| MetricsError::Persist {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    Ok(())
}

fn destination_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    use std::sync::Mutex;

    static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

    struct CurrentDirGuard(PathBuf);

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).expect("restore current directory");
        }
    }

    #[derive(Default)]
    struct FakeClock(Cell<Duration>);

    impl FakeClock {
        fn advance_ms(&self, milliseconds: u64) {
            self.0
                .set(self.0.get() + Duration::from_millis(milliseconds));
        }

        fn set(&self, elapsed: Duration) {
            self.0.set(elapsed);
        }
    }

    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.0.get()
        }
    }

    #[test]
    fn collector_accounts_for_stages_queueing_and_overlapping_workers() {
        let mut collector = MetricsCollector::with_clock("run-1", FakeClock::default());
        collector.begin_stage("analysis").unwrap();
        collector.clock.advance_ms(7);
        collector.finish_stage("analysis").unwrap();
        collector.queued(2).unwrap();
        collector.queued(1).unwrap();
        collector.clock.advance_ms(3);
        collector.process_started(2).unwrap();
        collector.clock.advance_ms(2);
        collector.process_started(1).unwrap();
        collector.clock.advance_ms(9);
        collector.process_finished(2, true).unwrap();
        collector.clock.advance_ms(2);
        collector.process_finished(1, true).unwrap();
        collector.discovered(4);

        let metrics = collector.finish(3, 2).unwrap();

        assert_eq!(metrics.elapsed_ms, 23);
        assert_eq!(metrics.discovered, 4);
        assert_eq!(metrics.executed, 2);
        assert_eq!(metrics.stages[0].elapsed_ms, 7);
        assert_eq!(
            metrics.workers[0],
            WorkerMetric {
                worker: 1,
                busy_ms: 11,
                queue_wait_ms: 5,
                processes: 1
            }
        );
        assert_eq!(
            metrics.workers[1],
            WorkerMetric {
                worker: 2,
                busy_ms: 11,
                queue_wait_ms: 3,
                processes: 1
            }
        );
    }

    #[test]
    fn output_is_sorted_by_stage_name_and_worker_id() {
        let mut collector = MetricsCollector::with_clock("run-1", FakeClock::default());
        collector.begin_stage("zeta").unwrap();
        collector.finish_stage("zeta").unwrap();
        collector.begin_stage("alpha").unwrap();
        collector.finish_stage("alpha").unwrap();
        collector.queued(9).unwrap();
        collector.process_started(9).unwrap();
        collector.process_finished(9, true).unwrap();
        collector.queued(2).unwrap();
        collector.process_started(2).unwrap();
        collector.process_finished(2, true).unwrap();

        let metrics = collector.finish(2, 2).unwrap();
        assert_eq!(
            metrics
                .stages
                .iter()
                .map(|stage| stage.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );
        assert_eq!(
            metrics
                .workers
                .iter()
                .map(|worker| worker.worker)
                .collect::<Vec<_>>(),
            vec![2, 9]
        );
    }

    #[test]
    fn invalid_state_transitions_are_typed_errors() {
        let mut collector = MetricsCollector::with_clock("run-1", FakeClock::default());
        collector.begin_stage("analysis").unwrap();
        collector.finish_stage("analysis").unwrap();
        assert!(matches!(
            collector.finish_stage("analysis"),
            Err(MetricsError::StageAlreadyFinished { .. })
        ));
        assert!(matches!(
            collector.process_finished(7, true),
            Err(MetricsError::ProcessNotStarted { worker: 7 })
        ));
    }

    #[test]
    fn finish_rejects_queued_and_running_processes() {
        let mut queued = MetricsCollector::with_clock("run-1", FakeClock::default());
        queued.queued(3).unwrap();
        assert!(matches!(
            queued.finish(0, 0),
            Err(MetricsError::ProcessOutstanding { worker: 3 })
        ));

        let mut running = MetricsCollector::with_clock("run-1", FakeClock::default());
        running.queued(4).unwrap();
        running.process_started(4).unwrap();
        assert!(matches!(
            running.finish(0, 0),
            Err(MetricsError::ProcessOutstanding { worker: 4 })
        ));
    }

    #[test]
    fn millisecond_conversion_saturates() {
        let collector = MetricsCollector::with_clock("run-1", FakeClock::default());
        collector
            .clock
            .set(Duration::from_millis(u64::MAX) + Duration::from_millis(1));
        assert_eq!(collector.finish(0, 0).unwrap().elapsed_ms, u64::MAX);
    }

    #[test]
    fn writer_replaces_destination_with_valid_json() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        fs::write(&path, "old").unwrap();
        let metrics = MetricsCollector::with_clock("run-1", FakeClock::default())
            .finish(0, 0)
            .unwrap();
        write_metrics(&path, &metrics).unwrap();
        let actual: RunMetrics = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(actual, metrics);
    }

    #[test]
    fn writer_preserves_destination_when_accounting_is_invalid() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        fs::write(&path, "old").unwrap();
        let mut metrics = RunMetrics::empty("run-1");
        metrics.discovered = 1;
        metrics.executed = 2;

        assert!(matches!(
            write_metrics(&path, &metrics),
            Err(MetricsError::Validation(_))
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "old");
    }

    #[test]
    fn writer_accepts_a_bare_relative_destination() {
        let _lock = CURRENT_DIR_LOCK.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(directory.path()).unwrap();
        let _restore = CurrentDirGuard(original);
        let metrics = MetricsCollector::with_clock("run-1", FakeClock::default())
            .finish(0, 0)
            .unwrap();
        write_metrics(Path::new("metrics.json"), &metrics).unwrap();
        let actual: RunMetrics =
            serde_json::from_slice(&fs::read("metrics.json").unwrap()).unwrap();
        assert_eq!(actual, metrics);
    }

    #[test]
    fn bare_relative_destination_uses_the_current_directory_as_parent() {
        assert_eq!(
            destination_parent(Path::new("metrics.json")),
            Path::new(".")
        );
    }

    #[test]
    fn production_collector_uses_a_monotonic_clock() {
        let collector = MetricsCollector::new("run-1");
        std::thread::sleep(Duration::from_millis(2));
        let metrics = collector.finish(0, 0).unwrap();

        assert_eq!(metrics.run_id, "run-1");
        assert!(metrics.elapsed_ms >= 1);
    }

    #[test]
    fn collector_can_adopt_the_runtime_run_id() {
        let mut collector = MetricsCollector::with_clock("pending", FakeClock::default());

        collector.set_run_id("run-1");

        assert_eq!(collector.finish(0, 0).unwrap().run_id, "run-1");
    }
}
