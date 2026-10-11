use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hoimin_core::{RunMetrics, StageMetric, WorkerMetric};
use std::sync::Arc;

use cap_std::fs::Dir;
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

/// A validated replacement entry in a retained parent directory.
pub(crate) struct MetricsOutput {
    parent: Arc<Dir>,
    name: std::ffi::OsString,
    path: PathBuf,
}

impl MetricsOutput {
    pub(crate) fn from_parent(parent: Arc<Dir>, path: PathBuf) -> std::io::Result<Self> {
        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("missing metrics filename"))?
            .to_owned();
        Ok(Self { parent, name, path })
    }

    #[cfg(test)]
    pub(crate) fn open(path: &Path) -> std::io::Result<Self> {
        let parent = Dir::open_ambient_dir(destination_parent(path), cap_std::ambient_authority())?;
        Self::from_parent(Arc::new(parent), path.to_owned())
    }
}

struct TemporaryMetrics<'a> {
    parent: &'a Dir,
    name: String,
    file: std::fs::File,
    committed: bool,
}

impl Drop for TemporaryMetrics<'_> {
    fn drop(&mut self) {
        if !self.committed {
            // Cleanup uses the same retained parent as creation, even if its
            // original pathname now names another directory.
            let _ = self.parent.remove_file(&self.name);
        }
    }
}

fn write_output(output: &MetricsOutput, metrics: &RunMetrics) -> Result<(), MetricsError> {
    metrics
        .validate()
        .map_err(|error| MetricsError::Validation(error.to_string()))?;
    let mut options = cap_std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let name = format!(".hoimin-metrics-{}.tmp", uuid::Uuid::new_v4());
    let file = output
        .parent
        .open_with(&name, &options)
        .map_err(|source| MetricsError::TemporaryFileCreation {
            parent: destination_parent(&output.path).to_owned(),
            source,
        })?
        .into_std();
    let mut temporary = TemporaryMetrics {
        parent: &output.parent,
        name,
        file,
        committed: false,
    };
    serde_json::to_writer(&mut temporary.file, metrics).map_err(MetricsError::Serialization)?;
    temporary
        .file
        .write_all(b"\n")
        .map_err(MetricsError::Write)?;
    temporary.file.flush().map_err(MetricsError::Flush)?;
    temporary.file.sync_all().map_err(MetricsError::Sync)?;
    output
        .parent
        .rename(&temporary.name, &output.parent, &output.name)
        .map_err(|source| MetricsError::Persist {
            path: output.path.clone(),
            source,
        })?;
    temporary.committed = true;
    Ok(())
}

#[cfg(test)]
pub(crate) fn write_metrics(path: &Path, metrics: &RunMetrics) -> Result<(), MetricsError> {
    let output =
        MetricsOutput::open(path).map_err(|source| MetricsError::TemporaryFileCreation {
            parent: destination_parent(path).to_owned(),
            source,
        })?;
    write_output(&output, metrics)
}

pub(crate) fn finalize_metrics<C: Clock>(
    output: &MetricsOutput,
    collector: Option<MetricsCollector<C>>,
    run_failure: Option<&str>,
    discovered: u64,
    executed: u64,
    warnings: &mut Vec<(&'static str, String)>,
) {
    if let Some(run_failure) = run_failure {
        warnings.push((
            "metrics.incomplete",
            format!("metrics output was not written because the run failed: {run_failure}"),
        ));
        return;
    }
    let Some(collector) = collector else {
        return;
    };
    match collector.finish(discovered, executed) {
        Ok(metrics) => {
            if let Err(error) = write_output(output, &metrics) {
                warnings.push(("metrics.write", error.to_string()));
            }
        }
        Err(error) => warnings.push(("metrics.state", error.to_string())),
    }
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
    fn failed_atomic_replacement_cleans_up_the_temporary_entry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("sentinel"), b"preserved").unwrap();
        let output = MetricsOutput::open(&path).unwrap();
        assert!(matches!(
            write_output(&output, &RunMetrics::empty("run-1")),
            Err(MetricsError::Persist { .. })
        ));
        assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"preserved");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn retained_parent_receives_output_and_failure_cleanup_after_symlink_swap() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("output");
        let parked = directory.path().join("parked");
        let protected = directory.path().join("project");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&protected).unwrap();
        fs::write(protected.join("calc.py"), b"protected source").unwrap();
        let output = MetricsOutput::open(&parent.join("calc.py")).unwrap();
        fs::rename(&parent, &parked).unwrap();
        std::os::unix::fs::symlink(&protected, &parent).unwrap();
        write_output(&output, &RunMetrics::empty("run-1")).unwrap();
        assert_eq!(
            fs::read(protected.join("calc.py")).unwrap(),
            b"protected source"
        );
        let actual: RunMetrics =
            serde_json::from_slice(&fs::read(parked.join("calc.py")).unwrap()).unwrap();
        actual.validate().unwrap();

        fs::remove_file(parked.join("calc.py")).unwrap();
        fs::create_dir(parked.join("calc.py")).unwrap();
        assert!(matches!(
            write_output(&output, &RunMetrics::empty("run-1")),
            Err(MetricsError::Persist { .. })
        ));
        assert_eq!(fs::read_dir(&parked).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&protected).unwrap().count(), 1);
        assert_eq!(
            fs::read(protected.join("calc.py")).unwrap(),
            b"protected source"
        );
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
    fn failed_run_skips_the_sidecar_and_queues_an_explicit_diagnostic() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        let collector = MetricsCollector::with_clock("run-1", FakeClock::default());
        let mut warnings = vec![("metrics.state", "earlier warning".to_owned())];

        finalize_metrics(
            &MetricsOutput::open(&path).unwrap(),
            Some(collector),
            Some("transition rejected"),
            3,
            1,
            &mut warnings,
        );

        assert!(!path.exists());
        assert_eq!(
            warnings,
            vec![
                ("metrics.state", "earlier warning".to_owned()),
                (
                    "metrics.incomplete",
                    "metrics output was not written because the run failed: transition rejected"
                        .to_owned(),
                ),
            ]
        );
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
