use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::io::Write;
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
#[cfg(test)]
use hoimin_core::DiskObservation;
use hoimin_core::disk::{
    DiskCleanupOutcome, DiskComponentState, DiskLifecycle, DiskLifecycleEvent, DiskRootId,
};
use hoimin_core::{
    CandidateLoaded, Diagnostic, DiskPolicy, DiskStopRequested, EffectFailed, EffectId, EmitOutput,
    FingerprintInput, ObserveRemainingBudget, OutputEvent, RemainingBudgetObserved, ReportVersions,
    ResourceMode, RunConfig, RunEffect, RunEvent, RunPhase, RunProcess, RunState, SourceHash,
    StartRequested, TargetSlice, fingerprint, transition,
};
#[cfg(test)]
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::analyzer::{AnalyzerHandler, CandidateStore};
use crate::metrics::{MetricsCollector, MetricsError, finalize_metrics};
use crate::process::{ProcessCancellation, ProcessHandler, ProcessRequest, ProcessStartGate};
use crate::report::{PreparedReport, ReportHandler};
#[cfg(not(any(windows, target_os = "linux")))]
use crate::resource::PortableBackend;
use crate::resource::ResourceBackend;
use crate::session::SessionDispatcher;
use crate::target::TargetHandler;
use crate::workspace::{
    CleanupRecord, CopyOptions, DiskMonitor, FilesystemKey, ManagedChild, ManagedRootCoordinator,
    ManagedRunRoot, OwnerKind, ReclaimReport, WorkspaceHandler, WorkspaceManifest, WorkspaceTask,
    WorkspaceTaskCompletion, available_for_managed_roots, measure_managed_roots,
    truncate_diagnostic_detail,
};
#[cfg(test)]
use crate::workspace::{
    DiskMeasurement, MaterializationPause, MaterializationPauseController, PreflightPause,
};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const CLEANUP_QUIESCENCE_UNPROVEN: &str =
    "process/output drain, disk monitor join, or final disk measurement join was not proven";

const fn cleanup_quiescence_proven(components: [bool; 4]) -> bool {
    components[0] && components[1] && components[2] && components[3]
}

fn lifecycle_safety_succeeded(lifecycle: &ShellDiskLifecycle) -> bool {
    let snapshot = lifecycle.snapshot();
    snapshot.process_drain == DiskComponentState::Succeeded
        && snapshot.output_drain == DiskComponentState::Succeeded
        && snapshot.monitor_join == DiskComponentState::Succeeded
}

/// Production-owned adapter for the disk lifecycle enforced at shell boundaries.
///
/// The shell and the Lean runtime correspondence test both use this adapter. Keeping the
/// transition owner here prevents the oracle from validating an unconnected shadow lifecycle.
#[doc(hidden)]
#[derive(Debug)]
pub struct ShellDiskLifecycle {
    lifecycle: DiskLifecycle,
}

impl ShellDiskLifecycle {
    /// Creates the shell lifecycle for the complete owned-root set.
    ///
    /// # Errors
    ///
    /// Returns the core lifecycle error when a logical root is duplicated.
    pub fn new(
        owned_roots: impl IntoIterator<Item = DiskRootId>,
    ) -> Result<Self, hoimin_core::DiskLifecycleError> {
        Ok(Self {
            lifecycle: DiskLifecycle::new(owned_roots)?,
        })
    }

    /// Applies one event at the same boundary used by the production shell.
    pub fn apply(&mut self, event: DiskLifecycleEvent) -> bool {
        self.lifecycle.apply(event)
    }

    /// Returns the complete observable lifecycle state.
    #[must_use]
    pub fn snapshot(&self) -> hoimin_core::DiskLifecycleSnapshot {
        self.lifecycle.snapshot()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownCause {
    TotalTimeout,
    Cancellation,
    Failure,
}

impl ShutdownCause {
    const fn label(self) -> &'static str {
        match self {
            Self::TotalTimeout => "total timeout",
            Self::Cancellation => "cancellation",
            Self::Failure => "failure",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShutdownBudget {
    cause: ShutdownCause,
    deadline: tokio::time::Instant,
}

impl ShutdownBudget {
    fn for_total_timeout_with_grace(run_deadline: tokio::time::Instant, grace: Duration) -> Self {
        Self {
            cause: ShutdownCause::TotalTimeout,
            deadline: run_deadline.checked_add(grace).unwrap_or(run_deadline),
        }
    }

    fn after_observation_with_grace(
        cause: ShutdownCause,
        observed_at: tokio::time::Instant,
        grace: Duration,
    ) -> Self {
        Self {
            cause,
            deadline: observed_at.checked_add(grace).unwrap_or(observed_at),
        }
    }

    const fn cause(self) -> ShutdownCause {
        self.cause
    }

    const fn deadline(self) -> tokio::time::Instant {
        self.deadline
    }

    async fn wait<F: Future>(&self, future: F) -> Result<F::Output, tokio::time::error::Elapsed> {
        tokio::time::timeout_at(self.deadline(), future).await
    }

    fn expiry_error(self, process_tasks: usize, io_tasks: usize) -> String {
        format!(
            "{}: shutdown grace expired after {}s (process tasks: {process_tasks}, blocking I/O tasks: {io_tasks})",
            self.cause().label(),
            SHUTDOWN_GRACE.as_secs(),
        )
    }
}

fn establish_shutdown_budget(
    active: &mut Option<ShutdownBudget>,
    candidate: ShutdownBudget,
) -> ShutdownBudget {
    *active.get_or_insert(candidate)
}

fn establish_event_shutdown_budget(
    active: &mut Option<ShutdownBudget>,
    event: &RunEvent,
    run_deadline: tokio::time::Instant,
    observed_at: tokio::time::Instant,
    grace: Duration,
) -> Option<ShutdownBudget> {
    let candidate = match event {
        RunEvent::DeadlineReached => {
            ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace)
        }
        RunEvent::CancellationRequested => ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            observed_at,
            grace,
        ),
        RunEvent::EffectFailed(_) | RunEvent::DiskStopRequested(_) => {
            ShutdownBudget::after_observation_with_grace(ShutdownCause::Failure, observed_at, grace)
        }
        _ => return None,
    };
    Some(establish_shutdown_budget(active, candidate))
}

fn ensure_outer_finalization_budget(
    active: &mut Option<ShutdownBudget>,
    run_failed: bool,
    run_deadline: tokio::time::Instant,
    observed_at: tokio::time::Instant,
    grace: Duration,
) -> ShutdownBudget {
    let candidate = if run_failed {
        ShutdownBudget::after_observation_with_grace(ShutdownCause::Failure, observed_at, grace)
    } else {
        ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace)
    };
    establish_shutdown_budget(active, candidate)
}

async fn finish_with_interrupt_monitor<T>(
    _interrupts: crate::interrupt::InterruptMonitor,
    finalization: impl Future<Output = T>,
) -> T {
    finalization.await
}

#[derive(Clone, Debug)]
pub struct RunControl {
    request: ProcessStartGate,
    max_process_tasks: Arc<AtomicUsize>,
    max_completion_in_flight: Arc<AtomicUsize>,
    #[cfg(test)]
    materialization_pause: Option<MaterializationPause>,
    #[cfg(test)]
    cancel_before_run_finished: Arc<AtomicBool>,
    #[cfg(test)]
    shutdown_grace: Duration,
    #[cfg(test)]
    disk_meter: Option<TestDiskMeter>,
    #[cfg(test)]
    dispatched_effects: Arc<AtomicUsize>,
    #[cfg(test)]
    disk_sample_interval: Duration,
    #[cfg(test)]
    managed_root_paths: Arc<std::sync::Mutex<Vec<Utf8PathBuf>>>,
    #[cfg(test)]
    finalization_events: Arc<std::sync::Mutex<Vec<&'static str>>>,
    #[cfg(test)]
    managed_parent: Arc<TempDir>,
    #[cfg(test)]
    sabotage_delivery_cleanup: Arc<AtomicBool>,
    #[cfg(test)]
    delivery_cleanup_sentinel: Arc<std::sync::Mutex<Option<Utf8PathBuf>>>,
    #[cfg(test)]
    end_available_override: TestEndAvailableOverride,
    #[cfg(test)]
    final_measurement_pause: Option<FinalMeasurementPause>,
    #[cfg(test)]
    post_drain_disk_failure: Arc<std::sync::Mutex<Option<hoimin_core::DiskFailure>>>,
    #[cfg(test)]
    force_execution_cleanup_deferred: Arc<AtomicBool>,
    #[cfg(test)]
    duplicate_execution_cleanup_request: Arc<AtomicBool>,
    #[cfg(test)]
    force_process_reap_failure: Arc<AtomicBool>,
}

#[cfg(test)]
#[derive(Clone)]
struct TestDiskMeter(Arc<std::sync::Mutex<Box<dyn DiskMeasurement>>>);

#[cfg(test)]
type TestEndAvailableOverride =
    Arc<std::sync::Mutex<Option<Result<BTreeMap<FilesystemKey, u64>, String>>>>;

#[cfg(test)]
#[derive(Clone, Debug)]
struct FinalMeasurementPause {
    entered: std::sync::mpsc::SyncSender<()>,
    release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
}

#[cfg(test)]
#[derive(Debug)]
struct FinalMeasurementPauseController {
    entered: std::sync::mpsc::Receiver<()>,
    release: std::sync::mpsc::SyncSender<()>,
}

#[cfg(test)]
impl FinalMeasurementPause {
    fn new() -> (Self, FinalMeasurementPauseController) {
        let (entered, entered_receiver) = std::sync::mpsc::sync_channel(0);
        let (release, release_receiver) = std::sync::mpsc::sync_channel(0);
        (
            Self {
                entered,
                release: Arc::new(std::sync::Mutex::new(release_receiver)),
            },
            FinalMeasurementPauseController {
                entered: entered_receiver,
                release,
            },
        )
    }

    fn wait(&self) {
        if self.entered.send(()).is_ok() {
            let _ = self
                .release
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .recv();
        }
    }
}

#[cfg(test)]
impl FinalMeasurementPauseController {
    fn wait_until_entered(&self) {
        self.entered
            .recv()
            .expect("final measurement pause entered");
    }

    fn release(&self) {
        self.release
            .send(())
            .expect("release final measurement pause");
    }
}

#[cfg(test)]
impl std::fmt::Debug for TestDiskMeter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestDiskMeter")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl DiskMeasurement for TestDiskMeter {
    fn measure(&self) -> std::io::Result<crate::workspace::MeterReading> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .measure()
    }
}

impl RunControl {
    #[must_use]
    #[cfg_attr(
        test,
        allow(
            clippy::missing_panics_doc,
            reason = "test-only managed-parent allocation is expected to succeed"
        )
    )]
    pub fn new() -> Self {
        Self {
            request: ProcessStartGate::new(),
            max_process_tasks: Arc::new(AtomicUsize::new(0)),
            max_completion_in_flight: Arc::new(AtomicUsize::new(0)),
            #[cfg(test)]
            materialization_pause: None,
            #[cfg(test)]
            cancel_before_run_finished: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            shutdown_grace: SHUTDOWN_GRACE,
            #[cfg(test)]
            disk_meter: None,
            #[cfg(test)]
            dispatched_effects: Arc::new(AtomicUsize::new(0)),
            #[cfg(test)]
            disk_sample_interval: hoimin_core::DISK_SAMPLE_INTERVAL,
            #[cfg(test)]
            managed_root_paths: Arc::new(std::sync::Mutex::new(Vec::new())),
            #[cfg(test)]
            finalization_events: Arc::new(std::sync::Mutex::new(Vec::new())),
            #[cfg(test)]
            managed_parent: Arc::new(tempfile::tempdir().expect("test managed parent")),
            #[cfg(test)]
            sabotage_delivery_cleanup: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            delivery_cleanup_sentinel: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            end_available_override: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            final_measurement_pause: None,
            #[cfg(test)]
            post_drain_disk_failure: Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            force_execution_cleanup_deferred: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            duplicate_execution_cleanup_request: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            force_process_reap_failure: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    fn with_disk_meter(meter: impl DiskMeasurement) -> Self {
        let mut control = Self::new();
        control.disk_meter = Some(TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(
            meter,
        )))));
        control
    }

    #[cfg(test)]
    fn with_disk_meter_and_interval(
        meter: impl DiskMeasurement,
        sample_interval: Duration,
    ) -> Self {
        let mut control = Self::with_disk_meter(meter);
        control.disk_sample_interval = sample_interval;
        control
    }

    #[cfg(test)]
    fn disk_meter(&self) -> Option<TestDiskMeter> {
        self.disk_meter.clone()
    }

    #[cfg(test)]
    fn observe_effect_dispatch(&self) {
        self.dispatched_effects.fetch_add(1, Ordering::AcqRel);
    }

    #[cfg(test)]
    fn dispatched_effect_count(&self) -> usize {
        self.dispatched_effects.load(Ordering::Acquire)
    }

    #[cfg(test)]
    fn observe_managed_roots(&self, roots: &ManagedShellRoots) {
        *self
            .managed_root_paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = roots.paths();
    }

    #[cfg(test)]
    fn managed_root_paths(&self) -> Vec<Utf8PathBuf> {
        self.managed_root_paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    fn observe_finalization_event(&self, event: &'static str) {
        self.finalization_events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }

    #[cfg(test)]
    fn finalization_events(&self) -> Vec<&'static str> {
        self.finalization_events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    fn inject_delivery_cleanup_identity_failure(&self) {
        self.sabotage_delivery_cleanup
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn before_delivery_cleanup(&self, root: &ManagedRunRoot) {
        if !self.sabotage_delivery_cleanup.swap(false, Ordering::AcqRel) {
            return;
        }
        let active = root.path();
        let sentinel = active.with_file_name(format!(
            "sentinel-{}",
            active.file_name().expect("managed root name")
        ));
        std::fs::rename(active, &sentinel).expect("move delivery root to sentinel");
        std::fs::create_dir(active).expect("create same-name replacement");
        *self
            .delivery_cleanup_sentinel
            .lock()
            .expect("delivery sentinel lock") = Some(sentinel);
    }

    #[cfg(test)]
    fn delivery_cleanup_sentinel(&self) -> Option<Utf8PathBuf> {
        self.delivery_cleanup_sentinel
            .lock()
            .expect("delivery sentinel lock")
            .clone()
    }

    #[cfg(test)]
    fn override_end_available(&self, result: Result<BTreeMap<FilesystemKey, u64>, String>) {
        *self
            .end_available_override
            .lock()
            .expect("end available override lock") = Some(result);
    }

    #[cfg_attr(
        not(test),
        allow(
            clippy::unused_self,
            reason = "test builds read the per-run end-capacity override"
        )
    )]
    fn end_available(
        &self,
        roots: &[Arc<ManagedRunRoot>],
    ) -> std::io::Result<BTreeMap<FilesystemKey, u64>> {
        #[cfg(test)]
        if let Some(result) = self
            .end_available_override
            .lock()
            .expect("end available override lock")
            .take()
        {
            return result.map_err(std::io::Error::other);
        }
        available_for_managed_roots(roots)
    }

    #[cfg(test)]
    fn with_materialization_pause(worker: u32) -> (Self, MaterializationPauseController) {
        let (pause, controller) = MaterializationPause::new(worker);
        let mut control = Self::new();
        control.materialization_pause = Some(pause);
        (control, controller)
    }

    #[cfg(test)]
    fn with_final_measurement_pause() -> (Self, FinalMeasurementPauseController) {
        let (pause, controller) = FinalMeasurementPause::new();
        let mut control = Self::new();
        control.final_measurement_pause = Some(pause);
        (control, controller)
    }

    #[cfg(test)]
    fn inject_post_drain_disk_failure(&self, failure: hoimin_core::DiskFailure) {
        *self
            .post_drain_disk_failure
            .lock()
            .expect("post-drain disk failure lock") = Some(failure);
    }

    #[cfg(test)]
    fn take_post_drain_disk_failure(&self) -> Option<hoimin_core::DiskFailure> {
        self.post_drain_disk_failure
            .lock()
            .expect("post-drain disk failure lock")
            .take()
    }

    #[cfg(test)]
    fn inject_execution_cleanup_deferred(&self) {
        self.force_execution_cleanup_deferred
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn take_execution_cleanup_deferred(&self) -> bool {
        self.force_execution_cleanup_deferred
            .swap(false, Ordering::AcqRel)
    }

    #[cfg(test)]
    fn inject_duplicate_execution_cleanup_request(&self) {
        self.duplicate_execution_cleanup_request
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn take_duplicate_execution_cleanup_request(&self) -> bool {
        self.duplicate_execution_cleanup_request
            .swap(false, Ordering::AcqRel)
    }

    #[cfg(test)]
    fn inject_process_reap_failure(&self) {
        self.force_process_reap_failure
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn with_materialization_pause_and_shutdown_grace(
        worker: u32,
        shutdown_grace: Duration,
    ) -> (Self, MaterializationPauseController) {
        let (mut control, controller) = Self::with_materialization_pause(worker);
        control.shutdown_grace = shutdown_grace;
        (control, controller)
    }

    #[cfg(test)]
    fn cancelling_before_run_finished() -> Self {
        let control = Self::new();
        control
            .cancel_before_run_finished
            .store(true, Ordering::Release);
        control
    }

    #[cfg(test)]
    fn before_effect_dispatch(&self, effect: &RunEffect) {
        if matches!(
            effect,
            RunEffect::EmitOutput(value)
                if matches!(&value.event, OutputEvent::RunFinished(_))
        ) && self
            .cancel_before_run_finished
            .swap(false, Ordering::AcqRel)
        {
            self.cancel();
        }
    }

    #[allow(
        clippy::unused_self,
        reason = "test builds read the per-run override; production always returns the fixed grace"
    )]
    fn shutdown_grace(&self) -> Duration {
        #[cfg(test)]
        {
            self.shutdown_grace
        }
        #[cfg(not(test))]
        {
            SHUTDOWN_GRACE
        }
    }

    pub fn cancel(&self) {
        self.request.cancel();
    }

    #[must_use]
    pub fn max_process_tasks(&self) -> usize {
        self.max_process_tasks.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn max_completion_in_flight(&self) -> usize {
        self.max_completion_in_flight.load(Ordering::Acquire)
    }

    fn observe_process_tasks(&self, value: usize) {
        self.max_process_tasks.fetch_max(value, Ordering::AcqRel);
    }

    fn observe_completion_in_flight(&self, value: usize) {
        self.max_completion_in_flight
            .fetch_max(value, Ordering::AcqRel);
    }

    async fn cancelled(&self) {
        self.request.cancelled().await;
    }

    fn is_cancelled(&self) -> bool {
        self.request.is_cancelled()
    }

    fn begin_dispatch(&self) -> Option<std::sync::MutexGuard<'_, ()>> {
        let guard = self.request.begin_spawn();
        if self.is_cancelled() {
            None
        } else {
            Some(guard)
        }
    }

    fn start_gate(&self) -> ProcessStartGate {
        self.request.clone()
    }
}

impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}

struct ShellCompletion {
    event: RunEvent,
    process_task: bool,
    io_task: bool,
    process: Option<(u32, bool)>,
    blocking: Option<Box<BlockingEffectCompletion>>,
}

enum BlockingEffect {
    Workspace(Box<WorkspaceTask>),
    Candidate(hoimin_core::ReadCandidate),
    Preflight {
        id: EffectId,
        workspace: Box<WorkspaceHandler>,
        request: hoimin_core::Preflight,
        config: Box<RunConfig>,
        copied_at_start: BTreeSet<Utf8PathBuf>,
        targets: Vec<TargetSlice>,
        resource_mode: ResourceMode,
    },
    Cleanup {
        id: EffectId,
        process: Arc<ProcessHandler>,
        workspace: Box<WorkspaceHandler>,
        request: hoimin_core::Cleanup,
    },
    #[cfg(test)]
    TestOperation {
        id: EffectId,
        operation: Box<dyn FnOnce() -> RunEvent + Send>,
    },
    #[cfg(test)]
    TestCompletion {
        id: EffectId,
        operation: Box<dyn FnOnce() -> BlockingEffectCompletion + Send>,
    },
}

enum BlockingEffectCompletion {
    Workspace(Box<WorkspaceTaskCompletion>),
    Candidate(Box<RunEvent>),
    OwnedWorkspace {
        id: EffectId,
        workspace: Box<WorkspaceHandler>,
        event: Box<RunEvent>,
        secondary_errors: Vec<String>,
    },
}

fn combine_cleanup_results(
    close_error: Option<String>,
    cleanup: Result<hoimin_core::CleanupFinished, EffectFailed>,
) -> (RunEvent, Vec<String>) {
    let event = cleanup.map_or_else(RunEvent::EffectFailed, RunEvent::CleanupFinished);
    let secondary_errors = close_error
        .map(|error| vec![format!("process.resource.close: {error}")])
        .unwrap_or_default();
    (event, secondary_errors)
}

impl BlockingEffect {
    fn id(&self) -> EffectId {
        match self {
            Self::Workspace(task) => task.id(),
            Self::Candidate(request) => request.id,
            Self::Preflight { id, .. } => *id,
            Self::Cleanup { id, .. } => *id,
            #[cfg(test)]
            Self::TestOperation { id, .. } => *id,
            #[cfg(test)]
            Self::TestCompletion { id, .. } => *id,
        }
    }

    fn execute(self) -> BlockingEffectCompletion {
        match self {
            Self::Workspace(task) => BlockingEffectCompletion::Workspace(Box::new(task.execute())),
            Self::Candidate(request) => {
                BlockingEffectCompletion::Candidate(Box::new(replay_candidate(&request)))
            }
            Self::Preflight {
                id,
                mut workspace,
                request,
                config,
                copied_at_start,
                targets,
                resource_mode,
            } => {
                let event = match workspace.handle_preflight_validated(request, |root, manifest| {
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id)
                }) {
                    Ok(mut value) => match prepare_fingerprint(&config, &targets, resource_mode) {
                        Ok(run_fingerprint) => {
                            value.fingerprint = Some(run_fingerprint);
                            RunEvent::PreflightCompleted(value)
                        }
                        Err(mut error) => {
                            error.id = value.id;
                            RunEvent::EffectFailed(error)
                        }
                    },
                    Err(error) => RunEvent::EffectFailed(error),
                };
                BlockingEffectCompletion::OwnedWorkspace {
                    id,
                    workspace,
                    event: Box::new(event),
                    secondary_errors: Vec::new(),
                }
            }
            Self::Cleanup {
                id,
                process,
                mut workspace,
                request,
            } => {
                let close_error = process.close().err().map(|error| error.to_string());
                let (event, secondary_errors) =
                    combine_cleanup_results(close_error, workspace.handle_cleanup(request));
                BlockingEffectCompletion::OwnedWorkspace {
                    id,
                    workspace,
                    event: Box::new(event),
                    secondary_errors,
                }
            }
            #[cfg(test)]
            Self::TestOperation { operation, .. } => {
                BlockingEffectCompletion::Candidate(Box::new(operation()))
            }
            #[cfg(test)]
            Self::TestCompletion { operation, .. } => operation(),
        }
    }
}

fn replay_candidate(request: &hoimin_core::ReadCandidate) -> RunEvent {
    match CandidateStore::replay_one(&request.spool, request.cursor) {
        Ok(Some((candidate, next_cursor))) => RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            next_cursor,
            candidate: Some(candidate),
        }),
        Ok(None) => RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            candidate: None,
            next_cursor: request.cursor,
        }),
        Err(error) => RunEvent::EffectFailed(EffectFailed::other(
            request.id,
            "candidate.replay",
            error.to_string(),
        )),
    }
}

fn is_blocking_io_effect(effect: &RunEffect) -> bool {
    matches!(
        effect,
        RunEffect::CreateWorker(_)
            | RunEffect::Preflight(_)
            | RunEffect::ReadCandidate(_)
            | RunEffect::ApplyMutation(_)
            | RunEffect::ResetWorker(_)
            | RunEffect::VerifyOriginals(_)
            | RunEffect::Cleanup(_)
    )
}

fn prepare_blocking_effect<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
) -> Result<BlockingEffect, EffectFailed>
where
    Stdout: Write,
    Stderr: Write,
{
    let task = match effect {
        RunEffect::Preflight(request) => {
            let id = request.id;
            let targets = context.resolved_targets.clone().ok_or_else(|| {
                EffectFailed::other(
                    id,
                    "shell.targets.missing",
                    "preflight preceded target resolution",
                )
            })?;
            let workspace = context.workspace.take().ok_or_else(|| {
                EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "workspace ownership is unavailable for preflight",
                )
            })?;
            return Ok(BlockingEffect::Preflight {
                id,
                workspace: Box::new(workspace),
                request,
                config: Box::new(context.config.clone()),
                copied_at_start: context.fingerprint_copy_inputs.clone(),
                targets,
                resource_mode: context.process.mode(),
            });
        }
        RunEffect::CreateWorker(request) => context.workspace_mut().prepare_create_task(request),
        RunEffect::ReadCandidate(request) => return Ok(BlockingEffect::Candidate(request)),
        RunEffect::ApplyMutation(request) => {
            let candidate = request.candidate.clone();
            context.active_candidates.insert(request.worker, candidate);
            context.workspace_mut().prepare_apply_task(request)
        }
        RunEffect::ResetWorker(request) => context.workspace_mut().prepare_reset_task(request),
        RunEffect::VerifyOriginals(request) => context.workspace().prepare_verify_task(request),
        RunEffect::Cleanup(request) => {
            let id = request.id;
            let workspace = context.workspace.take().ok_or_else(|| {
                EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "workspace ownership is unavailable for cleanup",
                )
            })?;
            return Ok(BlockingEffect::Cleanup {
                id,
                process: Arc::clone(&context.process),
                workspace: Box::new(workspace),
                request,
            });
        }
        _ => unreachable!("non-blocking effect passed to blocking preparation"),
    };
    task.map(Box::new).map(BlockingEffect::Workspace)
}

fn accept_blocking_completion<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    completion: BlockingEffectCompletion,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    let event = match completion {
        BlockingEffectCompletion::Workspace(completion) => context
            .workspace_mut()
            .accept_task_completion(*completion)
            .unwrap_or_else(RunEvent::EffectFailed),
        BlockingEffectCompletion::Candidate(event) => *event,
        BlockingEffectCompletion::OwnedWorkspace {
            id,
            workspace,
            event,
            secondary_errors,
        } => {
            if context.workspace.is_some() {
                return RunEvent::EffectFailed(EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "blocking operation returned duplicate workspace ownership",
                ));
            }
            context.workspace = Some(*workspace);
            context.blocking_secondary_errors.extend(secondary_errors);
            *event
        }
    };
    match &event {
        RunEvent::CandidateLoaded(value) => {
            if let Some(candidate) = &value.candidate {
                context
                    .active_candidates
                    .insert(value.worker, candidate.clone());
            } else {
                context.active_candidates.remove(&value.worker);
            }
        }
        RunEvent::WorkerReset(value) => {
            context.active_candidates.remove(&value.worker);
        }
        _ => {}
    }
    event
}

#[cfg(test)]
fn execute_direct_io_effect<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    let result = match effect {
        RunEffect::CreateWorker(request) => context
            .workspace_mut()
            .handle_create_worker(request)
            .map(RunEvent::WorkerCreated),
        RunEffect::ReadCandidate(request) => {
            return accept_blocking_completion(
                context,
                BlockingEffectCompletion::Candidate(Box::new(replay_candidate(&request))),
            );
        }
        RunEffect::ApplyMutation(request) => {
            let candidate = request.candidate.clone();
            context
                .active_candidates
                .insert(request.worker, candidate.clone());
            context
                .workspace_mut()
                .handle_apply_mutation(request, &candidate)
                .map(RunEvent::MutationApplied)
        }
        RunEffect::ResetWorker(request) => {
            let worker = request.worker;
            let result = context
                .workspace_mut()
                .handle_reset_worker(request)
                .map(RunEvent::WorkerReset);
            if result.is_ok() {
                context.active_candidates.remove(&worker);
            }
            result
        }
        RunEffect::VerifyOriginals(request) => context
            .workspace()
            .handle_verify_originals(request)
            .map(RunEvent::OriginalsVerified),
        _ => unreachable!("non-blocking effect passed to direct I/O execution"),
    };
    result.unwrap_or_else(RunEvent::EffectFailed)
}

fn remaining_budget_observed(
    request: &ObserveRemainingBudget,
    deadline: tokio::time::Instant,
    now: tokio::time::Instant,
) -> RemainingBudgetObserved {
    RemainingBudgetObserved {
        id: request.id,
        remaining: deadline.saturating_duration_since(now),
    }
}

async fn run_blocking_io<T>(
    id: EffectId,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, EffectFailed>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| EffectFailed::other(id, "shell.blocking_io", error.to_string()))
}

#[derive(Debug)]
enum OwnedBlockingError {
    Join(String),
    Expired(String),
}

async fn run_owned_blocking_until<T>(
    budget: &ShutdownBudget,
    before_start: OwnedStartHook,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, OwnedBlockingError>
where
    T: Send + 'static,
{
    let deadline = budget.deadline();
    let mut task = tokio::task::spawn_blocking(move || {
        before_start();
        (tokio::time::Instant::now() < deadline).then(operation)
    });
    await_owned_blocking_until(budget, &mut task).await
}

async fn await_owned_blocking_until<T>(
    budget: &ShutdownBudget,
    task: &mut tokio::task::JoinHandle<Option<T>>,
) -> Result<T, OwnedBlockingError>
where
    T: Send + 'static,
{
    match budget.wait(&mut *task).await {
        Ok(Ok(Some(value))) => Ok(value),
        Ok(Ok(None)) => Err(OwnedBlockingError::Expired(budget.expiry_error(0, 0))),
        Ok(Err(error)) => Err(OwnedBlockingError::Join(format!(
            "blocking I/O task failed: {error}"
        ))),
        Err(_) => {
            task.abort();
            Err(OwnedBlockingError::Expired(budget.expiry_error(0, 1)))
        }
    }
}

struct PreparedShellSetup {
    workspace: WorkspaceHandler,
    analyzer: AnalyzerHandler,
    process: Arc<ProcessHandler>,
    report: PreparedReport,
    spool_dir: Arc<ManagedShellRoots>,
    config: RunConfig,
}

#[derive(Debug)]
struct ManagedShellRoots {
    execution_root: Arc<ManagedRunRoot>,
    delivery_root: Arc<ManagedRunRoot>,
    execution_spool: std::sync::Mutex<Option<Arc<ManagedChild>>>,
    delivery_spool: std::sync::Mutex<Option<Arc<ManagedChild>>>,
    startup_reclaim: ReclaimReport,
}

impl ManagedShellRoots {
    fn monitor_roots(&self) -> Vec<Arc<ManagedRunRoot>> {
        vec![
            Arc::clone(&self.execution_root),
            Arc::clone(&self.delivery_root),
        ]
    }

    fn release_execution_spool(&self) {
        self.execution_spool
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    fn release_delivery_spool(&self) {
        self.delivery_spool
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    #[cfg(test)]
    fn paths(&self) -> Vec<Utf8PathBuf> {
        vec![
            self.execution_root.path().to_owned(),
            self.delivery_root.path().to_owned(),
        ]
    }
}

struct SetupRollback {
    execution: Arc<ManagedRunRoot>,
    delivery: Option<Arc<ManagedRunRoot>>,
    execution_spool: Option<Arc<ManagedChild>>,
    delivery_spool: Option<Arc<ManagedChild>>,
    armed: bool,
}

impl SetupRollback {
    fn new(execution: Arc<ManagedRunRoot>) -> Self {
        Self {
            execution,
            delivery: None,
            execution_spool: None,
            delivery_spool: None,
            armed: true,
        }
    }

    fn attach_delivery(&mut self, delivery: Arc<ManagedRunRoot>) {
        self.delivery = Some(delivery);
    }

    fn attach_execution_spool(&mut self, spool: Arc<ManagedChild>) {
        self.execution_spool = Some(spool);
    }

    fn attach_delivery_spool(&mut self, spool: Arc<ManagedChild>) {
        self.delivery_spool = Some(spool);
    }
}

impl Drop for SetupRollback {
    fn drop(&mut self) {
        if self.armed {
            // A concurrent creator may hold the coordinator lock long enough for immediate
            // cleanup to defer. Publish cleanup-ready first so releasing our leases never turns
            // a setup failure into a young root that the janitor must preserve for 24 hours.
            let _ = self.execution.mark_cleanup_ready();
            if let Some(delivery) = &self.delivery {
                let _ = delivery.mark_cleanup_ready();
            }
            // Close every child capability owned by the guard before the root lifecycle checks
            // for live handles. Callers bind this guard before their additional Arc clones, so
            // those clones are also dropped before this guard during error unwinding.
            self.delivery_spool.take();
            self.execution_spool.take();
            let _ = self.execution.cleanup(Duration::from_secs(60));
            if let Some(delivery) = &self.delivery {
                let _ = delivery.cleanup(Duration::from_secs(60));
            }
        }
    }
}

async fn prepare_shell_setup(
    config: RunConfig,
    before_start: OwnedStartHook,
) -> Result<PreparedShellSetup, String> {
    tokio::task::spawn_blocking(move || {
        before_start();
        prepare_shell_setup_sync(config)
    })
    .await
    .map_err(|error| format!("shell setup task failed: {error}"))?
}

#[cfg(test)]
async fn prepare_shell_setup_in(
    config: RunConfig,
    temporary_parent: Utf8PathBuf,
) -> Result<PreparedShellSetup, String> {
    tokio::task::spawn_blocking(move || {
        prepare_shell_setup_sync_in(config, &temporary_parent, &|_| {})
    })
    .await
    .map_err(|error| format!("shell setup task failed: {error}"))?
}

fn prepare_shell_setup_sync(config: RunConfig) -> Result<PreparedShellSetup, String> {
    let temporary_parent = Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .map_err(|_| "temporary workspace parent is not UTF-8".to_owned())?;
    prepare_shell_setup_sync_in(config, &temporary_parent, &|_| {})
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellSetupBoundary {
    RootsCreated,
    DiskPolicyVerified,
    WorkspaceCreated,
    BackendCreated,
    ProcessCreated,
    AnalyzerCreated,
    ReportCreated,
}

fn prepare_shell_setup_sync_in(
    config: RunConfig,
    temporary_parent: &Utf8Path,
    boundary: &impl Fn(ShellSetupBoundary),
) -> Result<PreparedShellSetup, String> {
    let (
        mut rollback,
        execution_root,
        delivery_root,
        execution_spool,
        delivery_spool,
        startup_reclaim,
    ) = create_managed_shell_roots_in(temporary_parent, &|_| {})?;
    boundary(ShellSetupBoundary::RootsCreated);
    boundary(ShellSetupBoundary::DiskPolicyVerified);
    let spool_path = execution_spool.path().to_owned();
    let requested_workers = u32::try_from(config.limits.jobs.get())
        .map_err(|_| "--jobs exceeds the supported worker count".to_owned())?;
    let workspace = WorkspaceHandler::new(
        config.root.clone(),
        config.selection.sources.clone(),
        requested_workers,
        CopyOptions {
            includes: config.selection.includes.clone(),
            excludes: config.selection.excludes.clone(),
        },
    )
    .with_managed_root(Arc::clone(&execution_root))
    .with_max_owned_bytes(config.limits.max_workspace_size.get());
    boundary(ShellSetupBoundary::WorkspaceCreated);
    let backend = resource_backend(&config).map_err(|error| error.to_string())?;
    boundary(ShellSetupBoundary::BackendCreated);
    let process = Arc::new(ProcessHandler::new(
        backend.clone(),
        spool_path.join("process"),
    ));
    boundary(ShellSetupBoundary::ProcessCreated);
    let analyzer = AnalyzerHandler::with_backend(
        config.root.clone(),
        backend,
        config.limits.max_memory.get(),
        u32::try_from(config.limits.max_processes.get())
            .map_err(|_| "--max-processes exceeds the supported process count".to_owned())?,
    )
    .map_err(|error| error.to_string())?
    .with_managed_candidate_spool_owner(Arc::clone(&execution_spool));
    boundary(ShellSetupBoundary::AnalyzerCreated);
    let report = PreparedReport::new(config.output.format, delivery_spool.path())
        .map_err(|error| error.to_string())?;
    boundary(ShellSetupBoundary::ReportCreated);
    let spool_dir = Arc::new(ManagedShellRoots {
        execution_root,
        delivery_root,
        execution_spool: std::sync::Mutex::new(Some(execution_spool)),
        delivery_spool: std::sync::Mutex::new(Some(delivery_spool)),
        startup_reclaim,
    });
    rollback.armed = false;

    Ok(PreparedShellSetup {
        workspace,
        analyzer,
        process,
        report,
        spool_dir,
        config,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagedRootSetupBoundary {
    ExecutionPublished,
    DeliveryPublished,
    ExecutionSpoolCreated,
    DeliverySpoolCreated,
}

#[allow(
    clippy::type_complexity,
    reason = "the tuple keeps the rollback guard armed alongside all four published owners"
)]
fn create_managed_shell_roots_in(
    temporary_parent: &Utf8Path,
    boundary: &impl Fn(ManagedRootSetupBoundary),
) -> Result<
    (
        SetupRollback,
        Arc<ManagedRunRoot>,
        Arc<ManagedRunRoot>,
        Arc<ManagedChild>,
        Arc<ManagedChild>,
        ReclaimReport,
    ),
    String,
> {
    let coordinator =
        ManagedRootCoordinator::open(temporary_parent).map_err(|error| error.to_string())?;
    let reclaim = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
    let execution_root = Arc::new(
        ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution)
            .map_err(|error| error.to_string())?,
    );
    let mut rollback = SetupRollback::new(Arc::clone(&execution_root));
    boundary(ManagedRootSetupBoundary::ExecutionPublished);
    let delivery_root = Arc::new(
        ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery)
            .map_err(|error| error.to_string())?,
    );
    rollback.attach_delivery(Arc::clone(&delivery_root));
    boundary(ManagedRootSetupBoundary::DeliveryPublished);
    let execution_spool = Arc::new(
        execution_root
            .create_child("spool-")
            .map_err(|error| error.to_string())?,
    );
    rollback.attach_execution_spool(Arc::clone(&execution_spool));
    boundary(ManagedRootSetupBoundary::ExecutionSpoolCreated);
    let delivery_spool = Arc::new(
        delivery_root
            .create_child("report-")
            .map_err(|error| error.to_string())?,
    );
    rollback.attach_delivery_spool(Arc::clone(&delivery_spool));
    boundary(ManagedRootSetupBoundary::DeliverySpoolCreated);
    Ok((
        rollback,
        execution_root,
        delivery_root,
        execution_spool,
        delivery_spool,
        reclaim,
    ))
}

async fn start_disk_monitor<Stdout, Stderr>(
    context: &ShellContext<Stdout, Stderr>,
    control: &RunControl,
) -> Result<DiskMonitor, String>
where
    Stdout: Write,
    Stderr: Write,
{
    #[cfg(not(test))]
    let _ = control;
    let policy = DiskPolicy {
        max_owned_bytes: context.config.limits.max_workspace_size,
        min_free_bytes: context.config.limits.min_free_space,
    };
    let roots = context.spool_dir.monitor_roots();
    #[cfg(test)]
    if let Some(meter) = control.disk_meter() {
        return Ok(DiskMonitor::start_with_meter_and_interval(
            meter,
            policy,
            roots,
            control.disk_sample_interval,
        )
        .await);
    }
    DiskMonitor::start(policy, roots)
        .await
        .map_err(|error| format!("disk.measurement.failed: {error}"))
}

fn is_disk_guarded_dispatch(effect: &RunEffect) -> bool {
    matches!(
        effect,
        RunEffect::AnalyzeFile(_) | RunEffect::RunBaseline(_) | RunEffect::RunMutant(_)
    )
}

async fn sample_after_process_drain(
    monitor: &DiskMonitor,
    process_completion: bool,
) -> Option<hoimin_core::DiskFailure> {
    if process_completion {
        monitor.sample_now().await
    } else {
        None
    }
}

async fn run_cleanup_thread<T: Send + 'static>(
    budget: &ShutdownBudget,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(operation());
    });
    budget
        .wait(receiver)
        .await
        .map_err(|_| "workspace.cleanup.deferred: cleanup operation timed out".to_owned())?
        .map_err(|_| "workspace.cleanup.failed: cleanup thread stopped without a result".to_owned())
}

#[derive(Debug)]
enum ManagedCleanupOutcome {
    Clean(CleanupRecord),
    Failed {
        record: Option<CleanupRecord>,
        error: String,
    },
    Deferred {
        record: Option<CleanupRecord>,
        error: String,
    },
}

fn cleanup_thread_error(error: String) -> ManagedCleanupOutcome {
    if error.starts_with("workspace.cleanup.deferred:") {
        ManagedCleanupOutcome::Deferred {
            record: None,
            error,
        }
    } else {
        ManagedCleanupOutcome::Failed {
            record: None,
            error,
        }
    }
}

fn attach_cleanup_ready_error(
    mut outcome: ManagedCleanupOutcome,
    marker_error: Option<&str>,
) -> ManagedCleanupOutcome {
    let Some(marker_error) = marker_error else {
        return outcome;
    };
    let detail = format!("cleanup-ready: {marker_error}");
    let bounded_detail = truncate_diagnostic_detail(detail.clone()).0;
    match &mut outcome {
        ManagedCleanupOutcome::Clean(record) => record.push_detail(detail),
        ManagedCleanupOutcome::Failed { record, error }
        | ManagedCleanupOutcome::Deferred { record, error } => {
            if let Some(record) = record {
                record.push_detail(detail);
            }
            error.push_str("; ");
            error.push_str(&bounded_detail);
        }
    }
    outcome
}

fn delivery_cleanup_secondary_errors(record: &CleanupRecord) -> Vec<String> {
    record
        .details
        .iter()
        .filter(|detail| detail.starts_with("cleanup-ready: "))
        .map(|detail| {
            truncate_diagnostic_detail(format!(
                "{}: {detail}",
                hoimin_core::WORKSPACE_CLEANUP_FAILED
            ))
            .0
        })
        .collect()
}

async fn cleanup_managed_root(
    root: Arc<ManagedRunRoot>,
    budget: &ShutdownBudget,
) -> ManagedCleanupOutcome {
    let marker_root = Arc::clone(&root);
    let marker_error =
        match run_cleanup_thread(budget, move || marker_root.mark_cleanup_ready()).await {
            Ok(result) => result.err().map(|error| error.to_string()),
            Err(error) => {
                return attach_cleanup_ready_error(
                    cleanup_thread_error(error),
                    Some("cleanup-ready operation did not complete"),
                );
            }
        };
    loop {
        let remaining = budget
            .deadline()
            .saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            let record = root.abandon_for_janitor("managed-root cleanup budget expired".to_owned());
            return attach_cleanup_ready_error(
                ManagedCleanupOutcome::Deferred {
                    record: Some(record),
                    error: "workspace.cleanup.deferred: managed-root cleanup budget expired"
                        .to_owned(),
                },
                marker_error.as_deref(),
            );
        }
        let cleanup_root = Arc::clone(&root);
        let cleanup =
            match run_cleanup_thread(budget, move || cleanup_root.cleanup(remaining)).await {
                Ok(cleanup) => cleanup,
                Err(error) => {
                    return attach_cleanup_ready_error(
                        cleanup_thread_error(error),
                        marker_error.as_deref(),
                    );
                }
            };
        let retryable_contention = cleanup.status == hoimin_core::DiskCleanupStatus::Deferred
            && cleanup.details.iter().any(|detail| {
                detail.contains("coordinator process lock is busy")
                    || detail.contains("coordinator lock failed")
            });
        if retryable_contention && tokio::time::Instant::now() < budget.deadline() {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }
        let detail = format!(
            "{}; omitted={}; remaining={:?}",
            cleanup.details.join("; "),
            cleanup.omitted_detail_count,
            cleanup.remaining_root,
        );
        let outcome = match cleanup.status {
            hoimin_core::DiskCleanupStatus::Clean => ManagedCleanupOutcome::Clean(cleanup),
            hoimin_core::DiskCleanupStatus::Failed => ManagedCleanupOutcome::Failed {
                record: Some(cleanup),
                error: format!("workspace.cleanup.failed: {detail}"),
            },
            hoimin_core::DiskCleanupStatus::Deferred => ManagedCleanupOutcome::Deferred {
                record: Some(cleanup),
                error: format!("workspace.cleanup.deferred: {detail}"),
            },
            hoimin_core::DiskCleanupStatus::Retained => ManagedCleanupOutcome::Deferred {
                record: Some(cleanup),
                error: format!("workspace.cleanup.deferred: retained: {detail}"),
            },
            hoimin_core::DiskCleanupStatus::CleanupAfterDelivery => ManagedCleanupOutcome::Failed {
                record: Some(cleanup),
                error: format!(
                    "workspace.cleanup.failed: invalid execution cleanup status: {detail}"
                ),
            },
        };
        return attach_cleanup_ready_error(outcome, marker_error.as_deref());
    }
}

fn append_finalization_error<T>(result: Result<T, String>, error: String) -> Result<T, String> {
    match result {
        Ok(_) => Err(error),
        Err(primary) => Err(combine_shutdown_errors(primary, Some(error))),
    }
}

fn apply_finalization_errors_to_summary(summary: &mut hoimin_core::RunSummary, errors: &[String]) {
    if errors.is_empty() {
        return;
    }
    summary.complete = false;
    if summary.exit_code == 0 {
        summary.exit_code = 2;
    }
    for error in errors {
        let (code, message) = error.split_once(": ").map_or_else(
            || ("run.finalization.failed".to_owned(), error.clone()),
            |(code, message)| (code.to_owned(), message.to_owned()),
        );
        if let Some(stop) = &mut summary.disk.stop {
            if stop.code == code
                || stop
                    .secondary
                    .iter()
                    .any(|secondary| secondary.code() == code)
            {
                continue;
            }
            stop.secondary
                .push(hoimin_core::DiskSecondary::Error { code, message });
        } else {
            summary.disk.stop = Some(hoimin_core::DiskStopReport {
                code,
                owned_bytes: None,
                available_bytes: None,
                message: Some(message),
                secondary: Vec::new(),
            });
        }
    }
}

fn apply_execution_cleanup_evidence(
    report: &mut hoimin_core::DiskCleanupReport,
    cleanup: &CleanupRecord,
    startup_reclaim: &ReclaimReport,
) {
    let mut cleanup = cleanup.clone();
    startup_reclaim.append_to_cleanup(&mut cleanup);
    report.status = cleanup.status;
    report.examined_entries = cleanup.examined_entries;
    report.removed_entries = cleanup.removed_entries;
    report.details = cleanup.details;
    report.omitted_detail_count = cleanup.omitted_detail_count;
    report.remaining_root = cleanup.remaining_root.as_ref().map(ToString::to_string);
}

fn merge_monitor_stop(report: &mut hoimin_core::DiskStopReport, latest: hoimin_core::DiskFailure) {
    let mut incoming = Vec::with_capacity(latest.secondary.len().saturating_add(1));
    if report.code != latest.code {
        if let Some(observation) = latest.observation {
            incoming.push(hoimin_core::DiskSecondary::Observation {
                reason: latest.reason,
                value: observation,
            });
        } else {
            incoming.push(hoimin_core::DiskSecondary::Error {
                code: latest.code,
                message: latest
                    .message
                    .unwrap_or_else(|| "disk monitor reported a secondary stop".to_owned()),
            });
        }
    }
    incoming.extend(latest.secondary);
    for secondary in incoming {
        if report.code == secondary.code()
            || report
                .secondary
                .iter()
                .any(|existing| existing.code() == secondary.code())
        {
            continue;
        }
        report.secondary.push(secondary);
    }
}

fn merge_lifecycle_stop(summary: &mut hoimin_core::RunSummary, failure: hoimin_core::DiskFailure) {
    if let Some(report) = &mut summary.disk.stop {
        merge_monitor_stop(report, failure);
        return;
    }
    summary.disk.stop = Some(hoimin_core::DiskStopReport {
        code: failure.code,
        owned_bytes: failure.observation.map(|value| value.owned_bytes),
        available_bytes: failure.observation.map(|value| value.available_bytes),
        message: failure.message,
        secondary: failure.secondary,
    });
}

pub struct ShellContext<Stdout, Stderr> {
    workspace: Option<WorkspaceHandler>,
    analyzer: AnalyzerHandler,
    process: Arc<ProcessHandler>,
    report: ReportHandler<Stdout, Stderr>,
    session: Option<SessionDispatcher>,
    session_path: Option<Utf8PathBuf>,
    active_candidates: BTreeMap<u32, hoimin_core::MutationCandidate>,
    spool_dir: Arc<ManagedShellRoots>,
    resolved_targets: Option<Vec<TargetSlice>>,
    config: RunConfig,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    report_versions: ReportVersions,
    blocking_secondary_errors: Vec<String>,
}

impl<Stdout, Stderr> ShellContext<Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    /// Creates all handlers required for a run.
    ///
    /// # Errors
    ///
    /// Returns an error when local run infrastructure cannot be initialized.
    pub async fn new(config: &RunConfig, stdout: Stdout, stderr: Stderr) -> Result<Self, String> {
        let prepared = prepare_shell_setup(config.clone(), Box::new(|| {})).await?;
        Ok(Self::from_prepared(prepared, stdout, stderr))
    }

    fn from_prepared(prepared: PreparedShellSetup, stdout: Stdout, stderr: Stderr) -> Self {
        let PreparedShellSetup {
            workspace,
            analyzer,
            process,
            report,
            spool_dir,
            config,
        } = prepared;
        let session_path = config.session.as_ref().map(|value| value.path.clone());

        Self {
            workspace: Some(workspace),
            analyzer,
            process,
            report: report.attach(stdout, stderr),
            session: None,
            session_path,
            active_candidates: BTreeMap::new(),
            spool_dir,
            resolved_targets: None,
            config,
            fingerprint_copy_inputs: BTreeSet::new(),
            report_versions: ReportVersions {
                os: std::env::consts::OS.to_owned(),
                hoimin: env!("CARGO_PKG_VERSION").to_owned(),
            },
            blocking_secondary_errors: Vec::new(),
        }
    }

    #[cfg(test)]
    async fn new_in(
        config: &RunConfig,
        stdout: Stdout,
        stderr: Stderr,
        temporary_parent: Utf8PathBuf,
    ) -> Result<Self, String> {
        let prepared = prepare_shell_setup_in(config.clone(), temporary_parent).await?;
        Ok(Self::from_prepared(prepared, stdout, stderr))
    }

    fn workspace(&self) -> &WorkspaceHandler {
        self.workspace
            .as_ref()
            .expect("workspace ownership is available outside an owned blocking operation")
    }

    fn workspace_mut(&mut self) -> &mut WorkspaceHandler {
        self.workspace
            .as_mut()
            .expect("workspace ownership is available outside an owned blocking operation")
    }
}

fn prepare_fingerprint(
    config: &RunConfig,
    targets: &[TargetSlice],
    resource_mode: ResourceMode,
) -> Result<hoimin_core::RunFingerprint, EffectFailed> {
    let id = EffectId(0);
    let mut sources = Vec::with_capacity(targets.len());
    for target in targets {
        let bytes = std::fs::read(config.root.join(&target.path)).map_err(|error| {
            EffectFailed::other(id, "fingerprint.source.read", error.to_string())
        })?;
        sources.push(SourceHash {
            path: target.path.clone(),
            hash: *blake3::hash(&bytes).as_bytes(),
        });
    }
    Ok(fingerprint(&FingerprintInput::from_config(
        config,
        sources,
        targets.to_vec(),
        resource_mode,
    )))
}

fn recheck_fingerprint_inputs(
    config: &RunConfig,
    root: &Utf8Path,
    manifest: &WorkspaceManifest,
    copied_at_start: &BTreeSet<Utf8PathBuf>,
    id: EffectId,
) -> Result<(), EffectFailed> {
    crate::fingerprint_inputs::recheck(
        root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
        &config.fingerprint_inputs,
    )
    .map_err(|error| {
        EffectFailed::other(id, "plan.fingerprint_input.changed", error.to_string())
    })?;
    crate::fingerprint_inputs::recheck_manifest(
        root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
        &config.fingerprint_inputs,
        manifest,
        copied_at_start,
    )
    .map_err(|error| EffectFailed::other(id, "plan.fingerprint_input.changed", error.to_string()))
}

#[cfg(windows)]
fn resource_backend(config: &RunConfig) -> Result<ResourceBackend, crate::resource::ResourceError> {
    crate::resource::WindowsBackend::new(&config.limits).map(ResourceBackend::Windows)
}

#[cfg(target_os = "linux")]
fn resource_backend(config: &RunConfig) -> Result<ResourceBackend, crate::resource::ResourceError> {
    crate::resource::select_linux_backend(
        crate::resource::probe_linux_cgroup(&config.limits),
        config.allow_best_effort_memory,
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
fn resource_backend(config: &RunConfig) -> Result<ResourceBackend, crate::resource::ResourceError> {
    PortableBackend::new(config.allow_best_effort_memory).map(ResourceBackend::Portable)
}

pub async fn execute_effect<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    if is_blocking_io_effect(&effect) {
        return match prepare_blocking_effect(context, effect) {
            Ok(task) => {
                let id = task.id();
                match run_blocking_io(id, move || task.execute()).await {
                    Ok(completion) => accept_blocking_completion(context, completion),
                    Err(error) => RunEvent::EffectFailed(error),
                }
            }
            Err(error) => RunEvent::EffectFailed(error),
        };
    }
    execute_effect_with_cancellation(context, effect, ProcessCancellation::new()).await
}

async fn execute_effect_with_cancellation<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
    cancellation: ProcessCancellation,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    debug_assert!(!is_blocking_io_effect(&effect));
    let id = effect.id();
    let result: Result<RunEvent, EffectFailed> = match effect {
        RunEffect::ResolveTargets(request) => match TargetHandler::handle(request).await {
            Ok(value) => {
                context.resolved_targets = Some(value.targets.clone());
                Ok(RunEvent::TargetsResolved(value))
            }
            Err(error) => Err(error),
        },
        RunEffect::Preflight(_)
        | RunEffect::CreateWorker(_)
        | RunEffect::ReadCandidate(_)
        | RunEffect::ApplyMutation(_)
        | RunEffect::ResetWorker(_)
        | RunEffect::VerifyOriginals(_) => {
            unreachable!("blocking effect bypassed owned dispatch")
        }
        RunEffect::RunBaseline(request) => {
            match worker_process_request(context, id, request, cancellation.clone(), None) {
                Ok(request) => context
                    .process
                    .run(request)
                    .await
                    .map(RunEvent::BaselineFinished),
                Err(error) => Err(error),
            }
        }
        RunEffect::AnalyzeFile(request) => context
            .analyzer
            .handle_with_cancellation(
                request,
                &context.config.operators,
                context.config.profile,
                cancellation.clone(),
            )
            .await
            .map(RunEvent::AnalysisFinished),
        RunEffect::RunMutant(request) => {
            match worker_process_request(context, id, request, cancellation.clone(), None) {
                Ok(request) => context
                    .process
                    .run(request)
                    .await
                    .map(RunEvent::MutantFinished),
                Err(error) => Err(error),
            }
        }
        RunEffect::ObserveRemainingBudget(_) => Err(EffectFailed::other(
            id,
            "shell.budget.scheduler",
            "remaining budget observation must run in the scheduler",
        )),
        RunEffect::EmitOutput(mut request) => {
            if let OutputEvent::RunStarted(run_started) = &mut request.event {
                run_started.versions = context.report_versions.clone();
            }
            context.report.handle(request).map(RunEvent::OutputEmitted)
        }
        RunEffect::Cleanup(_) => unreachable!("cleanup bypassed owned blocking dispatch"),
        RunEffect::LoadSession(request) => match session(context, id).await {
            Ok(handler) => handler.load(request).await.map(RunEvent::SessionLoaded),
            Err(error) => Err(error),
        },
        RunEffect::LookupStoredResult(request) => match session(context, id).await {
            Ok(handler) => handler
                .lookup(request)
                .await
                .map(RunEvent::StoredResultLoaded),
            Err(error) => Err(error),
        },
        RunEffect::BeginSession(request) => match session(context, id).await {
            Ok(handler) => handler.begin(request).await.map(RunEvent::SessionStarted),
            Err(error) => Err(error),
        },
        RunEffect::PersistResult(request) => match session(context, id).await {
            Ok(handler) => handler
                .persist(request)
                .await
                .map(RunEvent::ResultPersisted),
            Err(error) => Err(error),
        },
        RunEffect::FinishSession(request) => match session(context, id).await {
            Ok(handler) => handler.finish(request).await.map(RunEvent::SessionFinished),
            Err(error) => Err(error),
        },
    };
    result.unwrap_or_else(RunEvent::EffectFailed)
}

fn worker_process_request<Stdout, Stderr>(
    context: &ShellContext<Stdout, Stderr>,
    id: EffectId,
    mut request: RunProcess,
    cancellation: ProcessCancellation,
    start_gate: Option<ProcessStartGate>,
) -> Result<ProcessRequest, EffectFailed> {
    let worker = request.worker.ok_or_else(|| {
        EffectFailed::other(
            id,
            "shell.worker.missing",
            "process has no workspace worker",
        )
    })?;
    let inherited = std::env::vars_os().collect();
    let mut environment = context
        .workspace
        .as_ref()
        .expect("workspace ownership is available during process dispatch")
        .command_environment(worker, &inherited)
        .map_err(|error| EffectFailed::other(id, "shell.worker.environment", error.to_string()))?;
    set_worker_metadata(
        &mut environment,
        request.run_id.as_deref(),
        request.mutant_id.as_deref(),
    );
    request.cwd.clone_from(&environment.cwd);
    let request = ProcessRequest::from(request)
        .with_environment(environment)
        .with_cancellation(cancellation);
    Ok(match start_gate {
        Some(start_gate) => request.with_start_gate(start_gate),
        None => request,
    })
}

fn set_worker_metadata(
    environment: &mut crate::workspace::CommandEnvironment,
    run_id: Option<&str>,
    mutant_id: Option<&str>,
) {
    for name in ["HOIMIN_WORKER_ROOT", "HOIMIN_RUN_ID", "HOIMIN_MUTANT_ID"] {
        remove_environment_key(&mut environment.env, name);
    }
    environment.env.insert(
        std::ffi::OsString::from("HOIMIN_WORKER_ROOT"),
        std::ffi::OsString::from(environment.cwd.as_str()),
    );
    if let Some(run_id) = run_id {
        environment.env.insert(
            std::ffi::OsString::from("HOIMIN_RUN_ID"),
            std::ffi::OsString::from(run_id),
        );
    }
    if let Some(mutant_id) = mutant_id {
        environment.env.insert(
            std::ffi::OsString::from("HOIMIN_MUTANT_ID"),
            std::ffi::OsString::from(mutant_id),
        );
    }
}

fn remove_environment_key(
    environment: &mut BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    name: &str,
) {
    let keys = environment
        .keys()
        .filter(|key| key.to_string_lossy().eq_ignore_ascii_case(name))
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        environment.remove(&key);
    }
}

async fn session<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    id: EffectId,
) -> Result<SessionDispatcher, EffectFailed> {
    if context.session.is_none() {
        let path = context.session_path.clone().ok_or_else(|| {
            EffectFailed::other(id, "session.missing", "session effect without --session")
        })?;
        context.session = Some(
            SessionDispatcher::open(path)
                .await
                .map_err(|error| EffectFailed::other(id, "session.open", error.to_string()))?,
        );
    }
    Ok(context.session.as_ref().expect("initialized above").clone())
}

/// Resolves filesystem-backed records that participate in a run fingerprint.
///
/// # Errors
///
/// Returns an error when a configured fingerprint input pattern cannot be resolved.
pub fn prepare_run_config(
    mut config: RunConfig,
) -> Result<RunConfig, crate::fingerprint_inputs::FingerprintInputError> {
    config.fingerprint_inputs = crate::fingerprint_inputs::resolve(
        &config.root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
    )?;
    Ok(config)
}

/// Runs the configured mutation-test state machine.
///
/// # Errors
///
/// Returns an error when run infrastructure, state transitions, or cleanup fail.
pub async fn run_loop<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    let config = prepare_run_config(config).map_err(|error| error.to_string())?;
    run_loop_prepared(
        config,
        stdout,
        stderr,
        RunControl::new(),
        CandidateSelection::All,
        None,
    )
    .await
}

/// Runs an already-validated plan configuration for exactly the requested candidate IDs.
///
/// # Errors
///
/// Returns an error when a session or resume configuration is supplied, or when run
/// infrastructure, state transitions, or cleanup fail.
pub async fn run_selected_loop<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: BTreeSet<String>,
    verification_selection: hoimin_core::VerificationSelection,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    run_selected_loop_with_fingerprint_inputs(
        config,
        candidate_ids,
        verification_selection,
        BTreeSet::new(),
        stdout,
        stderr,
    )
    .await
}

#[doc(hidden)]
pub async fn run_selected_loop_with_fingerprint_inputs<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: BTreeSet<String>,
    verification_selection: hoimin_core::VerificationSelection,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    if config.session.is_some() || config.resume {
        return Err("selected candidate execution does not support sessions or resume".to_owned());
    }
    run_loop_prepared(
        config,
        stdout,
        stderr,
        RunControl::new(),
        CandidateSelection::Explicit(candidate_ids, verification_selection),
        Some(fingerprint_copy_inputs),
    )
    .await
}

/// Runs an already-validated plan configuration in saved manifest rank order.
///
/// # Errors
///
/// Returns an error when run infrastructure, state transitions, or cleanup fail.
pub async fn run_ordered_selected_loop<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: Vec<String>,
    verification_selection: hoimin_core::VerificationSelection,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    run_ordered_selected_loop_with_fingerprint_inputs(
        config,
        candidate_ids,
        verification_selection,
        BTreeSet::new(),
        stdout,
        stderr,
    )
    .await
}

pub(crate) async fn run_ordered_selected_loop_with_fingerprint_inputs<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: Vec<String>,
    verification_selection: hoimin_core::VerificationSelection,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    if config.session.is_some() || config.resume {
        return Err("selected candidate execution does not support sessions or resume".to_owned());
    }
    run_loop_prepared(
        config,
        stdout,
        stderr,
        RunControl::new(),
        CandidateSelection::Ordered(candidate_ids, verification_selection),
        Some(fingerprint_copy_inputs),
    )
    .await
}

#[doc(hidden)]
pub async fn run_loop_with_control<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    let config = prepare_run_config(config).map_err(|error| error.to_string())?;
    run_loop_prepared(
        config,
        stdout,
        stderr,
        control,
        CandidateSelection::All,
        None,
    )
    .await
}

enum CandidateSelection {
    All,
    Explicit(BTreeSet<String>, hoimin_core::VerificationSelection),
    Ordered(Vec<String>, hoimin_core::VerificationSelection),
}

#[expect(
    clippy::too_many_lines,
    reason = "the loop keeps cancellation, completion, and state-transition ordering in one auditable sequence"
)]
async fn run_loop_prepared<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
    candidate_selection: CandidateSelection,
    fingerprint_copy_inputs: Option<BTreeSet<Utf8PathBuf>>,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    let metrics_path = config.output.metrics.clone();
    #[cfg(not(test))]
    let mut context = ShellContext::new(&config, stdout, stderr).await?;
    #[cfg(test)]
    let mut context = ShellContext::new_in(
        &config,
        stdout,
        stderr,
        Utf8PathBuf::from_path_buf(control.managed_parent.path().to_owned())
            .map_err(|_| "test managed parent is not UTF-8".to_owned())?,
    )
    .await?;
    #[cfg(test)]
    control.observe_managed_roots(&context.spool_dir);
    #[cfg(test)]
    if control.force_process_reap_failure.load(Ordering::Acquire) {
        context.process.inject_process_reap_failure();
    }
    #[cfg(test)]
    if let Some(pause) = control.materialization_pause.clone() {
        context.workspace_mut().set_materialization_pause(pause);
    }
    if let Some(fingerprint_copy_inputs) = fingerprint_copy_inputs {
        context.fingerprint_copy_inputs = fingerprint_copy_inputs;
    }
    let deadline = tokio::time::Instant::now() + config.limits.total_timeout.get();
    let shutdown_grace = control.shutdown_grace();
    let max_jobs = config.limits.jobs.get();
    let channel_capacity = config.limits.jobs.get().saturating_add(1);
    let mut metrics = None;
    let mut metrics_warnings = Vec::new();
    let mut discovered = 0_u64;
    let mut executed = 0_u64;
    let initial_run_id = Uuid::new_v4().to_string();
    let mut diagnostic_run_id = initial_run_id.clone();
    let mut shutdown_budget = None;
    let mut shutdown_expiry_reported = false;
    let mut interrupts = crate::interrupt::InterruptMonitor::spawn();
    let mut disk_monitor = None;
    let mut execution_safety_proven = false;
    let mut final_measurement_joined = true;
    let mut execution_cleaned = false;
    let mut delivery_cleaned = false;
    let mut execution_cleanup_attempted = false;
    let mut delivery_cleanup_attempted = false;
    let mut execution_preclean = None;
    let mut execution_end_available: Option<BTreeMap<FilesystemKey, u64>> = None;
    let mut execution_cleanup_record: Option<CleanupRecord> = None;
    let mut finalization_errors = Vec::new();
    let mut disk_lifecycle = ShellDiskLifecycle::new([DiskRootId::Execution, DiskRootId::Delivery])
        .map_err(|error| format!("disk lifecycle initialization failed: {error}"))?;
    let run_result = async {
        let mut state = Box::new(match candidate_selection {
            CandidateSelection::Explicit(candidate_ids, verification_selection) => {
                RunState::with_candidate_filter(initial_run_id.clone(), config, candidate_ids)
                    .with_verification_selection(verification_selection)
            }
            CandidateSelection::Ordered(candidate_ids, verification_selection) => {
                RunState::with_ordered_candidate_filter(
                    initial_run_id.clone(),
                    config,
                    candidate_ids,
                )
                .with_verification_selection(verification_selection)
            }
            CandidateSelection::All => RunState::new(initial_run_id.clone(), config),
        });
        let (next, initial) = transition(*state, RunEvent::StartRequested(StartRequested))
            .map_err(|error| error.to_string())?;
        *state = next;
        track_diagnostic_run_id(&mut diagnostic_run_id, &state);
        let monitor = start_disk_monitor(&context, &control).await?;
        let mut disk_stop = monitor.stop_receiver();
        disk_monitor = Some(monitor);
        let mut disk_stop_delivered = false;
        #[cfg(test)]
        let mut disk_stop_order_observed = false;
        if metrics_path.is_some() {
            let mut collector = MetricsCollector::new(state.run_id());
            if let Err(error) = collector.begin_stage("targets") {
                metrics_warnings.push(("metrics.state", error.to_string()));
            }
            metrics = Some(collector);
        }
        let mut effects = VecDeque::from(initial);
        record_ready_processes(&effects, &mut metrics, &mut metrics_warnings);
        let cancellation = ProcessCancellation::new();
        let (completion_tx, mut completion_rx) = mpsc::channel(channel_capacity);
        let mut process_tasks = JoinSet::new();
        let mut io_tasks = JoinSet::new();
        #[cfg(test)]
        let mut final_output_ids = BTreeSet::new();
        let mut in_flight = 0_usize;
        let mut io_in_flight = 0_usize;
        let mut stop_signalled = false;

        while state.phase() != RunPhase::Finished {
            let mut serial_completion = None;
            let mut priority_event = if disk_stop_delivered {
                None
            } else {
                disk_stop.borrow_and_update().clone().map(|failure| {
                    disk_stop_delivered = true;
                    cancellation.cancel();
                    stop_signalled = true;
                    RunEvent::DiskStopRequested(DiskStopRequested { failure })
                })
            };
            if let Some(event) = &priority_event {
                establish_event_shutdown_budget(
                    &mut shutdown_budget,
                    event,
                    deadline,
                    tokio::time::Instant::now(),
                    shutdown_grace,
                );
            }
            let mut signal_failure = None;
            let ready_process_completion = if !stop_signalled
                && !control.is_cancelled()
                && tokio::time::Instant::now() < deadline
            {
                completion_rx.try_recv().ok()
            } else {
                None
            };
            while ready_process_completion.is_none() && priority_event.is_none() {
                let Some(effect) = effects.pop_front() else {
                    break;
                };
                #[cfg(test)]
                if matches!(
                    &effect,
                    RunEffect::EmitOutput(value)
                        if matches!(&value.event, OutputEvent::RunFinished(_))
                ) {
                    final_output_ids.insert(effect.id());
                }
                #[cfg(test)]
                control.before_effect_dispatch(&effect);
                if !stop_signalled
                    && (control.is_cancelled() || tokio::time::Instant::now() >= deadline)
                {
                    effects.push_front(effect);
                    cancellation.cancel();
                    stop_signalled = true;
                    let event = if control.is_cancelled() {
                        RunEvent::CancellationRequested
                    } else {
                        RunEvent::DeadlineReached
                    };
                    establish_event_shutdown_budget(
                        &mut shutdown_budget,
                        &event,
                        deadline,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    );
                    priority_event = Some(event);
                    break;
                }
                if !state.is_effect_pending(effect.id()) {
                    cancel_queued_effect(&effect, &mut metrics, &mut metrics_warnings);
                    continue;
                }
                if matches!(effect, RunEffect::Cleanup(_)) && !execution_safety_proven {
                    let monitor = disk_monitor
                        .as_ref()
                        .expect("disk monitor started before cleanup");
                    let cleanup_budget = shutdown_budget.unwrap_or_else(|| {
                        ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                    });
                    match cleanup_budget.wait(monitor.sample_now()).await {
                        Ok(Some(failure)) if !disk_stop_delivered => {
                            effects.push_front(effect);
                            cancellation.cancel();
                            stop_signalled = true;
                            disk_stop_delivered = true;
                            let event =
                                RunEvent::DiskStopRequested(DiskStopRequested { failure });
                            establish_event_shutdown_budget(
                                &mut shutdown_budget,
                                &event,
                                deadline,
                                tokio::time::Instant::now(),
                                shutdown_grace,
                            );
                            priority_event = Some(event);
                            break;
                        }
                        Ok(_) => {}
                        Err(_) => finalization_errors.push(
                            "disk.measurement.failed: final pre-clean sample exceeded the shutdown budget"
                                .to_owned(),
                        ),
                    }
                    let remaining = cleanup_budget
                        .deadline()
                        .saturating_duration_since(tokio::time::Instant::now());
                    let process_drain = context
                        .process
                        .drain_for_shutdown(remaining)
                        .await;
                    #[cfg(test)]
                    {
                        if process_drain.all_reaped {
                            control.observe_finalization_event("root_reaped");
                        }
                        if process_drain.output_drains_joined {
                            control.observe_finalization_event("output_drained");
                        }
                    }
                    finalization_errors.extend(process_drain.secondary_errors);
                    let measurement_root = Arc::clone(&context.spool_dir.execution_root);
                    #[cfg(test)]
                    let final_measurement_pause = control.final_measurement_pause.clone();
                    let measurement = run_cleanup_thread(&cleanup_budget, move || {
                        #[cfg(test)]
                        if let Some(pause) = final_measurement_pause {
                            pause.wait();
                        }
                        measure_managed_roots(&[measurement_root])
                    })
                    .await;
                    let measurement = match measurement {
                        Ok(measurement) => measurement,
                        Err(error) => {
                            final_measurement_joined = false;
                            Err(std::io::Error::new(std::io::ErrorKind::TimedOut, error))
                        }
                    };
                    match measurement {
                        Ok(reading) => execution_preclean = Some(reading),
                        Err(error) if !disk_stop_delivered => {
                            effects.push_front(effect);
                            cancellation.cancel();
                            stop_signalled = true;
                            disk_stop_delivered = true;
                            let event = RunEvent::DiskStopRequested(DiskStopRequested {
                                failure: hoimin_core::DiskFailure {
                                    code: hoimin_core::DISK_MEASUREMENT_FAILED.to_owned(),
                                    reason: hoimin_core::DiskStopReason::MeasurementFailed,
                                    observation: None,
                                    message: Some(error.to_string()),
                                    secondary: Vec::new(),
                                },
                            });
                            establish_event_shutdown_budget(
                                &mut shutdown_budget,
                                &event,
                                deadline,
                                tokio::time::Instant::now(),
                                shutdown_grace,
                            );
                            priority_event = Some(event);
                            break;
                        }
                        Err(error) => finalization_errors.push(format!(
                            "disk.measurement.failed: final execution measurement failed: {error}"
                        )),
                    }
                    #[cfg(test)]
                    control.observe_finalization_event("monitor_stopped");
                    let remaining = cleanup_budget
                        .deadline()
                        .saturating_duration_since(tokio::time::Instant::now());
                    let monitor_joined = monitor.stop_and_join(remaining).await;
                    #[cfg(test)]
                    if monitor_joined {
                        control.observe_finalization_event("monitor_joined");
                    }
                    for (event, component) in [
                        (
                            if process_drain.all_reaped {
                                DiskLifecycleEvent::ProcessDrainSucceeded
                            } else {
                                DiskLifecycleEvent::ProcessDrainFailed
                            },
                            "process drain",
                        ),
                        (
                            if process_drain.output_drains_joined {
                                DiskLifecycleEvent::OutputDrainSucceeded
                            } else {
                                DiskLifecycleEvent::OutputDrainFailed
                            },
                            "output drain",
                        ),
                        (
                            if monitor_joined {
                                DiskLifecycleEvent::MonitorJoinSucceeded
                            } else {
                                DiskLifecycleEvent::MonitorJoinFailed
                            },
                            "monitor join",
                        ),
                    ] {
                        if !disk_lifecycle.apply(event) {
                            return Err(format!(
                                "disk lifecycle rejected {component} completion"
                            ));
                        }
                    }
                    if !cleanup_quiescence_proven([
                        process_drain.all_reaped,
                        process_drain.output_drains_joined,
                        monitor_joined,
                        final_measurement_joined,
                    ]) {
                        let id = effect.id();
                        execution_cleanup_record = Some(CleanupRecord {
                            status: hoimin_core::DiskCleanupStatus::Deferred,
                            examined_entries: 0,
                            removed_entries: 0,
                            details: vec![CLEANUP_QUIESCENCE_UNPROVEN.to_owned()],
                            omitted_detail_count: 0,
                            remaining_root: Some(
                                context.spool_dir.execution_root.path().to_owned(),
                            ),
                        });
                        serial_completion = Some(ShellCompletion {
                            event: RunEvent::EffectFailed(EffectFailed::other(
                                id,
                                hoimin_core::WORKSPACE_CLEANUP_DEFERRED,
                                CLEANUP_QUIESCENCE_UNPROVEN,
                            )),
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        });
                        effects.push_front(effect);
                        break;
                    }
                    execution_safety_proven = true;
                }
                if is_disk_guarded_dispatch(&effect) {
                    let failure = disk_monitor
                        .as_ref()
                        .expect("disk monitor started before effect dispatch")
                        .sample_now()
                        .await;
                    if let Some(failure) = failure {
                        effects.push_front(effect);
                        cancellation.cancel();
                        stop_signalled = true;
                        disk_stop_delivered = true;
                        let event = RunEvent::DiskStopRequested(DiskStopRequested { failure });
                        establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        priority_event = Some(event);
                        break;
                    }
                    if !disk_lifecycle.apply(DiskLifecycleEvent::DispatchRequested) {
                        return Err(
                            "disk lifecycle rejected guarded effect dispatch".to_owned(),
                        );
                    }
                    #[cfg(test)]
                    control.observe_effect_dispatch();
                }
                match effect {
                    RunEffect::RunBaseline(request) => {
                        let id = request.id;
                        let worker = request.worker;
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                let Some(worker) = worker else {
                                    unreachable!("worker process request accepted without worker")
                                };
                                let Some(dispatch) = control.begin_dispatch() else {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    );
                                    priority_event = Some(event);
                                    break;
                                };
                                spawn_process(
                                    Arc::clone(&context.process),
                                    request,
                                    worker,
                                    true,
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    dispatch,
                                );
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                });
                            }
                        }
                        if serial_completion.is_none() {
                            control.observe_process_tasks(process_tasks.len());
                            debug_assert!(process_tasks.len() <= max_jobs);
                            in_flight += 1;
                            control.observe_completion_in_flight(in_flight);
                        }
                    }
                    RunEffect::RunMutant(request) => {
                        let id = request.id;
                        let worker = request.worker;
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                let Some(worker) = worker else {
                                    unreachable!("worker process request accepted without worker")
                                };
                                let Some(dispatch) = accept_process_dispatch(
                                    &control,
                                    &mut metrics,
                                    &mut metrics_warnings,
                                    worker,
                                ) else {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    );
                                    priority_event = Some(event);
                                    break;
                                };
                                spawn_process(
                                    Arc::clone(&context.process),
                                    request,
                                    worker,
                                    false,
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    dispatch,
                                );
                            }
                            Err(error) => {
                                cancel_queued_worker(worker, &mut metrics, &mut metrics_warnings);
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                });
                            }
                        }
                        if serial_completion.is_none() {
                            control.observe_process_tasks(process_tasks.len());
                            debug_assert!(process_tasks.len() <= max_jobs);
                            in_flight += 1;
                            control.observe_completion_in_flight(in_flight);
                        }
                    }
                    effect if is_blocking_io_effect(&effect) => {
                        match prepare_blocking_effect(&mut context, effect) {
                            Ok(task) => {
                                spawn_blocking_effect(task, completion_tx.clone(), &mut io_tasks);
                                in_flight += 1;
                                io_in_flight += 1;
                                debug_assert!(io_in_flight <= max_jobs);
                                control.observe_completion_in_flight(in_flight);
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                });
                            }
                        }
                    }
                    // Wall-clock observation is scheduler-owned so it reads the same absolute
                    // deadline that enforces the total timeout, without spawning worker work.
                    RunEffect::ObserveRemainingBudget(request) => {
                        serial_completion = Some(ShellCompletion {
                            event: RunEvent::RemainingBudgetObserved(remaining_budget_observed(
                                &request,
                                deadline,
                                tokio::time::Instant::now(),
                            )),
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        });
                    }
                    RunEffect::EmitOutput(mut request)
                        if matches!(&request.event, OutputEvent::RunFinished(_)) =>
                    {
                        if let OutputEvent::RunFinished(summary) = &mut request.event {
                            let disk_stats = disk_monitor
                                .as_ref()
                                .expect("disk monitor exists through final reporting")
                                .stats();
                            summary.disk.peak_owned_bytes = disk_stats.peak_owned_bytes;
                            summary.disk.minimum_available_bytes =
                                disk_stats.minimum_available_bytes;
                            summary.disk.sample_count = disk_stats.sample_count;
                            summary.disk.maximum_measurement_ms =
                                u64::try_from(disk_stats.maximum_measurement.as_millis())
                                    .unwrap_or(u64::MAX);
                            summary.disk.stale_roots_reclaimed =
                                context.spool_dir.startup_reclaim.reclaimed_roots;
                            summary.disk.removed_logical_bytes = execution_cleaned
                                .then(|| execution_preclean.as_ref().map(|value| value.owned_bytes))
                                .flatten();
                            summary.disk.filesystems = disk_stats
                                .filesystems
                                .iter()
                                .map(|(key, filesystem)| {
                                    let end_available_bytes = execution_end_available
                                        .as_ref()
                                        .and_then(|values| values.get(key))
                                        .copied();
                                    hoimin_core::DiskFilesystemReport {
                                        key: key.0.to_string(),
                                        start_available_bytes: Some(filesystem.start),
                                        minimum_available_bytes: Some(filesystem.minimum),
                                        end_available_bytes,
                                        available_bytes_change: end_available_bytes.map(|end| {
                                            i128::from(end) - i128::from(filesystem.start)
                                        }),
                                    }
                                })
                                .collect();
                            if let Some(cleanup) = &execution_cleanup_record
                                && let Some(report) = summary
                                    .disk
                                    .cleanup
                                    .iter_mut()
                                    .find(|report| report.root_id == "execution")
                            {
                                apply_execution_cleanup_evidence(
                                    report,
                                    cleanup,
                                    &context.spool_dir.startup_reclaim,
                                );
                            }
                            let latest_stop = {
                                let receiver = disk_monitor
                                    .as_ref()
                                    .expect("disk monitor exists through final reporting")
                                    .stop_receiver();
                                receiver.borrow().clone()
                            };
                            if let (Some(report), Some(latest)) =
                                (&mut summary.disk.stop, latest_stop)
                            {
                                merge_monitor_stop(report, latest);
                            }
                            if let Some(failure) = disk_lifecycle.snapshot().stop {
                                merge_lifecycle_stop(summary, failure);
                            }
                            apply_finalization_errors_to_summary(summary, &finalization_errors);
                            if summary.disk.enforcement.is_empty() {
                                summary
                                    .disk
                                    .enforcement
                                    .push(hoimin_core::DiskEnforcementReport::PortableGuard);
                            }
                            if !summary
                                .disk
                                .cleanup
                                .iter()
                                .any(|record| record.root_id == "delivery")
                            {
                                summary.disk.cleanup.push(hoimin_core::DiskCleanupReport {
                                    root_id: "delivery".to_owned(),
                                    owner: "hoimin".to_owned(),
                                    status: hoimin_core::DiskCleanupStatus::CleanupAfterDelivery,
                                    examined_entries: 0,
                                    removed_entries: 0,
                                    details: Vec::new(),
                                    omitted_detail_count: 0,
                                    remaining_root: None,
                                });
                            }
                        }
                        let emitted = context.report.handle(request);
                        #[cfg(test)]
                        if emitted.is_ok() {
                            control.observe_finalization_event("run_finished_written");
                        }
                        let mut delivery_errors = Vec::new();
                        let report_flushed = match context.report.flush_and_release_spool() {
                            Ok(()) => true,
                            Err(error) => {
                                delivery_errors.push(format!(
                                    "report.finalization.failed: report flush before delivery cleanup failed: {error}"
                                ));
                                false
                            }
                        };
                        let report_event = if emitted.is_ok() && report_flushed {
                            DiskLifecycleEvent::ReportSucceeded
                        } else {
                            DiskLifecycleEvent::ReportFailed
                        };
                        if !disk_lifecycle.apply(report_event) {
                            return Err("disk lifecycle rejected report completion".to_owned());
                        }
                        context.spool_dir.release_delivery_spool();
                        #[cfg(test)]
                        control.before_delivery_cleanup(&context.spool_dir.delivery_root);
                        let cleanup_budget = shutdown_budget.unwrap_or_else(|| {
                            ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                        });
                        #[cfg(test)]
                        control.observe_finalization_event("delivery_cleanup_requested_once");
                        delivery_cleanup_attempted = true;
                        if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                            root: DiskRootId::Delivery,
                        }) {
                            return Err(
                                "disk lifecycle rejected delivery cleanup request".to_owned(),
                            );
                        }
                        if execution_safety_proven {
                            match cleanup_managed_root(
                                Arc::clone(&context.spool_dir.delivery_root),
                                &cleanup_budget,
                            )
                            .await
                            {
                            ManagedCleanupOutcome::Clean(cleanup) => {
                                delivery_errors
                                    .extend(delivery_cleanup_secondary_errors(&cleanup));
                                if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                    root: DiskRootId::Delivery,
                                    outcome: DiskCleanupOutcome::Clean,
                                }) {
                                    return Err(
                                        "disk lifecycle rejected clean delivery cleanup".to_owned(),
                                    );
                                }
                                delivery_cleaned = true;
                                #[cfg(test)]
                                {
                                    control.observe_finalization_event("delivery_root_absent");
                                    control
                                        .observe_finalization_event("delivery_workspace_absent");
                                }
                            }
                            ManagedCleanupOutcome::Failed { error, .. }
                            | ManagedCleanupOutcome::Deferred { error, .. } => {
                                let outcome = if error.starts_with("workspace.cleanup.deferred:") {
                                    DiskCleanupOutcome::Deferred(error.clone())
                                } else {
                                    DiskCleanupOutcome::Failed(error.clone())
                                };
                                if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                    root: DiskRootId::Delivery,
                                    outcome,
                                }) {
                                    return Err(
                                        "disk lifecycle rejected incomplete delivery cleanup"
                                            .to_owned(),
                                    );
                                }
                                context.spool_dir.delivery_root.abandon_for_janitor(
                                    "delivery-root cleanup did not complete".to_owned(),
                                );
                                delivery_errors.push(error);
                            }
                            }
                        } else {
                            let detail = CLEANUP_QUIESCENCE_UNPROVEN.to_owned();
                            context
                                .spool_dir
                                .delivery_root
                                .abandon_for_janitor(detail.clone());
                            if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                root: DiskRootId::Delivery,
                                outcome: DiskCleanupOutcome::Deferred(detail.clone()),
                            }) {
                                return Err(
                                    "disk lifecycle rejected deferred delivery cleanup".to_owned(),
                                );
                            }
                            delivery_errors.push(format!(
                                "{}: {detail}",
                                hoimin_core::WORKSPACE_CLEANUP_DEFERRED
                            ));
                        }
                        let mut terminal_errors = finalization_errors.clone();
                        let output = match emitted {
                            Ok(value) => Some(value),
                            Err(error) => {
                                terminal_errors
                                    .push(format!("report.finalization.failed: {error:?}"));
                                None
                            }
                        };
                        terminal_errors.extend(delivery_errors);
                        if !terminal_errors.is_empty() {
                            let error = terminal_errors.join("; ");
                            finalization_errors.clear();
                            return Err(error);
                        }
                        if !disk_lifecycle.apply(DiskLifecycleEvent::FinishRequested) {
                            return Err("disk lifecycle rejected run completion".to_owned());
                        }
                        let event = RunEvent::OutputEmitted(
                            output.expect("successful finalization retains output acknowledgement"),
                        );
                        serial_completion = Some(ShellCompletion {
                            event,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        });
                    }
                    effect => {
                        let stopping = stop_signalled;
                        let event = if stopping {
                            let budget = shutdown_budget
                                .ok_or_else(|| "stopping run has no shutdown budget".to_owned())?;
                            let mut execution = Box::pin(execute_effect_with_cancellation(
                                &mut context,
                                effect,
                                cancellation.clone(),
                            ));
                            let Ok(event) = budget.wait(&mut execution).await else {
                                drop(execution);
                                let error = shutdown_expiry_error(
                                    &mut process_tasks,
                                    &mut io_tasks,
                                    &mut completion_rx,
                                    &mut in_flight,
                                    &budget,
                                    &mut shutdown_expiry_reported,
                                    &mut metrics,
                                    &mut metrics_warnings,
                                    |completion| {
                                        let _ =
                                            accept_blocking_completion(&mut context, completion);
                                    },
                                )
                                .await;
                                return Err(error);
                            };
                            event
                        } else {
                            enum SerialSelection {
                                Completed(RunEvent),
                                Stopped {
                                    event: RunEvent,
                                    budget: ShutdownBudget,
                                    signal_failure: Option<String>,
                                },
                            }
                            let mut execution = Box::pin(execute_effect_with_cancellation(
                                &mut context,
                                effect,
                                cancellation.clone(),
                            ));
                            let selection = tokio::select! {
                                biased;
                                changed = disk_stop.changed(), if !disk_stop_delivered && !execution_safety_proven => {
                                    changed.map_err(|_| {
                                        "disk.measurement.failed: disk monitor stop channel closed"
                                            .to_owned()
                                    })?;
                                    let failure = disk_stop
                                        .borrow_and_update()
                                        .clone()
                                        .ok_or_else(|| {
                                            "disk.measurement.failed: disk monitor signalled without a failure"
                                                .to_owned()
                                        })?;
                                    disk_stop_delivered = true;
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::DiskStopRequested(DiskStopRequested {
                                        failure,
                                    });
                                    let budget = establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    )
                                    .expect("disk stop establishes a shutdown budget");
                                    SerialSelection::Stopped {
                                        event,
                                        budget,
                                        signal_failure: None,
                                    }
                                }
                                event = &mut execution => SerialSelection::Completed(event),
                                () = control.cancelled() => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    let budget = establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    )
                                    .expect("cancellation establishes a shutdown budget");
                                    SerialSelection::Stopped {
                                        event,
                                        budget,
                                        signal_failure: None,
                                    }
                                }
                                () = tokio::time::sleep_until(deadline) => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::DeadlineReached;
                                    let budget = establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    )
                                    .expect("deadline establishes a shutdown budget");
                                    SerialSelection::Stopped {
                                        event,
                                        budget,
                                        signal_failure: None,
                                    }
                                }
                                signal = interrupts.first() => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    match first_interrupt_event(signal) {
                                        Ok(event) => {
                                            let budget = establish_event_shutdown_budget(
                                                &mut shutdown_budget,
                                                &event,
                                                deadline,
                                                tokio::time::Instant::now(),
                                                shutdown_grace,
                                            )
                                            .expect("interrupt establishes a shutdown budget");
                                            SerialSelection::Stopped {
                                                event,
                                                budget,
                                                signal_failure: None,
                                            }
                                        }
                                        Err(error) => {
                                            let budget = establish_shutdown_budget(
                                                &mut shutdown_budget,
                                                ShutdownBudget::after_observation_with_grace(
                                                    ShutdownCause::Failure,
                                                    tokio::time::Instant::now(),
                                                    shutdown_grace,
                                                ),
                                            );
                                            SerialSelection::Stopped {
                                                event: RunEvent::CancellationRequested,
                                                budget,
                                                signal_failure: Some(error),
                                            }
                                        }
                                    }
                                }
                            };
                            match selection {
                                SerialSelection::Completed(event) => event,
                                SerialSelection::Stopped {
                                    event,
                                    budget,
                                    signal_failure: pending_signal_failure,
                                } => {
                                    if budget.wait(&mut execution).await.is_err() {
                                        drop(execution);
                                        let expiry_error = shutdown_expiry_error(
                                            &mut process_tasks,
                                            &mut io_tasks,
                                            &mut completion_rx,
                                            &mut in_flight,
                                            &budget,
                                            &mut shutdown_expiry_reported,
                                            &mut metrics,
                                            &mut metrics_warnings,
                                            |completion| {
                                                let _ = accept_blocking_completion(
                                                    &mut context,
                                                    completion,
                                                );
                                            },
                                        )
                                        .await;
                                        return Err(match pending_signal_failure {
                                            Some(primary) => {
                                                combine_shutdown_errors(primary, Some(expiry_error))
                                            }
                                            None => expiry_error,
                                        });
                                    }
                                    signal_failure = pending_signal_failure;
                                    event
                                }
                            }
                        };
                        if matches!(
                            event,
                            RunEvent::DeadlineReached
                                | RunEvent::CancellationRequested
                                | RunEvent::DiskStopRequested(_)
                        ) {
                            priority_event = Some(event);
                        } else {
                            serial_completion = Some(ShellCompletion {
                                event,
                                process_task: false,
                                io_task: false,
                                process: None,
                                blocking: None,
                            });
                        }
                    }
                }
                if serial_completion.is_some() {
                    break;
                }
            }

            if let Some(error) = signal_failure.take() {
                cancellation.cancel();
                let drain_budget = establish_shutdown_budget(
                    &mut shutdown_budget,
                    ShutdownBudget::after_observation_with_grace(
                        ShutdownCause::Failure,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    ),
                );
                let drain_failure = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &drain_budget,
                    &mut shutdown_expiry_reported,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await
                .err();
                return Err(combine_shutdown_errors(error, drain_failure));
            }

            if in_flight == 0
                && priority_event.is_none()
                && serial_completion.is_none()
                && ready_process_completion.is_none()
            {
                return Err(format!("run stalled in {:?}", state.phase()));
            }

            let completion = if let Some(completion) = serial_completion {
                completion
            } else if let Some(event) = priority_event {
                ShellCompletion {
                    event,
                    process_task: false,
                    io_task: false,
                    process: None,
                    blocking: None,
                }
            } else if let Some(completion) = ready_process_completion {
                completion
            } else if stop_signalled {
                let budget = shutdown_budget
                    .ok_or_else(|| "stopping run has no shutdown budget".to_owned())?;
                match budget.wait(completion_rx.recv()).await {
                    Ok(Some(completion)) => completion,
                    Ok(None) => return Err("completion channel closed".to_owned()),
                    Err(_) => {
                        let error = shutdown_expiry_error(
                            &mut process_tasks,
                            &mut io_tasks,
                            &mut completion_rx,
                            &mut in_flight,
                            &budget,
                            &mut shutdown_expiry_reported,
                            &mut metrics,
                            &mut metrics_warnings,
                            |completion| {
                                let _ = accept_blocking_completion(&mut context, completion);
                            },
                        )
                        .await;
                        return Err(error);
                    }
                }
            } else {
                tokio::select! {
                    biased;
                    changed = disk_stop.changed(), if !disk_stop_delivered && !execution_safety_proven => {
                        changed.map_err(|_| {
                            "disk.measurement.failed: disk monitor stop channel closed".to_owned()
                        })?;
                        let failure = disk_stop
                            .borrow_and_update()
                            .clone()
                            .ok_or_else(|| {
                                "disk.measurement.failed: disk monitor signalled without a failure"
                                    .to_owned()
                            })?;
                        disk_stop_delivered = true;
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::DiskStopRequested(DiskStopRequested { failure });
                        establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion {
                            event,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        }
                    }
                    () = control.cancelled() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::CancellationRequested;
                        establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion {
                            event,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        }
                    }
                    () = tokio::time::sleep_until(deadline) => {
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::DeadlineReached;
                        establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion {
                            event,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        }
                    }
                    signal = interrupts.first() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        match first_interrupt_event(signal) {
                            Ok(event) => {
                                establish_event_shutdown_budget(
                                    &mut shutdown_budget,
                                    &event,
                                    deadline,
                                    tokio::time::Instant::now(),
                                    shutdown_grace,
                                );
                                ShellCompletion {
                                    event,
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                }
                            },
                            Err(error) => {
                                signal_failure = Some(error);
                                establish_shutdown_budget(
                                    &mut shutdown_budget,
                                    ShutdownBudget::after_observation_with_grace(
                                        ShutdownCause::Failure,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    ),
                                );
                                ShellCompletion {
                                    event: RunEvent::CancellationRequested,
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                }
                            }
                        }
                    }
                    event = completion_rx.recv() => {
                        event.ok_or_else(|| "completion channel closed".to_owned())?
                    }
                }
            };
            let mut completion = completion;
            let post_drain_failure = if stop_signalled || !completion.process_task {
                None
            } else {
                #[cfg(test)]
                if let Some(failure) = control.take_post_drain_disk_failure() {
                    Some(failure)
                } else {
                    sample_after_process_drain(
                        disk_monitor
                            .as_ref()
                            .expect("disk monitor started before process completion"),
                        true,
                    )
                    .await
                }
                #[cfg(not(test))]
                sample_after_process_drain(
                    disk_monitor
                        .as_ref()
                        .expect("disk monitor started before process completion"),
                    true,
                )
                .await
            };
            if let Some(mut failure) = post_drain_failure {
                if let RunEvent::EffectFailed(error) = &completion.event {
                    failure.secondary.push(hoimin_core::DiskSecondary::Error {
                        code: error.failure.code().to_owned(),
                        message: error.failure.message(),
                    });
                }
                let process_failure = match process_tasks.join_next().await {
                    Some(Ok(())) => None,
                    Some(Err(error)) => Some(format!("process task failed: {error}")),
                    None => Some("process completion had no task".to_owned()),
                };
                if let Some(error) = process_failure {
                    failure.secondary.push(hoimin_core::DiskSecondary::Error {
                        code: "process.drain.failed".to_owned(),
                        message: error,
                    });
                }
                if let Some((worker, _)) = completion.process {
                    record_metrics(&mut metrics, &mut metrics_warnings, |metrics| {
                        metrics.process_finished(worker, false)
                    });
                }
                in_flight = in_flight.saturating_sub(1);
                cancellation.cancel();
                stop_signalled = true;
                disk_stop_delivered = true;
                let event = RunEvent::DiskStopRequested(DiskStopRequested { failure });
                establish_event_shutdown_budget(
                    &mut shutdown_budget,
                    &event,
                    deadline,
                    tokio::time::Instant::now(),
                    shutdown_grace,
                );
                completion = ShellCompletion {
                    event,
                    process_task: false,
                    io_task: false,
                    process: None,
                    blocking: None,
                };
            }
            #[cfg(test)]
            if matches!(&completion.event, RunEvent::DiskStopRequested(_))
                && !disk_stop_order_observed
            {
                disk_stop_order_observed = true;
                control.observe_finalization_event("disk_stop");
                control.observe_finalization_event("dispatch_gate_closed");
                control.observe_finalization_event("root_termination_requested");
            }
            if let Some(error) = signal_failure.take() {
                cancellation.cancel();
                let drain_budget = establish_shutdown_budget(
                    &mut shutdown_budget,
                    ShutdownBudget::after_observation_with_grace(
                        ShutdownCause::Failure,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    ),
                );
                let drain_failure = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &drain_budget,
                    &mut shutdown_expiry_reported,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await
                .err();
                return Err(combine_shutdown_errors(error, drain_failure));
            }
            let ShellCompletion {
                mut event,
                process_task: process_completion,
                io_task: io_completion,
                process,
                blocking,
            } = completion;
            #[cfg(test)]
            let final_output_completed = matches!(
                &event,
                RunEvent::OutputEmitted(value) if final_output_ids.remove(&value.id)
            );
            if let Some(blocking) = blocking {
                event = accept_blocking_completion(&mut context, *blocking);
            }
            finalization_errors.append(&mut context.blocking_secondary_errors);
            if let RunEvent::CleanupFinished(value) = &event
                && execution_safety_proven
                && !execution_cleaned
            {
                let id = value.id;
                context.analyzer.release_candidate_spool();
                context.spool_dir.release_execution_spool();
                let cleanup_budget = shutdown_budget.unwrap_or_else(|| {
                    ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                });
                #[cfg(test)]
                control.observe_finalization_event("workspace_cleanup_requested_once");
                execution_cleanup_attempted = true;
                #[cfg(test)]
                if control.take_duplicate_execution_cleanup_request()
                    && !disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                        root: DiskRootId::Execution,
                    })
                {
                    return Err(
                        "disk lifecycle rejected injected execution cleanup request".to_owned(),
                    );
                }
                if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Execution,
                }) {
                    return Err(
                        "disk lifecycle rejected execution cleanup request".to_owned(),
                    );
                }
                #[cfg(test)]
                let cleanup_result = if control.take_execution_cleanup_deferred() {
                    ManagedCleanupOutcome::Deferred {
                        record: None,
                        error: "workspace.cleanup.deferred: injected execution cleanup timeout"
                            .to_owned(),
                    }
                } else {
                    cleanup_managed_root(
                        Arc::clone(&context.spool_dir.execution_root),
                        &cleanup_budget,
                    )
                    .await
                };
                #[cfg(not(test))]
                let cleanup_result = cleanup_managed_root(
                    Arc::clone(&context.spool_dir.execution_root),
                    &cleanup_budget,
                )
                .await;
                match cleanup_result {
                    ManagedCleanupOutcome::Clean(cleanup) => {
                        if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                            root: DiskRootId::Execution,
                            outcome: DiskCleanupOutcome::Clean,
                        }) {
                            return Err(
                                "disk lifecycle rejected clean execution cleanup".to_owned(),
                            );
                        }
                        execution_cleaned = true;
                        #[cfg(test)]
                        control.observe_finalization_event("workspace_absent");
                        execution_cleanup_record = Some(cleanup);
                        match control.end_available(&[Arc::clone(
                            &context.spool_dir.execution_root,
                        )]) {
                            Ok(available) => execution_end_available = Some(available),
                            Err(error) => finalization_errors.push(format!(
                                "disk.measurement.failed: end free-space query failed: {error}"
                            )),
                        }
                    }
                    ManagedCleanupOutcome::Failed { record, error } => {
                        execution_cleanup_record = record;
                        if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                            root: DiskRootId::Execution,
                            outcome: DiskCleanupOutcome::Failed(error.clone()),
                        }) {
                            return Err(
                                "disk lifecycle rejected failed execution cleanup".to_owned(),
                            );
                        }
                        event = RunEvent::EffectFailed(EffectFailed::other(
                            id,
                            hoimin_core::WORKSPACE_CLEANUP_FAILED,
                            error,
                        ));
                    }
                    ManagedCleanupOutcome::Deferred { record, error } => {
                        execution_cleanup_record = record;
                        if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                            root: DiskRootId::Execution,
                            outcome: DiskCleanupOutcome::Deferred(error.clone()),
                        }) {
                            return Err(
                                "disk lifecycle rejected deferred execution cleanup".to_owned(),
                            );
                        }
                        context.spool_dir.execution_root.abandon_for_janitor(
                            "execution-root cleanup did not complete".to_owned(),
                        );
                        return Err(error);
                    }
                }
            }
            let previous_phase = state.phase();
            let accepted_mutant = matches!(&event, RunEvent::MutantFinished(_));
            let targets_resolved = matches!(&event, RunEvent::TargetsResolved(_));
            let preflight_completed = matches!(&event, RunEvent::PreflightCompleted(_));
            let cleanup_finished = matches!(&event, RunEvent::CleanupFinished(_));
            #[cfg(test)]
            let session_finished = matches!(&event, RunEvent::SessionFinished(_));
            let analyzed_records = match &event {
                RunEvent::AnalysisFinished(value) => {
                    value.spool.as_ref().map(|spool| spool.records)
                }
                _ => None,
            };
            let external_stop = matches!(
                event,
                RunEvent::DeadlineReached
                    | RunEvent::CancellationRequested
                    | RunEvent::DiskStopRequested(_)
            );
            let deadline_stop = matches!(event, RunEvent::DeadlineReached);
            let failed = matches!(event, RunEvent::EffectFailed(_));
            let failed_primary = failed_event_primary(&event);
            if !external_stop && (process_completion || io_completion) {
                in_flight = in_flight.saturating_sub(1);
            }
            if !external_stop && io_completion {
                io_in_flight = io_in_flight.saturating_sub(1);
            }
            if external_stop || failed {
                establish_event_shutdown_budget(
                    &mut shutdown_budget,
                    &event,
                    deadline,
                    tokio::time::Instant::now(),
                    shutdown_grace,
                );
            }
            if failed {
                cancellation.cancel();
                stop_signalled = true;
            }
            let transition_result =
                transition(*state, event).map(|(next, produced)| (Box::new(next), produced));
            let (next, produced) = match transition_result {
                Ok(value) => value,
                Err(error) => {
                    cancellation.cancel();
                    let drain_budget = establish_shutdown_budget(
                        &mut shutdown_budget,
                        ShutdownBudget::after_observation_with_grace(
                            ShutdownCause::Failure,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        ),
                    );
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
                        &drain_budget,
                        &mut shutdown_expiry_reported,
                        &mut metrics,
                        &mut metrics_warnings,
                        |completion| {
                            let _ = accept_blocking_completion(&mut context, completion);
                        },
                    )
                    .await
                    .err();
                    return Err(combine_shutdown_errors(error.to_string(), drain_failure));
                }
            };
            state = next;
            #[cfg(test)]
            if final_output_completed {
                control.observe_finalization_event("output_acknowledged");
            }
            #[cfg(test)]
            if session_finished {
                control.observe_finalization_event("session_finished");
            }
            track_diagnostic_run_id(&mut diagnostic_run_id, &state);
            if let Some((worker, true)) = process {
                record_metrics(&mut metrics, &mut metrics_warnings, |metrics| {
                    metrics.process_finished(worker, accepted_mutant)
                });
            }
            if let Some(records) = analyzed_records {
                discovered = records;
                if let Some(metrics) = metrics.as_mut() {
                    metrics.discovered(records);
                }
            }
            if accepted_mutant {
                executed = executed.saturating_add(1);
            }
            observe_accepted_transition(
                &mut metrics,
                &mut metrics_warnings,
                previous_phase,
                state.phase(),
                targets_resolved,
                preflight_completed,
                cleanup_finished,
            );

            if external_stop || failed {
                retain_machine_pending_effects(
                    &mut effects,
                    &state,
                    &mut metrics,
                    &mut metrics_warnings,
                );
                let candidate_budget = if deadline_stop {
                    ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                } else {
                    let cause = if external_stop {
                        ShutdownCause::Cancellation
                    } else {
                        ShutdownCause::Failure
                    };
                    ShutdownBudget::after_observation_with_grace(
                        cause,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    )
                };
                let drain_budget =
                    establish_shutdown_budget(&mut shutdown_budget, candidate_budget);
                let drain_result = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &drain_budget,
                    &mut shutdown_expiry_reported,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await;
                finish_failed_event_drain(failed_primary, drain_result)?;
                io_in_flight = 0;
            } else if process_completion {
                let process_failure = match process_tasks.join_next().await {
                    Some(Ok(())) => None,
                    Some(Err(error)) => Some(format!("process task failed: {error}")),
                    None => Some("process completion had no task".to_owned()),
                };
                if let Some(process_failure) = process_failure {
                    cancellation.cancel();
                    let drain_budget = establish_shutdown_budget(
                        &mut shutdown_budget,
                        ShutdownBudget::after_observation_with_grace(
                            ShutdownCause::Failure,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        ),
                    );
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
                        &drain_budget,
                        &mut shutdown_expiry_reported,
                        &mut metrics,
                        &mut metrics_warnings,
                        |completion| {
                            let _ = accept_blocking_completion(&mut context, completion);
                        },
                    )
                    .await
                    .err();
                    return Err(combine_shutdown_errors(process_failure, drain_failure));
                }
            } else if io_completion {
                let io_failure = match io_tasks.join_next().await {
                    Some(Ok(())) => None,
                    Some(Err(error)) => Some(format!("blocking I/O task failed: {error}")),
                    None => Some("blocking I/O completion had no task".to_owned()),
                };
                if let Some(io_failure) = io_failure {
                    cancellation.cancel();
                    let drain_budget = establish_shutdown_budget(
                        &mut shutdown_budget,
                        ShutdownBudget::after_observation_with_grace(
                            ShutdownCause::Failure,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        ),
                    );
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
                        &drain_budget,
                        &mut shutdown_expiry_reported,
                        &mut metrics,
                        &mut metrics_warnings,
                        |completion| {
                            let _ = accept_blocking_completion(&mut context, completion);
                        },
                    )
                    .await
                    .err();
                    return Err(combine_shutdown_errors(io_failure, drain_failure));
                }
            }
            record_ready_processes(&produced, &mut metrics, &mut metrics_warnings);
            effects.extend(produced);
        }
        if let Some(metrics) = metrics.as_mut() {
            metrics.set_run_id(state.run_id());
        }
        Ok((state.exit_code(), state.run_id().to_owned()))
    }
    .await;
    let outer_finalization = async {
        ensure_outer_finalization_budget(
            &mut shutdown_budget,
            run_result.is_err(),
            deadline,
            tokio::time::Instant::now(),
            shutdown_grace,
        );
        let budget = shutdown_budget
            .as_ref()
            .expect("outer finalization established a shutdown budget");
        let _disk_stats = disk_monitor.as_ref().map(DiskMonitor::stats);
        let (process_reaped, output_drained, process_errors) = if execution_safety_proven {
            (true, true, Vec::new())
        } else {
            let report = context
                .process
                .drain_for_shutdown(
                    budget
                        .deadline()
                        .saturating_duration_since(tokio::time::Instant::now()),
                )
                .await;
            (
                report.all_reaped,
                report.output_drains_joined,
                report.secondary_errors,
            )
        };
        let monitor_joined = if execution_safety_proven {
            true
        } else if let Some(monitor) = disk_monitor.as_ref() {
            let remaining = shutdown_budget
                .as_ref()
                .expect("outer finalization established a shutdown budget")
                .deadline()
                .saturating_duration_since(tokio::time::Instant::now());
            monitor.stop_and_join(remaining).await
        } else {
            true
        };
        let lifecycle_snapshot = disk_lifecycle.snapshot();
        if lifecycle_snapshot.process_drain == DiskComponentState::Pending
            && !disk_lifecycle.apply(if process_reaped {
                DiskLifecycleEvent::ProcessDrainSucceeded
            } else {
                DiskLifecycleEvent::ProcessDrainFailed
            })
        {
            finalization_errors
                .push("disk lifecycle rejected outer process drain completion".to_owned());
        }
        let lifecycle_snapshot = disk_lifecycle.snapshot();
        if lifecycle_snapshot.output_drain == DiskComponentState::Pending
            && !disk_lifecycle.apply(if output_drained {
                DiskLifecycleEvent::OutputDrainSucceeded
            } else {
                DiskLifecycleEvent::OutputDrainFailed
            })
        {
            finalization_errors
                .push("disk lifecycle rejected outer output drain completion".to_owned());
        }
        let lifecycle_snapshot = disk_lifecycle.snapshot();
        if lifecycle_snapshot.monitor_join == DiskComponentState::Pending
            && !disk_lifecycle.apply(if monitor_joined {
                DiskLifecycleEvent::MonitorJoinSucceeded
            } else {
                DiskLifecycleEvent::MonitorJoinFailed
            })
        {
            finalization_errors
                .push("disk lifecycle rejected outer monitor join completion".to_owned());
        }
        let safe_to_remove = cleanup_quiescence_proven([
            process_reaped,
            output_drained,
            monitor_joined,
            final_measurement_joined,
        ]) && lifecycle_safety_succeeded(&disk_lifecycle);
        let close = close_context_resources(
            &mut context,
            shutdown_budget
                .as_ref()
                .expect("outer finalization established a shutdown budget"),
            shutdown_expiry_reported,
            safe_to_remove,
            Box::new(|| {}),
        )
        .await;
        if close.expiry.is_some() {
            shutdown_expiry_reported = true;
        }
        let mut run_result = if monitor_joined {
            run_result
        } else {
            let join_error =
                "disk.measurement.failed: disk monitor join exceeded shutdown budget".to_owned();
            match run_result {
                Ok(_) => Err(join_error),
                Err(primary) => Err(combine_shutdown_errors(primary, Some(join_error))),
            }
        };
        run_result = match (run_result, close.expiry) {
            (Ok(_), Some(expiry)) => Err(expiry),
            (Err(primary), Some(expiry)) => Err(combine_shutdown_errors(primary, Some(expiry))),
            (result, None) => result,
        };
        finalization_errors.append(&mut context.blocking_secondary_errors);
        for error in finalization_errors.drain(..).chain(process_errors) {
            run_result = append_finalization_error(run_result, error);
        }
        if safe_to_remove {
            if !execution_cleaned && !execution_cleanup_attempted {
                context.analyzer.release_candidate_spool();
                context.spool_dir.release_execution_spool();
                execution_cleanup_attempted = true;
                if disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Execution,
                }) {
                    match cleanup_managed_root(
                        Arc::clone(&context.spool_dir.execution_root),
                        budget,
                    )
                    .await
                    {
                        ManagedCleanupOutcome::Clean(_) => {
                            if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                root: DiskRootId::Execution,
                                outcome: DiskCleanupOutcome::Clean,
                            }) {
                                run_result = append_finalization_error(
                                    run_result,
                                    "disk lifecycle rejected outer clean execution cleanup"
                                        .to_owned(),
                                );
                            }
                            execution_cleaned = true;
                        }
                        ManagedCleanupOutcome::Failed { error, .. }
                        | ManagedCleanupOutcome::Deferred { error, .. } => {
                            let outcome = if error.starts_with("workspace.cleanup.deferred:") {
                                DiskCleanupOutcome::Deferred(error.clone())
                            } else {
                                DiskCleanupOutcome::Failed(error.clone())
                            };
                            if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                root: DiskRootId::Execution,
                                outcome,
                            }) {
                                run_result = append_finalization_error(
                                    run_result,
                                    "disk lifecycle rejected outer incomplete execution cleanup"
                                        .to_owned(),
                                );
                            }
                            context.spool_dir.execution_root.abandon_for_janitor(
                                "outer execution-root cleanup did not complete".to_owned(),
                            );
                            run_result = append_finalization_error(run_result, error);
                        }
                    }
                } else {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected outer execution cleanup request".to_owned(),
                    );
                }
            }
            if !execution_cleaned {
                context
                    .spool_dir
                    .execution_root
                    .abandon_for_janitor("execution-root cleanup was already attempted".to_owned());
            }
            if !delivery_cleaned && !delivery_cleanup_attempted {
                if let Err(error) = context.report.flush_and_release_spool() {
                    run_result = append_finalization_error(
                        run_result,
                        format!("report flush before delivery cleanup failed: {error}"),
                    );
                }
                if disk_lifecycle.snapshot().report == DiskComponentState::Pending
                    && !disk_lifecycle.apply(DiskLifecycleEvent::ReportFailed)
                {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected outer report failure".to_owned(),
                    );
                }
                context.spool_dir.release_delivery_spool();
                delivery_cleanup_attempted = true;
                if disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Delivery,
                }) {
                    match cleanup_managed_root(Arc::clone(&context.spool_dir.delivery_root), budget)
                        .await
                    {
                        ManagedCleanupOutcome::Clean(cleanup) => {
                            if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                root: DiskRootId::Delivery,
                                outcome: DiskCleanupOutcome::Clean,
                            }) {
                                run_result = append_finalization_error(
                                    run_result,
                                    "disk lifecycle rejected outer clean delivery cleanup"
                                        .to_owned(),
                                );
                            }
                            delivery_cleaned = true;
                            for error in delivery_cleanup_secondary_errors(&cleanup) {
                                run_result = append_finalization_error(run_result, error);
                            }
                            #[cfg(test)]
                            control.observe_finalization_event("delivery_root_absent");
                        }
                        ManagedCleanupOutcome::Failed { error, .. }
                        | ManagedCleanupOutcome::Deferred { error, .. } => {
                            let outcome = if error.starts_with("workspace.cleanup.deferred:") {
                                DiskCleanupOutcome::Deferred(error.clone())
                            } else {
                                DiskCleanupOutcome::Failed(error.clone())
                            };
                            if !disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                                root: DiskRootId::Delivery,
                                outcome,
                            }) {
                                run_result = append_finalization_error(
                                    run_result,
                                    "disk lifecycle rejected outer incomplete delivery cleanup"
                                        .to_owned(),
                                );
                            }
                            context.spool_dir.delivery_root.abandon_for_janitor(
                                "outer delivery-root cleanup did not complete".to_owned(),
                            );
                            run_result = append_finalization_error(run_result, error);
                        }
                    }
                } else {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected outer delivery cleanup request".to_owned(),
                    );
                }
            }
            if !delivery_cleaned {
                context
                    .spool_dir
                    .delivery_root
                    .abandon_for_janitor("delivery-root cleanup was already attempted".to_owned());
            }
        } else {
            if !execution_cleaned && !execution_cleanup_attempted {
                execution_cleanup_attempted = true;
                let requested = disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Execution,
                });
                let completed = requested
                    && disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                        root: DiskRootId::Execution,
                        outcome: DiskCleanupOutcome::Deferred(
                            CLEANUP_QUIESCENCE_UNPROVEN.to_owned(),
                        ),
                    });
                if !completed {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected deferred outer execution cleanup".to_owned(),
                    );
                }
            }
            if !execution_cleaned {
                context
                    .spool_dir
                    .execution_root
                    .abandon_for_janitor(CLEANUP_QUIESCENCE_UNPROVEN.to_owned());
            }
            if disk_lifecycle.snapshot().report == DiskComponentState::Pending {
                let _ = disk_lifecycle.apply(DiskLifecycleEvent::ReportFailed);
            }
            if !delivery_cleaned && !delivery_cleanup_attempted {
                delivery_cleanup_attempted = true;
                let requested = disk_lifecycle.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Delivery,
                });
                let completed = requested
                    && disk_lifecycle.apply(DiskLifecycleEvent::CleanupCompleted {
                        root: DiskRootId::Delivery,
                        outcome: DiskCleanupOutcome::Deferred(
                            CLEANUP_QUIESCENCE_UNPROVEN.to_owned(),
                        ),
                    });
                if !completed {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected deferred outer delivery cleanup".to_owned(),
                    );
                }
            }
            if !delivery_cleaned {
                context
                    .spool_dir
                    .delivery_root
                    .abandon_for_janitor(CLEANUP_QUIESCENCE_UNPROVEN.to_owned());
            }
            run_result = append_finalization_error(
                run_result,
                format!(
                    "{}: {CLEANUP_QUIESCENCE_UNPROVEN}",
                    hoimin_core::WORKSPACE_CLEANUP_DEFERRED
                ),
            );
        }
        if let Some(path) = metrics_path {
            match (shutdown_budget.as_ref(), shutdown_expiry_reported) {
                (Some(budget), false) => {
                    let finalized = finalize_metrics_with_shutdown(
                        path.as_std_path().to_owned(),
                        metrics,
                        run_result.as_ref().err().cloned(),
                        discovered,
                        executed,
                        metrics_warnings,
                        budget,
                        Box::new(|| {}),
                    )
                    .await;
                    metrics_warnings = finalized.warnings;
                    if let Some(expiry) = finalized.expiry {
                        run_result = match run_result {
                            Ok(_) => Err(expiry),
                            Err(primary) => Err(combine_shutdown_errors(primary, Some(expiry))),
                        };
                    }
                }
                (Some(_), true) => skip_expired_metrics_finalization(
                    metrics,
                    run_result.as_ref().err().map(String::as_str),
                    &mut metrics_warnings,
                ),
                (None, _) => unreachable!("outer finalization always has a shutdown budget"),
            }
            for (code, message) in metrics_warnings {
                emit_metrics_warning(&mut context, &diagnostic_run_id, code, message);
            }
        }
        combine_close_results(
            run_result.map(|(exit_code, _)| exit_code),
            close.workspace,
            close.process,
        )
    };
    finish_with_interrupt_monitor(interrupts, outer_finalization).await
}

fn track_diagnostic_run_id(diagnostic_run_id: &mut String, state: &RunState) {
    diagnostic_run_id.clear();
    diagnostic_run_id.push_str(state.run_id());
}

fn record_metrics(
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
    observation: impl FnOnce(&mut MetricsCollector) -> Result<(), MetricsError>,
) {
    if let Some(collector) = collector.as_mut()
        && let Err(error) = observation(collector)
    {
        warnings.push(("metrics.state", error.to_string()));
    }
}

fn cancel_queued_worker(
    worker: Option<u32>,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    if let Some(worker) = worker {
        record_metrics(collector, warnings, |metrics| metrics.cancel_queued(worker));
    }
}

fn cancel_queued_effect(
    effect: &RunEffect,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    if let RunEffect::RunMutant(request) = effect {
        cancel_queued_worker(request.worker, collector, warnings);
    }
}

fn discard_queued_effects(
    effects: &mut VecDeque<RunEffect>,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    for effect in effects.drain(..) {
        cancel_queued_effect(&effect, collector, warnings);
    }
}

fn retain_machine_pending_effects(
    effects: &mut VecDeque<RunEffect>,
    state: &RunState,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    let mut retained = VecDeque::with_capacity(effects.len());
    let mut discarded = VecDeque::new();
    while let Some(effect) = effects.pop_front() {
        if state.is_effect_pending(effect.id()) {
            retained.push_back(effect);
        } else {
            discarded.push_back(effect);
        }
    }
    discard_queued_effects(&mut discarded, collector, warnings);
    *effects = retained;
}

fn record_ready_processes<'a>(
    effects: impl IntoIterator<Item = &'a RunEffect>,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    for worker in effects.into_iter().filter_map(|effect| match effect {
        RunEffect::RunMutant(request) => request.worker,
        _ => None,
    }) {
        record_metrics(collector, warnings, |metrics| metrics.queued(worker));
    }
}

fn accept_process_dispatch<'a>(
    control: &'a RunControl,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
    worker: u32,
) -> Option<std::sync::MutexGuard<'a, ()>> {
    let Some(dispatch) = control.begin_dispatch() else {
        record_metrics(collector, warnings, |metrics| metrics.cancel_queued(worker));
        return None;
    };
    record_metrics(collector, warnings, |metrics| {
        metrics.process_started(worker)
    });
    Some(dispatch)
}

fn observe_accepted_transition(
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
    previous: RunPhase,
    next: RunPhase,
    targets_resolved: bool,
    preflight_completed: bool,
    cleanup_finished: bool,
) {
    record_metrics(collector, warnings, |metrics| {
        if targets_resolved {
            metrics.finish_stage("targets")?;
            metrics.begin_stage("preflight")?;
        }
        if preflight_completed {
            metrics.finish_stage("preflight")?;
        }
        if previous != next {
            let previous_stage = phase_stage(previous);
            let next_stage = phase_stage(next);
            if previous_stage != next_stage {
                if let Some(stage) = previous_stage
                    && !(previous == RunPhase::Cleaning && cleanup_finished)
                {
                    metrics.finish_stage(stage)?;
                }
                if let Some(stage) = next_stage {
                    metrics.begin_stage(stage)?;
                }
            }
        }
        if cleanup_finished {
            metrics.finish_stage("cleanup")?;
        }
        Ok(())
    });
}

fn phase_stage(phase: RunPhase) -> Option<&'static str> {
    match phase {
        RunPhase::Copy | RunPhase::MaterializationVerification => Some("copy"),
        RunPhase::Baseline => Some("baseline"),
        RunPhase::Analyze => Some("analysis"),
        RunPhase::Mutants => Some("mutants"),
        RunPhase::Cleaning => Some("cleanup"),
        _ => None,
    }
}

fn emit_metrics_warning<Stdout: Write, Stderr: Write>(
    context: &mut ShellContext<Stdout, Stderr>,
    run_id: &str,
    code: &str,
    message: String,
) {
    let _ = context.report.handle(EmitOutput {
        id: EffectId(u64::MAX),
        event: OutputEvent::Diagnostic(Diagnostic::new(run_id, u64::MAX, "warning", code, message)),
    });
}

fn first_interrupt_event(signal: Result<(), String>) -> Result<RunEvent, String> {
    signal.map(|()| RunEvent::CancellationRequested)
}

fn spawn_process(
    process: Arc<ProcessHandler>,
    request: ProcessRequest,
    worker: u32,
    baseline: bool,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
    _dispatch: std::sync::MutexGuard<'_, ()>,
) {
    tasks.spawn(async move {
        let event = match process.run(request).await {
            Ok(value) if baseline => RunEvent::BaselineFinished(value),
            Ok(value) => RunEvent::MutantFinished(value),
            Err(error) => RunEvent::EffectFailed(error),
        };
        let _ = sender
            .send(ShellCompletion {
                event,
                process_task: true,
                io_task: false,
                process: Some((worker, !baseline)),
                blocking: None,
            })
            .await;
    });
}

fn spawn_blocking_effect(
    task: BlockingEffect,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
) {
    tasks.spawn(async move {
        let id = task.id();
        let (event, blocking) = match run_blocking_io(id, move || task.execute()).await {
            Ok(completion) => {
                let event = match &completion {
                    BlockingEffectCompletion::Workspace(completion) => completion.event().clone(),
                    BlockingEffectCompletion::Candidate(event)
                    | BlockingEffectCompletion::OwnedWorkspace { event, .. } => {
                        event.as_ref().clone()
                    }
                };
                (event, Some(Box::new(completion)))
            }
            Err(error) => (RunEvent::EffectFailed(error), None),
        };
        let _ = sender
            .send(ShellCompletion {
                event,
                process_task: false,
                io_task: true,
                process: None,
                blocking,
            })
            .await;
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "grace expiry must drain both task sets and preserve buffered ownership accounting"
)]
async fn shutdown_expiry_error(
    process_tasks: &mut JoinSet<()>,
    io_tasks: &mut JoinSet<()>,
    receiver: &mut mpsc::Receiver<ShellCompletion>,
    in_flight: &mut usize,
    budget: &ShutdownBudget,
    shutdown_expiry_reported: &mut bool,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    accept_blocking: impl FnMut(BlockingEffectCompletion),
) -> String {
    *shutdown_expiry_reported = true;
    let process_task_count = process_tasks.len();
    let io_task_count = io_tasks.len();
    let expiry = budget.expiry_error(process_task_count, io_task_count);
    let drain_failure = drain_processes(
        process_tasks,
        io_tasks,
        receiver,
        in_flight,
        budget,
        shutdown_expiry_reported,
        metrics,
        metrics_warnings,
        accept_blocking,
    )
    .await
    .err();
    match drain_failure {
        Some(error) if !error.contains("shutdown grace expired") => {
            combine_shutdown_errors(expiry, Some(error))
        }
        Some(_) | None => expiry,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "shutdown draining needs both task sets, ownership returns, and accounting under one deadline"
)]
async fn drain_processes(
    process_tasks: &mut JoinSet<()>,
    io_tasks: &mut JoinSet<()>,
    receiver: &mut mpsc::Receiver<ShellCompletion>,
    in_flight: &mut usize,
    budget: &ShutdownBudget,
    shutdown_expiry_reported: &mut bool,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    mut accept_blocking: impl FnMut(BlockingEffectCompletion),
) -> Result<(), String> {
    let drain_result = budget
        .wait(async {
            let mut first_failure = None;
            while !process_tasks.is_empty() || !io_tasks.is_empty() {
                tokio::select! {
                    result = process_tasks.join_next(), if !process_tasks.is_empty() => {
                        if let Some(Err(error)) = result
                            && first_failure.is_none()
                        {
                            first_failure = Some(format!("process task failed while stopping: {error}"));
                        }
                    }
                    result = io_tasks.join_next(), if !io_tasks.is_empty() => {
                        if let Some(Err(error)) = result
                            && first_failure.is_none()
                        {
                            first_failure = Some(format!("blocking I/O task failed while stopping: {error}"));
                        }
                    }
                    completion = receiver.recv(), if *in_flight > 0 => {
                        if let Some(completion) = completion {
                            accept_drained_completion(
                                completion,
                                in_flight,
                                metrics,
                                metrics_warnings,
                                &mut accept_blocking,
                            );
                        }
                    }
                }
            }
            first_failure
        })
        .await;

    let process_task_count = process_tasks.len();
    let io_task_count = io_tasks.len();
    while let Ok(completion) = receiver.try_recv() {
        accept_drained_completion(
            completion,
            in_flight,
            metrics,
            metrics_warnings,
            &mut accept_blocking,
        );
    }

    if drain_result.is_err() {
        *shutdown_expiry_reported = true;
        process_tasks.abort_all();
        io_tasks.abort_all();
    }
    *in_flight = 0;

    match drain_result {
        Ok(Some(error)) => Err(error),
        Ok(None) => Ok(()),
        Err(_) => Err(budget.expiry_error(process_task_count, io_task_count)),
    }
}

fn accept_drained_completion(
    completion: ShellCompletion,
    in_flight: &mut usize,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    accept_blocking: &mut impl FnMut(BlockingEffectCompletion),
) {
    if let Some((worker, true)) = completion.process {
        record_metrics(metrics, metrics_warnings, |metrics| {
            metrics.process_finished(worker, false)
        });
    }
    if let Some(blocking) = completion.blocking {
        accept_blocking(*blocking);
    }
    *in_flight = in_flight.saturating_sub(1);
}

fn combine_shutdown_errors(primary: String, drain_failure: Option<String>) -> String {
    match drain_failure {
        Some(drain_failure) => format!("{primary}; {drain_failure}"),
        None => primary,
    }
}

fn failed_event_primary(event: &RunEvent) -> Option<String> {
    let RunEvent::EffectFailed(failed) = event else {
        return None;
    };
    Some(format!(
        "effect failed ({}): {}",
        failed.failure.code(),
        failed.failure.message()
    ))
}

fn finish_failed_event_drain(
    failed_primary: Option<String>,
    drain_result: Result<(), String>,
) -> Result<(), String> {
    match (failed_primary, drain_result) {
        (_, Ok(())) => Ok(()),
        (Some(primary), Err(drain_failure)) => {
            Err(combine_shutdown_errors(primary, Some(drain_failure)))
        }
        (None, Err(drain_failure)) => Err(drain_failure),
    }
}

struct ResourceCloseCompletion {
    workspace: Option<WorkspaceHandler>,
    workspace_result: Result<(), String>,
    process_result: Result<(), String>,
}

struct ResourceCloseResults {
    workspace: Result<(), String>,
    process: Result<(), String>,
    expiry: Option<String>,
}

type OwnedStartHook = Box<dyn FnOnce() + Send + 'static>;

fn detach_resource_cleanup(
    mut workspace: Option<WorkspaceHandler>,
    process: Arc<ProcessHandler>,
    before_start: OwnedStartHook,
) {
    drop(tokio::task::spawn_blocking(move || {
        before_start();
        let _ = process.close();
        if let Some(workspace) = workspace.as_mut() {
            let _ = workspace.close();
        }
    }));
}

fn detach_context_resources<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    close_workspace: bool,
    before_start: OwnedStartHook,
) {
    detach_resource_cleanup(
        close_workspace.then(|| context.workspace.take()).flatten(),
        Arc::clone(&context.process),
        before_start,
    );
}

async fn close_context_resources<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    budget: &ShutdownBudget,
    shutdown_already_expired: bool,
    close_workspace: bool,
    before_start: OwnedStartHook,
) -> ResourceCloseResults
where
    Stdout: Write,
    Stderr: Write,
{
    if !close_workspace && let Some(workspace) = context.workspace.as_mut() {
        workspace.retain_workers_for_janitor();
    }
    if shutdown_already_expired {
        detach_context_resources(context, close_workspace, before_start);
        return ResourceCloseResults {
            workspace: Ok(()),
            process: Ok(()),
            expiry: None,
        };
    }
    if tokio::time::Instant::now() >= budget.deadline() {
        let expiry = budget.expiry_error(0, 0);
        detach_context_resources(context, close_workspace, before_start);
        return ResourceCloseResults {
            workspace: Ok(()),
            process: Ok(()),
            expiry: Some(expiry),
        };
    }
    let workspace = if close_workspace {
        let Some(workspace) = context.workspace.take() else {
            return ResourceCloseResults {
                workspace: Err("workspace ownership is unavailable during final cleanup".to_owned()),
                process: Ok(()),
                expiry: None,
            };
        };
        Some(workspace)
    } else {
        None
    };
    let process = Arc::clone(&context.process);
    let mut task = tokio::task::spawn_blocking(move || {
        before_start();
        let mut workspace = workspace;
        let process_result = process.close().map_err(|error| error.to_string());
        let workspace_result = workspace.as_mut().map_or(Ok(()), |workspace| {
            workspace.close().map_err(|error| error.to_string())
        });
        ResourceCloseCompletion {
            workspace,
            workspace_result,
            process_result,
        }
    });
    let completion = match budget.wait(&mut task).await {
        Ok(Ok(completion)) => completion,
        Ok(Err(error)) => {
            return ResourceCloseResults {
                workspace: Err(format!("blocking I/O task failed: {error}")),
                process: Ok(()),
                expiry: None,
            };
        }
        Err(_) => {
            drop(task);
            return ResourceCloseResults {
                workspace: Ok(()),
                process: Ok(()),
                expiry: Some(budget.expiry_error(0, 1)),
            };
        }
    };
    if let Some(workspace) = completion.workspace {
        context.workspace = Some(workspace);
    }
    ResourceCloseResults {
        workspace: completion.workspace_result,
        process: completion.process_result,
        expiry: None,
    }
}

struct MetricsFinalizeResults {
    warnings: Vec<(&'static str, String)>,
    expiry: Option<String>,
}

fn skip_expired_metrics_finalization(
    collector: Option<MetricsCollector>,
    run_failure: Option<&str>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    drop(collector);
    let failure = run_failure.unwrap_or("shutdown grace expired");
    warnings.push((
        "metrics.incomplete",
        format!("metrics output was not written because the run failed: {failure}"),
    ));
}

#[expect(
    clippy::too_many_arguments,
    reason = "metrics finalization carries the existing run summary and the shared shutdown deadline"
)]
async fn finalize_metrics_with_shutdown(
    path: std::path::PathBuf,
    collector: Option<MetricsCollector>,
    run_failure: Option<String>,
    discovered: u64,
    executed: u64,
    mut warnings: Vec<(&'static str, String)>,
    budget: &ShutdownBudget,
    before_start: OwnedStartHook,
) -> MetricsFinalizeResults {
    let preserved_warnings = warnings.clone();
    let operation = move || {
        finalize_metrics(
            &path,
            collector,
            run_failure.as_deref(),
            discovered,
            executed,
            &mut warnings,
        );
        warnings
    };
    match run_owned_blocking_until(budget, before_start, operation).await {
        Ok(warnings) => MetricsFinalizeResults {
            warnings,
            expiry: None,
        },
        Err(OwnedBlockingError::Expired(expiry)) => MetricsFinalizeResults {
            warnings: preserved_warnings,
            expiry: Some(expiry),
        },
        Err(OwnedBlockingError::Join(error)) => {
            let mut warnings = preserved_warnings;
            warnings.push(("metrics.write", error));
            MetricsFinalizeResults {
                warnings,
                expiry: None,
            }
        }
    }
}

fn combine_close_results(
    run: Result<i32, String>,
    workspace: Result<(), String>,
    process: Result<(), String>,
) -> Result<i32, String> {
    let mut failures = Vec::new();
    if let Err(error) = workspace {
        failures.push(format!("workspace cleanup failed: {error}"));
    }
    if let Err(error) = process {
        failures.push(format!("process backend close failed: {error}"));
    }
    match (run, failures.is_empty()) {
        (Ok(code), true) => Ok(code),
        (Ok(_), false) => Err(failures.join("; ")),
        (Err(error), true) => Err(error),
        (Err(error), false) => Err(format!("{error}; {}", failures.join("; "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};
    use std::ffi::OsString;
    use std::time::{Duration, Instant};

    use hoimin_core::{
        ApplyMutation, BudgetLedger, ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateIdentity,
        Cleanup, CommandArg, CreateWorker, IntegrityCheckpoint, MutationCandidate,
        ObserveRemainingBudget, Preflight, ResetWorker, RunBudgets, VerifyOriginals,
        reserve_workspace_copy, stable_mutant_id,
    };

    use crate::metrics::write_metrics;
    use crate::resource::PortableBackend;
    use crate::workspace::{DiskMeasurement, FilesystemKey, MeterReading};

    const DISK_STOP_FINALIZATION_ORDER: [&str; 14] = [
        "disk_stop",
        "dispatch_gate_closed",
        "root_termination_requested",
        "root_reaped",
        "output_drained",
        "monitor_stopped",
        "monitor_joined",
        "workspace_cleanup_requested_once",
        "workspace_absent",
        "run_finished_written",
        "delivery_cleanup_requested_once",
        "delivery_workspace_absent",
        "output_acknowledged",
        "session_finished",
    ];

    struct FixedDiskMeter(MeterReading);

    impl DiskMeasurement for FixedDiskMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            Ok(self.0.clone())
        }
    }

    struct FailingDiskMeter;

    impl DiskMeasurement for FailingDiskMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            Err(std::io::Error::other("initial statvfs failed"))
        }
    }

    struct ScriptedDiskMeter(std::sync::Mutex<VecDeque<MeterReading>>);

    impl DiskMeasurement for ScriptedDiskMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            Ok(self
                .0
                .lock()
                .expect("scripted disk meter lock")
                .pop_front()
                .expect("scripted disk reading"))
        }
    }

    struct FailingAfterDiskMeter {
        calls: AtomicUsize,
        healthy: MeterReading,
        fail_at: usize,
    }

    impl DiskMeasurement for FailingAfterDiskMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            let call = self.calls.fetch_add(1, Ordering::AcqRel);
            if call >= self.fail_at {
                Err(std::io::Error::other("post-drain statvfs failed"))
            } else {
                Ok(self.healthy.clone())
            }
        }
    }

    struct SwitchingDiskMeter {
        stop: Arc<AtomicBool>,
    }

    impl DiskMeasurement for SwitchingDiskMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            Ok(MeterReading {
                owned_bytes: 0,
                available_by_filesystem: BTreeMap::from([(
                    FilesystemKey(7),
                    if self.stop.load(Ordering::Acquire) {
                        1
                    } else {
                        u64::MAX
                    },
                )]),
                conservative_entries: false,
                elapsed: Duration::from_millis(1),
            })
        }
    }

    struct BlockingAfterCallMeter {
        calls: Arc<AtomicUsize>,
        block_at: usize,
        entered: std::sync::mpsc::Sender<()>,
        release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    impl DiskMeasurement for BlockingAfterCallMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            let call = self.calls.fetch_add(1, Ordering::AcqRel) + 1;
            if call >= self.block_at {
                let _ = self.entered.send(());
                let (released, changed) = &*self.release;
                let mut released = released.lock().expect("disk release lock");
                while !*released {
                    released = changed.wait(released).expect("disk release wait");
                }
            }
            Ok(MeterReading {
                owned_bytes: 0,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: Duration::from_millis(1),
            })
        }
    }

    struct ReleaseBlockedMeter(Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);

    impl Drop for ReleaseBlockedMeter {
        fn drop(&mut self) {
            let (released, changed) = &*self.0;
            *released.lock().expect("disk release lock") = true;
            changed.notify_all();
        }
    }

    #[cfg(unix)]
    fn missing_executable_arg() -> CommandArg {
        CommandArg::Unix(b"definitely-missing-hoimin-executable".to_vec())
    }

    #[cfg(windows)]
    fn missing_executable_arg() -> CommandArg {
        use std::os::windows::ffi::OsStrExt;

        CommandArg::Windows(
            std::ffi::OsStr::new("definitely-missing-hoimin-executable")
                .encode_wide()
                .collect(),
        )
    }

    #[cfg(unix)]
    fn successful_test_command() -> Vec<OsString> {
        vec![OsString::from("/usr/bin/true")]
    }

    #[cfg(unix)]
    fn long_running_test_command() -> Vec<OsString> {
        vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from("sleep 30"),
        ]
    }

    #[cfg(windows)]
    fn long_running_test_command() -> Vec<OsString> {
        vec![
            OsString::from("cmd.exe"),
            OsString::from("/C"),
            OsString::from("ping -n 31 127.0.0.1 >NUL"),
        ]
    }

    #[cfg(windows)]
    fn successful_test_command() -> Vec<OsString> {
        vec![
            OsString::from("cmd.exe"),
            OsString::from("/C"),
            OsString::from("exit 0"),
        ]
    }

    fn shell_setup_test_config(project: &TempDir) -> RunConfig {
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap()
    }

    fn output_failure_test_config(project: &TempDir, format: &str) -> RunConfig {
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from(format),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        crate::cli::parse_config_from(args).unwrap()
    }

    #[tokio::test]
    async fn initial_disk_stop_prevents_the_first_effect_dispatch() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project);
        let control = RunControl::with_disk_meter(FixedDiskMeter(MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), 1)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        }));
        let observed = control.clone();

        let exit = run_loop_with_control(config, Vec::new(), Vec::new(), control)
            .await
            .expect("a typed disk stop must complete through the normal run state machine");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        assert_eq!(observed.max_process_tasks(), 0);
    }

    #[tokio::test]
    async fn real_initial_threshold_is_a_typed_stop_after_start_requested() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--min-free-space"),
            OsString::from("18446744073709551615B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let mut stdout = Vec::new();

        let exit = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect("initial threshold must use the typed stop lifecycle");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(
            report["summary"]["disk"]["stop"]["code"],
            hoimin_core::FILESYSTEM_RESERVE_REACHED
        );
    }

    #[tokio::test]
    async fn initial_measurement_failure_is_typed_after_start_requested_before_dispatch() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let control = RunControl::with_disk_meter(FailingDiskMeter);
        let observed = control.clone();
        let mut stdout = Vec::new();

        let exit = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect("initial measurement failure must use the typed stop lifecycle");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(
            report["summary"]["disk"]["stop"]["code"],
            hoimin_core::DISK_MEASUREMENT_FAILED
        );
        assert_eq!(
            report["summary"]["disk"]["stop"]["message"],
            "initial statvfs failed"
        );
    }

    #[tokio::test]
    async fn pre_dispatch_sample_stops_before_analyzer_dispatch() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project);
        let reading = |available_bytes| MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), available_bytes)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        };
        let control = RunControl::with_disk_meter_and_interval(
            ScriptedDiskMeter(std::sync::Mutex::new(VecDeque::from([
                reading(u64::MAX),
                reading(1),
                reading(1),
            ]))),
            Duration::from_secs(60),
        );
        let observed = control.clone();

        let exit = run_loop_with_control(config, Vec::new(), Vec::new(), control)
            .await
            .expect("a pre-dispatch disk stop must finish through the run state machine");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        assert_eq!(observed.max_process_tasks(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn periodic_disk_stop_interrupts_an_active_process() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        let session_parent = tempfile::tempdir().unwrap();
        let session = session_parent.path().join("session.sqlite3");
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--session"),
            session.as_os_str().to_owned(),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(long_running_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let control = RunControl::with_disk_meter_and_interval(
            SwitchingDiskMeter {
                stop: Arc::clone(&stop),
            },
            Duration::from_millis(10),
        );
        let observed = control.clone();
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    result = &mut run => {
                        panic!("run finished before baseline dispatch: {result:?}")
                    }
                    () = tokio::time::sleep(Duration::from_millis(5)) => {
                        if observed.max_process_tasks() != 0 {
                            break;
                        }
                    }
                }
            }
        })
        .await
        .expect("baseline process was not dispatched");

        stop.store(true, Ordering::Release);
        let stopped = tokio::time::timeout(Duration::from_secs(2), &mut run).await;
        if stopped.is_err() {
            observed.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(3), &mut run).await;
            panic!("periodic disk stop did not interrupt the active process");
        }
        let exit = stopped.unwrap().unwrap();

        assert_ne!(exit, 0);
        let events = observed.finalization_events();
        let observed_order = DISK_STOP_FINALIZATION_ORDER
            .iter()
            .map(|expected| {
                events
                    .iter()
                    .position(|event| event == expected)
                    .unwrap_or_else(|| panic!("missing {expected} in {events:?}"))
            })
            .collect::<Vec<_>>();
        assert!(
            observed_order.windows(2).all(|pair| pair[0] < pair[1]),
            "shutdown order mismatch: {events:?}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == "workspace_cleanup_requested_once")
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == "delivery_cleanup_requested_once")
                .count(),
            1
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blocked_monitor_join_defers_both_roots_without_recursive_cleanup() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(long_running_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let release_guard = ReleaseBlockedMeter(Arc::clone(&release));
        let mut control = RunControl::with_disk_meter_and_interval(
            BlockingAfterCallMeter {
                calls,
                block_at: 4,
                entered: entered_tx,
                release,
            },
            Duration::from_millis(50),
        );
        control.shutdown_grace = Duration::from_millis(50);
        let observed = control.clone();
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    result = &mut run => panic!("run finished before blocked periodic scan: {result:?}"),
                    () = tokio::time::sleep(Duration::from_millis(5)) => {
                        if observed.max_process_tasks() != 0 && entered_rx.try_recv().is_ok() {
                            break;
                        }
                    }
                }
            }
        })
        .await
        .expect("periodic scan did not block while the process was live");

        observed.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), &mut run)
            .await
            .expect("shutdown did not honor the join budget");
        let error = result.expect_err("blocked monitor join must make cleanup deferred");
        assert!(error.contains("workspace.cleanup.deferred"), "{error}");
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots.iter().all(|root| root.exists()),
            "a root was removed while the monitor still held its capability: {roots:?}"
        );
        drop(release_guard);
    }

    #[tokio::test]
    async fn process_completion_takes_a_post_drain_sample_before_classification() {
        let reading = |available_bytes| MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), available_bytes)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        };
        let meter = ScriptedDiskMeter(std::sync::Mutex::new(VecDeque::from([
            reading(u64::MAX),
            reading(1),
        ])));
        let policy = DiskPolicy {
            max_owned_bytes: std::num::NonZeroU64::new(u64::MAX).unwrap(),
            min_free_bytes: std::num::NonZeroU64::new(2).unwrap(),
        };
        let monitor = DiskMonitor::start_with_meter_and_interval(
            meter,
            policy,
            Vec::new(),
            Duration::from_secs(60),
        )
        .await;

        let failure = sample_after_process_drain(&monitor, true)
            .await
            .expect("post-drain reserve stop");

        assert_eq!(
            failure.reason,
            hoimin_core::DiskStopReason::FilesystemReserveReached
        );
        assert!(monitor.stop_and_join(Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn post_drain_disk_stop_retains_the_simultaneous_process_failure_as_secondary() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("definitely-missing-hoimin-executable"),
        ])
        .unwrap();
        let control = RunControl::with_disk_meter(FixedDiskMeter(MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        }));
        control.inject_post_drain_disk_failure(hoimin_core::DiskFailure {
            code: hoimin_core::FILESYSTEM_RESERVE_REACHED.to_owned(),
            reason: hoimin_core::DiskStopReason::FilesystemReserveReached,
            observation: Some(DiskObservation {
                owned_bytes: 0,
                available_bytes: 1,
                measured_in: Duration::from_millis(1),
            }),
            message: None,
            secondary: Vec::new(),
        });
        let mut stdout = Vec::new();

        let exit = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect("disk stop should complete through the typed state machine");

        assert_ne!(exit, 0);
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let stop = &report["summary"]["disk"]["stop"];
        assert_eq!(stop["code"], hoimin_core::FILESYSTEM_RESERVE_REACHED);
        assert!(
            stop["secondary"]
                .as_array()
                .unwrap()
                .iter()
                .any(|secondary| {
                    secondary["Error"]["code"] == "process.spawn"
                        && secondary["Error"]["message"]
                            .as_str()
                            .is_some_and(|message| message.contains("spawn process"))
                }),
            "{stop}"
        );
    }

    #[tokio::test]
    async fn real_monitor_stop_merge_preserves_the_simultaneous_process_failure() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("definitely-missing-hoimin-executable"),
        ])
        .unwrap();
        let healthy = MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        };
        let meter = FailingAfterDiskMeter {
            calls: AtomicUsize::new(0),
            healthy,
            fail_at: 2,
        };
        let control = RunControl::with_disk_meter_and_interval(meter, Duration::from_secs(60));
        let mut stdout = Vec::new();

        let exit = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect("monitor stop should complete through the typed state machine");

        assert_ne!(exit, 0);
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let stop = &report["summary"]["disk"]["stop"];
        assert_eq!(stop["code"], hoimin_core::DISK_MEASUREMENT_FAILED);
        assert_eq!(stop["message"], "post-drain statvfs failed");
        assert!(
            stop["secondary"]
                .as_array()
                .unwrap()
                .iter()
                .any(|secondary| {
                    secondary["Error"]["code"] == "process.spawn"
                        && secondary["Error"]["message"]
                            .as_str()
                            .is_some_and(|message| message.contains("spawn process"))
                }),
            "{stop}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn final_execution_measurement_does_not_block_the_async_runtime() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let (control, pause) = RunControl::with_final_measurement_pause();
        let probe_requested = Arc::new(AtomicBool::new(false));
        let probe_observed = Arc::new(AtomicBool::new(false));
        let probe = tokio::spawn({
            let requested = Arc::clone(&probe_requested);
            let observed = Arc::clone(&probe_observed);
            async move {
                while !requested.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
                observed.store(true, Ordering::Release);
            }
        });
        let observer = std::thread::spawn({
            let requested = Arc::clone(&probe_requested);
            let observed = Arc::clone(&probe_observed);
            move || {
                pause.wait_until_entered();
                requested.store(true, Ordering::Release);
                std::thread::sleep(Duration::from_millis(100));
                let runtime_was_responsive = observed.load(Ordering::Acquire);
                pause.release();
                runtime_was_responsive
            }
        });

        let result = run_loop_with_control(config, Vec::new(), Vec::new(), control).await;
        probe.await.unwrap();

        assert_eq!(result.unwrap(), 0);
        assert!(
            observer.join().unwrap(),
            "final execution measurement blocked the Tokio runtime worker"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn expired_final_measurement_defers_cleanup_while_the_scan_owns_the_root() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let (pause_hook, pause) = FinalMeasurementPause::new();
        let mut control = RunControl::with_disk_meter(FixedDiskMeter(MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), 1)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        }));
        control.final_measurement_pause = Some(pause_hook);
        control.shutdown_grace = Duration::from_millis(30);
        let observed = control.clone();
        let mut stdout = Vec::new();
        let release = std::thread::spawn(move || {
            pause.wait_until_entered();
            std::thread::sleep(Duration::from_millis(100));
            pause.release();
        });

        let error = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect_err("expired final measurement must defer recursive cleanup");
        release.join().unwrap();

        assert!(
            error.contains(&format!(
                "{}: {CLEANUP_QUIESCENCE_UNPROVEN}",
                hoimin_core::WORKSPACE_CLEANUP_DEFERRED
            )),
            "{error}"
        );
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots.iter().all(|root| root.exists()),
            "cleanup raced an unjoined final measurement: {roots:?}"
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let execution_cleanup = report["summary"]["disk"]["cleanup"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["root_id"] == "execution")
            .expect("execution cleanup evidence");
        assert_eq!(execution_cleanup["status"], "deferred");
        assert_eq!(execution_cleanup["remaining_root"], roots[0].as_str());
    }

    #[tokio::test]
    async fn unproven_process_reap_is_published_as_the_lifecycle_stop() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        control.inject_process_reap_failure();
        let observed = control.clone();
        let mut stdout = Vec::new();

        let error = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect_err("unproven process reap must leave cleanup incomplete");

        assert!(
            error.contains(hoimin_core::WORKSPACE_CLEANUP_DEFERRED),
            "{error}"
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(
            report["summary"]["disk"]["stop"]["code"],
            hoimin_core::PROCESS_LIFECYCLE_FAILED
        );
        assert_eq!(
            report["summary"]["disk"]["stop"]["message"],
            "process drain failed"
        );
        assert!(
            observed
                .managed_root_paths()
                .iter()
                .all(|root| root.exists()),
            "unproven reap removed an owned root"
        );
    }

    #[tokio::test]
    async fn unproven_process_reap_is_secondary_to_an_existing_disk_stop() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::with_disk_meter(FixedDiskMeter(MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), 1)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        }));
        control.inject_process_reap_failure();
        let mut stdout = Vec::new();

        let error = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect_err("disk stop plus unproven reap must remain incomplete");

        assert!(
            error.contains(hoimin_core::WORKSPACE_CLEANUP_DEFERRED),
            "{error}"
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let stop = &report["summary"]["disk"]["stop"];
        assert_eq!(stop["code"], hoimin_core::FILESYSTEM_RESERVE_REACHED);
        assert!(
            stop["secondary"].as_array().unwrap().iter().any(
                |secondary| secondary["Error"]["code"] == hoimin_core::PROCESS_LIFECYCLE_FAILED
            ),
            "{stop}"
        );
    }

    #[test]
    fn cleanup_quiescence_requires_the_final_measurement_to_join() {
        assert!(cleanup_quiescence_proven([true, true, true, true]));
        assert!(!cleanup_quiescence_proven([true, true, true, false]));
    }

    #[test]
    fn later_success_cannot_erase_a_failed_lifecycle_component() {
        let mut lifecycle = ShellDiskLifecycle::new([]).unwrap();
        assert!(lifecycle.apply(DiskLifecycleEvent::ProcessDrainSucceeded));
        assert!(lifecycle.apply(DiskLifecycleEvent::OutputDrainSucceeded));
        assert!(lifecycle.apply(DiskLifecycleEvent::MonitorJoinFailed));

        assert!(!lifecycle_safety_succeeded(&lifecycle));
    }

    #[tokio::test]
    async fn deferred_execution_cleanup_is_incomplete_without_output_ack_or_outer_retry() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        control.inject_execution_cleanup_deferred();
        let observed = control.clone();

        let error = run_loop_with_control(config, Vec::new(), Vec::new(), control)
            .await
            .expect_err("deferred execution cleanup must keep the run incomplete");

        assert!(error.contains("workspace.cleanup.deferred"), "{error}");
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "deferred cleanup was acknowledged as delivered"
        );
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots[0].exists(),
            "outer finalization retried and removed the deferred execution root"
        );
    }

    #[tokio::test]
    async fn shell_cleanup_is_rejected_when_the_production_disk_lifecycle_sees_a_duplicate_request()
    {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        control.inject_duplicate_execution_cleanup_request();
        let observed = control.clone();

        let error = run_loop_with_control(config, Vec::new(), Vec::new(), control)
            .await
            .expect_err("duplicate cleanup request must be rejected by the runtime lifecycle");

        assert!(
            error.contains("disk lifecycle rejected execution cleanup request"),
            "{error}"
        );
        assert!(!observed.finalization_events().contains(&"workspace_absent"));
    }

    #[tokio::test]
    async fn normal_run_removes_both_managed_roots_before_returning() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        let observed = control.clone();

        let exit = run_loop_with_control(config, Vec::new(), Vec::new(), control)
            .await
            .unwrap();

        assert_eq!(exit, 0);
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots.iter().all(|root| !root.exists()),
            "managed roots survived normal finalization: {roots:?}"
        );
    }

    #[tokio::test]
    async fn delivery_cleanup_precedes_output_acknowledgement() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        let observed = control.clone();

        assert_eq!(
            run_loop_with_control(config, Vec::new(), Vec::new(), control)
                .await
                .unwrap(),
            0
        );

        let events = observed.finalization_events();
        let position = |event| {
            events
                .iter()
                .position(|observed| *observed == event)
                .unwrap_or_else(|| panic!("missing {event} in {events:?}"))
        };
        assert!(position("run_finished_written") < position("delivery_root_absent"));
        assert!(position("delivery_root_absent") < position("output_acknowledged"));
    }

    #[tokio::test]
    async fn final_json_contains_runtime_disk_measurement_and_cleanup_evidence() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let mut stdout = Vec::new();

        assert_eq!(
            run_loop_with_control(config, &mut stdout, Vec::new(), RunControl::new())
                .await
                .unwrap(),
            0
        );

        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let disk = &report["summary"]["disk"];
        assert!(disk["sample_count"].as_u64().unwrap() >= 3, "{disk}");
        assert!(disk["minimum_available_bytes"].as_u64().is_some());
        assert!(disk["removed_logical_bytes"].as_u64().is_some(), "{disk}");
        let filesystems = disk["filesystems"].as_array().unwrap();
        assert!(!filesystems.is_empty(), "{disk}");
        assert!(filesystems.iter().all(|filesystem| {
            filesystem["start_available_bytes"].as_u64().is_some()
                && filesystem["minimum_available_bytes"].as_u64().is_some()
                && filesystem["end_available_bytes"].as_u64().is_some()
                && (filesystem["available_bytes_change"].as_i64().is_some()
                    || filesystem["available_bytes_change"].as_u64().is_some())
        }));
        assert_eq!(disk["enforcement"][0]["kind"], "portable_guard");
        let execution_cleanup = disk["cleanup"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["root_id"] == "execution")
            .expect("execution cleanup evidence");
        assert_eq!(execution_cleanup["status"], "clean");
        assert!(execution_cleanup["examined_entries"].as_u64().unwrap() > 0);
        assert!(execution_cleanup["removed_entries"].as_u64().unwrap() > 0);
        assert!(disk["cleanup"].as_array().unwrap().iter().any(|record| {
            record["root_id"] == "delivery" && record["status"] == "cleanup_after_delivery"
        }));
    }

    #[tokio::test]
    async fn final_disk_evidence_preserves_a_negative_free_space_change() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--min-free-space"),
            OsString::from("1B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::with_disk_meter(FixedDiskMeter(MeterReading {
            owned_bytes: 0,
            available_by_filesystem: BTreeMap::from([(FilesystemKey(7), 1_000)]),
            conservative_entries: false,
            elapsed: Duration::from_millis(1),
        }));
        control.override_end_available(Ok(BTreeMap::from([(FilesystemKey(7), 500)])));
        let mut stdout = Vec::new();

        assert_eq!(
            run_loop_with_control(config, &mut stdout, Vec::new(), control)
                .await
                .unwrap(),
            0
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["available_bytes_change"],
            -500
        );
    }

    #[tokio::test]
    async fn end_free_query_failure_rejects_output_acknowledgement() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        control.override_end_available(Err("injected end query failure".to_owned()));
        let observed = control.clone();
        let mut stdout = Vec::new();

        let error = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect_err("required end-free evidence failure must keep the run incomplete");

        assert!(error.contains("disk.measurement.failed"), "{error}");
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(report["summary"]["complete"], false);
        assert_eq!(
            report["summary"]["disk"]["stop"]["code"],
            "disk.measurement.failed"
        );
        assert!(report["summary"]["disk"]["filesystems"][0]["end_available_bytes"].is_null());
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "failed final disk evidence must not be acknowledged"
        );
    }

    struct BorrowedNonSendWriter<'a> {
        buffer: &'a mut Vec<u8>,
        _not_send: std::rc::Rc<()>,
    }

    struct AlwaysFailingWriter;

    impl std::io::Write for AlwaysFailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected report write failure"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("injected report flush failure"))
        }
    }

    #[tokio::test]
    async fn report_write_failure_still_removes_delivery_root_without_output_ack() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        let observed = control.clone();

        let error = run_loop_with_control(config, AlwaysFailingWriter, Vec::new(), control)
            .await
            .expect_err("report write failure must not be acknowledged");

        assert!(error.contains("report"), "{error}");
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots.iter().all(|root| !root.exists()),
            "report failure stranded managed roots: {roots:?}"
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged")
        );
    }

    #[tokio::test]
    async fn delivery_cleanup_identity_failure_rejects_output_ack_and_preserves_replacement() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        control.inject_delivery_cleanup_identity_failure();
        let observed = control.clone();
        let mut stdout = Vec::new();

        let error = run_loop_with_control(config, &mut stdout, Vec::new(), control)
            .await
            .expect_err("delivery cleanup failure must not be acknowledged");

        assert!(
            error.contains(hoimin_core::WORKSPACE_CLEANUP_FAILED),
            "{error}"
        );
        assert!(!stdout.is_empty(), "report bytes should be flushed first");
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(!roots[0].exists(), "execution root must already be absent");
        assert!(roots[1].exists(), "same-name replacement must be preserved");
        assert!(
            observed
                .delivery_cleanup_sentinel()
                .expect("original delivery sentinel")
                .exists(),
            "identity-pinned original delivery root must not be confused with the replacement"
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged")
        );
    }

    #[tokio::test]
    async fn every_output_format_rejects_ack_on_write_or_delivery_cleanup_failure() {
        for format in ["json", "jsonl", "human"] {
            let project = tempfile::tempdir().unwrap();
            let config = output_failure_test_config(&project, format);
            let control = RunControl::new();
            let observed = control.clone();

            let error = run_loop_with_control(config, AlwaysFailingWriter, Vec::new(), control)
                .await
                .expect_err("report write failure must not be acknowledged");

            assert!(error.contains("report"), "format={format}: {error}");
            assert!(
                observed
                    .managed_root_paths()
                    .iter()
                    .all(|root| !root.exists()),
                "format={format}: report failure stranded a managed root"
            );
            assert!(
                !observed
                    .finalization_events()
                    .contains(&"output_acknowledged"),
                "format={format}: write failure was acknowledged"
            );

            let project = tempfile::tempdir().unwrap();
            let config = output_failure_test_config(&project, format);
            let control = RunControl::new();
            control.inject_delivery_cleanup_identity_failure();
            let observed = control.clone();

            let error = run_loop_with_control(config, Vec::new(), Vec::new(), control)
                .await
                .expect_err("delivery cleanup failure must not be acknowledged");

            assert!(
                error.contains(hoimin_core::WORKSPACE_CLEANUP_FAILED),
                "format={format}: {error}"
            );
            let roots = observed.managed_root_paths();
            assert!(!roots[0].exists(), "format={format}: execution root");
            assert!(roots[1].exists(), "format={format}: replacement root");
            assert!(
                observed
                    .delivery_cleanup_sentinel()
                    .expect("original delivery sentinel")
                    .exists(),
                "format={format}: original delivery sentinel"
            );
            assert!(
                !observed
                    .finalization_events()
                    .contains(&"output_acknowledged"),
                "format={format}: cleanup failure was acknowledged"
            );
        }
    }

    impl std::io::Write for BorrowedNonSendWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.buffer.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn shell_context_constructor_accepts_borrowed_non_send_writers() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let future = ShellContext::new(
            &config,
            BorrowedNonSendWriter {
                buffer: &mut stdout,
                _not_send: std::rc::Rc::new(()),
            },
            BorrowedNonSendWriter {
                buffer: &mut stderr,
                _not_send: std::rc::Rc::new(()),
            },
        );

        drop(future);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shell_setup_does_not_block_the_async_runtime() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let (heartbeat_tx, heartbeat_rx) = std::sync::mpsc::channel();
        let controller = std::thread::spawn(move || {
            entered_rx.recv_timeout(Duration::from_secs(4)).unwrap();
            let observed = heartbeat_rx.recv_timeout(Duration::from_secs(1)).is_ok();
            release_tx.send(()).unwrap();
            observed
        });
        let before_start: OwnedStartHook = Box::new(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        let heartbeat = async move {
            tokio::task::yield_now().await;
            let _ = heartbeat_tx.send(());
        };

        let (prepared, ()) = tokio::time::timeout(Duration::from_secs(6), async {
            tokio::join!(prepare_shell_setup(config, before_start), heartbeat)
        })
        .await
        .expect("shell setup regression must remain bounded");

        drop(prepared.unwrap());
        assert!(
            controller.join().unwrap(),
            "shell setup blocked the runtime until its blocking work was released"
        );
    }

    #[tokio::test]
    async fn shell_setup_panic_is_reported_as_a_setup_join_failure() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project);
        let result =
            prepare_shell_setup(config, Box::new(|| panic!("controlled shell setup panic"))).await;
        let Err(error) = result else {
            panic!("controlled setup panic unexpectedly succeeded");
        };

        assert!(error.starts_with("shell setup task failed:"), "{error}");
        assert!(error.contains("controlled shell setup panic"), "{error}");
    }

    #[test]
    fn every_managed_root_publication_boundary_rolls_back_on_unwind() {
        for selected in [
            ManagedRootSetupBoundary::ExecutionPublished,
            ManagedRootSetupBoundary::DeliveryPublished,
            ManagedRootSetupBoundary::ExecutionSpoolCreated,
            ManagedRootSetupBoundary::DeliverySpoolCreated,
        ] {
            let parent = tempfile::tempdir().unwrap();
            let parent = Utf8Path::from_path(parent.path()).unwrap();

            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = create_managed_shell_roots_in(parent, &|observed| {
                    assert_ne!(observed, selected, "injected setup failure at {selected:?}");
                });
            }));

            assert!(unwind.is_err(), "boundary {selected:?} did not unwind");
            let managed = parent.join("hoimin-workspaces-v1");
            let residual = std::fs::read_dir(&managed)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter(|name| name != ".hoimin-coordinator")
                .collect::<Vec<_>>();
            assert!(
                residual.is_empty(),
                "boundary {selected:?} leaked managed roots: {residual:?}"
            );
        }
    }

    #[test]
    fn shell_root_setup_carries_the_startup_janitor_reclaim_count() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        drop(coordinator);
        let abandoned = parent
            .join("hoimin-workspaces-v1")
            .join(format!(".deleting-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&abandoned).unwrap();

        let (rollback, execution, delivery, execution_spool, delivery_spool, reclaim) =
            create_managed_shell_roots_in(parent, &|_| {}).unwrap();

        assert_eq!(reclaim.reclaimed_roots, 1);
        assert!(!abandoned.exists());
        drop(delivery_spool);
        drop(execution_spool);
        drop(delivery);
        drop(execution);
        drop(rollback);
    }

    #[test]
    fn startup_janitor_diagnostics_reach_the_execution_cleanup_report() {
        let cleanup = CleanupRecord {
            status: hoimin_core::DiskCleanupStatus::Clean,
            examined_entries: 4,
            removed_entries: 3,
            details: vec!["execution cleanup detail".to_owned()],
            omitted_detail_count: 0,
            remaining_root: None,
        };
        let reclaim = ReclaimReport {
            reclaimed_roots: 1,
            preserved_roots: 1,
            details: vec!["startup janitor enumeration failed".to_owned()],
            omitted_detail_count: 0,
            truncated_detail_count: 0,
        };
        let mut report = hoimin_core::DiskCleanupReport {
            root_id: "execution".to_owned(),
            owner: "hoimin".to_owned(),
            status: hoimin_core::DiskCleanupStatus::Failed,
            examined_entries: 0,
            removed_entries: 0,
            details: Vec::new(),
            omitted_detail_count: 0,
            remaining_root: Some("stale".to_owned()),
        };

        super::apply_execution_cleanup_evidence(&mut report, &cleanup, &reclaim);

        assert_eq!(report.status, hoimin_core::DiskCleanupStatus::Clean);
        assert_eq!(report.examined_entries, 4);
        assert_eq!(report.removed_entries, 3);
        assert_eq!(
            report.details,
            [
                "execution cleanup detail",
                "startup janitor preserved 1 managed roots",
                "startup janitor enumeration failed",
            ]
        );
        assert_eq!(report.remaining_root, None);
    }

    #[test]
    fn every_post_publication_setup_boundary_rolls_back_on_unwind() {
        for selected in [
            ShellSetupBoundary::RootsCreated,
            ShellSetupBoundary::DiskPolicyVerified,
            ShellSetupBoundary::WorkspaceCreated,
            ShellSetupBoundary::BackendCreated,
            ShellSetupBoundary::ProcessCreated,
            ShellSetupBoundary::AnalyzerCreated,
            ShellSetupBoundary::ReportCreated,
        ] {
            let project = tempfile::tempdir().unwrap();
            let managed_parent = tempfile::tempdir().unwrap();
            let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
            let config = shell_setup_test_config(&project);

            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = prepare_shell_setup_sync_in(config, parent, &|observed| {
                    assert_ne!(observed, selected, "injected setup failure at {selected:?}");
                });
            }));

            assert!(unwind.is_err(), "boundary {selected:?} did not unwind");
            let managed = parent.join("hoimin-workspaces-v1");
            let residual = std::fs::read_dir(&managed)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter(|name| name != ".hoimin-coordinator")
                .collect::<Vec<_>>();
            assert!(
                residual.is_empty(),
                "boundary {selected:?} leaked managed roots: {residual:?}"
            );
        }
    }

    #[test]
    fn rollback_contention_marks_root_for_immediate_janitor_recovery() {
        use fs2::FileExt;

        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let execution =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let execution_path = execution.path().to_owned();
        let rollback = SetupRollback::new(execution);
        let coordinator_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(parent.join("hoimin-workspaces-v1/.hoimin-coordinator"))
            .unwrap();
        FileExt::try_lock_exclusive(&coordinator_file).unwrap();

        drop(rollback);
        assert!(
            execution_path.exists(),
            "contention path unexpectedly deleted root"
        );
        FileExt::unlock(&coordinator_file).unwrap();

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(!execution_path.exists());
    }

    fn process_effect(worker: u32) -> RunEffect {
        RunEffect::RunMutant(RunProcess {
            id: EffectId(1),
            worker: Some(worker),
            run_id: Some("run-1".into()),
            mutant_id: Some("mutant-1".into()),
            argv: Vec::new(),
            cwd: Utf8PathBuf::from("."),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1,
                max_memory_bytes: 1,
                max_processes: 1,
            },
        })
    }

    async fn context_with_worker() -> (
        tempfile::TempDir,
        ShellContext<Vec<u8>, Vec<u8>>,
        MutationCandidate,
        CreateWorker,
    ) {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("pkg")).unwrap();
        std::fs::write(project.path().join("pkg/a.py"), b"original\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("pkg/a.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let completed = context
            .workspace_mut()
            .handle_preflight(Preflight { id: EffectId(1) })
            .unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        context
            .workspace_mut()
            .handle_create_worker(grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let hash = context
            .workspace()
            .worker(0)
            .unwrap()
            .manifest()
            .entry(Utf8Path::new("pkg/a.py"))
            .unwrap()
            .blake3
            .to_hex()
            .to_string();
        let mut candidate = MutationCandidate {
            id: "candidate".into(),
            sequence: 1,
            path: "pkg/a.py".into(),
            span: ByteSpan {
                start: 0,
                length: 8,
            },
            original: "original".into(),
            replacement: "mutated!".into(),
            operator: "test".into(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: hash,
        };
        candidate.id = stable_mutant_id(&CandidateIdentity {
            schema_version: CANDIDATE_SCHEMA_VERSION,
            file_hash: candidate.file_hash.clone(),
            path: candidate.path.clone(),
            span: candidate.span,
            operator: candidate.operator.clone(),
            replacement: candidate.replacement.clone(),
        })
        .to_string();
        let retry = grant.create_worker(EffectId(5), 0).unwrap();
        (project, context, candidate, retry)
    }

    async fn wait_until_path_is_removed(path: &Utf8Path) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("detached cleanup did not remove {path}"));
    }

    async fn wait_until_process_handler_is_released(process: &std::sync::Weak<ProcessHandler>) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while process.strong_count() != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("detached cleanup retained process backend ownership");
    }

    #[test]
    fn diagnostic_run_id_tracks_a_session_adopted_id() {
        let config = crate::cli::parse_config_from([
            "hoimin", "run", "--root", ".", "--source", ".", "--", "python", "-m", "pytest",
        ])
        .unwrap();
        let state = RunState::new("persisted-session-run", config);
        let mut diagnostic_run_id = "initial-run".to_owned();

        track_diagnostic_run_id(&mut diagnostic_run_id, &state);

        assert_eq!(diagnostic_run_id, "persisted-session-run");
    }

    #[test]
    fn shutdown_error_keeps_the_primary_failure_when_drain_succeeds() {
        assert_eq!(
            combine_shutdown_errors("transition rejected".to_owned(), None),
            "transition rejected"
        );
    }

    #[test]
    fn shutdown_error_appends_a_drain_failure_after_the_primary_failure() {
        assert_eq!(
            combine_shutdown_errors(
                "transition rejected".to_owned(),
                Some("process task failed while stopping: panic".to_owned()),
            ),
            "transition rejected; process task failed while stopping: panic"
        );
    }

    #[test]
    fn shutdown_budget_total_timeout_deadline_is_anchored_to_run_deadline() {
        let now = tokio::time::Instant::now();
        let run_deadline = now - Duration::from_secs(1);
        let grace = Duration::from_millis(25);

        let budget = ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace);

        assert_eq!(budget.deadline(), run_deadline + grace);
        assert!(budget.deadline() < now);
    }

    #[test]
    fn shutdown_budget_first_activation_cannot_be_extended() {
        let now = tokio::time::Instant::now();
        let first = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            now,
            Duration::from_millis(10),
        );
        let later = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            now + Duration::from_secs(1),
            Duration::from_secs(5),
        );
        let mut active = None;

        let first_deadline = establish_shutdown_budget(&mut active, first).deadline();
        let retained = establish_shutdown_budget(&mut active, later);

        assert_eq!(retained.cause(), ShutdownCause::Cancellation);
        assert_eq!(retained.deadline(), first_deadline);
    }

    #[tokio::test]
    async fn cancellation_before_final_report_dispatch_preserves_the_queued_output() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::cancelling_before_run_finished();
        let observed_control = control.clone();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = tokio::time::timeout(
            Duration::from_secs(10),
            run_loop_with_control(config, &mut stdout, &mut stderr, control),
        )
        .await
        .expect("final report dispatch must not stall")
        .expect("the queued final report must remain dispatchable");

        assert_eq!(exit, 0);
        assert!(
            !observed_control
                .cancel_before_run_finished
                .load(Ordering::Acquire),
            "the cancellation hook did not observe RunFinished"
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(report["summary"]["complete"], true);
        assert!(
            !String::from_utf8(stderr).unwrap().contains("run stalled"),
            "the scheduler reported a false stall"
        );
    }

    #[test]
    fn outer_failure_establishes_a_failure_budget_when_the_loop_has_none() {
        let now = tokio::time::Instant::now();
        let mut active = None;

        ensure_outer_finalization_budget(
            &mut active,
            true,
            now + Duration::from_secs(10),
            now,
            Duration::from_millis(70),
        );

        let budget = active.unwrap();
        assert_eq!(budget.cause(), ShutdownCause::Failure);
        assert_eq!(budget.deadline(), now + Duration::from_millis(70));
    }

    #[test]
    fn shutdown_grace_falls_back_at_the_instant_representation_boundary() {
        let now = tokio::time::Instant::now();
        let mut accepted_seconds = 0_u64;
        let mut rejected_seconds = u64::MAX;
        while accepted_seconds < rejected_seconds {
            let difference = rejected_seconds - accepted_seconds;
            let candidate = accepted_seconds + difference / 2 + difference % 2;
            if now.checked_add(Duration::from_secs(candidate)).is_some() {
                accepted_seconds = candidate;
            } else {
                rejected_seconds = candidate - 1;
            }
        }
        let edge = now
            .checked_add(Duration::from_secs(accepted_seconds))
            .expect("binary search retains a representable instant");
        let grace = Duration::from_secs(2);
        assert!(edge.checked_add(grace).is_none());

        let budget = ShutdownBudget::for_total_timeout_with_grace(edge, grace);

        assert_eq!(budget.deadline(), edge);
    }

    #[test]
    fn remaining_budget_observes_live_deadline_and_preserves_effect_id() {
        let now = tokio::time::Instant::now();
        let id = EffectId(17);

        let observed = remaining_budget_observed(
            &ObserveRemainingBudget { id },
            now + Duration::from_secs(281),
            now,
        );
        let expired = remaining_budget_observed(
            &ObserveRemainingBudget { id },
            now,
            now + Duration::from_secs(1),
        );

        assert_eq!(observed.id, id);
        assert_eq!(observed.remaining, Duration::from_secs(281));
        assert_eq!(expired.id, id);
        assert_eq!(expired.remaining, Duration::ZERO);
    }

    #[tokio::test]
    async fn blocking_effect_classification_covers_every_filesystem_variant() {
        let (_project, _context, candidate, create) = context_with_worker().await;
        let read = RunEffect::ReadCandidate(hoimin_core::ReadCandidate {
            id: EffectId(31),
            worker: 0,
            spool: hoimin_core::CandidateSpoolRef {
                token: "spool.jsonl".into(),
                records: 1,
            },
            cursor: hoimin_core::CandidateCursor::START,
        });
        let effects = [
            RunEffect::Preflight(Preflight { id: EffectId(30) }),
            RunEffect::CreateWorker(create),
            read,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(32),
                worker: 0,
                candidate,
            }),
            RunEffect::ResetWorker(ResetWorker {
                id: EffectId(33),
                worker: 0,
            }),
            RunEffect::VerifyOriginals(VerifyOriginals {
                id: EffectId(34),
                checkpoint: IntegrityCheckpoint::PreFinalReport,
            }),
            RunEffect::Cleanup(Cleanup {
                id: EffectId(35),
                reservations: Vec::new(),
            }),
        ];

        assert!(effects.iter().all(is_blocking_io_effect));
        assert!(!is_blocking_io_effect(&process_effect(0)));
    }

    #[test]
    fn process_close_failure_does_not_replace_successful_workspace_cleanup() {
        let id = EffectId(351);
        let (event, secondary) = combine_cleanup_results(
            Some("injected close failure".to_owned()),
            Ok(hoimin_core::CleanupFinished {
                id,
                released_reservations: Vec::new(),
            }),
        );

        assert!(matches!(event, RunEvent::CleanupFinished(value) if value.id == id));
        assert_eq!(
            secondary,
            ["process.resource.close: injected close failure"]
        );
    }

    #[tokio::test]
    async fn owned_cleanup_restores_workspace_only_when_completion_is_accepted() {
        let (_project, mut context, _candidate, create) = context_with_worker().await;
        let task = prepare_blocking_effect(
            &mut context,
            RunEffect::Cleanup(Cleanup {
                id: EffectId(36),
                reservations: vec![create.reservation_id()],
            }),
        )
        .unwrap();
        assert!(context.workspace.is_none());

        let completion = task.execute();
        assert!(context.workspace.is_none());
        let event = accept_blocking_completion(&mut context, completion);

        assert!(matches!(event, RunEvent::CleanupFinished(_)));
        assert_eq!(context.workspace().worker_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_budget_preempts_an_owned_blocking_close() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let mut close = tokio::task::spawn_blocking(move || {
            entered_tx.send(()).unwrap();
            Some({
                release_rx.recv().unwrap();
                17_u8
            })
        });
        entered_rx.await.unwrap();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_millis(20),
        );

        let error = await_owned_blocking_until(&budget, &mut close)
            .await
            .unwrap_err();
        release_tx.send(()).unwrap();
        assert_eq!(close.await.unwrap(), Some(17));

        let OwnedBlockingError::Expired(error) = error else {
            panic!("blocking close returned a join failure instead of expiry")
        };
        assert!(error.starts_with("cancellation: shutdown grace expired"));
        assert!(error.contains("blocking I/O tasks: 1"));
    }

    #[tokio::test]
    async fn expired_final_close_detaches_cleanup_without_extending_the_wait() {
        let (_project, mut context, _candidate, _create) = context_with_worker().await;
        let worker_root = context.workspace().worker(0).unwrap().root().to_owned();
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() - Duration::from_secs(1),
            Duration::ZERO,
        );
        let started = Instant::now();

        let close =
            close_context_resources(&mut context, &budget, false, true, Box::new(|| {})).await;

        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(
            close
                .expiry
                .is_some_and(|error| error.starts_with("total timeout: shutdown grace expired"))
        );
        assert!(
            context.workspace.is_none(),
            "detached cleanup owns the workspace after expiry"
        );
        wait_until_path_is_removed(&worker_root).await;
    }

    #[tokio::test]
    async fn unsafe_final_close_never_detaches_or_removes_the_worker_workspace() {
        let (_project, mut context, _candidate, _create) = context_with_worker().await;
        let worker_root = context.workspace().worker(0).unwrap().root().to_owned();
        let execution_root = Arc::clone(&context.spool_dir.execution_root);
        let delivery_root = Arc::clone(&context.spool_dir.delivery_root);
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() - Duration::from_secs(1),
            Duration::ZERO,
        );

        let close =
            close_context_resources(&mut context, &budget, false, false, Box::new(|| {})).await;

        assert!(close.expiry.is_some());
        assert!(
            context.workspace.is_some(),
            "unproven process/output quiescence must retain workspace ownership"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            worker_root.exists(),
            "detached resource close must not delete a live worker workspace"
        );
        drop(context);
        assert!(
            worker_root.exists(),
            "dropping retained workspace ownership must leave cleanup to the janitor"
        );
        let cleanup_budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(2),
            Duration::ZERO,
        );
        assert!(matches!(
            cleanup_managed_root(execution_root, &cleanup_budget).await,
            ManagedCleanupOutcome::Clean(_)
        ));
        let cleanup_budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(2),
            Duration::ZERO,
        );
        assert!(matches!(
            cleanup_managed_root(delivery_root, &cleanup_budget).await,
            ManagedCleanupOutcome::Clean(_)
        ));
    }

    #[tokio::test]
    async fn dropping_context_without_finalization_leaves_managed_workers_for_the_janitor() {
        let (_project, context, _candidate, _create) = context_with_worker().await;
        let worker_root = context.workspace().worker(0).unwrap().root().to_owned();
        let execution_root = Arc::clone(&context.spool_dir.execution_root);
        let delivery_root = Arc::clone(&context.spool_dir.delivery_root);

        drop(context);

        assert!(
            worker_root.exists(),
            "Drop must not recursively delete a managed worker without lifecycle proof"
        );
        let cleanup_budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(2),
            Duration::ZERO,
        );
        assert!(matches!(
            cleanup_managed_root(execution_root, &cleanup_budget).await,
            ManagedCleanupOutcome::Clean(_)
        ));
        let cleanup_budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(2),
            Duration::ZERO,
        );
        assert!(matches!(
            cleanup_managed_root(delivery_root, &cleanup_budget).await,
            ManagedCleanupOutcome::Clean(_)
        ));
    }

    #[tokio::test]
    async fn managed_cleanup_outcome_preserves_the_deferred_cleanup_record() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let child = root.create_child("live-").unwrap();
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
        );

        let outcome = cleanup_managed_root(Arc::clone(&root), &budget).await;

        let ManagedCleanupOutcome::Deferred {
            record: Some(record),
            error,
        } = outcome
        else {
            panic!("live managed child did not produce a recorded deferred cleanup")
        };
        assert_eq!(record.status, hoimin_core::DiskCleanupStatus::Deferred);
        assert_eq!(record.remaining_root.as_deref(), Some(root.path()));
        assert!(error.starts_with("workspace.cleanup.deferred:"), "{error}");
        drop(child);
        let _ = root.cleanup(Duration::from_secs(1));
    }

    #[tokio::test]
    async fn managed_cleanup_record_preserves_cleanup_ready_marker_failure() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let child = root.create_child("live-").unwrap();
        let marker = root.path().join(".hoimin-cleanup-ready.json");
        std::fs::create_dir(&marker).unwrap();
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
        );

        let outcome = cleanup_managed_root(Arc::clone(&root), &budget).await;

        let ManagedCleanupOutcome::Deferred {
            record: Some(record),
            ..
        } = outcome
        else {
            panic!("live managed child did not produce a recorded deferred cleanup")
        };
        assert!(
            record
                .details
                .iter()
                .any(|detail| detail.starts_with("cleanup-ready: ")),
            "cleanup-ready failure was missing from {record:?}"
        );
        drop(child);
        std::fs::remove_dir(marker).unwrap();
        let _ = root.cleanup(Duration::from_secs(1));
    }

    #[tokio::test]
    async fn expired_cleanup_record_preserves_cleanup_ready_marker_failure() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let marker = root.path().join(".hoimin-cleanup-ready.json");
        std::fs::create_dir(&marker).unwrap();
        let marker_error = root.mark_cleanup_ready().unwrap_err().to_string();
        let record = root.abandon_for_janitor("managed-root cleanup budget expired".to_owned());

        let outcome = attach_cleanup_ready_error(
            ManagedCleanupOutcome::Deferred {
                record: Some(record),
                error: "workspace.cleanup.deferred: managed-root cleanup budget expired".to_owned(),
            },
            Some(&marker_error),
        );

        let ManagedCleanupOutcome::Deferred {
            record: Some(record),
            error,
        } = outcome
        else {
            panic!("expired cleanup did not preserve its deferred record")
        };
        assert!(
            record
                .details
                .iter()
                .any(|detail| detail.starts_with("cleanup-ready: ")),
            "cleanup-ready failure was missing from {record:?}"
        );
        assert!(error.contains("cleanup-ready: "), "{error}");
        std::fs::remove_dir(marker).unwrap();
        let _ = root.cleanup(Duration::from_secs(1));
    }

    #[test]
    fn clean_delivery_cleanup_surfaces_cleanup_ready_secondary() {
        let record = CleanupRecord {
            status: hoimin_core::DiskCleanupStatus::Clean,
            examined_entries: 1,
            removed_entries: 1,
            details: vec!["cleanup-ready: injected marker failure".to_owned()],
            omitted_detail_count: 0,
            remaining_root: None,
        };

        assert_eq!(
            delivery_cleanup_secondary_errors(&record),
            ["workspace.cleanup.failed: cleanup-ready: injected marker failure"]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_run_outer_close_uses_the_original_deadline_and_detaches_cleanup() {
        let (project, mut context, _candidate, _create) = context_with_worker().await;
        let worker_root = context.workspace().worker(0).unwrap().root().to_owned();
        let process = Arc::downgrade(&context.process);
        let run_deadline = tokio::time::Instant::now() + Duration::from_millis(20);
        let grace = Duration::from_millis(20);
        let mut active = None;
        let budget = ensure_outer_finalization_budget(
            &mut active,
            false,
            run_deadline,
            tokio::time::Instant::now(),
            grace,
        );
        assert_eq!(budget.cause(), ShutdownCause::TotalTimeout);
        assert_eq!(budget.deadline(), run_deadline + grace);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);

        let close = tokio::spawn(async move {
            let result = close_context_resources(
                &mut context,
                &budget,
                false,
                true,
                Box::new(move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }),
            )
            .await;
            (project, context, result)
        });
        entered_rx.await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let (_project, context, close) = close.await.unwrap();
        assert!(close.expiry.is_some());
        assert!(context.workspace.is_none());
        assert!(worker_root.exists(), "cleanup must still be paused");
        drop(context);

        release_tx.send(()).unwrap();
        wait_until_path_is_removed(&worker_root).await;
        wait_until_process_handler_is_released(&process).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_run_metrics_use_the_original_total_timeout_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        let run_deadline = tokio::time::Instant::now() + Duration::from_millis(20);
        let grace = Duration::from_millis(20);
        let mut active = None;
        let budget = ensure_outer_finalization_budget(
            &mut active,
            false,
            run_deadline,
            tokio::time::Instant::now(),
            grace,
        );
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let task_path = path.clone();

        let finalize = tokio::spawn(async move {
            finalize_metrics_with_shutdown(
                task_path,
                Some(MetricsCollector::new("run-1")),
                None,
                0,
                0,
                Vec::new(),
                &budget,
                Box::new(move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }),
            )
            .await
        });
        entered_rx.await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let result = finalize.await.unwrap();
        release_tx.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(result.expiry.is_some());
        assert!(
            !path.exists(),
            "metrics write started after shutdown expiry"
        );
    }

    #[tokio::test]
    async fn interrupt_monitor_remains_live_while_outer_finalization_is_pending() {
        let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
        let (forced_tx, forced_rx) = tokio::sync::oneshot::channel();
        let monitor = crate::interrupt::spawn_test_monitor(signal_rx, move |code| {
            let _ = forced_tx.send(code);
        });
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let finalization = finish_with_interrupt_monitor(monitor, async move {
            let _ = release_rx.await;
        });
        tokio::pin!(finalization);

        tokio::select! {
            () = &mut finalization => panic!("outer finalization was not released"),
            () = tokio::task::yield_now() => {}
        }
        signal_tx.send(Ok(())).unwrap();
        signal_tx.send(Ok(())).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), forced_rx)
                .await
                .expect("second signal must retain process-level precedence")
                .unwrap(),
            130
        );
        release_tx.send(()).unwrap();
        finalization.await;
    }

    #[test]
    fn expired_shutdown_skips_metrics_with_an_incomplete_warning() {
        let mut warnings = vec![("metrics.state", "preserved warning".to_owned())];

        skip_expired_metrics_finalization(
            Some(MetricsCollector::new("run-1")),
            Some("primary failure"),
            &mut warnings,
        );

        assert_eq!(warnings[0].1, "preserved warning");
        assert_eq!(warnings[1].0, "metrics.incomplete");
        assert!(warnings[1].1.contains("primary failure"));
    }

    #[tokio::test]
    async fn shutdown_expiry_keeps_cause_before_an_immediate_join_failure() {
        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(async { panic!("controlled process join failure") });
        tokio::task::yield_now().await;
        let mut io_tasks = JoinSet::new();
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut in_flight = 0;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::TotalTimeout,
            tokio::time::Instant::now(),
            Duration::from_secs(1),
        );

        let error = shutdown_expiry_error(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await;

        assert!(error.starts_with("total timeout: shutdown grace expired"));
        assert!(error.contains("process task failed while stopping"));
    }

    #[tokio::test]
    async fn failed_event_drain_expiry_keeps_effect_code_and_message() {
        let event = RunEvent::EffectFailed(EffectFailed::other(
            EffectId(91),
            "controlled.effect.code",
            "controlled effect message",
        ));
        let primary = failed_event_primary(&event);
        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(std::future::pending::<()>());
        let mut io_tasks = JoinSet::new();
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut in_flight = 1;
        let mut expiry_reported = false;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_millis(10),
        );

        let drain = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await;
        let error = finish_failed_event_drain(primary, drain).unwrap_err();

        assert!(error.contains("controlled.effect.code"), "{error}");
        assert!(error.contains("controlled effect message"), "{error}");
        assert!(error.contains("shutdown grace expired"), "{error}");
    }

    #[tokio::test]
    async fn direct_workspace_effect_completes_before_returning_worker_access() {
        let (_project, mut context, candidate, _retry) = context_with_worker().await;

        let event = execute_direct_io_effect(
            &mut context,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(3),
                worker: 0,
                candidate,
            }),
        );

        assert!(matches!(event, RunEvent::MutationApplied(_)));
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_drain_accepts_owned_workspace_state_without_process_metrics() {
        let (_project, mut context, candidate, _retry) = context_with_worker().await;
        let effect = RunEffect::ApplyMutation(ApplyMutation {
            id: EffectId(51),
            worker: 0,
            candidate,
        });
        let task = prepare_blocking_effect(&mut context, effect).unwrap();
        assert!(context.workspace().worker(0).is_none());
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let task = BlockingEffect::TestCompletion {
            id: EffectId(51),
            operation: Box::new(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                BlockingEffectCompletion::Workspace(Box::new(match task {
                    BlockingEffect::Workspace(task) => task.execute(),
                    _ => unreachable!("prepared apply task was not a workspace task"),
                }))
            }),
        };
        let (sender, mut receiver) = mpsc::channel(1);
        let mut io_tasks = JoinSet::new();
        spawn_blocking_effect(task, sender, &mut io_tasks);
        entered_rx.await.unwrap();
        let mut process_tasks = JoinSet::new();
        let mut in_flight = 1;
        let mut metrics = Some(MetricsCollector::new("run-1"));
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );

        let mut drain = Box::pin(drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            |completion| {
                let event = accept_blocking_completion(&mut context, completion);
                assert!(matches!(event, RunEvent::MutationApplied(_)));
            },
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut drain)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        drain.await.unwrap();

        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert!(metrics.unwrap().finish(0, 0).unwrap().workers.is_empty());
    }

    #[tokio::test]
    async fn shutdown_drain_expiry_reports_process_and_blocking_task_counts() {
        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(std::future::pending::<()>());
        let mut io_tasks = JoinSet::new();
        io_tasks.spawn(std::future::pending::<()>());
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut in_flight = 2;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_millis(10),
        );

        let error = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await
        .unwrap_err();

        assert_eq!(
            error,
            "cancellation: shutdown grace expired after 2s (process tasks: 1, blocking I/O tasks: 1)"
        );
        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn shutdown_drain_expiry_accepts_buffered_workspace_and_process_metrics() {
        let (_project, mut context, candidate, _retry) = context_with_worker().await;
        let task = prepare_blocking_effect(
            &mut context,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(61),
                worker: 0,
                candidate,
            }),
        )
        .unwrap();
        assert!(context.workspace().worker(0).is_none());
        let blocking = task.execute();

        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(7).unwrap();
        metrics.as_mut().unwrap().process_started(7).unwrap();
        let mut warnings = Vec::new();
        let (sender, mut receiver) = mpsc::channel(2);
        sender
            .send(ShellCompletion {
                event: RunEvent::CancellationRequested,
                process_task: true,
                io_task: false,
                process: Some((7, true)),
                blocking: None,
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion {
                event: RunEvent::CancellationRequested,
                process_task: false,
                io_task: true,
                process: None,
                blocking: Some(Box::new(blocking)),
            })
            .await
            .unwrap();
        drop(sender);

        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(std::future::pending::<()>());
        let mut io_tasks = JoinSet::new();
        io_tasks.spawn(std::future::pending::<()>());
        let mut in_flight = 2;
        let mut expiry_reported = false;
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() - Duration::from_secs(1),
            Duration::ZERO,
        );

        let error = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            |completion| {
                let event = accept_blocking_completion(&mut context, completion);
                assert!(matches!(event, RunEvent::MutationApplied(_)));
            },
        )
        .await
        .unwrap_err();

        assert!(error.starts_with("total timeout: shutdown grace expired after 2s"));
        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert!(metrics.unwrap().finish(1, 0).unwrap().workers.is_empty());
    }

    #[tokio::test]
    async fn blocking_io_keeps_the_async_runtime_responsive() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let operation = tokio::spawn(run_blocking_io(EffectId(17), move || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            42_u8
        }));

        entered_rx.await.unwrap();
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(1)).await;
        })
        .await
        .unwrap();
        release_tx.send(()).unwrap();

        assert_eq!(operation.await.unwrap().unwrap(), 42);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn preflight_filesystem_work_does_not_block_the_async_runtime() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let targets = vec![TargetSlice {
            path: Utf8PathBuf::from("target.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let expected_fingerprint = fingerprint(&FingerprintInput::from_config(
            &config,
            vec![SourceHash {
                path: Utf8PathBuf::from("target.py"),
                hash: *blake3::hash(b"pass\n").as_bytes(),
            }],
            targets.clone(),
            context.process.mode(),
        ));
        context.resolved_targets = Some(targets);
        let (pause, controller) = PreflightPause::new();
        context.workspace_mut().set_preflight_pause(pause);
        let (heartbeat_tx, heartbeat_rx) = std::sync::mpsc::channel();
        let preflight_controller = std::thread::spawn(move || {
            controller.wait_until_entered();
            let heartbeat_observed = heartbeat_rx
                .recv_timeout(Duration::from_millis(500))
                .is_ok();
            controller.release();
            heartbeat_observed
        });
        let heartbeat = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            heartbeat_tx.send(()).unwrap();
        };

        let (event, ()) = tokio::join!(
            execute_effect(
                &mut context,
                RunEffect::Preflight(Preflight { id: EffectId(17) }),
            ),
            heartbeat,
        );
        let heartbeat_observed = preflight_controller.join().unwrap();

        assert!(
            heartbeat_observed,
            "preflight blocked the runtime until its filesystem work was released"
        );
        let RunEvent::PreflightCompleted(completed) = event else {
            panic!("preflight failed: {event:?}")
        };
        assert_eq!(completed.id, EffectId(17));
        assert_eq!(completed.fingerprint, Some(expected_fingerprint));
        assert!(context.workspace.is_some());
    }

    #[tokio::test]
    async fn blocking_io_join_failure_preserves_effect_identity() {
        let id = EffectId(23);

        let error = run_blocking_io(id, || -> () { panic!("controlled blocking panic") })
            .await
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "shell.blocking_io");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blocking_io_effects_overlap_before_either_completes() {
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(2);
        let (release_first_tx, release_first_rx) = std::sync::mpsc::sync_channel(0);
        let (release_second_tx, release_second_rx) = std::sync::mpsc::sync_channel(0);
        let (completion_tx, mut completion_rx) = tokio::sync::mpsc::channel(2);
        let mut tasks = JoinSet::new();
        let first_entered = entered_tx.clone();
        spawn_blocking_effect(
            BlockingEffect::TestOperation {
                id: EffectId(41),
                operation: Box::new(move || {
                    first_entered.send(1).unwrap();
                    release_first_rx.recv().unwrap();
                    RunEvent::CancellationRequested
                }),
            },
            completion_tx.clone(),
            &mut tasks,
        );
        spawn_blocking_effect(
            BlockingEffect::TestOperation {
                id: EffectId(42),
                operation: Box::new(move || {
                    entered_tx.send(2).unwrap();
                    release_second_rx.recv().unwrap();
                    RunEvent::CancellationRequested
                }),
            },
            completion_tx,
            &mut tasks,
        );

        let mut entered = [
            entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        ];
        entered.sort_unstable();
        assert_eq!(entered, [1, 2]);
        release_first_tx.send(()).unwrap();
        release_second_tx.send(()).unwrap();

        assert!(completion_rx.recv().await.unwrap().io_task);
        assert!(completion_rx.recv().await.unwrap().io_task);
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn original_change_during_materialization_fails_post_materialization_before_baseline() {
        let project = tempfile::tempdir().unwrap();
        let original = project.path().join("target.py");
        std::fs::write(&original, b"original\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--jobs"),
            OsString::from("2"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let (control, mut pause_controller) = RunControl::with_materialization_pause(0);
        let observed_control = control.clone();
        let mutation = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(5))
                .expect("worker 0 did not enter materialization before the bounded wait expired");
            let release = pause_controller.release_guard();
            let mutation = std::fs::write(original, b"changed during materialization\n");
            drop(release);
            mutation.expect("change original during active worker materialization");
        });
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_loop_with_control(config, &mut stdout, &mut stderr, control)
            .await
            .unwrap();
        mutation.await.unwrap();

        assert_ne!(exit, 0);
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("workspace.original.changed"), "{stderr}");
        assert_eq!(
            observed_control.max_process_tasks(),
            0,
            "the baseline must not be dispatched after post-materialization verification fails"
        );
    }

    fn paused_materialization_config(
        project: &tempfile::TempDir,
        total_timeout: &str,
    ) -> RunConfig {
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--total-timeout"),
            OsString::from(total_timeout),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn total_timeout_bounds_paused_materialization_at_first_shutdown_deadline() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "200ms");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(0, Duration::from_millis(80));
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(2))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        let result = tokio::time::timeout(Duration::from_millis(500), &mut run).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();
        let Ok(result) = result else {
            let _ = tokio::time::timeout(Duration::from_secs(3), &mut run).await;
            panic!("paused materialization outlived the first shutdown deadline");
        };
        let error = result.unwrap_err();

        assert!(
            error.contains("total timeout: shutdown grace expired"),
            "{error}"
        );
        assert!(error.contains("blocking I/O tasks: 1"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repeated_cancellation_does_not_extend_paused_materialization_shutdown() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "5s");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(
                0,
                Duration::from_millis(250),
            );
        let cancelling = control.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(2))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        cancelling.cancel();
        tokio::select! {
            result = &mut run => panic!("run finished before the second cancellation: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(150)) => {}
        }
        cancelling.cancel();

        // About 100 ms remains on the first 250 ms grace period. A reset to the
        // second cancellation would take another 250 ms and exceed this bound.
        let result = tokio::time::timeout(Duration::from_millis(180), &mut run).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();
        let Ok(result) = result else {
            let _ = tokio::time::timeout(Duration::from_secs(3), &mut run).await;
            panic!("a later cancellation extended the first shutdown deadline");
        };
        let error = result.unwrap_err();

        assert!(
            error.contains("cancellation: shutdown grace expired"),
            "{error}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_released_inside_grace_finishes_orderly() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "5s");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(
                0,
                Duration::from_millis(300),
            );
        let cancelling = control.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(2))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        cancelling.cancel();
        tokio::time::sleep(Duration::from_millis(30)).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();

        let exit = tokio::time::timeout(Duration::from_secs(2), &mut run)
            .await
            .expect("released cancellation must finish inside grace")
            .unwrap();

        assert_eq!(exit, 130);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "manual blocking-I/O scheduler performance evidence"]
    async fn benchmark_blocking_io_dispatch() {
        const OPERATIONS: usize = 4;
        const OPERATION_MILLIS: u64 = 50;
        let serial_started = std::time::Instant::now();
        for sequence in 0..OPERATIONS {
            run_blocking_io(EffectId(u64::try_from(sequence).unwrap() + 1), || {
                std::thread::sleep(Duration::from_millis(OPERATION_MILLIS));
            })
            .await
            .unwrap();
        }
        let serial_millis = serial_started.elapsed().as_millis();

        let (completion_tx, mut completion_rx) = tokio::sync::mpsc::channel(OPERATIONS);
        let mut tasks = JoinSet::new();
        let concurrent_started = std::time::Instant::now();
        let mut io_in_flight = 0_usize;
        let mut max_io_in_flight = 0_usize;
        for sequence in 0..OPERATIONS {
            spawn_blocking_effect(
                BlockingEffect::TestOperation {
                    id: EffectId(u64::try_from(sequence).unwrap() + 1),
                    operation: Box::new(|| {
                        std::thread::sleep(Duration::from_millis(OPERATION_MILLIS));
                        RunEvent::CancellationRequested
                    }),
                },
                completion_tx.clone(),
                &mut tasks,
            );
            io_in_flight += 1;
            max_io_in_flight = max_io_in_flight.max(io_in_flight);
        }
        drop(completion_tx);
        for _ in 0..OPERATIONS {
            completion_rx.recv().await.unwrap();
            io_in_flight -= 1;
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
        let concurrent_millis = concurrent_started.elapsed().as_millis();
        assert_eq!(io_in_flight, 0);

        println!(
            "operations={OPERATIONS} operation_ms={OPERATION_MILLIS} serial_ms={serial_millis} concurrent_ms={concurrent_millis} max_io_in_flight={max_io_in_flight}"
        );
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_aba_change_during_validated_preflight() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        let input = project.path().join("config.toml");
        std::fs::write(&input, "version = 'A'\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--fingerprint-include"),
            OsString::from("config.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(41);
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        std::fs::write(&input, "version = 'B'\n").unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                std::fs::write(&input, "version = 'A'\n").unwrap();
                let result =
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id);
                std::fs::write(&input, "version = 'B'\n").unwrap();
                result
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_delete_restore_delete_aba() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        let input = project.path().join("config.toml");
        std::fs::write(&input, "version = 'A'\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--fingerprint-file"),
            OsString::from("config.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(43);
        context
            .fingerprint_copy_inputs
            .insert(Utf8PathBuf::from("config.toml"));
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        std::fs::remove_file(&input).unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                std::fs::write(&input, "version = 'A'\n").unwrap();
                let result =
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id);
                std::fs::remove_file(&input).unwrap();
                result
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_add_remove_add_aba() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        std::fs::write(project.path().join("base.cfg"), "base = true\n").unwrap();
        let added = project.path().join("added.cfg");
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--fingerprint-include"),
            OsString::from("*.cfg"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(44);
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        std::fs::write(&added, "added = true\n").unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                std::fs::remove_file(&added).unwrap();
                let result =
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id);
                std::fs::write(&added, "added = true\n").unwrap();
                result
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    #[tokio::test]
    async fn fingerprint_recheck_accepts_unchanged_inputs_excluded_from_worker_copy() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        std::fs::write(project.path().join(".gitignore"), "ignored.cfg\n").unwrap();
        std::fs::write(project.path().join("ignored.cfg"), "glob = true\n").unwrap();
        std::fs::write(project.path().join("excluded.toml"), "exact = true\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--exclude"),
            OsString::from("excluded.toml"),
            OsString::from("--fingerprint-include"),
            OsString::from("*.cfg"),
            OsString::from("--fingerprint-file"),
            OsString::from("excluded.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(42);
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        let completed = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id)
            })
            .unwrap();

        assert_eq!(completed.id, id);
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_changed_exact_input_excluded_from_worker_copy() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        let input = project.path().join("excluded.toml");
        std::fs::write(&input, "value = 'A'\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--exclude"),
            OsString::from("excluded.toml"),
            OsString::from("--fingerprint-file"),
            OsString::from("excluded.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        assert_eq!(config.fingerprint_inputs.len(), 1);
        assert_eq!(config.fingerprint_inputs[0].path, "excluded.toml");
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(45);
        let copied_at_start = BTreeSet::new();

        std::fs::write(&input, "value = 'B'\n").unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                assert!(manifest.entry(Utf8Path::new("excluded.toml")).is_none());
                recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id)
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    fn assert_metrics_sidecar_finishes(
        metrics: Option<MetricsCollector>,
        warnings: &[(&'static str, String)],
    ) {
        assert!(warnings.is_empty(), "warnings={warnings:?}");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        write_metrics(&path, &metrics).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn rejected_dispatch_does_not_start_a_queued_metrics_process() {
        let control = RunControl::new();
        control.cancel();
        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(0).unwrap();
        let mut warnings = Vec::new();

        let dispatch = accept_process_dispatch(&control, &mut metrics, &mut warnings, 0);

        assert!(dispatch.is_none());
        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert!(metrics.workers.is_empty());
    }

    #[test]
    fn cancellation_before_dispatch_discards_all_queued_process_metrics() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        for worker in [0, 1] {
            metrics.as_mut().unwrap().queued(worker).unwrap();
        }
        let mut effects = VecDeque::from([process_effect(0), process_effect(1)]);
        let mut warnings = Vec::new();

        let first = effects.pop_front().unwrap();
        cancel_queued_effect(&first, &mut metrics, &mut warnings);
        discard_queued_effects(&mut effects, &mut metrics, &mut warnings);

        assert_metrics_sidecar_finishes(metrics, &warnings);
    }

    #[test]
    fn stop_queue_filter_retains_only_effects_still_pending_in_the_machine() {
        let config = crate::cli::parse_config_from([
            "hoimin",
            "run",
            "--root",
            ".",
            "--source",
            ".",
            "--",
            "unused-test-command",
        ])
        .unwrap();
        let (state, pending_effects) = transition(
            RunState::new("run-1", config),
            RunEvent::StartRequested(StartRequested),
        )
        .unwrap();
        let pending = pending_effects.into_iter().next().unwrap();
        let pending_id = pending.id();
        let mut obsolete_effect = process_effect(7);
        let RunEffect::RunMutant(request) = &mut obsolete_effect else {
            unreachable!()
        };
        request.id = EffectId(999);
        assert!(state.is_effect_pending(pending_id));
        assert!(!state.is_effect_pending(obsolete_effect.id()));
        let mut effects = VecDeque::from([obsolete_effect, pending]);
        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(7).unwrap();
        let mut warnings = Vec::new();

        retain_machine_pending_effects(&mut effects, &state, &mut metrics, &mut warnings);

        assert_eq!(
            effects.iter().map(RunEffect::id).collect::<Vec<_>>(),
            [pending_id]
        );
        assert_metrics_sidecar_finishes(metrics, &warnings);
    }

    #[test]
    fn process_preparation_failure_discards_its_queued_metric() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(7).unwrap();
        let mut warnings = Vec::new();

        cancel_queued_worker(Some(7), &mut metrics, &mut warnings);

        assert_metrics_sidecar_finishes(metrics, &warnings);
    }

    #[test]
    fn ready_process_effects_are_recorded_as_queued() {
        let effects = [process_effect(3)];
        let mut metrics = Some(MetricsCollector::new("run-1"));
        let mut warnings = Vec::new();

        record_ready_processes(&effects, &mut metrics, &mut warnings);
        cancel_queued_effect(&effects[0], &mut metrics, &mut warnings);

        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert!(metrics.workers.is_empty());
    }

    #[tokio::test]
    async fn drain_closes_failed_and_cancelled_process_metrics() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        for worker in [0, 1] {
            metrics.as_mut().unwrap().queued(worker).unwrap();
            metrics.as_mut().unwrap().process_started(worker).unwrap();
        }
        let mut warnings = Vec::new();
        let mut tasks = JoinSet::new();
        let (sender, mut receiver) = mpsc::channel(2);
        sender
            .send(ShellCompletion {
                event: RunEvent::EffectFailed(hoimin_core::EffectFailed::other(
                    EffectId(1),
                    "process.spawn",
                    "failed",
                )),
                process_task: true,
                io_task: false,
                process: Some((0, true)),
                blocking: None,
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion {
                event: RunEvent::CancellationRequested,
                process_task: true,
                io_task: false,
                process: Some((1, true)),
                blocking: None,
            })
            .await
            .unwrap();
        drop(sender);
        let mut in_flight = 2;
        let mut io_tasks = JoinSet::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );

        drain_processes(
            &mut tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await
        .unwrap();

        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(2, 0).unwrap();
        assert_eq!(
            metrics
                .workers
                .iter()
                .map(|worker| worker.processes)
                .sum::<u64>(),
            0
        );
    }

    #[test]
    fn materialization_verification_keeps_copy_metrics_stage_open() {
        let mut collector = MetricsCollector::new("run-1");
        collector.begin_stage("copy").unwrap();
        let mut metrics = Some(collector);
        let mut warnings = Vec::new();

        observe_accepted_transition(
            &mut metrics,
            &mut warnings,
            RunPhase::Copy,
            RunPhase::MaterializationVerification,
            false,
            false,
            false,
        );

        assert!(warnings.is_empty());
        metrics
            .as_mut()
            .unwrap()
            .finish_stage("copy")
            .expect("post-materialization verification remains part of the copy stage");
    }

    #[test]
    fn direct_early_cleanup_finishes_the_departed_active_stage() {
        for (phase, stage) in [
            (RunPhase::Baseline, "baseline"),
            (RunPhase::Analyze, "analysis"),
        ] {
            let mut collector = MetricsCollector::new("run-1");
            collector.begin_stage(stage).unwrap();
            let mut metrics = Some(collector);
            let mut warnings = Vec::new();

            observe_accepted_transition(
                &mut metrics,
                &mut warnings,
                phase,
                RunPhase::Cleaning,
                false,
                false,
                false,
            );
            observe_accepted_transition(
                &mut metrics,
                &mut warnings,
                RunPhase::Cleaning,
                RunPhase::Finished,
                false,
                false,
                true,
            );

            assert!(warnings.is_empty());
            let metrics = metrics.unwrap().finish(0, 0).unwrap();
            assert!(metrics.stages.iter().any(|metric| metric.name == stage));
            assert!(metrics.stages.iter().any(|metric| metric.name == "cleanup"));
        }
    }

    #[test]
    fn leaving_cleanup_without_a_completion_signal_finishes_the_stage() {
        let mut collector = MetricsCollector::new("run-1");
        collector.begin_stage("cleanup").unwrap();
        let mut metrics = Some(collector);
        let mut warnings = Vec::new();

        observe_accepted_transition(
            &mut metrics,
            &mut warnings,
            RunPhase::Cleaning,
            RunPhase::Finished,
            false,
            false,
            false,
        );

        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert_eq!(metrics.stages.len(), 1);
        assert_eq!(metrics.stages[0].name, "cleanup");
    }

    #[tokio::test]
    async fn spawned_mutant_reports_completion_and_mutant_accounting() {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let process = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        ));
        let request = RunProcess {
            id: EffectId(9),
            worker: Some(4),
            run_id: Some("run-1".into()),
            mutant_id: Some("mutant-1".into()),
            argv: vec![missing_executable_arg()],
            cwd: Utf8PathBuf::from("."),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1,
                max_memory_bytes: 1,
                max_processes: 1,
            },
        };
        let control = RunControl::new();
        let dispatch = control.begin_dispatch().unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let mut tasks = JoinSet::new();

        spawn_process(
            process,
            request.into(),
            4,
            false,
            sender,
            &mut tasks,
            dispatch,
        );

        let completion = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("spawned process must complete")
            .expect("completion channel must remain open");
        assert!(matches!(completion.event, RunEvent::EffectFailed(_)));
        assert!(completion.process_task);
        assert_eq!(completion.process, Some((4, true)));
        tasks.join_next().await.unwrap().unwrap();
    }

    #[test]
    fn worker_metadata_removes_case_variants_and_baseline_mutant_leakage() {
        let mut environment = crate::workspace::CommandEnvironment {
            cwd: "worker/root".into(),
            env: BTreeMap::from([
                (
                    OsString::from("hoimin_worker_root"),
                    OsString::from("parent"),
                ),
                (OsString::from("HoImIn_RuN_Id"), OsString::from("parent")),
                (OsString::from("hoimin_mutant_id"), OsString::from("parent")),
            ]),
        };

        set_worker_metadata(&mut environment, Some("run-1"), None);

        assert_eq!(
            environment.env.get(&OsString::from("HOIMIN_WORKER_ROOT")),
            Some(&OsString::from("worker/root"))
        );
        assert_eq!(
            environment.env.get(&OsString::from("HOIMIN_RUN_ID")),
            Some(&OsString::from("run-1"))
        );
        assert!(environment.env.keys().all(|key| {
            !key.to_string_lossy()
                .eq_ignore_ascii_case("HOIMIN_MUTANT_ID")
        }));
        assert_eq!(
            environment
                .env
                .keys()
                .filter(|key| key.to_string_lossy().eq_ignore_ascii_case("HOIMIN_RUN_ID"))
                .count(),
            1
        );
    }

    #[test]
    fn first_interrupt_maps_success_to_cancellation_and_preserves_failure() {
        assert!(matches!(
            first_interrupt_event(Ok(())),
            Ok(RunEvent::CancellationRequested)
        ));
        let error =
            first_interrupt_event(Err("install Ctrl+C handler: fixture".to_owned())).unwrap_err();
        assert_eq!(error, "install Ctrl+C handler: fixture");
    }
}
