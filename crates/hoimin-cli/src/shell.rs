//! Coordinates effects, completion acceptance and resource finalization for a run.
//!
//! [`hoimin_core::transition`] computes state and effects without I/O. This module
//! dispatches those effects and feeds accepted events back to core. Keep stop
//! observation, pending-effect checks, completion acceptance and transition in
//! `run_loop_context` in their audited order when changing scheduling.
//!
//! Process tasks own requests and share the process handler; their completions
//! carry worker accounting through acceptance and join. Blocking effects take
//! workspace state, capabilities, or the candidate replay owner out of [`ShellContext`];
//! `accept_blocking_completion` restores returned ownership. A task join alone
//! does not acknowledge a core effect. At shutdown expiry, `drain_processes`
//! recovers buffered payloads and accounting before aborting async wrappers;
//! this recovery does not certify cleanup, session or final-output completion.
//!
//! The first shutdown cause and absolute deadline remain fixed through draining
//! and outer finalization. An already-running blocking operation can outlive an
//! aborted wrapper or timed-out wait. Resource close may detach with ownership;
//! unsafe workspace removal is deferred to the managed-root janitor. Later task
//! termination is not evidence of an accepted completion before expiry.
//!
//! [`hoimin_core::disk::DiskLifecycle`] governs dispatch, cleanup and finish
//! admission. `CleanupProgress` records local attempts, including rejected
//! requests, so outer finalization does not repeat them; it grants no removal
//! permission. Execution-root removal requires process/output drain, monitor
//! and final-measurement quiescence. On the successful path, execution cleanup
//! precedes final report delivery; write/flush and delivery-root cleanup precede
//! the final `OutputEmitted` event. Failures retain incomplete cleanup evidence.
//!
//! Saved-plan execution consumes [`crate::plan::VerifiedPlan`], keeping checked
//! configuration, candidate selection, provenance and protected inputs together.
//! [`crate::plan::VerifiedPlan::execute`] consumes owned output writers and enters
//! the same report lifecycle as configured runs.
//!
//! Run and verify report delivery moves the writer and delivery-root leases into
//! blocking work and bounds the async wait. Expiry cannot stop a running write
//! or flush, so its ownership must remain live.
//!
//! For deadline details, see the [development guide]. For cancellation, expiry
//! or output-failure changes, start with this module's `tests` (budget, drain,
//! cleanup and finalization), `report::delivery::tests`, and CLI integration tests
//! `lean_shutdown_oracle.rs` and `lean_disk_shutdown_oracle.rs`. Their model
//! entry points are `ShutdownModel.lean` and `DiskGuardModel.lean` under
//! `formal/HoiminOracle/HoiminOracle/`: proofs concern the models, while adapters
//! compare selected implementation observations, not all Rust/OS executions.
//!
//! [development guide]: https://github.com/HayaoSuzuki/hoimin/blob/main/docs/development.md#shutdown-deadline-invariants

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
use tracing::Instrument;
use uuid::Uuid;

use crate::analyzer::{AnalyzerHandler, CandidateReplay};
use crate::metrics::{MetricsCollector, MetricsError, finalize_metrics};
use crate::metrics_destination::{MetricsDestination, validate_metrics_destination};
use crate::process::{ProcessCancellation, ProcessHandler, ProcessRequest, ProcessStartGate};
use crate::report::ReportHandler;
use crate::report::delivery::ReportDelivery;
#[cfg(not(any(windows, target_os = "linux")))]
use crate::resource::PortableBackend;
use crate::resource::ResourceBackend;
use crate::session::{SessionArtifacts, SessionDispatcher};
use crate::target::TargetHandler;
use crate::workspace::{
    CleanupDisposition, CleanupEvidence, CleanupRecord, CopyOptions, DiskMonitor, FilesystemKey,
    ManagedChild, ManagedRootCoordinator, ManagedRunRoot, OwnerKind, ReclaimReport,
    WorkspaceHandler, WorkspaceManifest, WorkspaceTask, WorkspaceTaskCompletion,
    available_for_managed_roots, measure_managed_roots, truncate_diagnostic_detail,
};
#[cfg(test)]
use crate::workspace::{
    DiskMeasurement, MaterializationPause, MaterializationPauseController, PreflightPause,
};

mod finalization;
use finalization::{DrainEvidence, ExecutionFinalization};

mod preparation;
use preparation::{PreparationError, PreparationToken};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
#[cfg(test)]
const FINAL_MEASUREMENT_PAUSE_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_QUIESCENCE_UNPROVEN: &str =
    "process/output drain, disk monitor join, or final disk measurement join was not proven";

const fn cleanup_quiescence_proven(components: [bool; 4]) -> bool {
    components[0] && components[1] && components[2] && components[3]
}

fn lifecycle_safety_succeeded(lifecycle: &ShellDiskLifecycle) -> bool {
    lifecycle.lifecycle.safety_succeeded()
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
    cancelled_at: Arc<std::sync::Mutex<Option<tokio::time::Instant>>>,
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
    force_execution_cleanup_failed_without_record: Arc<AtomicBool>,
    #[cfg(test)]
    force_inner_monitor_join_timeout_once: Arc<AtomicBool>,
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
    entered: std::sync::mpsc::Sender<()>,
    release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    release_timeout: Duration,
}

#[cfg(test)]
#[derive(Debug)]
struct FinalMeasurementPauseController {
    entered: std::sync::mpsc::Receiver<()>,
    release: std::sync::mpsc::Sender<()>,
    entered_timeout: Duration,
}

#[cfg(test)]
impl FinalMeasurementPause {
    fn new() -> (Self, FinalMeasurementPauseController) {
        Self::with_timeout(FINAL_MEASUREMENT_PAUSE_TIMEOUT)
    }

    fn with_timeout(timeout: Duration) -> (Self, FinalMeasurementPauseController) {
        let (entered, entered_receiver) = std::sync::mpsc::channel();
        let (release, release_receiver) = std::sync::mpsc::channel();
        (
            Self {
                entered,
                release: Arc::new(std::sync::Mutex::new(release_receiver)),
                release_timeout: timeout,
            },
            FinalMeasurementPauseController {
                entered: entered_receiver,
                release,
                entered_timeout: timeout,
            },
        )
    }

    fn wait(&self) {
        if self.entered.send(()).is_ok() {
            let _ = self
                .release
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .recv_timeout(self.release_timeout);
        }
    }
}

#[cfg(test)]
impl FinalMeasurementPauseController {
    fn wait_until_entered(&self) -> Result<(), std::sync::mpsc::RecvTimeoutError> {
        self.entered.recv_timeout(self.entered_timeout)
    }

    fn release(&self) -> Result<(), std::sync::mpsc::SendError<()>> {
        self.release.send(())
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
            cancelled_at: Arc::new(std::sync::Mutex::new(None)),
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
            force_execution_cleanup_failed_without_record: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            force_inner_monitor_join_timeout_once: Arc::new(AtomicBool::new(false)),
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
        root.move_for_identity_replacement_test(&sentinel)
            .expect("move delivery root to sentinel");
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
    fn inject_execution_cleanup_failed_without_record(&self) {
        self.force_execution_cleanup_failed_without_record
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn take_execution_cleanup_failed_without_record(&self) -> bool {
        self.force_execution_cleanup_failed_without_record
            .swap(false, Ordering::AcqRel)
    }

    #[cfg(test)]
    fn inject_inner_monitor_join_timeout_once(&self) {
        self.force_inner_monitor_join_timeout_once
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn take_inner_monitor_join_timeout_once(&self) -> bool {
        self.force_inner_monitor_join_timeout_once
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
        self.cancelled_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(tokio::time::Instant::now);
        self.request.cancel();
    }

    fn establish_event_shutdown_budget(
        &self,
        active: &mut Option<ShutdownBudget>,
        event: &RunEvent,
        run_deadline: tokio::time::Instant,
        observed_at: tokio::time::Instant,
        grace: Duration,
    ) -> Option<ShutdownBudget> {
        let observed_at = if matches!(event, RunEvent::CancellationRequested) {
            self.cancelled_at
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .unwrap_or(observed_at)
        } else {
            observed_at
        };
        if matches!(event, RunEvent::CancellationRequested) && observed_at >= run_deadline {
            // Cancellation retains event precedence, but cannot extend the
            // shutdown window of a deadline that has already elapsed.
            return Some(establish_shutdown_budget(
                active,
                ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace),
            ));
        }
        establish_event_shutdown_budget(active, event, run_deadline, observed_at, grace)
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

/// A serial event owns no task; spawned completions retain the data needed to
/// join and account for their task, or to return blocking-operation ownership.
enum ShellCompletion {
    Serial(RunEvent),
    Process {
        event: RunEvent,
        context: WorkerProcessContext,
    },
    Blocking(Result<Box<BlockingEffectCompletion>, EffectFailed>),
}

/// Task origin survives payload acceptance until transition, accounting, and join.
#[derive(Clone, Copy)]
enum CompletionSource {
    Serial,
    Process(WorkerProcessContext),
    Blocking,
}

enum BlockingEffect {
    Workspace(Box<WorkspaceTask>),
    Candidate {
        request: hoimin_core::ReadCandidate,
        replay: CandidateReplay,
    },
    Preflight {
        id: EffectId,
        workspace: Box<WorkspaceHandler>,
        request: hoimin_core::Preflight,
        config: Box<RunConfig>,
        copied_at_start: BTreeSet<Utf8PathBuf>,
        metrics_protected_inputs: Vec<std::path::PathBuf>,
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
    Candidate {
        id: EffectId,
        replay: CandidateReplay,
        event: Box<RunEvent>,
    },
    #[cfg(test)]
    TestEvent(Box<RunEvent>),
    OwnedWorkspace {
        id: EffectId,
        workspace: Box<WorkspaceHandler>,
        event: Box<RunEvent>,
        secondary_errors: Vec<String>,
        metrics_destination: Option<MetricsDestination>,
        session_path: Option<Utf8PathBuf>,
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
            Self::Candidate { request, .. } => request.id,
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
            Self::Candidate {
                request,
                mut replay,
            } => {
                let event = replay_candidate(&mut replay, &request);
                BlockingEffectCompletion::Candidate {
                    id: request.id,
                    replay,
                    event: Box::new(event),
                }
            }
            Self::Preflight {
                id,
                mut workspace,
                request,
                config,
                copied_at_start,
                metrics_protected_inputs,
                targets,
                resource_mode,
            } => {
                let mut run_fingerprint = None;
                let mut metrics_destination = None;
                let mut resolved_session_path = None;
                let validation =
                    validate_metrics_destination(&config, &targets, &metrics_protected_inputs, id)
                        .map(|value| {
                            metrics_destination = Some(value);
                        });
                let preflight = validation.and_then(|()| {
                    prepare_session_artifacts(&config)
                        .map_err(|error| EffectFailed::other(id, "session.path", error))
                        .and_then(|(exclusions, session_path)| {
                            resolved_session_path = session_path;
                            workspace.set_literal_exclusions(exclusions);
                            workspace.handle_preflight_validated(request, |root, manifest| {
                                recheck_fingerprint_inputs(
                                    &config,
                                    root,
                                    manifest,
                                    &copied_at_start,
                                    id,
                                )?;
                                run_fingerprint = Some(prepare_manifest_fingerprint(
                                    &config,
                                    &targets,
                                    resource_mode,
                                    manifest,
                                    id,
                                )?);
                                Ok(())
                            })
                        })
                });
                let (event, session_path) = match preflight {
                    Ok(mut value) => {
                        value.fingerprint = run_fingerprint;
                        (RunEvent::PreflightCompleted(value), resolved_session_path)
                    }
                    Err(error) => (RunEvent::EffectFailed(error), None),
                };
                BlockingEffectCompletion::OwnedWorkspace {
                    id,
                    workspace,
                    event: Box::new(event),
                    secondary_errors: Vec::new(),
                    metrics_destination,
                    session_path,
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
                    metrics_destination: None,
                    session_path: None,
                }
            }
            #[cfg(test)]
            Self::TestOperation { operation, .. } => {
                BlockingEffectCompletion::TestEvent(Box::new(operation()))
            }
            #[cfg(test)]
            Self::TestCompletion { operation, .. } => operation(),
        }
    }
}

fn replay_candidate(
    replay: &mut CandidateReplay,
    request: &hoimin_core::ReadCandidate,
) -> RunEvent {
    match replay.read(&request.spool, &request.target) {
        Ok(candidate) => RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            candidate,
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

fn prepare_session_artifacts(
    config: &RunConfig,
) -> Result<(Vec<crate::workspace::LiteralExclusion>, Option<Utf8PathBuf>), String> {
    let mut literal_exclusions = Vec::new();
    let mut resolved_path = None;
    if let Some(session) = config.session.config() {
        let artifacts =
            SessionArtifacts::resolve(&session.path).map_err(|error| error.to_string())?;
        let root =
            std::fs::canonicalize(&config.selection.root).map_err(|error| error.to_string())?;
        let trees = artifacts.lock_trees().map_err(|error| error.to_string())?;
        for (path, tree) in artifacts
            .files()
            .into_iter()
            .map(|path| (path, false))
            .chain(trees.into_iter().map(|path| (path, true)))
        {
            if let Some(relative) = crate::workspace::relative_inside(&path, &root) {
                let relative = Utf8PathBuf::from_path_buf(relative)
                    .map_err(|_| "session artifact path is not UTF-8".to_owned())?;
                let relative = crate::portable_path::from_native(relative.as_str())
                    .map_err(|error| error.to_string())?;
                let relative = Utf8PathBuf::from(relative.into_owned());
                literal_exclusions.push(if tree {
                    crate::workspace::LiteralExclusion::Tree(relative)
                } else {
                    crate::workspace::LiteralExclusion::File(relative)
                });
            }
        }
        resolved_path = Some(
            Utf8PathBuf::from_path_buf(artifacts.database().to_owned())
                .map_err(|_| "session database path is not UTF-8".to_owned())?,
        );
    }
    Ok((literal_exclusions, resolved_path))
}

fn prepare_blocking_effect(
    context: &mut ShellContext,
    effect: RunEffect,
) -> Result<BlockingEffect, EffectFailed> {
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
                metrics_protected_inputs: context.metrics_protected_inputs.clone(),
                targets,
                resource_mode: context.process.mode(),
            });
        }
        RunEffect::CreateWorker(request) => context.workspace_mut().prepare_create_task(request),
        RunEffect::ReadCandidate(request) => {
            let replay = context.candidate_replay.take().ok_or_else(|| {
                EffectFailed::other(
                    request.id,
                    "shell.blocking_io",
                    "candidate replay ownership is unavailable",
                )
            })?;
            return Ok(BlockingEffect::Candidate { request, replay });
        }
        RunEffect::ApplyMutation(request) => context.workspace_mut().prepare_apply_task(request),
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

fn accept_blocking_completion(
    context: &mut ShellContext,
    completion: BlockingEffectCompletion,
) -> RunEvent {
    match completion {
        BlockingEffectCompletion::Workspace(completion) => context
            .workspace_mut()
            .accept_task_completion(*completion)
            .unwrap_or_else(RunEvent::EffectFailed),
        BlockingEffectCompletion::Candidate { id, replay, event } => {
            if context.candidate_replay.is_some() {
                return RunEvent::EffectFailed(EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "blocking operation returned duplicate candidate replay ownership",
                ));
            }
            context.candidate_replay = Some(replay);
            *event
        }
        #[cfg(test)]
        BlockingEffectCompletion::TestEvent(event) => *event,
        BlockingEffectCompletion::OwnedWorkspace {
            id,
            workspace,
            event,
            secondary_errors,
            metrics_destination,
            session_path,
        } => {
            if context.workspace.is_some() {
                return RunEvent::EffectFailed(EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "blocking operation returned duplicate workspace ownership",
                ));
            }
            context.workspace = Some(*workspace);
            if let Some(destination) = metrics_destination {
                context.metrics_destination = destination;
            }
            if let Some(path) = session_path {
                context.session_path = Some(path);
            }
            context.blocking_secondary_errors.extend(secondary_errors);
            *event
        }
    }
}

#[cfg(test)]
fn execute_direct_io_effect(context: &mut ShellContext, effect: RunEffect) -> RunEvent {
    let result = match effect {
        RunEffect::CreateWorker(request) => context
            .workspace_mut()
            .handle_create_worker(request)
            .map(RunEvent::WorkerCreated),
        RunEffect::ReadCandidate(request) => {
            return match prepare_blocking_effect(context, RunEffect::ReadCandidate(request)) {
                Ok(task) => accept_blocking_completion(context, task.execute()),
                Err(error) => RunEvent::EffectFailed(error),
            };
        }
        RunEffect::ApplyMutation(request) => context
            .workspace_mut()
            .handle_apply_mutation(request)
            .map(RunEvent::MutationApplied),
        RunEffect::ResetWorker(request) => context
            .workspace_mut()
            .handle_reset_worker(request)
            .map(RunEvent::WorkerReset),
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
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(operation))
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
    spool_dir: Arc<ManagedShellRoots>,
    config: RunConfig,
    // Keep this last so every handler/root clone is dropped before an armed rollback.
    rollback: SetupRollback,
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
    timing: Option<PreparationToken>,
}

impl SetupRollback {
    fn new(execution: Arc<ManagedRunRoot>) -> Self {
        Self {
            execution,
            delivery: None,
            execution_spool: None,
            delivery_spool: None,
            armed: true,
            timing: None,
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

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SetupRollback {
    fn drop(&mut self) {
        if self.armed {
            if let Some(timing) = &self.timing {
                timing.failure_budget();
            }
            // A concurrent creator may hold the coordinator lock long enough for immediate
            // cleanup to defer. Publish cleanup-ready first so releasing our leases never turns
            // a setup failure into a young root that the janitor must preserve for 24 hours.
            for root in std::iter::once(&self.execution).chain(self.delivery.iter()) {
                if let Err(error) = root.mark_cleanup_ready()
                    && let Some(timing) = &self.timing
                {
                    timing.record_failure(format!("preparation rollback marker failed: {error}"));
                }
            }
            // Close every child capability owned by the guard before the root lifecycle checks
            // for live handles. Callers bind this guard before their additional Arc clones, so
            // those clones are also dropped before this guard during error unwinding.
            self.delivery_spool.take();
            self.execution_spool.take();
            // Both roots share one absolute shutdown deadline. A late blocking
            // owner cannot acquire another grace interval when it finally returns.
            let fallback = tokio::time::Instant::now() + SHUTDOWN_GRACE;
            for root in std::iter::once(&self.execution).chain(self.delivery.iter()) {
                let remaining = self.timing.as_ref().map_or_else(
                    || fallback.saturating_duration_since(tokio::time::Instant::now()),
                    PreparationToken::rollback_remaining,
                );
                if remaining.is_zero() {
                    continue;
                }
                let cleanup = root.cleanup(remaining);
                if cleanup.disposition == CleanupDisposition::Failed
                    && let Some(timing) = &self.timing
                {
                    timing.record_failure(format!(
                        "preparation rollback failed: {}",
                        cleanup.evidence.details.join("; ")
                    ));
                }
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
    DeliveryReady,
}

fn prepare_shell_setup_sync_in(
    config: RunConfig,
    temporary_parent: &Utf8Path,
    boundary: &impl Fn(ShellSetupBoundary),
) -> Result<PreparedShellSetup, String> {
    prepare_shell_setup_controlled(config, temporary_parent, boundary, None).map_err(|error| {
        match error {
            PreparationError::Failed(error) => error,
            PreparationError::Stopped(cause) => format!("{} during preparation", cause.label()),
        }
    })
}

fn preparation_check(timing: Option<&PreparationToken>) -> Result<(), PreparationError> {
    timing.map_or(Ok(()), PreparationToken::check)
}

fn prepare_shell_setup_controlled(
    config: RunConfig,
    temporary_parent: &Utf8Path,
    boundary: &impl Fn(ShellSetupBoundary),
    timing: Option<&PreparationToken>,
) -> Result<PreparedShellSetup, PreparationError> {
    preparation_check(timing)?;
    let (rollback, execution_root, delivery_root, execution_spool, delivery_spool, startup_reclaim) =
        create_managed_shell_roots_controlled(temporary_parent, &|_| {}, timing)?;
    boundary(ShellSetupBoundary::RootsCreated);
    preparation_check(timing)?;
    boundary(ShellSetupBoundary::DiskPolicyVerified);
    preparation_check(timing)?;
    let spool_path = execution_spool.path().to_owned();
    let requested_workers = u32::try_from(config.limits.jobs.get())
        .map_err(|_| "--jobs exceeds the supported worker count".to_owned())?;
    let workspace = WorkspaceHandler::new(
        config.selection.root.clone(),
        config.selection.sources.clone(),
        requested_workers,
        CopyOptions {
            includes: config.selection.includes.clone(),
            excludes: config.selection.excludes.clone(),
            literal_exclusions: Vec::new(),
        },
    )
    .with_import_roots(config.import_roots.clone())
    .with_managed_root(Arc::clone(&execution_root))
    .with_max_owned_bytes(config.limits.max_workspace_size.get())
    .with_max_copy_bytes(config.limits.max_copy_size.get());
    boundary(ShellSetupBoundary::WorkspaceCreated);
    preparation_check(timing)?;
    let backend = resource_backend(&config).map_err(|error| error.to_string())?;
    boundary(ShellSetupBoundary::BackendCreated);
    preparation_check(timing)?;
    let process = Arc::new(ProcessHandler::new(backend, spool_path.join("process")));
    boundary(ShellSetupBoundary::ProcessCreated);
    preparation_check(timing)?;
    let analyzer = AnalyzerHandler::new(
        config.selection.root.clone(),
        config.operators.clone(),
        config.profile,
        config.limits.max_candidates,
    )
    .with_managed_candidate_spool_owner(Arc::clone(&execution_spool))
    .with_exception_project(&config);
    boundary(ShellSetupBoundary::AnalyzerCreated);
    preparation_check(timing)?;
    boundary(ShellSetupBoundary::DeliveryReady);
    preparation_check(timing)?;
    let spool_dir = Arc::new(ManagedShellRoots {
        execution_root,
        delivery_root,
        execution_spool: std::sync::Mutex::new(Some(execution_spool)),
        delivery_spool: std::sync::Mutex::new(Some(delivery_spool)),
        startup_reclaim,
    });
    Ok(PreparedShellSetup {
        workspace,
        analyzer,
        process,
        spool_dir,
        config,
        rollback,
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
#[cfg(test)]
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
    create_managed_shell_roots_controlled(temporary_parent, boundary, None).map_err(|error| {
        match error {
            PreparationError::Failed(error) => error,
            PreparationError::Stopped(cause) => format!("{} during preparation", cause.label()),
        }
    })
}

#[allow(
    clippy::type_complexity,
    reason = "rollback stays armed alongside every published owner"
)]
fn create_managed_shell_roots_controlled(
    temporary_parent: &Utf8Path,
    boundary: &impl Fn(ManagedRootSetupBoundary),
    timing: Option<&PreparationToken>,
) -> Result<
    (
        SetupRollback,
        Arc<ManagedRunRoot>,
        Arc<ManagedRunRoot>,
        Arc<ManagedChild>,
        Arc<ManagedChild>,
        ReclaimReport,
    ),
    PreparationError,
> {
    preparation_check(timing)?;
    let coordinator =
        ManagedRootCoordinator::open(temporary_parent).map_err(|error| error.to_string())?;
    preparation_check(timing)?;
    let reclaim = match timing {
        Some(timing) => ManagedRunRoot::reclaim_abandoned_controlled(
            &coordinator,
            std::time::SystemTime::now(),
            timing.deadline,
            &|| timing.check().is_err(),
        ),
        None => ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now()),
    };
    preparation_check(timing)?;
    let execution_root = Arc::new(
        ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution)
            .map_err(|error| error.to_string())?,
    );
    let mut rollback = SetupRollback::new(Arc::clone(&execution_root));
    rollback.timing = timing.cloned();
    boundary(ManagedRootSetupBoundary::ExecutionPublished);
    preparation_check(timing)?;
    let delivery_root = Arc::new(
        ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery)
            .map_err(|error| error.to_string())?,
    );
    rollback.attach_delivery(Arc::clone(&delivery_root));
    boundary(ManagedRootSetupBoundary::DeliveryPublished);
    preparation_check(timing)?;
    let execution_spool = Arc::new(
        execution_root
            .create_child("spool-")
            .map_err(|error| error.to_string())?,
    );
    rollback.attach_execution_spool(Arc::clone(&execution_spool));
    boundary(ManagedRootSetupBoundary::ExecutionSpoolCreated);
    preparation_check(timing)?;
    let delivery_spool = Arc::new(
        delivery_root
            .create_child("report-")
            .map_err(|error| error.to_string())?,
    );
    rollback.attach_delivery_spool(Arc::clone(&delivery_spool));
    boundary(ManagedRootSetupBoundary::DeliverySpoolCreated);
    preparation_check(timing)?;
    Ok((
        rollback,
        execution_root,
        delivery_root,
        execution_spool,
        delivery_spool,
        reclaim,
    ))
}

fn begin_disk_monitor(
    context: &PreparedShellSetup,
    control: &RunControl,
) -> Result<DiskMonitor, String> {
    #[cfg(not(test))]
    let _ = control;
    let policy = DiskPolicy {
        max_owned_bytes: context.config.limits.max_workspace_size,
        min_free_bytes: context.config.limits.min_free_space,
    };
    let roots = context.spool_dir.monitor_roots();
    #[cfg(test)]
    if let Some(meter) = control.disk_meter() {
        return DiskMonitor::start_unmeasured_with_meter(
            meter,
            policy,
            roots,
            control.disk_sample_interval,
        )
        .map_err(|error| format!("disk.measurement.failed: {error}"));
    }
    DiskMonitor::start_unmeasured(policy, roots)
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

type CleanupThreadJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug, PartialEq, Eq)]
enum CleanupThreadFailure {
    Spawn(String),
    TimedOut,
    Stopped,
}

impl CleanupThreadFailure {
    fn message(&self) -> String {
        match self {
            Self::Spawn(error) => format!("cleanup thread could not start: {error}"),
            Self::TimedOut => "cleanup operation timed out".to_owned(),
            Self::Stopped => "cleanup thread stopped without a result".to_owned(),
        }
    }
}

impl std::fmt::Display for CleanupThreadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = if matches!(self, Self::TimedOut) {
            hoimin_core::WORKSPACE_CLEANUP_DEFERRED
        } else {
            hoimin_core::WORKSPACE_CLEANUP_FAILED
        };
        write!(formatter, "{code}: {}", self.message())
    }
}

fn spawn_cleanup_thread(job: CleanupThreadJob) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("hoimin-cleanup".to_owned())
        .spawn(job)
        .map(|_| ())
}

async fn run_cleanup_thread<T: Send + 'static>(
    budget: &ShutdownBudget,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CleanupThreadFailure> {
    run_cleanup_thread_with_spawner(budget, operation, spawn_cleanup_thread).await
}

async fn run_cleanup_thread_with_spawner<T: Send + 'static>(
    budget: &ShutdownBudget,
    operation: impl FnOnce() -> T + Send + 'static,
    spawner: impl FnOnce(CleanupThreadJob) -> std::io::Result<()>,
) -> Result<T, CleanupThreadFailure> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    spawner(Box::new(move || {
        let _ = sender.send(operation());
    }))
    .map_err(|error| CleanupThreadFailure::Spawn(error.to_string()))?;
    budget
        .wait(receiver)
        .await
        .map_err(|_| CleanupThreadFailure::TimedOut)?
        .map_err(|_| CleanupThreadFailure::Stopped)
}

/// Local attempt progress, distinct from `DiskLifecycle`'s safety/admission state.
/// Retain a request even if admission fails, so outer finalization does not retry it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanupProgress {
    NotAttempted,
    Attempted,
    Cleaned,
}

/// Classification belongs to the workspace record; error is presentation only.
#[derive(Debug)]
struct ManagedCleanupOutcome {
    record: CleanupRecord,
    error: String,
}

impl ManagedCleanupOutcome {
    fn disk_outcome(&self) -> DiskCleanupOutcome {
        match self.record.disposition {
            CleanupDisposition::Clean => DiskCleanupOutcome::Clean,
            CleanupDisposition::Failed => DiskCleanupOutcome::Failed(self.error.clone()),
            CleanupDisposition::Deferred(_) => DiskCleanupOutcome::Deferred(self.error.clone()),
        }
    }
}

fn recordless_cleanup_outcome(
    failure: &CleanupThreadFailure,
    stable_remaining_root: Option<Utf8PathBuf>,
) -> ManagedCleanupOutcome {
    // A failed wait cannot certify the root identity after cleanup may have renamed it.
    let deferred =
        matches!(failure, CleanupThreadFailure::TimedOut) || stable_remaining_root.is_none();
    let detail = failure.to_string();
    let error = if deferred {
        format!(
            "{}: {}",
            hoimin_core::WORKSPACE_CLEANUP_DEFERRED,
            failure.message()
        )
    } else {
        detail.clone()
    };
    let mut record = CleanupRecord {
        disposition: if deferred {
            CleanupDisposition::Deferred(None)
        } else {
            CleanupDisposition::Failed
        },
        evidence: CleanupEvidence {
            examined_entries: 0,
            removed_entries: 0,
            details: Vec::new(),
            omitted_detail_count: 0,
            remaining_root: stable_remaining_root,
        },
    };
    record.evidence.push_detail(detail);
    ManagedCleanupOutcome { record, error }
}

fn cleanup_thread_failure_outcome(
    root: &ManagedRunRoot,
    failure: &CleanupThreadFailure,
) -> ManagedCleanupOutcome {
    let stable_remaining_root =
        matches!(failure, CleanupThreadFailure::Spawn(_)).then(|| root.path().to_owned());
    recordless_cleanup_outcome(failure, stable_remaining_root)
}

fn cleanup_with_panic_recovery(
    root: &ManagedRunRoot,
    operation: impl FnOnce() -> CleanupRecord,
) -> CleanupRecord {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or_else(|_| {
        root.interrupted_cleanup_record(
            true,
            "workspace.cleanup.failed: cleanup worker panicked".to_owned(),
        )
    })
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
    outcome.record.evidence.push_detail(detail);
    if outcome.record.disposition != CleanupDisposition::Clean {
        outcome.error.push_str("; ");
        outcome.error.push_str(&bounded_detail);
    }
    outcome
}

fn delivery_cleanup_secondary_errors(record: &CleanupRecord) -> Vec<String> {
    record
        .evidence
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
                    recordless_cleanup_outcome(&error, Some(root.path().to_owned())),
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
                ManagedCleanupOutcome {
                    record,
                    error: "workspace.cleanup.deferred: managed-root cleanup budget expired"
                        .to_owned(),
                },
                marker_error.as_deref(),
            );
        }
        let cleanup_root = Arc::clone(&root);
        let cleanup = match run_cleanup_thread(budget, move || {
            cleanup_with_panic_recovery(&cleanup_root, || cleanup_root.cleanup(remaining))
        })
        .await
        {
            Ok(cleanup) => cleanup,
            Err(error) => {
                return attach_cleanup_ready_error(
                    cleanup_thread_failure_outcome(&root, &error),
                    marker_error.as_deref(),
                );
            }
        };
        if cleanup.retryable_contention() && tokio::time::Instant::now() < budget.deadline() {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }
        let detail = format!(
            "{}; omitted={}; remaining={:?}",
            cleanup.evidence.details.join("; "),
            cleanup.evidence.omitted_detail_count,
            cleanup.evidence.remaining_root,
        );
        let error = match cleanup.disposition {
            CleanupDisposition::Clean => String::new(),
            CleanupDisposition::Failed => format!("workspace.cleanup.failed: {detail}"),
            CleanupDisposition::Deferred(_) => format!("workspace.cleanup.deferred: {detail}"),
        };
        let outcome = ManagedCleanupOutcome {
            record: cleanup,
            error,
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
            let secondary = hoimin_core::DiskSecondary::Error { code, message };
            if disk_secondary_matches_primary(stop, &secondary)
                || stop.secondary.contains(&secondary)
            {
                continue;
            }
            stop.secondary.push(secondary);
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

fn disk_secondary_matches_primary(
    report: &hoimin_core::DiskStopReport,
    secondary: &hoimin_core::DiskSecondary,
) -> bool {
    match secondary {
        hoimin_core::DiskSecondary::Observation { reason, value } => {
            report.code == reason.code()
                && report.owned_bytes == Some(value.owned_bytes)
                && report.available_bytes == Some(value.available_bytes)
        }
        hoimin_core::DiskSecondary::Error { code, message } => {
            report.code == *code && report.message.as_deref() == Some(message)
        }
    }
}

fn apply_execution_cleanup_evidence(
    report: &mut hoimin_core::DiskCleanupReport,
    cleanup: Option<&CleanupRecord>,
) {
    let evidence = match cleanup {
        Some(cleanup) => {
            report.status = cleanup.disposition.report_status();
            cleanup.evidence.clone()
        }
        None => CleanupEvidence {
            examined_entries: report.examined_entries,
            removed_entries: report.removed_entries,
            details: report.details.clone(),
            omitted_detail_count: report.omitted_detail_count,
            remaining_root: report.remaining_root.as_ref().map(Utf8PathBuf::from),
        },
    };
    report.examined_entries = evidence.examined_entries;
    report.removed_entries = evidence.removed_entries;
    report.details = evidence.details;
    report.omitted_detail_count = evidence.omitted_detail_count;
    report.remaining_root = evidence.remaining_root.as_ref().map(ToString::to_string);
}

fn merge_monitor_stop(report: &mut hoimin_core::DiskStopReport, latest: hoimin_core::DiskFailure) {
    let mut incoming = vec![latest.reason.to_secondary()];
    incoming.extend(latest.secondary);
    for secondary in incoming {
        if disk_secondary_matches_primary(report, &secondary)
            || report.secondary.contains(&secondary)
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
    summary.disk.stop = Some(failure.into());
}

pub struct ShellContext {
    git: crate::target::git::GitRunner,
    workspace: Option<WorkspaceHandler>,
    analyzer: AnalyzerHandler,
    candidate_replay: Option<CandidateReplay>,
    process: Arc<ProcessHandler>,
    report: ReportDelivery,
    session: Option<SessionDispatcher>,
    session_path: Option<Utf8PathBuf>,
    spool_dir: Arc<ManagedShellRoots>,
    resolved_targets: Option<Vec<TargetSlice>>,
    config: RunConfig,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    report_versions: ReportVersions,
    blocking_secondary_errors: Vec<String>,
    metrics_destination: MetricsDestination,
    metrics_protected_inputs: Vec<std::path::PathBuf>,
    // Keep this last so every handler/root clone is dropped before an armed rollback.
    setup_rollback: Option<SetupRollback>,
}

impl ShellContext {
    /// Creates all handlers required for a run.
    ///
    /// # Errors
    ///
    /// Returns an error when local run infrastructure cannot be initialized.
    pub async fn new<Stdout: Write + Send + 'static, Stderr: Write + Send + 'static>(
        config: &RunConfig,
        stdout: Stdout,
        stderr: Stderr,
    ) -> Result<Self, String> {
        let prepared = prepare_shell_setup(config.clone(), Box::new(|| {})).await?;
        Ok(Self::from_prepared(
            prepared,
            stdout,
            stderr,
            crate::report::ReportPresentation::default(),
        ))
    }

    fn from_prepared<Stdout: Write + Send + 'static, Stderr: Write + Send + 'static>(
        prepared: PreparedShellSetup,
        stdout: Stdout,
        stderr: Stderr,
        presentation: crate::report::ReportPresentation,
    ) -> Self {
        let PreparedShellSetup {
            workspace,
            analyzer,
            process,
            spool_dir,
            config,
            rollback,
        } = prepared;
        let session_path = config.session.config().map(|value| value.path.clone());
        let delivery_child = spool_dir
            .delivery_spool
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let delivery_owner = Arc::new((Arc::clone(&spool_dir.delivery_root), delivery_child));

        Self {
            git: crate::target::git::GitRunner::default(),
            workspace: Some(workspace),
            analyzer,
            candidate_replay: Some(CandidateReplay::new([])),
            process,
            report: ReportDelivery::new(
                ReportHandler::new(stdout, stderr).with_presentation(presentation),
                delivery_owner,
            ),
            session: None,
            session_path,
            spool_dir,
            resolved_targets: None,
            config,
            fingerprint_copy_inputs: BTreeSet::new(),
            report_versions: ReportVersions {
                os: std::env::consts::OS.to_owned(),
                hoimin: env!("CARGO_PKG_VERSION").to_owned(),
            },
            blocking_secondary_errors: Vec::new(),
            metrics_destination: MetricsDestination::Disabled,
            metrics_protected_inputs: Vec::new(),
            setup_rollback: Some(rollback),
        }
    }

    fn commit_setup(&mut self) {
        if let Some(mut rollback) = self.setup_rollback.take() {
            rollback.disarm();
        }
    }

    #[cfg(test)]
    async fn new_in<Stdout: Write + Send + 'static, Stderr: Write + Send + 'static>(
        config: &RunConfig,
        stdout: Stdout,
        stderr: Stderr,
        temporary_parent: Utf8PathBuf,
    ) -> Result<Self, String> {
        let prepared = prepare_shell_setup_in(config.clone(), temporary_parent).await?;
        Ok(Self::from_prepared(
            prepared,
            stdout,
            stderr,
            crate::report::ReportPresentation::default(),
        ))
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

fn prepare_manifest_fingerprint(
    config: &RunConfig,
    targets: &[TargetSlice],
    resource_mode: ResourceMode,
    manifest: &WorkspaceManifest,
    id: EffectId,
) -> Result<hoimin_core::RunFingerprint, EffectFailed> {
    let mut sources = Vec::with_capacity(targets.len());
    for target in targets {
        let entry = manifest.entry(&target.path).ok_or_else(|| {
            EffectFailed::other(
                id,
                "fingerprint.source.manifest_missing",
                format!(
                    "fingerprint source is not in the workspace manifest: {}",
                    target.path
                ),
            )
        })?;
        sources.push(SourceHash {
            path: target.path.clone(),
            hash: *entry.blake3.as_bytes(),
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
    crate::fingerprint_inputs::recheck_config(config, root).map_err(|error| {
        EffectFailed::other(id, "plan.fingerprint_input.changed", error.to_string())
    })?;
    let files = if config
        .operators
        .contains(hoimin_core::MutationOperator::ExceptionHierarchy)
    {
        config
            .fingerprint_inputs
            .iter()
            .map(|r| r.path.to_string())
            .collect::<Vec<_>>()
    } else {
        config.fingerprint_files.clone()
    };
    crate::fingerprint_inputs::recheck_manifest(
        root,
        &config.fingerprint_includes,
        &files,
        &config.fingerprint_inputs,
        manifest,
        copied_at_start,
    )
    .map_err(|error| EffectFailed::other(id, "plan.fingerprint_input.changed", error.to_string()))
}

#[cfg(windows)]
fn resource_backend(
    _config: &RunConfig,
) -> Result<ResourceBackend, crate::resource::ResourceError> {
    crate::resource::WindowsBackend::new().map(ResourceBackend::Windows)
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

pub async fn execute_effect(context: &mut ShellContext, effect: RunEffect) -> RunEvent {
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

async fn execute_effect_with_cancellation(
    context: &mut ShellContext,
    effect: RunEffect,
    cancellation: ProcessCancellation,
) -> RunEvent {
    debug_assert!(!is_blocking_io_effect(&effect));
    let id = effect.id();
    let result: Result<RunEvent, EffectFailed> = match effect {
        RunEffect::ResolveTargets(request) => match TargetHandler::handle_with_cancellation(
            request,
            cancellation.clone(),
            &context.git,
        )
        .await
        {
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
                    .run(request.request)
                    .await
                    .map(RunEvent::BaselineFinished),
                Err(error) => Err(error),
            }
        }
        RunEffect::AnalyzeFile(request) => context
            .analyzer
            .handle_with_cancellation(request, cancellation.clone())
            .await
            .map(RunEvent::AnalysisFinished),
        RunEffect::RunMutant(request) => {
            match worker_process_request(context, id, request, cancellation.clone(), None) {
                Ok(request) => context
                    .process
                    .run(request.request)
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
            if let OutputEvent::BaselineFinished(baseline) = &request.event
                && baseline.termination != hoimin_core::ProcessTermination::Exit(0)
            {
                crate::baseline_output::emit(&context.process, &mut context.report, baseline).await;
            }
            if let OutputEvent::RunStarted(run_started) = &mut request.event {
                run_started.versions = context.report_versions.clone();
            }
            context
                .report
                .handle(request)
                .await
                .map(RunEvent::OutputEmitted)
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

/// Request identity retained after the runner consumes the command. Failed and
/// retired events cannot supply this information for worker metrics or draining.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerProcessContext {
    Baseline(u32),
    Mutant(u32),
}

/// Keeps completion identity derived from the typed request together with its
/// runner representation. Only workspace contexts can construct this request.
struct PreparedWorkerProcess {
    request: ProcessRequest,
    context: WorkerProcessContext,
}

impl<C: WorkspaceProcessContext> From<RunProcess<C>> for PreparedWorkerProcess
where
    ProcessRequest: From<RunProcess<C>>,
{
    fn from(request: RunProcess<C>) -> Self {
        let context = request.context.completion_context();
        Self {
            request: request.into(),
            context,
        }
    }
}

/// Only workspace contexts can prepare worker commands; probes cannot enter here.
trait WorkspaceProcessContext {
    fn completion_context(&self) -> WorkerProcessContext;
    fn worker(&self) -> u32;
    fn run_id(&self) -> &str;
    fn mutant_id(&self) -> Option<&str>;
}

impl WorkspaceProcessContext for hoimin_core::BaselineContext {
    fn completion_context(&self) -> WorkerProcessContext {
        WorkerProcessContext::Baseline(self.worker)
    }
    fn worker(&self) -> u32 {
        self.worker
    }
    fn run_id(&self) -> &str {
        &self.run_id
    }
    fn mutant_id(&self) -> Option<&str> {
        None
    }
}

impl WorkspaceProcessContext for hoimin_core::MutantContext {
    fn completion_context(&self) -> WorkerProcessContext {
        WorkerProcessContext::Mutant(self.worker)
    }
    fn worker(&self) -> u32 {
        self.worker
    }
    fn run_id(&self) -> &str {
        &self.run_id
    }
    fn mutant_id(&self) -> Option<&str> {
        Some(&self.mutant_id)
    }
}

fn worker_process_request<C: WorkspaceProcessContext>(
    context: &ShellContext,
    id: EffectId,
    mut request: RunProcess<C>,
    cancellation: ProcessCancellation,
    start_gate: Option<ProcessStartGate>,
) -> Result<PreparedWorkerProcess, EffectFailed>
where
    ProcessRequest: From<RunProcess<C>>,
{
    let worker = request.context.worker();
    let inherited = std::env::vars_os().collect();
    let mut environment = context
        .workspace
        .as_ref()
        .expect("workspace ownership is available during process dispatch")
        .command_environment(worker, &inherited)
        .map_err(|error| EffectFailed::other(id, "shell.worker.environment", error.to_string()))?;
    set_worker_metadata(&mut environment, &request.context);
    request.cwd.clone_from(&environment.cwd);
    let mut prepared = PreparedWorkerProcess::from(request);
    prepared.request = prepared
        .request
        .with_environment(environment)
        .with_cancellation(cancellation);
    if let Some(start_gate) = start_gate {
        prepared.request = prepared.request.with_start_gate(start_gate);
    }
    Ok(prepared)
}

fn set_worker_metadata<C: WorkspaceProcessContext>(
    environment: &mut crate::workspace::CommandEnvironment,
    context: &C,
) {
    for name in ["HOIMIN_WORKER_ROOT", "HOIMIN_RUN_ID", "HOIMIN_MUTANT_ID"] {
        remove_environment_key(&mut environment.env, name);
    }
    environment.env.insert(
        std::ffi::OsString::from("HOIMIN_WORKER_ROOT"),
        std::ffi::OsString::from(environment.cwd.as_str()),
    );
    environment.env.insert(
        std::ffi::OsString::from("HOIMIN_RUN_ID"),
        std::ffi::OsString::from(context.run_id()),
    );
    if let Some(mutant_id) = context.mutant_id() {
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

async fn session(
    context: &mut ShellContext,
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

#[derive(Debug, thiserror::Error)]
pub enum PrepareRunConfigError {
    #[error(transparent)]
    Config(#[from] hoimin_core::ConfigError),
    #[error(transparent)]
    File(#[from] crate::fingerprint_inputs::FingerprintInputError),
}

/// Resolves fingerprint inputs for semantically checked request settings.
/// Invalid normalized settings cannot enter this boundary; checked construction
/// and deserialization run before environment capture or filesystem resolution.
///
/// # Errors
///
/// Returns an error when a configured fingerprint input pattern cannot be resolved.
pub fn prepare_run_config(
    config: RunConfig<hoimin_core::EnvironmentRequest>,
) -> Result<RunConfig, PrepareRunConfigError> {
    let environment = crate::fingerprint_env::capture(&config.environment)?;
    let config = config.with_environment(environment)?;
    let inputs = crate::fingerprint_inputs::resolve_config(&config)?;
    Ok(config.with_fingerprint_inputs(inputs))
}

/// Captures the request once, then runs its validated snapshot with owned output writers.
///
/// # Errors
/// Owns preparation and infrastructure diagnostics; callers may inspect errors
/// but must not print them again. Deadline expiry can omit a final diagnostic.
pub async fn run_loop<Stdout, Stderr>(
    config: RunConfig<hoimin_core::EnvironmentRequest>,
    stdout: Stdout,
    stderr: Stderr,
    presentation: crate::report::ReportPresentation,
) -> Result<i32, String>
where
    Stdout: Write + Send + 'static,
    Stderr: Write + Send + 'static,
{
    run_loop_with_control(config, stdout, stderr, RunControl::new(), presentation).await
}

/// Runs with caller-controlled cancellation and owned output writers.
///
/// # Errors
/// Owns error reporting; callers must not print returned errors again.
/// Deadline expiry can omit a final diagnostic.
#[doc(hidden)]
pub async fn run_loop_with_control<Stdout, Stderr>(
    config: RunConfig<hoimin_core::EnvironmentRequest>,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
    presentation: crate::report::ReportPresentation,
) -> Result<i32, String>
where
    Stdout: Write + Send + 'static,
    Stderr: Write + Send + 'static,
{
    let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
    let mut interrupts = crate::interrupt::InterruptMonitor::spawn();
    crate::live_progress::stage("preparing run");
    let config_token = timing.clone();
    let config = match timing
        .run(&mut interrupts, move || {
            config_token.check()?;
            let environment = crate::fingerprint_env::capture(&config.environment)
                .map_err(|error| PreparationError::Failed(error.to_string()))?;
            config_token.check()?;
            let config = config
                .with_environment(environment)
                .map_err(|error| PreparationError::Failed(error.to_string()))?;
            let inputs = crate::fingerprint_inputs::resolve_config_controlled(&config, &|| {
                config_token.check().is_err()
            })
            .map_err(|error| {
                config_token
                    .check()
                    .err()
                    .unwrap_or_else(|| PreparationError::Failed(error.to_string()))
            })?;
            config_token.check()?;
            Ok(config.with_fingerprint_inputs(inputs))
        })
        .await
    {
        Ok(config) => config,
        Err(error) => return timing.report(error, stderr).await,
    };
    Box::pin(run_loop_prepared_timed(
        config,
        stdout,
        stderr,
        control,
        hoimin_core::RunSelection::all(),
        None,
        presentation,
        timing,
        interrupts,
    ))
    .await
}

pub(crate) async fn run_verified<Stdout, Stderr>(
    verified: crate::plan::VerifiedPlan,
    stdout: Stdout,
    stderr: Stderr,
    presentation: crate::report::ReportPresentation,
) -> Result<i32, String>
where
    Stdout: Write + Send + 'static,
    Stderr: Write + Send + 'static,
{
    let verified = verified.into_execution();
    if verified.config.session.config().is_some() {
        let error = "selected candidate execution does not support sessions or resume".to_owned();
        crate::write_terminal_error(error.clone(), stderr).await;
        return Err(error);
    }
    let control = RunControl::new();
    let timing = PreparationToken::new(verified.config.limits.total_timeout.get(), control.clone());
    let mut interrupts = crate::interrupt::InterruptMonitor::spawn();
    let context = match Box::pin(prepare_loop_context(
        &verified.config,
        &control,
        &timing,
        &mut interrupts,
    ))
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => return timing.report(error, stderr).await,
    };
    let (prepared, monitor) = context;
    let mut context = ShellContext::from_prepared(prepared, stdout, stderr, presentation);
    context.metrics_protected_inputs = verified.manifest_inputs;
    Box::pin(run_loop_context(
        verified.config,
        context,
        control,
        verified.selection,
        Some(verified.fingerprint_copy_inputs),
        timing,
        interrupts,
        monitor,
    ))
    .await
}

#[cfg(test)]
async fn run_loop_prepared<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
    candidate_selection: hoimin_core::RunSelection,
    fingerprint_copy_inputs: Option<BTreeSet<Utf8PathBuf>>,
    presentation: crate::report::ReportPresentation,
) -> Result<i32, String>
where
    Stdout: Write + Send + 'static,
    Stderr: Write + Send + 'static,
{
    let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
    let interrupts = crate::interrupt::InterruptMonitor::spawn();
    Box::pin(run_loop_prepared_timed(
        config,
        stdout,
        stderr,
        control,
        candidate_selection,
        fingerprint_copy_inputs,
        presentation,
        timing,
        interrupts,
    ))
    .await
}

#[allow(
    clippy::too_many_arguments,
    reason = "owned execution inputs include the original deadline and interrupt monitor"
)]
async fn run_loop_prepared_timed<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
    candidate_selection: hoimin_core::RunSelection,
    fingerprint_copy_inputs: Option<BTreeSet<Utf8PathBuf>>,
    presentation: crate::report::ReportPresentation,
    timing: PreparationToken,
    mut interrupts: crate::interrupt::InterruptMonitor,
) -> Result<i32, String>
where
    Stdout: Write + Send + 'static,
    Stderr: Write + Send + 'static,
{
    let context = match Box::pin(prepare_loop_context(
        &config,
        &control,
        &timing,
        &mut interrupts,
    ))
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => return timing.report(error, stderr).await,
    };
    let (prepared, monitor) = context;
    let context = ShellContext::from_prepared(prepared, stdout, stderr, presentation);
    Box::pin(run_loop_context(
        config,
        context,
        control,
        candidate_selection,
        fingerprint_copy_inputs,
        timing,
        interrupts,
        monitor,
    ))
    .await
}

fn reconcile_target_completion(
    completed: RunEvent,
    control: &RunControl,
    deadline: tokio::time::Instant,
) -> RunEvent {
    if matches!(&completed, RunEvent::EffectFailed(failure)
        if matches!(&failure.failure, hoimin_core::EffectFailure::Other { code, .. } if code == "target.cancelled"))
    {
        if control.is_cancelled() {
            return RunEvent::CancellationRequested;
        }
        if tokio::time::Instant::now() >= deadline {
            return RunEvent::DeadlineReached;
        }
    }
    completed
}

async fn prepare_loop_context(
    config: &RunConfig,
    control: &RunControl,
    timing: &PreparationToken,
    interrupts: &mut crate::interrupt::InterruptMonitor,
) -> Result<(PreparedShellSetup, DiskMonitor), PreparationError> {
    #[cfg(not(test))]
    let parent = Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .map_err(|_| PreparationError::Failed("temporary workspace parent is not UTF-8".into()))?;
    #[cfg(test)]
    let parent = Utf8PathBuf::from_path_buf(control.managed_parent.path().to_owned())
        .map_err(|_| PreparationError::Failed("test managed parent is not UTF-8".into()))?;
    #[cfg(not(test))]
    let _ = control;
    let token = timing.clone();
    let config = config.clone();
    let monitor_control = control.clone();
    let (monitor, prepared) = timing
        .run(interrupts, move || {
            let prepared = prepare_shell_setup_controlled(config, &parent, &|_| {}, Some(&token))?;
            token.check()?;
            let monitor = begin_disk_monitor(&prepared, &monitor_control)?;
            // Drop an undelivered idle monitor before its setup. No measurements
            // start until the first explicit sample; retained leases still prevent
            // workspace deletion until the monitor thread actually exits.
            Ok((monitor, prepared))
        })
        .await?;
    if let Err(error) = timing.admit(interrupts, monitor.sample_now()).await {
        let joined = monitor.stop_and_join(timing.rollback_remaining()).await;
        drop(monitor);
        let rollback = timing.dispose(prepared).await;
        if !joined {
            return Err(PreparationError::Failed("preparation disk monitor did not join before shutdown grace; live measurement retains its roots".into()));
        }
        rollback?;
        return Err(error);
    }
    Ok((prepared, monitor))
}

#[expect(
    clippy::too_many_lines,
    reason = "the loop keeps cancellation, completion, and state-transition ordering in one auditable sequence"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "execution receives the prepared monitor and original deadline owners"
)]
#[tracing::instrument(name = "mutation_run", level = "debug", skip_all, fields(run_id = tracing::field::Empty))]
async fn run_loop_context(
    config: RunConfig,
    mut context: ShellContext,
    control: RunControl,
    candidate_selection: hoimin_core::RunSelection,
    fingerprint_copy_inputs: Option<BTreeSet<Utf8PathBuf>>,
    timing: PreparationToken,
    mut interrupts: crate::interrupt::InterruptMonitor,
    prepared_monitor: DiskMonitor,
) -> Result<i32, String> {
    context.git = crate::target::git::GitRunner::for_run(control.start_gate(), timing.deadline);
    let metrics_requested = config.output.metrics.is_some();
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
    let deadline = timing.deadline;
    let shutdown_grace = control.shutdown_grace();
    let report_admission = context.report.admission();
    report_admission.restrict_to(
        ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace).deadline(),
    );
    let max_jobs = config.limits.jobs.get();
    let channel_capacity = config.limits.jobs.get().saturating_add(1);
    let mut metrics = None;
    let mut metrics_warnings = Vec::new();
    let mut discovered = 0_u64;
    let mut executed = 0_u64;
    let initial_run_id = Uuid::new_v4().to_string();
    let mut diagnostic_run_id = initial_run_id.clone();
    let _ = timing.check();
    let mut shutdown_budget = timing.shutdown();
    let mut shutdown_expiry_reported = false;
    let mut disk_monitor = None;
    let mut delivery_cleanup = CleanupProgress::NotAttempted;
    let mut finalization_errors = Vec::new();
    let mut finalization = ExecutionFinalization::new()
        .map_err(|error| format!("disk lifecycle initialization failed: {error}"))?;
    context.candidate_replay = Some(CandidateReplay::new(
        candidate_selection
            .ordered_ids()
            .into_iter()
            .flatten()
            .cloned(),
    ));
    let run_result = async {
        let mut state = Box::new(RunState::with_selection(
            initial_run_id.clone(),
            config,
            candidate_selection,
            context.process.resource_control(),
        ));
        let (next, initial) = transition(*state, RunEvent::StartRequested(StartRequested))
            .map_err(|error| error.to_string())?;
        *state = next;
        crate::live_progress::observe(&state);
        track_diagnostic_run_id(&mut diagnostic_run_id, &state);
        let monitor = prepared_monitor;
        context.commit_setup();
        let mut disk_stop = monitor.stop_receiver();
        disk_monitor = Some(monitor);
        let mut disk_stop_delivered = false;
        #[cfg(test)]
        let mut disk_stop_order_observed = false;
        if metrics_requested {
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
        let mut priority_event = None;

        while state.phase() != RunPhase::Finished {
            let mut serial_completion = None;
            if priority_event.is_none() && !disk_stop_delivered {
                priority_event = disk_stop.borrow_and_update().clone().map(|failure| {
                    disk_stop_delivered = true;
                    cancellation.cancel();
                    stop_signalled = true;
                    RunEvent::DiskStopRequested(DiskStopRequested { failure })
                });
            }
            if let Some(event) = &priority_event {
                control.establish_event_shutdown_budget(
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
                    control.establish_event_shutdown_budget(
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
                if matches!(effect, RunEffect::Cleanup(_)) && !finalization.safety_proven() {
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
                            control.establish_event_shutdown_budget(
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
                    let git_drain = context.git.drain_for_shutdown(remaining).await;
                    let remaining = cleanup_budget.deadline().saturating_duration_since(tokio::time::Instant::now());
                    let mut process_drain = context
                        .process
                        .drain_for_shutdown(remaining)
                        .await;
                    if let Err(error) = git_drain {
                        process_drain.all_reaped = false;
                        process_drain.secondary_errors.push(error);
                    }
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
                    let measurement = finalization.observe_measurement(
                        measurement.map(|result| result.map(|reading| reading.owned_bytes)),
                    );
                    match measurement {
                        Ok(()) => {},
                        Err(error) if !disk_stop_delivered => {
                            effects.push_front(effect);
                            cancellation.cancel();
                            stop_signalled = true;
                            disk_stop_delivered = true;
                            let event = RunEvent::DiskStopRequested(DiskStopRequested {
                                failure: hoimin_core::DiskFailure {
                                    reason: hoimin_core::DiskFailureReason::MeasurementFailed(error.to_string()),
                                    secondary: Vec::new(),
                                },
                            });
                            control.establish_event_shutdown_budget(
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
                    #[cfg(test)]
                    let monitor_joined = if control.take_inner_monitor_join_timeout_once() {
                        false
                    } else {
                        monitor.stop_and_join(remaining).await
                    };
                    #[cfg(not(test))]
                    let monitor_joined = monitor.stop_and_join(remaining).await;
                    if !monitor_joined {
                        finalization_errors.push(
                            "disk.measurement.failed: disk monitor join exceeded shutdown budget"
                                .to_owned(),
                        );
                    }
                    #[cfg(test)]
                    if monitor_joined {
                        control.observe_finalization_event("monitor_joined");
                    }
                    let safe = finalization.observe_inner_drains(DrainEvidence {
                        process_reaped: process_drain.all_reaped,
                        output_drained: process_drain.output_drains_joined,
                        monitor_joined,
                    })?;
                    if !safe {
                        let id = effect.id();
                        finalization.defer_without_attempt(
                            context.spool_dir.execution_root.path().to_owned(),
                        );
                        serial_completion = Some(ShellCompletion::Serial(RunEvent::EffectFailed(
                            EffectFailed::other(
                                id,
                                hoimin_core::WORKSPACE_CLEANUP_DEFERRED,
                                CLEANUP_QUIESCENCE_UNPROVEN,
                            ),
                        )));
                        effects.push_front(effect);
                        break;
                    }

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
                        control.establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        priority_event = Some(event);
                        break;
                    }
                    if !finalization.apply(DiskLifecycleEvent::DispatchRequested) {
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
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                let Some(dispatch) = control.begin_dispatch() else {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    control.establish_event_shutdown_budget(
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
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    dispatch,
                                );
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion::Serial(
                                    RunEvent::EffectFailed(error),
                                ));
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
                        let worker = request.context.worker;
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                let Some(dispatch) = accept_process_dispatch(
                                    &control,
                                    &mut metrics,
                                    &mut metrics_warnings,
                                    worker,
                                ) else {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    control.establish_event_shutdown_budget(
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
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    dispatch,
                                );
                            }
                            Err(error) => {
                                cancel_queued_worker(worker, &mut metrics, &mut metrics_warnings);
                                serial_completion = Some(ShellCompletion::Serial(
                                    RunEvent::EffectFailed(error),
                                ));
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
                                serial_completion = Some(ShellCompletion::Serial(
                                    RunEvent::EffectFailed(error),
                                ));
                            }
                        }
                    }
                    // Wall-clock observation is scheduler-owned so it reads the same absolute
                    // deadline that enforces the total timeout, without spawning worker work.
                    RunEffect::ObserveRemainingBudget(request) => {
                        serial_completion = Some(ShellCompletion::Serial(
                            RunEvent::RemainingBudgetObserved(remaining_budget_observed(
                                &request,
                                deadline,
                                tokio::time::Instant::now(),
                            )),
                        ));
                    }
                    RunEffect::EmitOutput(mut request)
                        if matches!(&request.event, OutputEvent::RunFinished(_)) =>
                    {
                        if let Err(error) = finalization.observe_end_available(|| {
                            control.end_available(&[Arc::clone(&context.spool_dir.execution_root)])
                        }) {
                            finalization_errors.push(format!(
                                "disk.measurement.failed: end free-space query failed: {error}"
                            ));
                        }
                        if let OutputEvent::RunFinished(summary) = &mut request.event {
                            let disk_stats = disk_monitor
                                .as_ref()
                                .expect("disk monitor exists through final reporting")
                                .stats();
                            summary.disk.peak_owned_bytes = disk_stats.peak_owned_bytes;
                            summary.disk.minimum_available_bytes =
                                disk_stats.minimum_available_bytes();
                            summary.disk.sample_count = disk_stats.sample_count;
                            summary.disk.maximum_measurement_ms =
                                u64::try_from(disk_stats.maximum_measurement.as_millis())
                                    .unwrap_or(u64::MAX);
                            summary.disk.janitor = Some(context.spool_dir.startup_reclaim.clone());
                            summary.disk.filesystems = disk_stats
                                .filesystems
                                .iter()
                                .map(|(key, filesystem)| {
                                    let end_available_bytes = finalization.end_available_bytes(*key);
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
                            finalization.project_summary(summary);
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
                        let (emitted, flushed) = {
                            let delivery = async {
                                let emitted = context.report.handle(request).await;
                                #[cfg(test)]
                                if emitted.is_ok() {
                                    control.observe_finalization_event("run_finished_written");
                                }
                                let flushed = context.report.flush().await;
                                (emitted, flushed)
                            };
                            tokio::pin!(delivery);
                            loop {
                                let limit = shutdown_budget.unwrap_or_else(|| {
                                    ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                                });
                                report_admission.restrict_to(limit.deadline());
                                tokio::select! {
                                    biased;
                                    () = tokio::time::sleep_until(limit.deadline()) => {
                                        return Err(limit.expiry_error(process_tasks.len(), io_tasks.len() + 1));
                                    }
                                    () = control.cancelled(), if !stop_signalled => {
                                        cancellation.cancel();
                                        stop_signalled = true;
                                        control.establish_event_shutdown_budget(
                                            &mut shutdown_budget, &RunEvent::CancellationRequested,
                                            deadline, tokio::time::Instant::now(), shutdown_grace,
                                        );
                                    }
                                    signal = interrupts.first(), if !stop_signalled => {
                                        cancellation.cancel();
                                        stop_signalled = true;
                                        let event = first_interrupt_event(signal)?;
                                        control.establish_event_shutdown_budget(
                                            &mut shutdown_budget, &event, deadline,
                                            tokio::time::Instant::now(), shutdown_grace,
                                        );
                                    }
                                    () = tokio::time::sleep_until(deadline), if !stop_signalled => {
                                        cancellation.cancel();
                                        stop_signalled = true;
                                        control.establish_event_shutdown_budget(
                                            &mut shutdown_budget, &RunEvent::DeadlineReached,
                                            deadline, tokio::time::Instant::now(), shutdown_grace,
                                        );
                                    }
                                    completed = &mut delivery => break completed,
                                }
                            }
                        };
                        let mut delivery_errors = Vec::new();
                        let report_flushed = match flushed {
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
                        if !finalization.apply(report_event) {
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
                        delivery_cleanup = CleanupProgress::Attempted;
                        if !finalization.apply(DiskLifecycleEvent::CleanupRequested {
                            root: DiskRootId::Delivery,
                        }) {
                            return Err(
                                "disk lifecycle rejected delivery cleanup request".to_owned(),
                            );
                        }
                        if finalization.safety_proven() {
                            let cleanup_result = cleanup_managed_root(
                                Arc::clone(&context.spool_dir.delivery_root),
                                &cleanup_budget,
                            )
                            .await;
                            let disk_cleanup_outcome = cleanup_result.disk_outcome();
                            let ManagedCleanupOutcome { record: cleanup, error } = cleanup_result;
                            match cleanup.disposition {
                            CleanupDisposition::Clean => {
                                delivery_errors
                                    .extend(delivery_cleanup_secondary_errors(&cleanup));
                                if !finalization.apply(DiskLifecycleEvent::CleanupCompleted {
                                    root: DiskRootId::Delivery,
                                    outcome: DiskCleanupOutcome::Clean,
                                }) {
                                    return Err(
                                        "disk lifecycle rejected clean delivery cleanup".to_owned(),
                                    );
                                }
                                delivery_cleanup = CleanupProgress::Cleaned;
                                #[cfg(test)]
                                {
                                    control.observe_finalization_event("delivery_root_absent");
                                    control
                                        .observe_finalization_event("delivery_workspace_absent");
                                }
                            }
                            CleanupDisposition::Failed
                            | CleanupDisposition::Deferred(_) => {
                                let outcome = disk_cleanup_outcome;
                                if !finalization.apply(DiskLifecycleEvent::CleanupCompleted {
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
                            if !finalization.apply(DiskLifecycleEvent::CleanupCompleted {
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
                        if !finalization.apply(DiskLifecycleEvent::FinishRequested) {
                            return Err("disk lifecycle rejected run completion".to_owned());
                        }
                        let event = RunEvent::OutputEmitted(
                            output.expect("successful finalization retains output acknowledgement"),
                        );
                        serial_completion = Some(ShellCompletion::Serial(event));
                    }
                    effect => {
                        let stopping = stop_signalled;
                        let event = if stopping {
                            let budget = shutdown_budget
                                .ok_or_else(|| "stopping run has no shutdown budget".to_owned())?;
                            report_admission.restrict_to(budget.deadline());
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
                                changed = disk_stop.changed(), if !disk_stop_delivered && !finalization.safety_proven() => {
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
                                    let budget = control.establish_event_shutdown_budget(
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
                                () = control.cancelled() => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    let budget = control.establish_event_shutdown_budget(
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
                                    let budget = control.establish_event_shutdown_budget(
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
                                            let budget = control.establish_event_shutdown_budget(
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
                                event = &mut execution => SerialSelection::Completed(event),
                            };
                            match selection {
                                SerialSelection::Completed(event) => reconcile_target_completion(event, &control, deadline),
                                SerialSelection::Stopped {
                                    event,
                                    budget,
                                    signal_failure: pending_signal_failure,
                                } => {
                                    report_admission.restrict_to(budget.deadline());
                                    let completed = match budget.wait(&mut execution).await {
                                        Ok(completed) if tokio::time::Instant::now() < budget.deadline() => completed,
                                        _ => {
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
                                    };
                                    signal_failure = pending_signal_failure;
                                    // Accept the completed effect before its observed stop.
                                    // Otherwise finalization can replay output already written.
                                    // Target cancellation is reported only after the owned
                                    // Git child has been terminated and reaped. Its synthetic
                                    // failure must not replace an orderly observed stop.
                                    if matches!(&completed, RunEvent::EffectFailed(failure)
                                        if matches!(&failure.failure, hoimin_core::EffectFailure::Other { code, .. } if code == "target.cancelled")) {
                                        event
                                    } else {
                                        priority_event = Some(event);
                                        completed
                                    }
                                }
                            }
                        };
                        if matches!(
                            event,
                            RunEvent::DeadlineReached
                                | RunEvent::CancellationRequested
                                | RunEvent::DiskStopRequested(_)
                        ) {
                            cancellation.cancel();
                            stop_signalled = true;
                            control.establish_event_shutdown_budget(&mut shutdown_budget, &event, deadline, tokio::time::Instant::now(), shutdown_grace);
                            priority_event = Some(event);
                        } else {
                            serial_completion = Some(ShellCompletion::Serial(event));
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
            } else if let Some(event) = priority_event.take() {
                ShellCompletion::Serial(event)
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
                    changed = disk_stop.changed(), if !disk_stop_delivered && !finalization.safety_proven() => {
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
                        control.establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion::Serial(event)
                    }
                    () = control.cancelled() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::CancellationRequested;
                        control.establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion::Serial(event)
                    }
                    () = tokio::time::sleep_until(deadline) => {
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::DeadlineReached;
                        control.establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion::Serial(event)
                    }
                    signal = interrupts.first() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        match first_interrupt_event(signal) {
                            Ok(event) => {
                                control.establish_event_shutdown_budget(
                                    &mut shutdown_budget,
                                    &event,
                                    deadline,
                                    tokio::time::Instant::now(),
                                    shutdown_grace,
                                );
                                ShellCompletion::Serial(event)
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
                                ShellCompletion::Serial(RunEvent::CancellationRequested)
                            }
                        }
                    }
                    event = completion_rx.recv() => {
                        event.ok_or_else(|| "completion channel closed".to_owned())?
                    }
                }
            };
            let mut completion = completion;
            let post_drain_failure = if stop_signalled
                || !matches!(completion, ShellCompletion::Process { .. })
            {
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
                if let ShellCompletion::Process {
                    event: RunEvent::EffectFailed(error),
                    ..
                } = &completion
                {
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
                if let ShellCompletion::Process {
                    context: WorkerProcessContext::Baseline(worker) | WorkerProcessContext::Mutant(worker),
                    ..
                } = completion {
                    record_metrics(&mut metrics, &mut metrics_warnings, |metrics| {
                        metrics.process_finished(worker, false)
                    });
                }
                in_flight = in_flight.saturating_sub(1);
                cancellation.cancel();
                stop_signalled = true;
                disk_stop_delivered = true;
                let event = RunEvent::DiskStopRequested(DiskStopRequested { failure });
                control.establish_event_shutdown_budget(
                    &mut shutdown_budget,
                    &event,
                    deadline,
                    tokio::time::Instant::now(),
                    shutdown_grace,
                );
                completion = ShellCompletion::Serial(event);
            }
            #[cfg(test)]
            if matches!(
                &completion,
                ShellCompletion::Serial(RunEvent::DiskStopRequested(_))
            )
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
            #[cfg(test)]
            let final_output_completed = match &completion {
                ShellCompletion::Serial(event) | ShellCompletion::Process { event, .. } => {
                    Some(event)
                }
                ShellCompletion::Blocking(Ok(blocking)) => Some(match blocking.as_ref() {
                    BlockingEffectCompletion::Workspace(completion) => completion.event(),
                    BlockingEffectCompletion::Candidate { event, .. }
                    | BlockingEffectCompletion::TestEvent(event)
                    | BlockingEffectCompletion::OwnedWorkspace { event, .. } => event.as_ref(),
                }),
                ShellCompletion::Blocking(Err(_)) => None,
            }
            .is_some_and(|event| {
                matches!(event, RunEvent::OutputEmitted(value) if final_output_ids.remove(&value.id))
            });
            let (mut event, source) = match completion {
                ShellCompletion::Serial(event) => (event, CompletionSource::Serial),
                ShellCompletion::Process { event, context } => {
                    (event, CompletionSource::Process(context))
                }
                ShellCompletion::Blocking(result) => (
                    match result {
                        Ok(blocking) => accept_blocking_completion(&mut context, *blocking),
                        Err(error) => RunEvent::EffectFailed(error),
                    },
                    CompletionSource::Blocking,
                ),
            };
            finalization_errors.append(&mut context.blocking_secondary_errors);
            if let RunEvent::CleanupFinished(value) = &event
                && finalization.safety_proven()
                && !finalization.cleaned()
            {
                let id = value.id;
                context.analyzer.release_candidate_spool();
                context.spool_dir.release_execution_spool();
                let cleanup_budget = shutdown_budget.unwrap_or_else(|| {
                    ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                });
                #[cfg(test)]
                control.observe_finalization_event("workspace_cleanup_requested_once");
                #[cfg(test)]
                if control.take_duplicate_execution_cleanup_request()
                    && !finalization.request_cleanup()
                {
                    return Err(
                        "disk lifecycle rejected injected execution cleanup request".to_owned(),
                    );
                }
                if !finalization.request_cleanup() {
                    return Err(
                        "disk lifecycle rejected execution cleanup request".to_owned(),
                    );
                }
                #[cfg(test)]
                let cleanup_result = if control.take_execution_cleanup_failed_without_record() {
                    recordless_cleanup_outcome(
                        &CleanupThreadFailure::Spawn("injected cleanup thread failure".to_owned()),
                        Some(context.spool_dir.execution_root.path().to_owned()),
                    )
                } else if control.take_execution_cleanup_deferred() {
                    recordless_cleanup_outcome(
                        &CleanupThreadFailure::TimedOut,
                        None,
                    )
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
                let ManagedCleanupOutcome { record: cleanup, error } = cleanup_result;
                let disposition = cleanup.disposition;
                let accepted = finalization.complete_cleanup(cleanup, &error);
                match disposition {
                    CleanupDisposition::Clean => {
                        if !accepted {
                            return Err(
                                "disk lifecycle rejected clean execution cleanup".to_owned(),
                            );
                        }
                        #[cfg(test)]
                        control.observe_finalization_event("workspace_absent");
                    }
                    CleanupDisposition::Failed => {
                        if !accepted {
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
                    CleanupDisposition::Deferred(_) => {
                        if !accepted {
                            return Err(
                                "disk lifecycle rejected deferred execution cleanup".to_owned(),
                            );
                        }
                        context.spool_dir.execution_root.abandon_for_janitor(
                            "execution-root cleanup did not complete".to_owned(),
                        );
                        finalization_errors.push(error.clone());
                        event = RunEvent::EffectFailed(EffectFailed::other(
                            id,
                            hoimin_core::WORKSPACE_CLEANUP_DEFERRED,
                            error,
                        ));
                    }
                }
            }
            let previous_phase = state.phase();
            let previous_completed = crate::live_progress::completed(&state);
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
            if !external_stop
                && matches!(
                    source,
                    CompletionSource::Process(..) | CompletionSource::Blocking
                )
            {
                in_flight = in_flight.saturating_sub(1);
            }
            if !external_stop && matches!(source, CompletionSource::Blocking) {
                io_in_flight = io_in_flight.saturating_sub(1);
            }
            if external_stop || failed {
                control.establish_event_shutdown_budget(
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
            if let CompletionSource::Process(WorkerProcessContext::Mutant(worker)) = source
            {
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
            if previous_phase != state.phase()
                || targets_resolved
                || previous_completed != crate::live_progress::completed(&state)
                || analyzed_records.is_some()
            {
                crate::live_progress::observe(&state);
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
            } else if matches!(source, CompletionSource::Process(..)) {
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
            } else if matches!(source, CompletionSource::Blocking) {
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
        let mut post_report_output = false;
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
        report_admission.restrict_to(budget.deadline());
        let _disk_stats = disk_monitor.as_ref().map(DiskMonitor::stats);
        let (process_reaped, output_drained, process_errors) = if finalization.safety_proven() {
            (true, true, Vec::new())
        } else {
            let git_drain = context
                .git
                .drain_for_shutdown(
                    budget
                        .deadline()
                        .saturating_duration_since(tokio::time::Instant::now()),
                )
                .await;
            let mut report = context
                .process
                .drain_for_shutdown(
                    budget
                        .deadline()
                        .saturating_duration_since(tokio::time::Instant::now()),
                )
                .await;
            if let Err(error) = git_drain {
                report.all_reaped = false;
                report.secondary_errors.push(error);
            }
            (
                report.all_reaped,
                report.output_drains_joined,
                report.secondary_errors,
            )
        };
        let monitor_joined = if finalization.safety_proven() {
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
        let execution_quiescent = finalization.observe_outer_drains(
            DrainEvidence {
                process_reaped,
                output_drained,
                monitor_joined,
            },
            &mut finalization_errors,
        );
        let mut report_released = delivery_cleanup != CleanupProgress::NotAttempted;
        if !report_released && tokio::time::Instant::now() < budget.deadline() {
            match budget.wait(context.report.flush()).await {
                Ok(result) => {
                    report_released = context.report.is_quiescent();
                    if let Err(error) = result {
                        finalization_errors.push(format!(
                            "report flush before delivery cleanup failed: {error}"
                        ));
                    }
                }
                Err(_) => finalization_errors.push(budget.expiry_error(0, 1)),
            }
        }
        let close = close_context_resources(
            &mut context,
            shutdown_budget
                .as_ref()
                .expect("outer finalization established a shutdown budget"),
            shutdown_expiry_reported,
            execution_quiescent,
            Box::new(|| {}),
        )
        .await;
        let execution_safe_to_remove = execution_quiescent
            && context.workspace.is_some()
            && close.workspace.is_ok()
            && close.expiry.is_none();
        let delivery_safe_to_remove =
            execution_safe_to_remove && report_released && context.report.is_quiescent();
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
        if execution_safe_to_remove {
            if !finalization.cleanup_attempted() {
                context.analyzer.release_candidate_spool();
                context.spool_dir.release_execution_spool();
                if finalization.request_cleanup() {
                    let cleanup_result =
                        cleanup_managed_root(Arc::clone(&context.spool_dir.execution_root), budget)
                            .await;
                    let ManagedCleanupOutcome {
                        record: cleanup,
                        error,
                    } = cleanup_result;
                    let disposition = cleanup.disposition;
                    let accepted = finalization.complete_cleanup(cleanup, &error);
                    match disposition {
                        CleanupDisposition::Clean => {
                            if !accepted {
                                run_result = append_finalization_error(
                                    run_result,
                                    "disk lifecycle rejected outer clean execution cleanup"
                                        .to_owned(),
                                );
                            }
                        }
                        CleanupDisposition::Failed | CleanupDisposition::Deferred(_) => {
                            if !accepted {
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
            if !finalization.cleaned() {
                context
                    .spool_dir
                    .execution_root
                    .abandon_for_janitor("execution-root cleanup was already attempted".to_owned());
            }
        } else {
            if !finalization.cleanup_attempted() {
                let completed =
                    finalization.defer_cleanup(context.spool_dir.execution_root.path().to_owned());
                if !completed {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected deferred outer execution cleanup".to_owned(),
                    );
                }
            }
            if !finalization.cleaned() {
                context
                    .spool_dir
                    .execution_root
                    .abandon_for_janitor(CLEANUP_QUIESCENCE_UNPROVEN.to_owned());
            }
        }
        if delivery_safe_to_remove {
            if delivery_cleanup == CleanupProgress::NotAttempted {
                if finalization.snapshot().report == DiskComponentState::Pending
                    && !finalization.apply(DiskLifecycleEvent::ReportFailed)
                {
                    run_result = append_finalization_error(
                        run_result,
                        "disk lifecycle rejected outer report failure".to_owned(),
                    );
                }
                context.spool_dir.release_delivery_spool();
                delivery_cleanup = CleanupProgress::Attempted;
                if finalization.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Delivery,
                }) {
                    let cleanup_result =
                        cleanup_managed_root(Arc::clone(&context.spool_dir.delivery_root), budget)
                            .await;
                    let disk_cleanup_outcome = cleanup_result.disk_outcome();
                    let ManagedCleanupOutcome {
                        record: cleanup,
                        error,
                    } = cleanup_result;
                    match cleanup.disposition {
                        CleanupDisposition::Clean => {
                            if !finalization.apply(DiskLifecycleEvent::CleanupCompleted {
                                root: DiskRootId::Delivery,
                                outcome: DiskCleanupOutcome::Clean,
                            }) {
                                run_result = append_finalization_error(
                                    run_result,
                                    "disk lifecycle rejected outer clean delivery cleanup"
                                        .to_owned(),
                                );
                            }
                            delivery_cleanup = CleanupProgress::Cleaned;
                            for error in delivery_cleanup_secondary_errors(&cleanup) {
                                run_result = append_finalization_error(run_result, error);
                            }
                            #[cfg(test)]
                            control.observe_finalization_event("delivery_root_absent");
                        }
                        CleanupDisposition::Failed | CleanupDisposition::Deferred(_) => {
                            let outcome = disk_cleanup_outcome;
                            if !finalization.apply(DiskLifecycleEvent::CleanupCompleted {
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
            if delivery_cleanup != CleanupProgress::Cleaned {
                context
                    .spool_dir
                    .delivery_root
                    .abandon_for_janitor("delivery-root cleanup was already attempted".to_owned());
            }
        } else {
            if finalization.snapshot().report == DiskComponentState::Pending {
                let _ = finalization.apply(DiskLifecycleEvent::ReportFailed);
            }
            if delivery_cleanup == CleanupProgress::NotAttempted {
                delivery_cleanup = CleanupProgress::Attempted;
                let requested = finalization.apply(DiskLifecycleEvent::CleanupRequested {
                    root: DiskRootId::Delivery,
                });
                let completed = requested
                    && finalization.apply(DiskLifecycleEvent::CleanupCompleted {
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
            if delivery_cleanup != CleanupProgress::Cleaned {
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
        if metrics_requested {
            let path = match std::mem::take(&mut context.metrics_destination) {
                MetricsDestination::Ready(path) => Some(path),
                MetricsDestination::Unavailable(message) => {
                    metrics_warnings.push(("metrics.write", message));
                    None
                }
                MetricsDestination::Disabled => None,
            };
            if let Some(path) = path {
                match (shutdown_budget.as_ref(), shutdown_expiry_reported) {
                    (Some(budget), false) => {
                        let finalized = finalize_metrics_with_shutdown(
                            path,
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
            }
            for (code, message) in metrics_warnings {
                if tokio::time::Instant::now() >= budget.deadline()
                    || !context.report.is_quiescent()
                {
                    break;
                }
                post_report_output = true;
                let _ = budget
                    .wait(emit_metrics_warning(
                        &mut context,
                        &diagnostic_run_id,
                        code,
                        message,
                    ))
                    .await;
            }
        }
        let result = combine_close_results(
            run_result.map(|(exit_code, _)| exit_code),
            close.workspace,
            close.process,
        );
        if let Err(error) = &result
            && context.report.is_quiescent()
            && tokio::time::Instant::now() < budget.deadline()
        {
            post_report_output = true;
            let _ = budget
                .wait(context.report.handle(EmitOutput {
                    id: EffectId(u64::MAX),
                    event: OutputEvent::Diagnostic(Diagnostic::new(
                        &diagnostic_run_id,
                        u64::MAX,
                        "error",
                        "run.finalization.failed",
                        error,
                    )),
                }))
                .await;
        }
        if post_report_output
            && context.report.is_quiescent()
            && tokio::time::Instant::now() < budget.deadline()
        {
            let _ = budget.wait(context.report.flush()).await;
        }
        result
    };
    finish_with_interrupt_monitor(interrupts, outer_finalization).await
}

fn track_diagnostic_run_id(diagnostic_run_id: &mut String, state: &RunState) {
    tracing::Span::current().record("run_id", state.run_id());
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
    worker: u32,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    record_metrics(collector, warnings, |metrics| metrics.cancel_queued(worker));
}

fn cancel_queued_effect(
    effect: &RunEffect,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    if let RunEffect::RunMutant(request) = effect {
        cancel_queued_worker(request.context.worker, collector, warnings);
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
        RunEffect::RunMutant(request) => Some(request.context.worker),
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
        RunPhase::Baseline | RunPhase::BaselineRestoration => Some("baseline"),
        RunPhase::Analyze => Some("analysis"),
        RunPhase::Mutants => Some("mutants"),
        RunPhase::Cleaning => Some("cleanup"),
        _ => None,
    }
}

async fn emit_metrics_warning(
    context: &mut ShellContext,
    run_id: &str,
    code: &str,
    message: String,
) {
    let _ = context
        .report
        .handle(EmitOutput {
            id: EffectId(u64::MAX),
            event: OutputEvent::Diagnostic(Diagnostic::new(
                run_id,
                u64::MAX,
                "warning",
                code,
                message,
            )),
        })
        .await;
}

fn first_interrupt_event(signal: Result<(), String>) -> Result<RunEvent, String> {
    signal.map(|()| RunEvent::CancellationRequested)
}

fn spawn_process(
    process: Arc<ProcessHandler>,
    prepared: PreparedWorkerProcess,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
    _dispatch: std::sync::MutexGuard<'_, ()>,
) {
    tasks.spawn(
        async move {
            let PreparedWorkerProcess { request, context } = prepared;
            let event = match process.run(request).await {
                Ok(value) => match context {
                    WorkerProcessContext::Baseline(_) => RunEvent::BaselineFinished(value),
                    WorkerProcessContext::Mutant(_) => RunEvent::MutantFinished(value),
                },
                Err(error) => RunEvent::EffectFailed(error),
            };
            let _ = sender
                .send(ShellCompletion::Process { event, context })
                .await;
        }
        .in_current_span(),
    );
}

fn spawn_blocking_effect(
    task: BlockingEffect,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
) {
    tasks.spawn(
        async move {
            let id = task.id();
            let result = run_blocking_io(id, move || task.execute())
                .await
                .map(Box::new);
            let _ = sender.send(ShellCompletion::Blocking(result)).await;
        }
        .in_current_span(),
    );
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
    match completion {
        ShellCompletion::Process {
            context: WorkerProcessContext::Mutant(worker),
            ..
        } => record_metrics(metrics, metrics_warnings, |metrics| {
            metrics.process_finished(worker, false)
        }),
        ShellCompletion::Blocking(Ok(mut blocking)) => {
            if let BlockingEffectCompletion::Candidate { replay, .. } = blocking.as_mut() {
                replay.retire_read();
            }
            accept_blocking(*blocking);
        }
        ShellCompletion::Serial(_)
        | ShellCompletion::Process {
            context: WorkerProcessContext::Baseline(_),
            ..
        }
        | ShellCompletion::Blocking(Err(_)) => {}
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

fn detach_context_resources(
    context: &mut ShellContext,
    close_workspace: bool,
    before_start: OwnedStartHook,
) {
    detach_resource_cleanup(
        close_workspace.then(|| context.workspace.take()).flatten(),
        Arc::clone(&context.process),
        before_start,
    );
}

async fn close_context_resources(
    context: &mut ShellContext,
    budget: &ShutdownBudget,
    shutdown_already_expired: bool,
    close_workspace: bool,
    before_start: OwnedStartHook,
) -> ResourceCloseResults {
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
    path: crate::metrics::MetricsOutput,
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
    use crate::test_writer::CollectingWriter;
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

    fn shell_setup_test_config(project: &TempDir) -> RunConfig<hoimin_core::EnvironmentRequest> {
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

    fn assert_host_independent_test_reserve<E>(config: &RunConfig<E>) {
        assert_eq!(
            config.limits.min_free_space.get(),
            1,
            "ordinary shell tests must not depend on host free space"
        );
    }

    fn parse_host_independent_shell_test_config(
        mut args: Vec<OsString>,
    ) -> RunConfig<hoimin_core::EnvironmentRequest> {
        assert!(
            !args.iter().any(|argument| argument == "--min-free-space"),
            "host-independent shell test config must not override an explicit reserve"
        );
        let command_separator = args
            .iter()
            .position(|argument| argument == "--")
            .expect("shell test config contains a command separator");
        args.splice(
            command_separator..command_separator,
            [OsString::from("--min-free-space"), OsString::from("1B")],
        );
        let config = crate::cli::parse_config_from(args).unwrap();
        assert_host_independent_test_reserve(&config);
        config
    }

    fn output_failure_test_config(project: &TempDir) -> RunConfig<hoimin_core::EnvironmentRequest> {
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
        parse_host_independent_shell_test_config(args)
    }

    // Inject invalid selection inside this module: public saved-plan execution
    // rejects these IDs during preparation, before runtime spool exhaustion.
    async fn run_missing_explicit_candidate(source: &str, missing_id: &str) -> (String, Vec<u8>) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        std::fs::write(project.path().join("target.py"), source).unwrap();
        let selection = hoimin_core::RunSelection::explicit(
            BTreeSet::from([missing_id.to_owned()]),
            hoimin_core::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
        )
        .unwrap();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let error = run_loop_prepared(
            config,
            stdout.clone(),
            stderr.clone(),
            RunControl::new(),
            selection,
            Some(BTreeSet::new()),
            crate::report::ReportPresentation::default(),
        )
        .await
        .unwrap_err();
        (error, stdout.snapshot())
    }

    #[tokio::test(start_paused = true)]
    async fn execution_handoff_does_not_restart_preparation_cancellation_grace() {
        // Arrange.
        let control = RunControl::new();
        let requested = tokio::time::Instant::now();
        let deadline = requested + Duration::from_secs(30);
        control.cancel();
        tokio::time::advance(Duration::from_secs(1)).await;
        let mut active = None;

        // Act.
        let budget = control
            .establish_event_shutdown_budget(
                &mut active,
                &RunEvent::CancellationRequested,
                deadline,
                tokio::time::Instant::now(),
                Duration::from_secs(2),
            )
            .unwrap();

        // Assert.
        assert_eq!(budget.deadline(), requested + Duration::from_secs(2));
    }

    #[tokio::test(start_paused = true)]
    async fn execution_late_cancellation_cannot_extend_expired_total_deadline() {
        // Arrange: both stop reasons become ready before the scheduler observes either.
        let control = RunControl::new();
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_secs(1);
        tokio::time::advance(Duration::from_secs(2)).await;
        control.cancel();
        let mut active = None;

        // Act.
        let budget = control
            .establish_event_shutdown_budget(
                &mut active,
                &RunEvent::CancellationRequested,
                deadline,
                tokio::time::Instant::now(),
                Duration::from_secs(2),
            )
            .unwrap();

        // Assert: cancellation precedence does not grant another second of grace.
        assert_eq!(budget.deadline(), started + Duration::from_secs(3));
    }

    #[test]
    fn completed_git_cancellation_racing_stop_maps_to_cancellation() {
        // Arrange.
        let control = RunControl::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let completion = RunEvent::EffectFailed(EffectFailed::other(
            EffectId(1),
            "target.cancelled",
            "Git reaped",
        ));
        control.cancel();

        // Act.
        let event = reconcile_target_completion(completion, &control, deadline);

        // Assert.
        assert!(matches!(event, RunEvent::CancellationRequested));
    }

    #[test]
    fn completed_git_cancellation_racing_deadline_maps_to_timeout() {
        // Arrange.
        let control = RunControl::new();
        let deadline = tokio::time::Instant::now();
        let completion = RunEvent::EffectFailed(EffectFailed::other(
            EffectId(1),
            "target.cancelled",
            "Git reaped",
        ));

        // Act.
        let event = reconcile_target_completion(completion, &control, deadline);

        // Assert.
        assert!(matches!(event, RunEvent::DeadlineReached));
    }

    #[test]
    fn completed_git_reap_failure_is_preserved_after_timeout() {
        // Arrange.
        let control = RunControl::new();
        let deadline = tokio::time::Instant::now();
        let completion = RunEvent::EffectFailed(EffectFailed::other(
            EffectId(1),
            "target.resolve",
            "Git reap failed",
        ));

        // Act.
        let event = reconcile_target_completion(completion, &control, deadline);

        // Assert.
        assert!(
            matches!(event, RunEvent::EffectFailed(failure) if matches!(&failure.failure,
            hoimin_core::EffectFailure::Other {message, ..} if message == "Git reap failed"))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_cancellation_bounds_initial_disk_measurement() {
        // Arrange: the initial monitor sample owns roots until its meter returns.
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let control = RunControl::with_disk_meter(BlockingAfterCallMeter {
            calls: Arc::new(AtomicUsize::new(0)),
            block_at: 1,
            entered: entered_tx,
            release: Arc::clone(&release),
        });
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let output = stdout.clone();
        let mut run = tokio::spawn(run_loop_with_control(
            config,
            stdout,
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        ));
        tokio::task::spawn_blocking(move || {
            entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        })
        .await
        .unwrap();

        // Act.
        observed.cancel();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        let stopped = run.is_finished();
        let (released, changed) = &*release;
        *released.lock().unwrap() = true;
        changed.notify_all();
        let _ = (&mut run).await;

        // Assert: releasing the OS operation is fixture cleanup, not stop evidence.
        assert!(
            stopped,
            "initial preparation sample ignored cancellation and shared grace"
        );
        assert_eq!(output.snapshot(), [] as [u8; 0]);
    }

    #[tokio::test]
    async fn preparation_cancellation_skips_invalid_fingerprint_inputs() {
        // Arrange: cancelled preparation must not inspect missing input files.
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let mut data = config.into_data();
        data.fingerprint_files = vec!["missing.toml".into()];
        let config = RunConfig::try_from(data).unwrap();
        let control = RunControl::new();
        control.cancel();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();

        // Act.
        let result = run_loop_with_control(
            config,
            stdout.clone(),
            stderr.clone(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await;

        // Assert.
        assert_eq!(result, Ok(130));
        assert_eq!(stdout.snapshot(), [] as [u8; 0]);
        assert!(
            String::from_utf8(stderr.snapshot())
                .unwrap()
                .contains("preparation")
        );
    }

    #[tokio::test]
    async fn explicit_candidate_run_rejects_a_missing_candidate() {
        let missing_id = "candidate-that-is-not-in-the-spool";
        let (error, stdout) = run_missing_explicit_candidate("value = 1 + 2\n", missing_id).await;

        assert!(error.contains(missing_id), "{error}");
        assert!(
            !std::str::from_utf8(&stdout)
                .unwrap()
                .contains("\"complete\":true"),
            "a missing explicit candidate must not produce a complete report"
        );
    }

    #[tokio::test]
    async fn explicit_candidate_run_rejects_an_empty_candidate_spool() {
        let missing_id = "candidate-missing-from-empty-spool";
        let (error, stdout) = run_missing_explicit_candidate("pass\n", missing_id).await;

        assert!(error.contains(missing_id), "{error}");
        assert!(
            !std::str::from_utf8(&stdout)
                .unwrap()
                .contains("\"complete\":true"),
            "an empty spool must not produce a complete explicit verification report"
        );
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

        let exit = run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
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
            OsString::from("--min-free-space"),
            OsString::from("18446744073709551615B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let exit = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect("initial threshold must use the typed stop lifecycle");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let control = RunControl::with_disk_meter(FailingDiskMeter);
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let exit = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect("initial measurement failure must use the typed stop lifecycle");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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

        let exit = run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect("a pre-dispatch disk stop must finish through the run state machine");

        assert_ne!(exit, 0);
        assert_eq!(observed.dispatched_effect_count(), 0);
        assert_eq!(observed.max_process_tasks(), 0);
    }

    async fn wait_for_live_process<F>(run: &mut F, ready: impl Fn() -> bool) -> Result<(), String>
    where
        F: std::future::Future + Unpin,
        F::Output: std::fmt::Debug,
    {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    result = &mut *run => return Err(format!("run finished before fixture readiness: {result:?}")),
                    () = tokio::time::sleep(Duration::from_millis(5)) => {
                        if ready() { return Ok(()); }
                    }
                }
            }
        }).await.map_err(|_| "live process fixture did not become ready".to_owned())?
    }

    async fn wait_for_disk_stop<F>(run: &mut F, observed: &RunControl) -> Result<F::Output, String>
    where
        F: std::future::Future + Unpin,
    {
        let Ok(result) = tokio::time::timeout(Duration::from_secs(2), &mut *run).await else {
            observed.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(3), run).await;
            return Err("periodic disk stop did not interrupt the active process".to_owned());
        };
        Ok(result)
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
            crate::report::ReportPresentation::default(),
        ));
        wait_for_live_process(&mut run, || observed.max_process_tasks() != 0)
            .await
            .expect("baseline process was not dispatched");

        stop.store(true, Ordering::Release);
        let exit = wait_for_disk_stop(&mut run, &observed)
            .await
            .unwrap()
            .unwrap();

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
            crate::report::ReportPresentation::default(),
        ));

        wait_for_live_process(&mut run, || {
            observed.max_process_tasks() != 0 && entered_rx.try_recv().is_ok()
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
        .await
        .expect("monitor thread starts");

        let failure = sample_after_process_drain(&monitor, true)
            .await
            .expect("post-drain reserve stop");

        assert_eq!(
            failure.reason.kind(),
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
            reason: hoimin_core::DiskFailureReason::FilesystemReserveReached(DiskObservation {
                owned_bytes: 0,
                available_bytes: 1,
                measured_in: Duration::from_millis(1),
            }),
            secondary: Vec::new(),
        });
        let stdout = CollectingWriter::default();

        let exit = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect("disk stop should complete through the typed state machine");

        assert_ne!(exit, 0);
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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
        let stdout = CollectingWriter::default();

        let exit = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect("monitor stop should complete through the typed state machine");

        assert_ne!(exit, 0);
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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

    #[test]
    fn final_measurement_pause_returns_when_release_is_missing() {
        let (pause, controller) = FinalMeasurementPause::with_timeout(Duration::from_millis(25));
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let waiter = std::thread::spawn(move || {
            pause.wait();
            let _ = finished_tx.send(());
        });

        controller
            .wait_until_entered()
            .expect("final measurement pause entered");
        let finished = finished_rx.recv_timeout(Duration::from_millis(250));
        drop(controller);
        waiter.join().unwrap();

        assert_eq!(
            finished,
            Ok(()),
            "final measurement pause remained blocked without a release signal"
        );
    }

    #[test]
    fn final_measurement_pause_controller_times_out_when_hook_never_enters() {
        let (pause, controller) = FinalMeasurementPause::with_timeout(Duration::from_millis(25));
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let waiter = std::thread::spawn(move || {
            let result = controller.wait_until_entered();
            let _ = finished_tx.send(result);
        });

        let finished = finished_rx.recv_timeout(Duration::from_millis(250));
        drop(pause);
        waiter.join().unwrap();
        assert_eq!(
            finished,
            Ok(Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
            "final measurement pause controller remained blocked waiting for entry"
        );
    }

    #[test]
    fn final_measurement_pause_hook_returns_when_entry_is_not_received() {
        let (pause, controller) = FinalMeasurementPause::with_timeout(Duration::from_millis(25));
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let waiter = std::thread::spawn(move || {
            pause.wait();
            let _ = finished_tx.send(());
        });

        let finished = finished_rx.recv_timeout(Duration::from_millis(250));
        drop(controller);
        waiter.join().unwrap();

        assert_eq!(
            finished,
            Ok(()),
            "final measurement pause blocked while reporting that it was entered"
        );
    }

    #[test]
    fn final_measurement_pause_release_returns_before_hook_waits() {
        let (pause, controller) = FinalMeasurementPause::with_timeout(Duration::from_millis(25));
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let releaser = std::thread::spawn(move || {
            let returned = controller.release().is_ok();
            let _ = finished_tx.send(returned);
        });

        let finished = finished_rx.recv_timeout(Duration::from_millis(250));
        drop(pause);
        releaser.join().unwrap();

        assert_eq!(
            finished,
            Ok(true),
            "final measurement pause blocked while sending an early release"
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
        let config = parse_host_independent_shell_test_config(args);
        let (control, pause) = RunControl::with_final_measurement_pause();
        let (probe_request_tx, probe_request_rx) = tokio::sync::oneshot::channel();
        let (probe_observed_tx, probe_observed_rx) = std::sync::mpsc::channel();
        let probe = tokio::spawn(async move {
            let _ = probe_request_rx.await.map(|()| probe_observed_tx.send(()));
        });
        let observer = std::thread::spawn(move || {
            pause
                .wait_until_entered()
                .map_err(|error| format!("wait for final measurement pause: {error}"))?;
            let request_sent = probe_request_tx.send(()).is_ok();
            let runtime_was_responsive = request_sent
                && probe_observed_rx
                    .recv_timeout(Duration::from_millis(500))
                    .is_ok();
            pause
                .release()
                .map_err(|error| format!("release final measurement pause: {error}"))?;
            Ok::<bool, String>(runtime_was_responsive)
        });

        let result = run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await;
        probe.await.expect("runtime responsiveness probe joined");
        let runtime_was_responsive = observer
            .join()
            .expect("final measurement pause observer joined");

        assert_eq!(result.unwrap(), 0);
        assert!(
            runtime_was_responsive.expect("final measurement pause observer completed"),
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
        let stdout = CollectingWriter::default();
        let release = std::thread::spawn(move || {
            pause
                .wait_until_entered()
                .map_err(|error| format!("wait for final measurement pause: {error}"))?;
            std::thread::sleep(Duration::from_millis(100));
            pause
                .release()
                .map_err(|error| format!("release final measurement pause: {error}"))
        });

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("expired final measurement must defer recursive cleanup");
        let release_result = release
            .join()
            .expect("final measurement pause releaser joined");

        assert!(
            error.contains(&format!(
                "{}: {CLEANUP_QUIESCENCE_UNPROVEN}",
                hoimin_core::WORKSPACE_CLEANUP_DEFERRED
            )),
            "{error}"
        );
        release_result.expect("final measurement pause releaser completed");
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots.iter().all(|root| root.exists()),
            "cleanup raced an unjoined final measurement: {roots:?}"
        );
        assert!(
            !stdout
                .snapshot()
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
                .any(|event| event["kind"] == "run_finished"),
            "expired delivery admitted a final report"
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "expired delivery acknowledged final output",
        );
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
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = parse_host_independent_shell_test_config(args);
        let control = RunControl::new();
        control.inject_process_reap_failure();
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("unproven process reap must leave cleanup incomplete");

        assert!(
            error.contains(hoimin_core::WORKSPACE_CLEANUP_DEFERRED),
            "{error}"
        );
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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
        let stdout = CollectingWriter::default();

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("disk stop plus unproven reap must remain incomplete");

        assert!(
            error.contains(hoimin_core::WORKSPACE_CLEANUP_DEFERRED),
            "{error}"
        );
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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
        let managed_parent = Utf8Path::from_path(control.managed_parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(managed_parent).unwrap();
        let preserved = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        drop(preserved);
        drop(coordinator);
        control.inject_execution_cleanup_deferred();
        control.override_end_available(Ok(BTreeMap::from([(FilesystemKey(7), 500)])));
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
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
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
        let execution_cleanup = report["summary"]["disk"]["cleanup"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["root_id"] == "execution")
            .expect("execution cleanup evidence");
        assert_eq!(execution_cleanup["status"], "deferred");
        assert!(execution_cleanup["remaining_root"].is_null());
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["end_available_bytes"],
            500
        );
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["available_bytes_change"],
            -500
        );
        let janitor = &report["summary"]["disk"]["janitor"];
        assert_eq!(janitor["preserved_roots"], 1);
        assert_eq!(janitor["preserved_by_reason"]["young"], 1);
        assert_eq!(janitor["details"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn inner_monitor_join_timeout_is_persisted_when_outer_retry_joins() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
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
        control.inject_post_drain_disk_failure(hoimin_core::DiskFailure {
            reason: hoimin_core::DiskFailureReason::MeasurementFailed(
                "injected prior measurement failure".to_owned(),
            ),
            secondary: Vec::new(),
        });
        control.inject_inner_monitor_join_timeout_once();
        control.override_end_available(Ok(BTreeMap::from([(FilesystemKey(7), 500)])));
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("an unjoined monitor must keep the run incomplete");

        assert!(error.contains("disk.measurement.failed"), "{error}");
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
        assert_eq!(
            report["summary"]["disk"]["stop"]["code"],
            hoimin_core::DISK_MEASUREMENT_FAILED
        );
        assert_eq!(
            report["summary"]["disk"]["stop"]["message"],
            "injected prior measurement failure"
        );
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["end_available_bytes"],
            500
        );
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["available_bytes_change"],
            -500
        );
        assert!(
            report["summary"]["disk"]["stop"]["secondary"]
                .as_array()
                .unwrap()
                .iter()
                .any(|secondary| {
                    secondary["Error"]["code"] == hoimin_core::DISK_MEASUREMENT_FAILED
                        && secondary["Error"]["message"]
                            .as_str()
                            .is_some_and(|message| message.contains("monitor join"))
                }),
            "{}",
            report["summary"]["disk"]["stop"]
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "monitor join timeout was acknowledged as delivered"
        );
    }

    #[test]
    fn cleanup_disposition_projects_without_diagnostic_classification() {
        for (disposition, expected) in [
            (
                CleanupDisposition::Clean,
                hoimin_core::DiskCleanupStatus::Clean,
            ),
            (
                CleanupDisposition::Failed,
                hoimin_core::DiskCleanupStatus::Failed,
            ),
            (
                CleanupDisposition::Deferred(None),
                hoimin_core::DiskCleanupStatus::Deferred,
            ),
        ] {
            let cleanup = CleanupRecord {
                disposition,
                evidence: CleanupEvidence::default(),
            };
            let mut report = hoimin_core::DiskCleanupReport {
                root_id: "execution".to_owned(),
                owner: "hoimin".to_owned(),
                status: hoimin_core::DiskCleanupStatus::Retained,
                examined_entries: 0,
                removed_entries: 0,
                details: Vec::new(),
                omitted_detail_count: 0,
                remaining_root: None,
            };

            apply_execution_cleanup_evidence(&mut report, Some(&cleanup));

            assert_eq!(report.status, expected);
        }
    }

    #[test]
    fn managed_cleanup_disk_classification_ignores_diagnostic_wording() {
        for (disposition, error, expected) in [
            (
                CleanupDisposition::Failed,
                "workspace.cleanup.deferred: misleading diagnostic",
                DiskCleanupOutcome::Failed(
                    "workspace.cleanup.deferred: misleading diagnostic".to_owned(),
                ),
            ),
            (
                CleanupDisposition::Deferred(None),
                "workspace.cleanup.failed: misleading diagnostic",
                DiskCleanupOutcome::Deferred(
                    "workspace.cleanup.failed: misleading diagnostic".to_owned(),
                ),
            ),
        ] {
            let outcome = ManagedCleanupOutcome {
                record: CleanupRecord {
                    disposition,
                    evidence: CleanupEvidence::default(),
                },
                error: error.to_owned(),
            };

            let actual = outcome.disk_outcome();

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn report_only_cleanup_states_preserve_evidence_without_an_execution_result() {
        for status in [
            hoimin_core::DiskCleanupStatus::Retained,
            hoimin_core::DiskCleanupStatus::CleanupAfterDelivery,
        ] {
            let mut report = hoimin_core::DiskCleanupReport {
                root_id: "execution".to_owned(),
                owner: "hoimin".to_owned(),
                status,
                examined_entries: 8,
                removed_entries: 3,
                details: vec!["delivery pending".to_owned()],
                omitted_detail_count: 2,
                remaining_root: Some("known-root".to_owned()),
            };

            apply_execution_cleanup_evidence(&mut report, None);

            assert_eq!(report.status, status);
            assert_eq!((report.examined_entries, report.removed_entries), (8, 3));
            assert_eq!(report.remaining_root.as_deref(), Some("known-root"));
            assert_eq!(report.details, ["delivery pending"]);
            assert_eq!(report.omitted_detail_count, 2);
        }
    }

    #[test]
    fn cleanup_ready_failure_retains_each_original_classification() {
        for (disposition, expected, expected_error) in [
            (
                CleanupDisposition::Clean,
                DiskCleanupOutcome::Clean,
                "original",
            ),
            (
                CleanupDisposition::Failed,
                DiskCleanupOutcome::Failed("original; cleanup-ready: marker failed".to_owned()),
                "original; cleanup-ready: marker failed",
            ),
            (
                CleanupDisposition::Deferred(None),
                DiskCleanupOutcome::Deferred("original; cleanup-ready: marker failed".to_owned()),
                "original; cleanup-ready: marker failed",
            ),
        ] {
            let outcome = ManagedCleanupOutcome {
                record: CleanupRecord {
                    disposition,
                    evidence: CleanupEvidence::default(),
                },
                error: "original".to_owned(),
            };

            let outcome = attach_cleanup_ready_error(outcome, Some("marker failed"));

            assert_eq!(outcome.disk_outcome(), expected);
            assert_eq!(outcome.error, expected_error);
            assert_eq!(
                outcome.record.evidence.details,
                ["cleanup-ready: marker failed"]
            );
        }
    }

    proptest::proptest! {
        #[test]
        fn cleanup_projection_preserves_generated_evidence_and_ignores_detail(
            detail in ".{0,4096}",
            error in ".{0,128}",
            remaining_root in proptest::option::of("[a-z]{1,32}"),
            examined in 0_u64..10000,
            removed in 0_u64..10000,
            omitted in proptest::arbitrary::any::<u64>(),
        ) {
            for (disposition, status, disk) in [
                (CleanupDisposition::Clean, hoimin_core::DiskCleanupStatus::Clean, DiskCleanupOutcome::Clean),
                (CleanupDisposition::Failed, hoimin_core::DiskCleanupStatus::Failed, DiskCleanupOutcome::Failed(error.clone())),
                (CleanupDisposition::Deferred(None), hoimin_core::DiskCleanupStatus::Deferred, DiskCleanupOutcome::Deferred(error.clone())),
            ] {
                let mut evidence = CleanupEvidence {
                    examined_entries: examined, removed_entries: removed,
                    omitted_detail_count: omitted,
                    remaining_root: remaining_root.clone().map(Utf8PathBuf::from),
                    ..CleanupEvidence::default()
                };
                evidence.push_detail(detail.clone());
                let outcome = ManagedCleanupOutcome { record: CleanupRecord { disposition, evidence: evidence.clone() }, error: error.clone() };
                let mut report = hoimin_core::DiskCleanupReport {
                    root_id: "execution".to_owned(), owner: "hoimin".to_owned(),
                    status: hoimin_core::DiskCleanupStatus::Retained,
                    examined_entries: 0, removed_entries: 0, details: Vec::new(),
                    omitted_detail_count: 0, remaining_root: None,
                };

                let actual = outcome.disk_outcome();
                apply_execution_cleanup_evidence(&mut report, Some(&outcome.record));

                proptest::prop_assert_eq!(actual, disk);
                proptest::prop_assert_eq!(report.status, status);
                proptest::prop_assert_eq!(report.remaining_root.as_ref(), remaining_root.as_ref());
                proptest::prop_assert_eq!(report.examined_entries, examined);
                proptest::prop_assert_eq!(report.removed_entries, removed);
                proptest::prop_assert_eq!(report.details, evidence.details);
                proptest::prop_assert_eq!(report.omitted_detail_count, evidence.omitted_detail_count);
            }
        }
    }

    #[test]
    fn recordless_thread_failure_preserves_kind_and_root_evidence() {
        let stable = Utf8PathBuf::from("stable-active-root");
        for (failure, remaining_root, expected) in [
            (
                CleanupThreadFailure::Spawn("diagnostic changed".to_owned()),
                Some(stable.clone()),
                CleanupDisposition::Failed,
            ),
            (
                CleanupThreadFailure::Spawn("diagnostic changed".to_owned()),
                None,
                CleanupDisposition::Deferred(None),
            ),
            (
                CleanupThreadFailure::Stopped,
                Some(stable.clone()),
                CleanupDisposition::Failed,
            ),
            (
                CleanupThreadFailure::Stopped,
                None,
                CleanupDisposition::Deferred(None),
            ),
            (
                CleanupThreadFailure::TimedOut,
                Some(stable.clone()),
                CleanupDisposition::Deferred(None),
            ),
            (
                CleanupThreadFailure::TimedOut,
                None,
                CleanupDisposition::Deferred(None),
            ),
        ] {
            let outcome = recordless_cleanup_outcome(&failure, remaining_root.clone());

            assert_eq!(outcome.record.disposition, expected);
            assert_eq!(outcome.record.evidence.remaining_root, remaining_root);
        }
    }

    #[test]
    fn recordless_failed_cleanup_without_a_verified_path_is_deferred() {
        let outcome = recordless_cleanup_outcome(&CleanupThreadFailure::Stopped, None);

        let ManagedCleanupOutcome { record, error } = outcome;
        assert!(matches!(
            record.disposition,
            CleanupDisposition::Deferred(_)
        ));
        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert!(record.evidence.remaining_root.is_none());
        assert!(error.starts_with("workspace.cleanup.deferred:"), "{error}");
        assert_eq!(
            record.evidence.details,
            vec!["workspace.cleanup.failed: cleanup thread stopped without a result"]
        );
    }

    #[test]
    fn cleanup_worker_panic_becomes_a_bounded_identity_checked_record() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();

        let record = cleanup_with_panic_recovery(&root, || panic!("injected cleanup panic"));

        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Failed
        );
        assert_eq!(record.evidence.remaining_root.as_deref(), Some(root.path()));
        assert_eq!(record.evidence.details.len(), 1);
        assert!(record.evidence.details[0].contains("cleanup worker panicked"));
    }

    #[tokio::test]
    async fn cleanup_thread_spawn_failure_is_typed_without_running_the_operation() {
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(1),
            Duration::ZERO,
        );
        let operation_ran = Arc::new(AtomicBool::new(false));
        let observed_operation = Arc::clone(&operation_ran);

        let error = run_cleanup_thread_with_spawner(
            &budget,
            move || {
                observed_operation.store(true, Ordering::Release);
            },
            |_| Err(std::io::Error::other("injected thread exhaustion")),
        )
        .await
        .expect_err("thread creation failure must be typed");

        assert_eq!(
            error,
            CleanupThreadFailure::Spawn("injected thread exhaustion".to_owned())
        );
        assert_eq!(
            error.to_string(),
            "workspace.cleanup.failed: cleanup thread could not start: injected thread exhaustion"
        );
        assert!(!operation_ran.load(Ordering::Acquire));
    }

    #[test]
    fn cleanup_spawn_failure_is_failed_with_the_stable_active_root() {
        let parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent).unwrap();
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();

        let outcome = cleanup_thread_failure_outcome(
            &root,
            &CleanupThreadFailure::Spawn("injected thread exhaustion".to_owned()),
        );

        let ManagedCleanupOutcome { record, error } = outcome;
        assert!(matches!(record.disposition, CleanupDisposition::Failed));
        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Failed
        );
        assert_eq!(record.evidence.remaining_root.as_deref(), Some(root.path()));
        assert!(error.starts_with("workspace.cleanup.failed:"), "{error}");
    }

    #[tokio::test]
    async fn startup_janitor_evidence_survives_recordless_execution_cleanup_failure() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
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
        let managed_parent = Utf8Path::from_path(control.managed_parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(managed_parent).unwrap();
        let preserved = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        drop(preserved);
        drop(coordinator);
        control.inject_execution_cleanup_failed_without_record();
        control.override_end_available(Ok(BTreeMap::from([(FilesystemKey(7), 500)])));
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let _result = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await;

        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
        let execution_cleanup = report["summary"]["disk"]["cleanup"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["root_id"] == "execution")
            .expect("execution cleanup evidence");
        assert_eq!(
            execution_cleanup["remaining_root"],
            observed.managed_root_paths()[0].as_str()
        );
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["end_available_bytes"],
            500
        );
        assert_eq!(
            report["summary"]["disk"]["filesystems"][0]["available_bytes_change"],
            -500
        );
        assert_eq!(report["summary"]["disk"]["janitor"]["preserved_roots"], 1);
        assert_eq!(
            report["summary"]["disk"]["janitor"]["preserved_by_reason"]["young"],
            1
        );
        assert_eq!(
            report["summary"]["disk"]["janitor"]["details"],
            serde_json::json!([])
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

        let error = run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
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
        let config = parse_host_independent_shell_test_config(args);
        let control = RunControl::new();
        let observed = control.clone();

        let exit = run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
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
        let config = parse_host_independent_shell_test_config(args);
        let control = RunControl::new();
        let observed = control.clone();

        assert_eq!(
            run_loop_with_control(
                config,
                Vec::new(),
                Vec::new(),
                control,
                crate::report::ReportPresentation::default()
            )
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
    async fn final_report_contains_runtime_disk_measurement_and_cleanup_evidence() {
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
        let config = parse_host_independent_shell_test_config(args);
        let stdout = CollectingWriter::default();

        assert_eq!(
            run_loop_with_control(
                config,
                stdout.clone(),
                Vec::new(),
                RunControl::new(),
                crate::report::ReportPresentation::default()
            )
            .await
            .unwrap(),
            0
        );

        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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
        let stdout = CollectingWriter::default();

        assert_eq!(
            run_loop_with_control(
                config,
                stdout.clone(),
                Vec::new(),
                control,
                crate::report::ReportPresentation::default()
            )
            .await
            .unwrap(),
            0
        );
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = parse_host_independent_shell_test_config(args);
        let control = RunControl::new();
        control.override_end_available(Err("injected end query failure".to_owned()));
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("required end-free evidence failure must keep the run incomplete");

        assert!(error.contains("disk.measurement.failed"), "{error}");
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
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

    struct AlwaysFailingWriter;

    #[derive(Clone, Default)]
    struct SharedDiagnosticCapture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for SharedDiagnosticCapture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct PausedReportWriter {
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        release: std::sync::mpsc::Receiver<()>,
        pause_flush: bool,
        record: Vec<u8>,
        terminal: bool,
    }

    impl PausedReportWriter {
        fn pause(&mut self) -> std::io::Result<()> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
                self.release
                    .recv_timeout(Duration::from_secs(4))
                    .map_err(std::io::Error::other)?;
            }
            Ok(())
        }
    }

    impl Write for PausedReportWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.record.extend_from_slice(bytes);
            self.terminal |= self
                .record
                .windows(b"run_finished".len())
                .any(|part| part == b"run_finished");
            if !self.pause_flush && self.terminal {
                self.pause()?;
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.pause_flush && self.terminal {
                self.pause()?;
            }
            self.record.clear();
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn interrupted_delivery_retains_its_owner_until_write_or_flush_returns() {
        for pause_flush in [false, true] {
            let project = tempfile::tempdir().unwrap();
            let config = output_failure_test_config(&project);
            assert_host_independent_test_reserve(&config);
            let mut control = RunControl::new();
            control.shutdown_grace = Duration::from_millis(50);
            let observed = control.clone();
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let writer = PausedReportWriter {
                entered: Some(entered_tx),
                release: release_rx,
                pause_flush,
                record: Vec::new(),
                terminal: false,
            };
            let diagnostics = SharedDiagnosticCapture::default();
            let captured = diagnostics.clone();
            let mut run = Box::pin(run_loop_with_control(
                config,
                writer,
                diagnostics,
                control,
                crate::report::ReportPresentation::default(),
            ));
            tokio::select! {
                result = &mut run => panic!(
                    "run finished before report pause: {result:?}; stderr={}",
                    String::from_utf8_lossy(&captured.0.lock().unwrap()),
                ),
                entered = entered_rx => entered.unwrap_or_else(|error| panic!(
                    "report pause signal dropped: {error}; stderr={}",
                    String::from_utf8_lossy(&captured.0.lock().unwrap()),
                )),
            }
            let roots = observed.managed_root_paths();
            let delivery = &roots[1];
            let child = std::fs::read_dir(delivery)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("report-")
                })
                .expect("delivery child exists during JSONL write");
            observed.cancel();
            let result = tokio::time::timeout(Duration::from_secs(1), &mut run).await;
            let retained = delivery.exists() && child.exists();
            let execution_removed = !roots[0].exists();
            let acknowledged = observed
                .finalization_events()
                .contains(&"output_acknowledged");
            release_tx.send(()).unwrap();
            drop(run);
            assert_eq!(result.unwrap().unwrap_or(2), 2);
            assert!(
                retained,
                "cleanup removed a report resource while its operation was blocked"
            );
            assert!(
                execution_removed,
                "report delivery retained an already-cleaned execution root"
            );
            assert!(!acknowledged, "incomplete report delivery was acknowledged");
        }
    }

    impl Write for AlwaysFailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected report write failure"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("injected report flush failure"))
        }
    }

    #[tokio::test]
    async fn owned_report_io_failure_releases_managed_roots_without_output_acknowledgement() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let control = RunControl::new();
        let observed = control.clone();
        assert_eq!(
            run_loop_with_control(
                config,
                AlwaysFailingWriter,
                Vec::new(),
                control,
                crate::report::ReportPresentation::default()
            )
            .await
            .unwrap_or(2),
            2
        );
        assert!(
            observed
                .managed_root_paths()
                .iter()
                .all(|root| !root.exists()),
            "JSONL delivery"
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "JSONL delivery"
        );
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
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        let observed = control.clone();

        let error = run_loop_with_control(
            config,
            AlwaysFailingWriter,
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("report write failure must not be acknowledged");

        assert!(error.contains("report"), "{error}");
        let roots = observed.managed_root_paths();
        assert_eq!(roots.len(), 2);
        assert!(
            roots.iter().all(|root| !root.exists()),
            "report failure stranded managed roots: {roots:?}; run error: {error}"
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
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = crate::cli::parse_config_from(args).unwrap();
        let control = RunControl::new();
        control.inject_delivery_cleanup_identity_failure();
        let observed = control.clone();
        let stdout = CollectingWriter::default();

        let error = run_loop_with_control(
            config,
            stdout.clone(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
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
    async fn output_rejects_ack_on_write_or_delivery_cleanup_failure() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let control = RunControl::new();
        let observed = control.clone();

        let error = run_loop_with_control(
            config,
            AlwaysFailingWriter,
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("report write failure must not be acknowledged");

        assert!(error.contains("report"), "{error}");
        assert!(
            observed
                .managed_root_paths()
                .iter()
                .all(|root| !root.exists()),
            "report failure stranded a managed root"
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "write failure was acknowledged"
        );

        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let control = RunControl::new();
        control.inject_delivery_cleanup_identity_failure();
        let observed = control.clone();

        let error = run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .expect_err("delivery cleanup failure must not be acknowledged");

        assert!(
            error.contains(hoimin_core::WORKSPACE_CLEANUP_FAILED),
            "{error}"
        );
        let roots = observed.managed_root_paths();
        assert!(!roots[0].exists(), "execution root");
        assert!(roots[1].exists(), "replacement root");
        assert!(
            observed
                .delivery_cleanup_sentinel()
                .expect("original delivery sentinel")
                .exists(),
            "original delivery sentinel"
        );
        assert!(
            !observed
                .finalization_events()
                .contains(&"output_acknowledged"),
            "cleanup failure was acknowledged"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shell_setup_does_not_block_the_async_runtime() {
        let project = tempfile::tempdir().unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
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
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let result =
            prepare_shell_setup(config, Box::new(|| panic!("controlled shell setup panic"))).await;
        let Err(error) = result else {
            panic!("controlled setup panic unexpectedly succeeded");
        };

        assert!(error.starts_with("shell setup task failed:"), "{error}");
        assert!(error.contains("controlled shell setup panic"), "{error}");
    }

    #[tokio::test]
    async fn preparation_rollback_marker_failure_is_infrastructure_failure() {
        // Arrange: an invalid cleanup-ready entry prevents publishing safe recovery.
        let project = tempfile::tempdir().unwrap();
        let managed_parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());

        // Act.
        let stopped = prepare_shell_setup_controlled(
            config,
            parent,
            &|stage| {
                if stage == ShellSetupBoundary::RootsCreated {
                    for entry in std::fs::read_dir(parent.join("hoimin-workspaces-v2")).unwrap() {
                        let entry = entry.unwrap();
                        if entry.file_name().to_string_lossy().starts_with("run-") {
                            std::fs::create_dir(entry.path().join(".hoimin-cleanup-ready.json"))
                                .unwrap();
                        }
                    }
                    control.cancel();
                }
            },
            Some(&timing),
        );
        let Err(error) = stopped else {
            panic!("marker failure fixture must stop setup");
        };
        let result = timing.report(error, Vec::new()).await;

        // Assert.
        assert!(
            result
                .unwrap_err()
                .contains("preparation rollback marker failed")
        );
    }

    #[test]
    fn preparation_late_rollback_does_not_restart_root_cleanup_budget() {
        // Arrange.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        let _entered = runtime.enter();
        let project = tempfile::tempdir().unwrap();
        let managed_parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let prepared =
            prepare_shell_setup_controlled(config, parent, &|_| {}, Some(&timing)).unwrap();
        let roots = prepared.spool_dir.paths();
        runtime.block_on(tokio::time::advance(Duration::from_secs(32)));

        // Act: a late owner may publish recovery evidence, but gets no new cleanup window.
        drop(prepared);

        // Assert.
        assert!(roots.iter().all(|root| root.exists()));
        assert!(
            roots
                .iter()
                .all(|root| root.join(".hoimin-cleanup-ready.json").is_file())
        );
        assert_eq!(timing.rollback_remaining(), Duration::ZERO);
    }

    macro_rules! preparation_stop_case {
        ($name:ident, $stage:ident, $cancel:expr) => {
            #[test]
            fn $name() {
                // Arrange.
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build().unwrap();
                let _entered = runtime.enter();
                let project = tempfile::tempdir().unwrap();
                let managed_parent = tempfile::tempdir().unwrap();
                let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
                let config = shell_setup_test_config(&project).with_environment(hoimin_core::EnvironmentSnapshot::Untracked).unwrap();
                let control = RunControl::new();
                let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
                let observed = std::cell::RefCell::new(Vec::new());
                let selected = ShellSetupBoundary::$stage;

                // Act.
                let result = prepare_shell_setup_controlled(config, parent, &|stage| {
                    observed.borrow_mut().push(stage);
                    if stage == selected {
                        if $cancel { control.cancel(); }
                        else { runtime.block_on(tokio::time::advance(Duration::from_secs(30))); }
                    }
                }, Some(&timing));

                // Assert: no subsequent preparation stage was entered.
                let cause = if $cancel { ShutdownCause::Cancellation } else { ShutdownCause::TotalTimeout };
                assert!(matches!(result, Err(PreparationError::Stopped(actual)) if actual == cause));
                assert_eq!(observed.borrow().last(), Some(&selected));
                let residual = std::fs::read_dir(parent.join("hoimin-workspaces-v2")).unwrap()
                    .map(|entry| entry.unwrap().file_name()).filter(|name| name != ".hoimin-coordinator").collect::<Vec<_>>();
                assert!(residual.is_empty(), "stopped preparation leaked roots: {residual:?}");
            }
        };
    }
    preparation_stop_case!(preparation_timeout_after_roots, RootsCreated, false);
    preparation_stop_case!(preparation_cancel_after_roots, RootsCreated, true);
    preparation_stop_case!(
        preparation_timeout_after_disk_policy,
        DiskPolicyVerified,
        false
    );
    preparation_stop_case!(
        preparation_cancel_after_disk_policy,
        DiskPolicyVerified,
        true
    );
    preparation_stop_case!(preparation_timeout_after_workspace, WorkspaceCreated, false);
    preparation_stop_case!(preparation_cancel_after_workspace, WorkspaceCreated, true);
    preparation_stop_case!(preparation_timeout_after_backend, BackendCreated, false);
    preparation_stop_case!(preparation_cancel_after_backend, BackendCreated, true);
    preparation_stop_case!(preparation_timeout_after_process, ProcessCreated, false);
    preparation_stop_case!(preparation_cancel_after_process, ProcessCreated, true);
    preparation_stop_case!(preparation_timeout_after_analyzer, AnalyzerCreated, false);
    preparation_stop_case!(preparation_cancel_after_analyzer, AnalyzerCreated, true);
    preparation_stop_case!(
        preparation_timeout_after_delivery_ready,
        DeliveryReady,
        false
    );
    preparation_stop_case!(preparation_cancel_after_delivery_ready, DeliveryReady, true);

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
            let managed = parent.join("hoimin-workspaces-v2");
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
        let abandoned_root =
            ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let active = abandoned_root.path().to_owned();
        let run_id = active
            .file_name()
            .unwrap()
            .strip_prefix("run-")
            .unwrap()
            .to_owned();
        let abandoned = parent
            .join("hoimin-workspaces-v2")
            .join(format!(".deleting-{run_id}"));
        drop(abandoned_root);
        drop(coordinator);
        for entry in std::fs::read_dir(&active).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::rename(active, &abandoned).unwrap();

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
    fn execution_cleanup_projection_retains_only_its_own_evidence() {
        let cleanup = CleanupRecord {
            disposition: CleanupDisposition::Clean,
            evidence: CleanupEvidence {
                examined_entries: 4,
                removed_entries: 3,
                details: vec!["execution cleanup detail".to_owned()],
                omitted_detail_count: 0,
                remaining_root: None,
            },
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

        apply_execution_cleanup_evidence(&mut report, Some(&cleanup));

        assert_eq!(report.status, hoimin_core::DiskCleanupStatus::Clean);
        assert_eq!(report.examined_entries, 4);
        assert_eq!(report.removed_entries, 3);
        assert_eq!(report.details, ["execution cleanup detail"]);
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
            ShellSetupBoundary::DeliveryReady,
        ] {
            let project = tempfile::tempdir().unwrap();
            let managed_parent = tempfile::tempdir().unwrap();
            let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
            let config = shell_setup_test_config(&project)
                .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                .unwrap();

            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = prepare_shell_setup_sync_in(config, parent, &|observed| {
                    assert_ne!(observed, selected, "injected setup failure at {selected:?}");
                });
            }));

            assert!(unwind.is_err(), "boundary {selected:?} did not unwind");
            let managed = parent.join("hoimin-workspaces-v2");
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
    fn shell_setup_passes_copy_cap_to_workspace_admission() {
        // Arrange: this bypasses the final core reservation to test early admission.
        let project = tempfile::tempdir().unwrap();
        let managed_parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
        let config = shell_setup_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut config = config.into_data();
        config.limits.jobs = std::num::NonZeroUsize::new(1).unwrap();
        config.limits.max_copy_size = std::num::NonZeroU64::new(1).unwrap();
        let config = RunConfig::try_from(config).unwrap();
        let mut prepared = prepare_shell_setup_sync_in(config, parent, &|_| {}).unwrap();

        // Act.
        let result = prepared
            .workspace
            .handle_preflight(Preflight { id: EffectId(1) });

        // Assert: the source fixture contains ten bytes and there is one worker.
        assert!(matches!(
            result.unwrap_err().failure,
            hoimin_core::EffectFailure::CopyLimit {
                requested: 10,
                allowance: 1
            }
        ));
    }

    #[test]
    fn prepared_setup_remains_rollback_armed_until_monitor_starts() {
        let project = tempfile::tempdir().unwrap();
        let managed_parent = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(managed_parent.path()).unwrap();
        let prepared = prepare_shell_setup_sync_in(
            shell_setup_test_config(&project)
                .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
                .unwrap(),
            parent,
            &|_| {},
        )
        .unwrap();
        let roots = prepared.spool_dir.paths();

        drop(prepared);

        assert!(
            roots.iter().all(|root| !root.exists()),
            "prepared setup leaked managed roots before monitor start: {roots:?}"
        );
    }

    #[tokio::test]
    async fn rollback_contention_marks_root_for_immediate_janitor_recovery() {
        const COMPLETED: &str = "hoimin rollback contention assertions completed";
        const CHILD_MARKER: &str = "HOIMIN_ISOLATED_ROLLBACK_CONTENTION_TEST";
        // A parallel test's fork can inherit the lease's open file description
        // until exec. Dropping the rollback guard then need not release flock.
        // Create the fixture in a process with no unrelated child spawns so
        // the single janitor pass below really observes an abandoned lease.
        let output = tokio::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("shell::tests::rollback_contention_isolated_child")
            .arg("--ignored")
            .arg("--test-threads=1")
            .arg("--nocapture")
            .env(CHILD_MARKER, "1")
            .kill_on_drop(true)
            .output();
        let output = tokio::time::timeout(Duration::from_secs(30), output)
            .await
            .expect("isolated rollback contention test timed out")
            .expect("isolated rollback contention test failed to start");
        assert!(
            output.status.success(),
            "isolated rollback contention test failed: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(COMPLETED),
            "isolated rollback contention test did not complete its assertions: {output:?}",
        );
    }

    #[tokio::test]
    #[ignore = "subprocess fixture for rollback contention"]
    async fn rollback_contention_isolated_child() {
        const COMPLETED: &str = "hoimin rollback contention assertions completed";

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
            .open(parent.join("hoimin-workspaces-v2/.hoimin-coordinator"))
            .unwrap();
        coordinator_file.try_lock().unwrap();

        drop(rollback);
        assert!(
            execution_path.exists(),
            "contention path unexpectedly deleted root"
        );
        coordinator_file.unlock().unwrap();

        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, std::time::SystemTime::now());
        assert_eq!(report.reclaimed_roots, 1, "{report:?}");
        assert!(!execution_path.exists());
        println!("{COMPLETED}");
    }

    fn process_effect(worker: u32) -> RunEffect {
        RunEffect::RunMutant(RunProcess {
            id: EffectId(1),
            context: hoimin_core::MutantContext {
                worker,
                run_id: "run-1".into(),
                mutant_id: "mutant-1".into(),
            },
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
        TempDir,
        TempDir,
        ShellContext,
        MutationCandidate,
        CreateWorker,
    ) {
        let project = tempfile::tempdir().unwrap();
        let managed_parent = tempfile::tempdir().unwrap();
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let mut context = ShellContext::new_in(
            &config,
            Vec::new(),
            Vec::new(),
            Utf8PathBuf::from_path_buf(managed_parent.path().to_path_buf()).unwrap(),
        )
        .await
        .unwrap();
        // This fixture models an active run after the monitor-start boundary.
        context.commit_setup();
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
        (project, managed_parent, context, candidate, retry)
    }

    #[tokio::test]
    async fn context_with_worker_fixtures_use_distinct_managed_parents() {
        let (_first_project, _first_managed_parent, first, _first_candidate, _first_retry) =
            context_with_worker().await;
        let (_second_project, _second_managed_parent, second, _second_candidate, _second_retry) =
            context_with_worker().await;
        let first_parent = first
            .spool_dir
            .execution_root
            .path()
            .parent()
            .expect("execution root has a managed parent")
            .to_owned();
        let second_parent = second
            .spool_dir
            .execution_root
            .path()
            .parent()
            .expect("execution root has a managed parent")
            .to_owned();

        assert_ne!(
            first_parent, second_parent,
            "worker fixtures must not share a managed-root coordinator"
        );
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let state = RunState::new(
            "persisted-session-run",
            config,
            hoimin_core::ResourceControl {
                mode: hoimin_core::ResourceMode::Hard,
                mechanism: "test_supplied_hard".into(),
            },
        );
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
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
        ];
        args.extend(successful_test_command());
        let config = parse_host_independent_shell_test_config(args);
        let control = RunControl::cancelling_before_run_finished();
        let observed_control = control.clone();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();

        let exit = tokio::time::timeout(
            Duration::from_secs(10),
            run_loop_with_control(
                config,
                stdout.clone(),
                stderr.clone(),
                control,
                crate::report::ReportPresentation::default(),
            ),
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
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();
        assert_eq!(report["summary"]["complete"], true);
        assert!(
            !String::from_utf8(stderr.snapshot())
                .unwrap()
                .contains("run stalled"),
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

    fn last_representable_instant(now: tokio::time::Instant) -> tokio::time::Instant {
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
        now.checked_add(Duration::from_secs(accepted_seconds))
            .expect("binary search retains a representable instant")
    }

    #[test]
    fn shutdown_grace_falls_back_at_the_instant_representation_boundary() {
        let now = tokio::time::Instant::now();
        let edge = last_representable_instant(now);
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
        let (_project, _managed_parent, _context, candidate, create) = context_with_worker().await;
        let read = RunEffect::ReadCandidate(hoimin_core::ReadCandidate {
            id: EffectId(31),
            worker: 0,
            spool: hoimin_core::CandidateSpoolRef {
                token: "spool.jsonl".into(),
                records: 1,
            },
            target: hoimin_core::CandidateReadTarget::Next,
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

    fn candidate_read_effect(
        id: u64,
        spool: hoimin_core::CandidateSpoolRef,
        target: hoimin_core::CandidateReadTarget,
    ) -> RunEffect {
        RunEffect::ReadCandidate(hoimin_core::ReadCandidate {
            id: EffectId(id),
            worker: 0,
            spool,
            target,
        })
    }

    fn waiting_for_sequential_read(
        config: RunConfig,
        selection: hoimin_core::RunSelection,
        spool: &hoimin_core::CandidateSpoolRef,
    ) -> (RunState, hoimin_core::ReadCandidate) {
        let jobs = u32::try_from(config.limits.jobs.get()).unwrap();
        let state = RunState::with_selection(
            "replay-stop",
            config,
            selection,
            hoimin_core::ResourceControl {
                mode: ResourceMode::Hard,
                mechanism: "test".into(),
            },
        );
        let (mut state, effects) =
            transition(state, RunEvent::StartRequested(StartRequested)).unwrap();
        let mut pending: VecDeque<_> = effects.into();
        while let Some(effect) = pending.pop_front() {
            let event = match effect {
                RunEffect::ReadCandidate(request) => {
                    assert!(pending.is_empty());
                    return (state, request);
                }
                RunEffect::ResolveTargets(request) => {
                    RunEvent::TargetsResolved(hoimin_core::TargetsResolved {
                        id: request.id,
                        targets: vec![TargetSlice {
                            path: "pkg/a.py".into(),
                            lines: Vec::new(),
                            symbols: Vec::new(),
                        }],
                    })
                }
                RunEffect::Preflight(request) => {
                    RunEvent::PreflightCompleted(hoimin_core::PreflightCompleted {
                        id: request.id,
                        per_worker_logical_bytes: 10,
                        requested_workers: jobs,
                        aggregate_logical_bytes: 10 * u64::from(jobs),
                        fingerprint: None,
                    })
                }
                RunEffect::EmitOutput(request) => {
                    RunEvent::OutputEmitted(hoimin_core::OutputEmitted { id: request.id })
                }
                RunEffect::CreateWorker(request) => {
                    RunEvent::WorkerCreated(hoimin_core::WorkerCreated {
                        id: request.id(),
                        worker: request.worker(),
                        reservation_id: request.reservation_id(),
                    })
                }
                RunEffect::VerifyOriginals(request) => {
                    RunEvent::OriginalsVerified(hoimin_core::OriginalsVerified {
                        id: request.id,
                        checkpoint: request.checkpoint,
                    })
                }
                RunEffect::RunBaseline(request) => {
                    RunEvent::BaselineFinished(hoimin_core::ProcessFinished {
                        id: request.id,
                        worker: Some(request.context.worker),
                        termination: hoimin_core::ProcessTermination::Exit(0),
                        output_state: hoimin_core::ProcessOutputState::Complete,
                        output: hoimin_core::OutputSpoolRef {
                            token: "baseline".into(),
                            retained: 0,
                            observed: 0,
                        },
                        elapsed: Duration::from_millis(1),
                        resource_mode: ResourceMode::Hard,
                    })
                }
                RunEffect::ResetWorker(request) => {
                    RunEvent::WorkerReset(hoimin_core::WorkerReset {
                        id: request.id,
                        worker: request.worker,
                    })
                }
                RunEffect::ObserveRemainingBudget(request) => {
                    RunEvent::RemainingBudgetObserved(hoimin_core::RemainingBudgetObserved {
                        id: request.id,
                        remaining: Duration::from_hours(24),
                    })
                }
                RunEffect::AnalyzeFile(request) => {
                    RunEvent::AnalysisFinished(hoimin_core::AnalysisFinished {
                        id: request.id,
                        spool: Some(spool.clone()),
                        truncated: false,
                        diagnostics: Vec::new(),
                    })
                }
                unexpected => panic!("unexpected setup effect: {unexpected:?}"),
            };
            let effects;
            (state, effects) = transition(state, event).unwrap();
            pending.extend(effects);
        }
        panic!("setup did not request its first candidate");
    }

    async fn outputs_after_unaccepted_sequential_reads(
        explicit: bool,
        retirements: usize,
    ) -> Vec<OutputEvent> {
        let (project, _managed_parent, mut context, mut first, _create) =
            context_with_worker().await;
        first.id = "first".into();
        let mut second = first.clone();
        second.id = "second".into();
        second.sequence = 2;
        let mut store = crate::analyzer::CandidateStore::new_in(2, project.path()).unwrap();
        store.push(&first).unwrap();
        store.push(&second).unwrap();
        let spool = store.finish().unwrap();
        let selection = if explicit {
            hoimin_core::RunSelection::explicit(
                [first.id],
                hoimin_core::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
            )
            .unwrap()
        } else {
            hoimin_core::RunSelection::all()
        };
        let (mut state, first_read) =
            waiting_for_sequential_read(context.config.clone(), selection, &spool);
        let mut pending = VecDeque::from([RunEffect::ReadCandidate(first_read)]);
        let mut retired = 0;
        let mut outputs = Vec::new();
        while let Some(effect) = pending.pop_front() {
            let event = match effect {
                RunEffect::ReadCandidate(request) => {
                    let id = request.id;
                    let task =
                        prepare_blocking_effect(&mut context, RunEffect::ReadCandidate(request))
                            .unwrap();
                    // The real file read finishes before core observes the stop.
                    let completion = task.execute();
                    if retired < retirements {
                        let (next, effects) =
                            transition(state, RunEvent::CancellationRequested).unwrap();
                        state = next;
                        assert!(state.is_effect_retired(id));
                        pending.extend(effects);
                        let mut in_flight = 1;
                        accept_drained_completion(
                            ShellCompletion::Blocking(Ok(Box::new(completion))),
                            &mut in_flight,
                            &mut None,
                            &mut Vec::new(),
                            &mut |completion| {
                                let _ = accept_blocking_completion(&mut context, completion);
                            },
                        );
                        assert_eq!(in_flight, 0);
                        retired += 1;
                        continue;
                    }
                    accept_blocking_completion(&mut context, completion)
                }
                RunEffect::EmitOutput(request) => {
                    outputs.push(request.event);
                    RunEvent::OutputEmitted(hoimin_core::OutputEmitted { id: request.id })
                }
                RunEffect::VerifyOriginals(request) => {
                    RunEvent::OriginalsVerified(hoimin_core::OriginalsVerified {
                        id: request.id,
                        checkpoint: request.checkpoint,
                    })
                }
                RunEffect::Cleanup(request) => {
                    RunEvent::CleanupFinished(hoimin_core::CleanupFinished {
                        id: request.id,
                        released_reservations: request.reservations,
                    })
                }
                unexpected => panic!("unexpected stopped effect: {unexpected:?}"),
            };
            let effects;
            (state, effects) = transition(state, event).unwrap();
            pending.extend(effects);
        }
        assert_eq!(retired, retirements);
        assert_eq!(state.phase(), RunPhase::Finished);
        outputs
    }

    fn finished_id_status(outputs: &[OutputEvent]) -> Vec<(&str, hoimin_core::MutationStatus)> {
        outputs
            .iter()
            .filter_map(|event| match event {
                OutputEvent::MutantFinished(value) => {
                    Some((value.candidate.id.as_str(), value.outcome.status()))
                }
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn unaccepted_sequential_read_reports_every_candidate_after_all_selection_stops() {
        // Arrange / Act: stop after the first real read but before core acceptance.
        let outputs = outputs_after_unaccepted_sequential_reads(false, 1).await;

        // Assert.
        assert_eq!(
            finished_id_status(&outputs),
            [
                ("first", hoimin_core::MutationStatus::NotRun),
                ("second", hoimin_core::MutationStatus::NotRun),
            ]
        );
    }

    #[tokio::test]
    async fn unaccepted_sequential_read_preserves_explicit_selected_membership_after_stop() {
        // Arrange / Act.
        let outputs = outputs_after_unaccepted_sequential_reads(true, 1).await;

        // Assert.
        assert_eq!(
            finished_id_status(&outputs),
            [("first", hoimin_core::MutationStatus::NotRun)]
        );
        assert!(
            !outputs
                .iter()
                .any(|event| matches!(event, OutputEvent::Diagnostic(_)))
        );
    }

    #[tokio::test]
    async fn unaccepted_sequential_read_survives_repeated_stop_without_duplicate_reports() {
        // Arrange / Act: the retried first record is also retired before acceptance.
        let outputs = outputs_after_unaccepted_sequential_reads(false, 2).await;

        // Assert.
        assert_eq!(
            finished_id_status(&outputs),
            [
                ("first", hoimin_core::MutationStatus::NotRun),
                ("second", hoimin_core::MutationStatus::NotRun),
            ]
        );
    }

    #[tokio::test]
    async fn candidate_completion_returns_the_index_for_later_selected_reads() {
        // Arrange.
        let (project, _managed_parent, mut context, candidate, _create) =
            context_with_worker().await;
        let mut store = crate::analyzer::CandidateStore::new_in(1, project.path()).unwrap();
        store.push(&candidate).unwrap();
        let spool = store.finish().unwrap();
        context.candidate_replay = Some(CandidateReplay::new([candidate.id.clone()]));
        let task = prepare_blocking_effect(
            &mut context,
            candidate_read_effect(301, spool.clone(), hoimin_core::CandidateReadTarget::Next),
        )
        .unwrap();
        assert!(context.candidate_replay.is_none());

        // Act.
        let completion = task.execute();
        let discovered = accept_blocking_completion(&mut context, completion);
        let selected = execute_effect(
            &mut context,
            candidate_read_effect(
                302,
                spool,
                hoimin_core::CandidateReadTarget::Id(candidate.id.clone()),
            ),
        )
        .await;

        // Assert.
        assert!(matches!(
            discovered,
            RunEvent::CandidateLoaded(CandidateLoaded { candidate: Some(value), .. })
                if value == candidate
        ));
        assert!(matches!(
            selected,
            RunEvent::CandidateLoaded(CandidateLoaded { candidate: Some(value), .. })
                if value == candidate
        ));
    }

    #[tokio::test]
    async fn candidate_replay_error_returns_the_reader_without_advancing_the_scan() {
        // Arrange.
        let (project, _managed_parent, mut context, first, _create) = context_with_worker().await;
        let mut second = first.clone();
        second.sequence = 2;
        second.id = "second".into();
        let mut store = crate::analyzer::CandidateStore::new_in(2, project.path()).unwrap();
        store.push(&first).unwrap();
        store.push(&second).unwrap();
        let spool = store.finish().unwrap();
        execute_effect(
            &mut context,
            candidate_read_effect(303, spool.clone(), hoimin_core::CandidateReadTarget::Next),
        )
        .await;
        let original = std::fs::read(&spool.token).unwrap();
        let mut corrupt = original.clone();
        let tail = corrupt.iter().position(|byte| *byte == b'\n').unwrap() + 1;
        corrupt[tail] = b'!';
        std::fs::write(&spool.token, corrupt).unwrap();

        // Act.
        let failed = execute_effect(
            &mut context,
            candidate_read_effect(304, spool.clone(), hoimin_core::CandidateReadTarget::Next),
        )
        .await;
        std::fs::write(&spool.token, original).unwrap();
        let retried = execute_effect(
            &mut context,
            candidate_read_effect(305, spool, hoimin_core::CandidateReadTarget::Next),
        )
        .await;

        // Assert.
        assert!(matches!(
            failed,
            RunEvent::EffectFailed(error) if error.failure.code() == "candidate.replay"
        ));
        assert!(matches!(
            retried,
            RunEvent::CandidateLoaded(CandidateLoaded { candidate: Some(value), .. })
                if value == second
        ));
    }

    #[tokio::test]
    async fn candidate_reader_cannot_be_taken_by_two_blocking_tasks() {
        // Arrange.
        let (project, _managed_parent, mut context, candidate, _create) =
            context_with_worker().await;
        let mut store = crate::analyzer::CandidateStore::new_in(1, project.path()).unwrap();
        store.push(&candidate).unwrap();
        let spool = store.finish().unwrap();
        let task = prepare_blocking_effect(
            &mut context,
            candidate_read_effect(306, spool.clone(), hoimin_core::CandidateReadTarget::Next),
        )
        .unwrap();

        // Act.
        let duplicate = prepare_blocking_effect(
            &mut context,
            candidate_read_effect(307, spool, hoimin_core::CandidateReadTarget::Next),
        );
        let restored = accept_blocking_completion(&mut context, task.execute());

        // Assert.
        assert!(matches!(
            duplicate,
            Err(error) if error.id == EffectId(307) && error.failure.code() == "shell.blocking_io"
        ));
        assert!(matches!(restored, RunEvent::CandidateLoaded(_)));
        assert!(context.candidate_replay.is_some());
    }

    #[tokio::test]
    async fn shutdown_drain_returns_the_candidate_index_for_stopped_reads() {
        // Arrange.
        let (project, _managed_parent, mut context, candidate, _create) =
            context_with_worker().await;
        let mut store = crate::analyzer::CandidateStore::new_in(1, project.path()).unwrap();
        store.push(&candidate).unwrap();
        let spool = store.finish().unwrap();
        context.candidate_replay = Some(CandidateReplay::new([candidate.id.clone()]));
        let task = prepare_blocking_effect(
            &mut context,
            candidate_read_effect(308, spool.clone(), hoimin_core::CandidateReadTarget::Next),
        )
        .unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let mut io_tasks = JoinSet::new();
        spawn_blocking_effect(task, sender, &mut io_tasks);
        let mut process_tasks = JoinSet::new();
        let mut in_flight = 1;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );

        // Act: shutdown accepts ownership even though it discards the candidate event.
        drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            |completion| {
                let _ = accept_blocking_completion(&mut context, completion);
            },
        )
        .await
        .unwrap();
        let stopped = execute_effect(
            &mut context,
            candidate_read_effect(
                309,
                spool,
                hoimin_core::CandidateReadTarget::Id(candidate.id.clone()),
            ),
        )
        .await;

        // Assert.
        assert_eq!(in_flight, 0);
        assert_eq!(warnings, Vec::new());
        assert!(!expiry_reported);
        assert!(matches!(
            stopped,
            RunEvent::CandidateLoaded(CandidateLoaded { candidate: Some(value), .. })
                if value == candidate
        ));
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
        let (_project, _managed_parent, mut context, _candidate, create) =
            context_with_worker().await;
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
        let (_project, _managed_parent, mut context, _candidate, _create) =
            context_with_worker().await;
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
        let (_project, _managed_parent, mut context, _candidate, _create) =
            context_with_worker().await;
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
        let outcome = cleanup_managed_root(execution_root, &cleanup_budget).await;
        assert!(
            matches!(outcome.record.disposition, CleanupDisposition::Clean),
            "{outcome:#?}"
        );
        let cleanup_budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(2),
            Duration::ZERO,
        );
        let outcome = cleanup_managed_root(delivery_root, &cleanup_budget).await;
        assert!(
            matches!(outcome.record.disposition, CleanupDisposition::Clean),
            "{outcome:#?}"
        );
    }

    #[tokio::test]
    async fn dropping_context_without_finalization_leaves_managed_workers_for_the_janitor() {
        let (_project, _managed_parent, context, _candidate, _create) = context_with_worker().await;
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
        let outcome = cleanup_managed_root(execution_root, &cleanup_budget).await;
        assert!(
            matches!(outcome.record.disposition, CleanupDisposition::Clean),
            "{outcome:#?}"
        );
        let cleanup_budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() + Duration::from_secs(2),
            Duration::ZERO,
        );
        let outcome = cleanup_managed_root(delivery_root, &cleanup_budget).await;
        assert!(
            matches!(outcome.record.disposition, CleanupDisposition::Clean),
            "{outcome:#?}"
        );
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

        let ManagedCleanupOutcome { record, error } = outcome;
        assert!(matches!(
            record.disposition,
            CleanupDisposition::Deferred(_)
        ));
        assert_eq!(
            record.disposition.report_status(),
            hoimin_core::DiskCleanupStatus::Deferred
        );
        assert_eq!(record.evidence.remaining_root.as_deref(), Some(root.path()));
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

        let record = outcome.record;
        assert!(matches!(
            record.disposition,
            CleanupDisposition::Deferred(_)
        ));
        assert!(
            record
                .evidence
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
            ManagedCleanupOutcome {
                record,
                error: "workspace.cleanup.deferred: managed-root cleanup budget expired".to_owned(),
            },
            Some(&marker_error),
        );

        let ManagedCleanupOutcome { record, error } = outcome;
        assert!(matches!(
            record.disposition,
            CleanupDisposition::Deferred(_)
        ));
        assert!(
            record
                .evidence
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
            disposition: CleanupDisposition::Clean,
            evidence: CleanupEvidence {
                examined_entries: 1,
                removed_entries: 1,
                details: vec!["cleanup-ready: injected marker failure".to_owned()],
                omitted_detail_count: 0,
                remaining_root: None,
            },
        };

        assert_eq!(
            delivery_cleanup_secondary_errors(&record),
            ["workspace.cleanup.failed: cleanup-ready: injected marker failure"]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_run_outer_close_uses_the_original_deadline_and_detaches_cleanup() {
        let (project, _managed_parent, mut context, _candidate, _create) =
            context_with_worker().await;
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
        let task_path = crate::metrics::MetricsOutput::open(&path).unwrap();

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
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
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
        let (_project, _managed_parent, mut context, candidate, _retry) =
            context_with_worker().await;

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
        let (_project, _managed_parent, mut context, candidate, _retry) =
            context_with_worker().await;
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
                let BlockingEffect::Workspace(task) = task else {
                    unreachable!("prepared apply task was not a workspace task");
                };
                BlockingEffectCompletion::Workspace(Box::new(task.execute()))
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
        assert_eq!(warnings, Vec::new());
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert_eq!(metrics.unwrap().finish(0, 0).unwrap().workers, Vec::new());
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
        assert_eq!(warnings, Vec::new());
    }

    #[tokio::test]
    async fn shutdown_drain_expiry_accepts_buffered_workspace_and_process_metrics() {
        let (_project, _managed_parent, mut context, candidate, _retry) =
            context_with_worker().await;
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
            .send(ShellCompletion::Process {
                event: RunEvent::CancellationRequested,
                context: WorkerProcessContext::Mutant(7),
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion::Blocking(Ok(Box::new(blocking))))
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
        assert_eq!(warnings, Vec::new());
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert_eq!(metrics.unwrap().finish(1, 0).unwrap().workers, Vec::new());
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
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

    #[test]
    fn manifest_fingerprint_uses_snapshotted_hash_after_target_changes() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"original\n").unwrap();
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let targets = vec![TargetSlice {
            path: Utf8PathBuf::from("target.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let manifest = crate::workspace::build_validation_manifest(
            &config.selection.root,
            &CopyOptions::default(),
        )
        .unwrap();
        std::fs::write(project.path().join("target.py"), b"changed\n").unwrap();

        let actual = prepare_manifest_fingerprint(
            &config,
            &targets,
            ResourceMode::BestEffort,
            &manifest,
            EffectId(51),
        )
        .unwrap();
        let expected = fingerprint(&FingerprintInput::from_config(
            &config,
            vec![SourceHash {
                path: Utf8PathBuf::from("target.py"),
                hash: *blake3::hash(b"original\n").as_bytes(),
            }],
            targets,
            ResourceMode::BestEffort,
        ));

        assert_eq!(actual, expected);
    }

    #[test]
    fn manifest_fingerprint_uses_snapshotted_hash_after_target_is_removed() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"original\n").unwrap();
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let targets = vec![TargetSlice {
            path: Utf8PathBuf::from("target.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let manifest = crate::workspace::build_validation_manifest(
            &config.selection.root,
            &CopyOptions::default(),
        )
        .unwrap();
        std::fs::remove_file(project.path().join("target.py")).unwrap();

        let actual = prepare_manifest_fingerprint(
            &config,
            &targets,
            ResourceMode::BestEffort,
            &manifest,
            EffectId(52),
        )
        .unwrap();
        let expected = fingerprint(&FingerprintInput::from_config(
            &config,
            vec![SourceHash {
                path: Utf8PathBuf::from("target.py"),
                hash: *blake3::hash(b"original\n").as_bytes(),
            }],
            targets,
            ResourceMode::BestEffort,
        ));

        assert_eq!(actual, expected);
    }

    #[test]
    fn manifest_fingerprint_rejects_target_missing_from_manifest() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"original\n").unwrap();
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let targets = vec![TargetSlice {
            path: Utf8PathBuf::from("target.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let manifest = crate::workspace::build_validation_manifest(
            &config.selection.root,
            &CopyOptions {
                includes: Vec::new(),
                excludes: vec!["target.py".to_owned()],
                literal_exclusions: Vec::new(),
            },
        )
        .unwrap();

        let error = prepare_manifest_fingerprint(
            &config,
            &targets,
            ResourceMode::BestEffort,
            &manifest,
            EffectId(53),
        )
        .unwrap_err();

        assert_eq!(error.id, EffectId(53));
        assert_eq!(error.failure.code(), "fingerprint.source.manifest_missing");
        assert!(error.failure.message().contains("target.py"));
    }

    #[tokio::test]
    async fn shutdown_drain_failed_blocking_task_releases_its_slot_without_ownership() {
        let (sender, mut receiver) = mpsc::channel(1);
        let mut process_tasks = JoinSet::new();
        let mut io_tasks = JoinSet::new();
        spawn_blocking_effect(
            BlockingEffect::TestOperation {
                id: EffectId(24),
                operation: Box::new(|| panic!("controlled blocking task failure")),
            },
            sender,
            &mut io_tasks,
        );
        let mut in_flight = 1;
        let mut expiry_reported = false;
        let mut metrics = Some(MetricsCollector::new("run-1"));
        let mut warnings = Vec::new();
        let mut returned_ownership = Vec::new();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );

        let result = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            |completion| returned_ownership.push(completion),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(in_flight, 0);
        assert!(!expiry_reported);
        assert!(process_tasks.is_empty());
        assert!(io_tasks.is_empty());
        assert!(returned_ownership.is_empty());
        assert_eq!(warnings, Vec::new());
        assert_eq!(metrics.unwrap().finish(0, 0).unwrap().workers, Vec::new());
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
        let (completion_tx, mut completion_rx) = mpsc::channel(2);
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

        assert!(matches!(
            completion_rx.recv().await.unwrap(),
            ShellCompletion::Blocking(Ok(_))
        ));
        assert!(matches!(
            completion_rx.recv().await.unwrap(),
            ShellCompletion::Blocking(Ok(_))
        ));
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn original_change_during_materialization_fails_post_materialization_before_baseline() {
        let project = tempfile::tempdir().unwrap();
        let original = project.path().join("target.py");
        std::fs::write(&original, b"original\n").unwrap();
        let config = parse_host_independent_shell_test_config(vec![
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
        ]);
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
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();

        let exit = run_loop_with_control(
            config,
            stdout.clone(),
            stderr.clone(),
            control,
            crate::report::ReportPresentation::default(),
        )
        .await
        .unwrap();
        mutation.await.unwrap();

        assert_ne!(exit, 0);
        let stderr = String::from_utf8(stderr.snapshot()).unwrap();
        assert!(stderr.contains("workspace.original.changed"), "{stderr}");
        assert_eq!(
            observed_control.max_process_tasks(),
            0,
            "the baseline must not be dispatched after post-materialization verification fails"
        );
    }

    fn paused_materialization_config(
        project: &TempDir,
        total_timeout: &str,
    ) -> RunConfig<hoimin_core::EnvironmentRequest> {
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        parse_host_independent_shell_test_config(vec![
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
    }

    #[tokio::test(start_paused = true)]
    async fn total_timeout_bounds_paused_materialization_at_first_shutdown_deadline() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "200ms");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(0, Duration::from_millis(80));
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        // The blocking controller also prevents the paused clock from auto-advancing
        // while filesystem setup is still running.
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(5))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let started = tokio::time::Instant::now();
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
            crate::report::ReportPresentation::default(),
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        assert_eq!(started.elapsed(), Duration::ZERO);
        // Reach the run deadline only after the fixture is ready, then retain a
        // real-time bound on shutdown even while the blocking controller is live.
        tokio::time::advance(Duration::from_millis(200)).await;
        tokio::time::resume();
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
            crate::report::ReportPresentation::default(),
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
        let (control, mut pause_controller) = RunControl::with_materialization_pause(0);
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
            crate::report::ReportPresentation::default(),
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        cancelling.cancel();
        tokio::time::sleep(Duration::from_millis(30)).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();

        let exit = tokio::time::timeout(
            SHUTDOWN_GRACE.saturating_add(Duration::from_secs(1)),
            &mut run,
        )
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
        let serial_started = Instant::now();
        for sequence in 0..OPERATIONS {
            run_blocking_io(EffectId(u64::try_from(sequence).unwrap() + 1), || {
                std::thread::sleep(Duration::from_millis(OPERATION_MILLIS));
            })
            .await
            .unwrap();
        }
        let serial_millis = serial_started.elapsed().as_millis();

        let (completion_tx, mut completion_rx) = mpsc::channel(OPERATIONS);
        let mut tasks = JoinSet::new();
        let concurrent_started = Instant::now();
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
            .handle_preflight_validated(Preflight { id }, |root, manifest| {
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
            .handle_preflight_validated(Preflight { id }, |root, manifest| {
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
            .handle_preflight_validated(Preflight { id }, |root, manifest| {
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
            .handle_preflight_validated(Preflight { id }, |root, manifest| {
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
            .handle_preflight_validated(Preflight { id }, |root, manifest| {
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
        assert_eq!(warnings, Vec::new());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert_eq!(metrics.workers, Vec::new());
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
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let (state, pending_effects) = transition(
            RunState::new(
                "run-1",
                config,
                hoimin_core::ResourceControl {
                    mode: hoimin_core::ResourceMode::Hard,
                    mechanism: "test_supplied_hard".into(),
                },
            ),
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

        cancel_queued_worker(7, &mut metrics, &mut warnings);

        assert_metrics_sidecar_finishes(metrics, &warnings);
    }

    #[test]
    fn ready_process_effects_are_recorded_as_queued() {
        let effects = [process_effect(3)];
        let mut metrics = Some(MetricsCollector::new("run-1"));
        let mut warnings = Vec::new();

        record_ready_processes(&effects, &mut metrics, &mut warnings);
        cancel_queued_effect(&effects[0], &mut metrics, &mut warnings);

        assert_eq!(warnings, Vec::new());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert_eq!(metrics.workers, Vec::new());
    }

    #[tokio::test]
    async fn drain_closes_mutant_metrics_and_ignores_baseline() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        for worker in [0, 1] {
            metrics.as_mut().unwrap().queued(worker).unwrap();
            metrics.as_mut().unwrap().process_started(worker).unwrap();
        }
        let mut warnings = Vec::new();
        let mut tasks = JoinSet::new();
        let (sender, mut receiver) = mpsc::channel(3);
        sender
            .send(ShellCompletion::Process {
                event: RunEvent::EffectFailed(EffectFailed::other(
                    EffectId(1),
                    "process.spawn",
                    "failed",
                )),
                context: WorkerProcessContext::Mutant(0),
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion::Process {
                event: RunEvent::CancellationRequested,
                context: WorkerProcessContext::Mutant(1),
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion::Process {
                event: RunEvent::CancellationRequested,
                context: WorkerProcessContext::Baseline(7),
            })
            .await
            .unwrap();
        drop(sender);
        let mut in_flight = 3;
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
        assert_eq!(warnings, Vec::new());
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

        assert_eq!(warnings, Vec::new());
        metrics
            .as_mut()
            .unwrap()
            .finish_stage("copy")
            .expect("post-materialization verification remains part of the copy stage");
    }

    macro_rules! direct_early_cleanup_stage_case {
        ($name:ident, $phase:expr, $stage:literal) => {
            #[test]
            fn $name() {
                // Arrange.
                let mut collector = MetricsCollector::new("run-1");
                collector.begin_stage($stage).unwrap();
                let mut metrics = Some(collector);
                let mut warnings = Vec::new();
                // Act.
                observe_accepted_transition(
                    &mut metrics,
                    &mut warnings,
                    $phase,
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
                // Assert.
                assert_eq!(warnings, Vec::new());
                let metrics = metrics.unwrap().finish(0, 0).unwrap();
                assert!(metrics.stages.iter().any(|metric| metric.name == $stage));
                assert!(metrics.stages.iter().any(|metric| metric.name == "cleanup"));
            }
        };
    }

    direct_early_cleanup_stage_case!(
        direct_early_cleanup_finishes_baseline,
        RunPhase::Baseline,
        "baseline"
    );
    direct_early_cleanup_stage_case!(
        direct_early_cleanup_finishes_baseline_restoration,
        RunPhase::BaselineRestoration,
        "baseline"
    );
    direct_early_cleanup_stage_case!(
        direct_early_cleanup_finishes_analysis,
        RunPhase::Analyze,
        "analysis"
    );

    #[test]
    fn baseline_restoration_keeps_baseline_metrics_stage_open() {
        // Arrange.
        let mut collector = MetricsCollector::new("run-1");
        collector.begin_stage("baseline").unwrap();
        let mut metrics = Some(collector);
        let mut warnings = Vec::new();
        // Act.
        observe_accepted_transition(
            &mut metrics,
            &mut warnings,
            RunPhase::Baseline,
            RunPhase::BaselineRestoration,
            false,
            false,
            false,
        );
        // Assert.
        assert_eq!(warnings, Vec::new());
        metrics
            .as_mut()
            .unwrap()
            .finish_stage("baseline")
            .expect("restoration remains in the baseline stage");
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

        assert_eq!(warnings, Vec::new());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert_eq!(metrics.stages.len(), 1);
        assert_eq!(metrics.stages[0].name, "cleanup");
    }

    fn dispatch_test_arg(value: &std::ffi::OsStr) -> CommandArg {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            CommandArg::Unix(value.as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            CommandArg::Windows(value.encode_wide().collect())
        }
    }

    fn worker_dispatch_requests(
        argv: Vec<CommandArg>,
    ) -> [(PreparedWorkerProcess, WorkerProcessContext); 2] {
        fn request<C: WorkspaceProcessContext>(
            context: C,
            argv: Vec<CommandArg>,
        ) -> PreparedWorkerProcess
        where
            ProcessRequest: From<RunProcess<C>>,
        {
            RunProcess {
                id: EffectId(9),
                context,
                argv,
                cwd: Utf8PathBuf::from("."),
                limits: hoimin_core::ProcessLimits {
                    timeout: Duration::from_secs(5),
                    max_output_bytes: 4096,
                    max_memory_bytes: 512 * 1024 * 1024,
                    max_processes: 16,
                },
            }
            .into()
        }
        [
            (
                request(
                    hoimin_core::BaselineContext {
                        worker: 4,
                        run_id: "run-1".into(),
                    },
                    argv.clone(),
                ),
                WorkerProcessContext::Baseline(4),
            ),
            (
                request(
                    hoimin_core::MutantContext {
                        worker: 7,
                        run_id: "run-1".into(),
                        mutant_id: "mutant-1".into(),
                    },
                    argv,
                ),
                WorkerProcessContext::Mutant(7),
            ),
        ]
    }

    async fn dispatch_test_process(request: PreparedWorkerProcess) -> ShellCompletion {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let process = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        ));
        let control = RunControl::new();
        let dispatch = control.begin_dispatch().unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let mut tasks = JoinSet::new();

        spawn_process(process, request, sender, &mut tasks, dispatch);
        let completion = tokio::time::timeout(Duration::from_secs(10), receiver.recv())
            .await
            .expect("spawned process must complete")
            .expect("completion channel must remain open");
        tasks.join_next().await.unwrap().unwrap();
        completion
    }

    #[tokio::test]
    async fn spawned_worker_success_uses_request_context() {
        let argv = vec![
            dispatch_test_arg(std::env::current_exe().unwrap().as_os_str()),
            dispatch_test_arg(std::ffi::OsStr::new("--exact")),
            dispatch_test_arg(std::ffi::OsStr::new("hoimin_nonexistent_dispatch_fixture")),
        ];
        for (request, expected) in worker_dispatch_requests(argv) {
            let completion = dispatch_test_process(request).await;

            let ShellCompletion::Process { event, context } = completion else {
                panic!("worker dispatch must produce a process completion");
            };
            let event_context = match event {
                RunEvent::BaselineFinished(value) => {
                    WorkerProcessContext::Baseline(value.worker.unwrap())
                }
                RunEvent::MutantFinished(value) => {
                    WorkerProcessContext::Mutant(value.worker.unwrap())
                }
                other => panic!("expected successful process completion, got {other:?}"),
            };
            assert_eq!((context, event_context), (expected, expected));
        }
    }

    #[tokio::test]
    async fn spawned_worker_failure_retains_request_context() {
        for (request, expected) in worker_dispatch_requests(vec![missing_executable_arg()]) {
            let completion = dispatch_test_process(request).await;

            let ShellCompletion::Process {
                event: RunEvent::EffectFailed(error),
                context,
            } = completion
            else {
                panic!("missing executable must produce a failed process completion");
            };
            assert_eq!(error.id, EffectId(9));
            assert_eq!(context, expected);
        }
    }

    fn running_dispatch_metrics(context: WorkerProcessContext) -> MetricsCollector {
        let mut collector = MetricsCollector::new("run-1");
        if let WorkerProcessContext::Mutant(worker) = context {
            collector.queued(worker).unwrap();
            collector.process_started(worker).unwrap();
        }
        collector
    }

    #[tokio::test]
    async fn drained_worker_failure_closes_only_mutant_metrics() {
        for (request, expected) in worker_dispatch_requests(vec![missing_executable_arg()]) {
            let mut metrics = Some(running_dispatch_metrics(expected));
            let mut warnings = Vec::new();
            let mut in_flight = 1;
            let completion = dispatch_test_process(request).await;

            accept_drained_completion(
                completion,
                &mut in_flight,
                &mut metrics,
                &mut warnings,
                &mut drop,
            );

            assert_eq!(warnings, Vec::new());
            assert_eq!(in_flight, 0);
            let result = metrics.unwrap().finish(0, 0).unwrap();
            assert_eq!(result.workers, Vec::new());
        }
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

        set_worker_metadata(
            &mut environment,
            &hoimin_core::BaselineContext {
                worker: 0,
                run_id: "run-1".into(),
            },
        );

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

    fn mixed_worker_completions(
        cases: &[(bool, u8)],
    ) -> (Vec<ShellCompletion>, Option<MetricsCollector>) {
        let mut collector = MetricsCollector::new("run-1");
        let mut completions = Vec::new();
        for (worker, &(mutant, rank)) in cases.iter().enumerate() {
            let worker = u32::try_from(worker).unwrap();
            let context = if mutant {
                collector.queued(worker).unwrap();
                collector.process_started(worker).unwrap();
                WorkerProcessContext::Mutant(worker)
            } else {
                WorkerProcessContext::Baseline(worker)
            };
            completions.push((
                rank,
                ShellCompletion::Process {
                    event: RunEvent::EffectFailed(EffectFailed::other(
                        EffectId(u64::from(worker)),
                        "process.spawn",
                        "failed",
                    )),
                    context,
                },
            ));
        }
        completions.sort_by_key(|(rank, _)| *rank);
        (
            completions
                .into_iter()
                .map(|(_, completion)| completion)
                .collect(),
            Some(collector),
        )
    }

    proptest::proptest! {
        #[test]
        fn draining_mixed_worker_completions_accounts_each_once(
            cases in proptest::collection::vec((proptest::bool::ANY, proptest::arbitrary::any::<u8>()), 0..32),
        ) {
            let (completions, mut metrics) = mixed_worker_completions(&cases);
            let mut in_flight = completions.len();
            let mut warnings = Vec::new();
            let mut remaining = Vec::new();
            let expected_remaining: Vec<_> = (0..completions.len()).rev().collect();

            for completion in completions {
                accept_drained_completion(completion, &mut in_flight, &mut metrics, &mut warnings, &mut drop);
                remaining.push(in_flight);
            }
            let result = metrics.unwrap().finish(0, 0);

            proptest::prop_assert_eq!(remaining, expected_remaining);
            proptest::prop_assert!(warnings.is_empty());
            proptest::prop_assert!(result.is_ok(), "outstanding metric after drain: {result:?}");
            proptest::prop_assert!(result.unwrap().workers.is_empty());
        }


        #[test]
        fn baseline_metadata_preserves_required_run_id(run_id in proptest::arbitrary::any::<String>()) {
            // Arrange.
            let mut environment = crate::workspace::CommandEnvironment {
                cwd: "worker/root".into(), env: BTreeMap::new(),
            };
            let context = hoimin_core::BaselineContext { worker: 0, run_id: run_id.clone() };
            // Act.
            set_worker_metadata(&mut environment, &context);
            // Assert.
            proptest::prop_assert_eq!(environment.env.get(&OsString::from("HOIMIN_RUN_ID")), Some(&OsString::from(run_id)));
        }

        #[test]
        fn mutant_metadata_preserves_required_mutant_id(mutant_id in proptest::arbitrary::any::<String>()) {
            // Arrange.
            let mut environment = crate::workspace::CommandEnvironment {
                cwd: "worker/root".into(), env: BTreeMap::new(),
            };
            let context = hoimin_core::MutantContext { worker: 0, run_id: "run".into(), mutant_id: mutant_id.clone() };
            // Act.
            set_worker_metadata(&mut environment, &context);
            // Assert.
            proptest::prop_assert_eq!(environment.env.get(&OsString::from("HOIMIN_MUTANT_ID")), Some(&OsString::from(mutant_id)));
        }
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
