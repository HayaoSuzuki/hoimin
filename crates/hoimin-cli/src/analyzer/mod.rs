//! Runs the in-process Rust analyzer and validates its typed candidates before
//! storing them for replay. Analyzer results do not use a JSONL transport.
//!
//! # Handler session contract
//!
//! A session is the serial sequence of requests contributing to one candidate
//! spool. The handler owns its root, operators, profile, and positive candidate
//! limit, fixed at construction. Every file uses the remaining capacity of that
//! same limit. Different settings require a new handler.
//!
//! Accepted candidates have contiguous, one-based sequences across requests.
//! A successful non-final, non-truncated request returns `spool: None`: candidates
//! remain in the handler, even when that request adds no candidates. Success with
//! `final_target` or truncation finalizes the entire accumulated spool (including
//! an empty spool), then removes the store from the handler. Truncation can end a
//! session before the final target. A subsequent request starts a fresh store and
//! sequence with the same fixed settings; it does not append to a returned spool.
//! Diagnostics belong to each request's result rather than to the accumulated spool.
//!
//! # Interruption and ownership
//!
//! Store creation and source reading precede transfer of the store to blocking
//! analysis. Cancellation observed before that transfer, or a source-read error,
//! leaves an initialized store and its accumulated candidates in the handler.
//! Store-creation failure leaves no store. After transfer, cancellation, analysis
//! or store errors, and task-join failure leave the handler without that store;
//! only a successful non-final, non-truncated result restores it. Reusing the
//! handler then starts an empty session, not a retry or resume of earlier files.
//! If the interruption stage is unknown, abandon the session rather than assume
//! accumulated candidates survived.
//!
//! Dropping a polled handler future does not request cancellation or join its
//! blocking task. Before transfer, the store stays in the handler; after transfer,
//! the result cannot restore it. Explicit cancellation is cooperative and can
//! return before blocking work exits. The worker retains its store and a cloned
//! spool-directory owner, when configured, until the worker exits. Neither a
//! dropped future nor a cancellation result proves that analysis has stopped or
//! that files are safe to remove.
//!
//! An unfinished store owns its temporary file. Finalization preserves the file
//! and returns a `CandidateSpoolRef`, which is a reference, not a cleanup guard.
//! The caller owns cleanup of a finalized spool, including arranging directory
//! ownership if a detached worker can finalize after its result is abandoned.
//! The shell supplies a managed directory owner and coordinates cleanup; dropping
//! the handler only releases its own store/owner references and does not delete a
//! finalized standalone spool. See the [shell coordination contract](crate::shell)
//! and the [development guide](https://github.com/HayaoSuzuki/hoimin/blob/main/docs/development/guides/contributing.md#internal-ownership-and-performance-changes)
//! for the surrounding ownership boundaries and analysis documentation.

pub(crate) mod exception_project;
use exception_project::ExceptionProject;
mod store;

#[cfg(test)]
mod payload_probes;
#[cfg(test)]
mod payload_tests;

use hoimin_python::payload::{Charged, PayloadBudget};

use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use hoimin_python::{AnalyzerCandidate, AnalyzerDiagnostic, AnalyzerDiagnosticCode};
pub use store::*;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    AnalysisDiagnostic as RunAnalysisDiagnostic, AnalysisFinished, AnalyzeFile,
    CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateValidationContext, EffectFailed,
    EffectId, MutationCandidate, MutationOperatorSelection, MutationProfile, TargetSlice,
    validate_candidate_with_context,
};

use crate::process::ProcessCancellation;
use crate::source_file::PortableFileReader;
use crate::workspace::ManagedChild;
#[cfg(test)]
use tempfile::TempDir;

#[derive(Clone)]
enum CandidateSpoolOwner {
    #[cfg(test)]
    Temporary(Arc<TempDir>),
    Managed(Arc<ManagedChild>),
}

impl CandidateSpoolOwner {
    fn path(&self) -> &std::path::Path {
        match self {
            #[cfg(test)]
            Self::Temporary(owner) => owner.path(),
            Self::Managed(owner) => owner.path().as_std_path(),
        }
    }
}

/// Accumulates validated candidates across serial requests until finalization.
///
/// See the [module contract](self) for session limits, interruption state, and
/// spool cleanup responsibilities. Use one handler for one active session.
pub struct AnalyzerHandler {
    payload: PayloadBudget,
    root_path: Utf8PathBuf,
    operators: MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: NonZeroUsize,
    exception_project: Option<ExceptionProject>,
    root: Option<PortableFileReader>,
    store: Option<CandidateStore>,
    candidate_spool_owner: Option<CandidateSpoolOwner>,
    #[cfg(test)]
    analysis_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    checkpoint_hook: Option<Arc<dyn Fn(AnalysisCheckpoint) + Send + Sync>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
    pub candidates: Vec<MutationCandidate>,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub truncated: bool,
}

struct DiscoveryWork {
    payload: PayloadBudget,
    exception_project: Option<ExceptionProject>,
    root: Utf8PathBuf,
    targets: Vec<TargetSlice>,
    operators: MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    cancellation: ProcessCancellation,
    #[cfg(test)]
    control: Option<DiscoveryControl>,
}

#[cfg(test)]
pub(crate) struct DiscoveryControl {
    _owner: Arc<TempDir>,
    before_analysis: Box<dyn FnMut() + Send>,
    deadline_arm: Option<tokio::sync::oneshot::Sender<tokio::time::Instant>>,
    before_entry: Option<Box<dyn FnOnce() + Send>>,
    checkpoint_hook: Option<Box<dyn FnMut(DiscoveryCheckpoint) + Send>>,
}

#[cfg(test)]
impl DiscoveryControl {
    pub(crate) fn new(owner: Arc<TempDir>, before_analysis: impl FnMut() + Send + 'static) -> Self {
        Self {
            _owner: owner,
            before_analysis: Box::new(before_analysis),
            deadline_arm: None,
            before_entry: None,
            checkpoint_hook: None,
        }
    }
}

/// # Errors
///
/// Returns an error when a source cannot be read or decoded, or a generated candidate is invalid.
pub async fn discover_targets(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
) -> Result<Discovery, EffectFailed> {
    discover_unbounded(
        root,
        targets,
        operators,
        profile,
        max_candidates,
        #[cfg(test)]
        None,
    )
    .await
}

async fn discover_unbounded(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<Discovery, EffectFailed> {
    let cancellation = ProcessCancellation::new();
    let work = discovery_work(
        root,
        targets,
        operators,
        profile,
        max_candidates,
        cancellation,
        #[cfg(test)]
        control,
    );
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(|| discover_targets_blocking(work)))
        .await
        .map_err(|error| EffectFailed::other(EffectId(0), "analyzer.task", error.to_string()))?
}

/// Discovers targets within one timeout covering the complete discovery operation.
///
/// # Errors
///
/// Returns `analyzer.timeout` when the deadline expires, or an analyzer error when discovery
/// cannot complete successfully.
#[cfg(test)]
pub(crate) async fn discover_targets_with_timeout(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    analyzer_timeout: Duration,
) -> Result<Discovery, EffectFailed> {
    discover_targets_inner(
        None,
        root,
        targets,
        operators,
        profile,
        max_candidates,
        analyzer_timeout,
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(test)]
pub(crate) async fn discover_targets_with_control(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    analyzer_timeout: Duration,
    control: Option<DiscoveryControl>,
) -> Result<Discovery, EffectFailed> {
    discover_targets_inner(
        None,
        root,
        targets,
        operators,
        profile,
        max_candidates,
        analyzer_timeout,
        control,
    )
    .await
}

pub(crate) async fn discover_config_targets(
    config: &hoimin_core::RunConfig,
    discovery: &hoimin_core::DiscoveryConfig,
    targets: &[TargetSlice],
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<Discovery, EffectFailed> {
    discover_targets_inner(
        ExceptionProject::from_config(config),
        &config.selection.root,
        targets,
        &config.operators,
        config.profile,
        config.limits.max_candidates.get(),
        discovery.analyzer_timeout.get(),
        #[cfg(test)]
        control,
    )
    .await
}

#[tracing::instrument(name = "discover_candidates", level = "debug", skip_all, fields(files = targets.len(), max_candidates))]
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "test deadline control and inline invariant guards supplement shared discovery parameters"
)]
async fn discover_targets_inner(
    exception_project: Option<ExceptionProject>,
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    analyzer_timeout: Duration,
    #[cfg(test)] mut control: Option<DiscoveryControl>,
) -> Result<Discovery, EffectFailed> {
    let cancellation = ProcessCancellation::new();
    let task_cancellation = cancellation.clone();
    #[cfg(test)]
    let (initial_deadline, deadline_arm) = if let Some(control) = control.as_mut() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        control.deadline_arm = Some(sender);
        (None, Some(receiver))
    } else {
        (
            Some(discovery_deadline(
                tokio::time::Instant::now(),
                analyzer_timeout,
            )?),
            None,
        )
    };
    #[cfg(not(test))]
    let deadline = discovery_deadline(tokio::time::Instant::now(), analyzer_timeout)?;
    let mut work = discovery_work(
        root,
        targets,
        operators,
        profile,
        max_candidates,
        task_cancellation,
        #[cfg(test)]
        control,
    );
    if exception_project.is_some() {
        work.exception_project = exception_project;
    }
    let span = tracing::Span::current();
    let mut task = tokio::task::spawn_blocking(move || {
        let _entered = span.enter();
        let discovery = discover_targets_blocking(work);
        (tokio::time::Instant::now(), discovery)
    });
    #[cfg(test)]
    let deadline = if let Some(deadline_arm) = deadline_arm {
        let armed_at = match tokio::time::timeout(Duration::from_secs(2), deadline_arm).await {
            Ok(Ok(armed_at)) => armed_at,
            Ok(Err(_)) => {
                let Ok(outcome) = tokio::time::timeout(Duration::from_secs(2), &mut task).await
                else {
                    return {
                        // This cfg(test) watchdog fails only if an already-ended controlled worker or scheduler fails to complete.
                        #[cfg(coverage_nightly)]
                        {
                            (#[coverage(off)]
                            || {
                                Err(EffectFailed::other(
                                    EffectId(0),
                                    "analyzer.test_control",
                                    "controlled discovery did not finish within 2s after ending before entry",
                                ))
                            })()
                        }
                        #[cfg(not(coverage_nightly))]
                        {
                            Err(EffectFailed::other(
                                EffectId(0),
                                "analyzer.test_control",
                                "controlled discovery did not finish within 2s after ending before entry",
                            ))
                        }
                    };
                };
                return outcome
                    .map_err(|error| {
                        EffectFailed::other(EffectId(0), "analyzer.task", error.to_string())
                    })?
                    .1;
            }
            Err(_) => {
                cancellation.cancel();
                return Err(EffectFailed::other(
                    EffectId(0),
                    "analyzer.test_control",
                    "controlled discovery worker did not enter within 2s",
                ));
            }
        };
        match discovery_deadline(armed_at, analyzer_timeout) {
            Ok(deadline) => deadline,
            Err(error) => {
                cancellation.cancel();
                return Err(error);
            }
        }
    } else {
        initial_deadline.expect("uncontrolled discovery has an initial deadline")
    };
    tokio::select! {
        biased;
        result = &mut task => {
            let (finished_at, discovery) = result.map_err(|error| {
                EffectFailed::other(EffectId(0), "analyzer.task", error.to_string())
            })?;
            if finished_at <= deadline {
                discovery
            } else {
                cancellation.cancel();
                Err(discovery_timeout(analyzer_timeout))
            }
        },
        () = tokio::time::sleep_until(deadline) => {
            cancellation.cancel();
            Err(discovery_timeout(analyzer_timeout))
        }
    }
}

fn discovery_deadline(
    started_at: tokio::time::Instant,
    analyzer_timeout: Duration,
) -> Result<tokio::time::Instant, EffectFailed> {
    started_at
        .checked_add(analyzer_timeout)
        .ok_or_else(|| discovery_timeout_out_of_range(analyzer_timeout))
}

fn discovery_timeout_out_of_range(analyzer_timeout: Duration) -> EffectFailed {
    EffectFailed::other(
        EffectId(0),
        "analyzer.timeout",
        format!(
            "--analyzer-timeout duration {} is outside the supported deadline range",
            humantime::format_duration(analyzer_timeout)
        ),
    )
}

fn discovery_timeout(analyzer_timeout: Duration) -> EffectFailed {
    EffectFailed::other(
        EffectId(0),
        "analyzer.timeout",
        format!(
            "--analyzer-timeout expired after {}",
            humantime::format_duration(analyzer_timeout)
        ),
    )
}

fn discovery_work(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    cancellation: ProcessCancellation,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> DiscoveryWork {
    DiscoveryWork {
        payload: PayloadBudget::default(),
        exception_project: operators
            .contains(hoimin_core::MutationOperator::ExceptionHierarchy)
            .then(|| ExceptionProject::new(root)),
        root: root.to_owned(),
        targets: targets.to_vec(),
        operators: operators.clone(),
        profile,
        max_candidates,
        cancellation,
        #[cfg(test)]
        control,
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "inline invariant guards and test-only checkpoints preserve the established operation order"
)]
fn discover_targets_blocking(work: DiscoveryWork) -> Result<Discovery, EffectFailed> {
    let DiscoveryWork {
        payload,
        exception_project,
        root,
        targets,
        operators,
        profile,
        max_candidates,
        cancellation,
        #[cfg(test)]
        mut control,
    } = work;
    #[cfg(test)]
    if let Some(before_entry) = control
        .as_mut()
        .and_then(|control| control.before_entry.take())
    {
        before_entry();
    }
    ensure_discovery_active(&cancellation)?;
    let root = PortableFileReader::open(&root).map_err(|error| {
        EffectFailed::other(EffectId(0), "analyzer.source.read", error.to_string())
    })?;
    let exceptions = exception_project
        .as_ref()
        .map(|p| p.load(&|| cancellation.is_cancelled()))
        .transpose()
        .map_err(|e| EffectFailed::other(EffectId(0), "analyzer.exception_hierarchy", e))?;
    let mut candidates = Vec::new();
    let mut discovery = Discovery {
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        truncated: false,
    };
    for target in &targets {
        #[cfg(test)]
        discovery_checkpoint(&mut control, DiscoveryCheckpoint::BeforeSource);
        ensure_discovery_active(&cancellation)?;
        let source = root.read(&target.path).map_err(|error| {
            EffectFailed::other(EffectId(0), "analyzer.source.read", error.to_string())
        })?;
        #[cfg(test)]
        discovery_checkpoint(&mut control, DiscoveryCheckpoint::AfterSource);
        ensure_discovery_active(&cancellation)?;
        let validation = source_validation(&source, &target.path, EffectId(0))?;
        let module = validation
            .decoded_source()
            .expect("source was decoded")
            .text();
        #[cfg(test)]
        if let Some(control) = control.as_mut() {
            if let Some(deadline_arm) = control.deadline_arm.take() {
                let _ = deadline_arm.send(tokio::time::Instant::now());
            }
            (control.before_analysis)();
        }
        let output = hoimin_python::analyze_source_with_exceptions(
            &hoimin_python::AnalyzeRequest {
                payload: payload.clone(),
                path: &target.path,
                lines: &target.lines,
                symbols: &target.symbols,
                operators: &operators,
                profile,
                max_candidates: max_candidates.saturating_sub(candidates.len()),
            },
            module,
            &|| cancellation.is_cancelled(),
            exceptions.as_deref(),
        )
        .map_err(|error| analysis_failure(EffectId(0), &target.path, error))?;
        for candidate in output.candidates {
            #[cfg(test)]
            discovery_checkpoint(&mut control, DiscoveryCheckpoint::BeforeCandidate);
            ensure_discovery_active(&cancellation)?;
            let Some(sequence) = u64::try_from(candidates.len())
                .ok()
                .and_then(|count| count.checked_add(1))
            else {
                return {
                    // A non-ZST Vec length is bounded by isize::MAX, so converting and adding one fits u64 on supported 32/64-bit targets.
                    #[cfg(all(
                        coverage_nightly,
                        any(target_pointer_width = "32", target_pointer_width = "64")
                    ))]
                    {
                        (#[coverage(off)]
                        || {
                            Err(EffectFailed::other(
                                EffectId(0),
                                "analyzer.candidate",
                                "candidate sequence overflow",
                            ))
                        })()
                    }
                    #[cfg(not(all(
                        coverage_nightly,
                        any(target_pointer_width = "32", target_pointer_width = "64")
                    )))]
                    {
                        Err(EffectFailed::other(
                            EffectId(0),
                            "analyzer.candidate",
                            "candidate sequence overflow",
                        ))
                    }
                };
            };
            let candidate = mutation_candidate(EffectId(0), &validation, candidate, sequence)?;
            CandidateStore::record_size(&candidate)
                .map_err(|error| candidate_store_error(EffectId(0), &candidate, &error))?;
            candidates.push(candidate);
        }
        discovery.diagnostics.extend(output.diagnostics);
        if output.truncated {
            discovery.truncated = true;
            break;
        }
    }
    // All producers and conversion are finished. This owner is never reused.
    discovery.candidates = candidates
        .into_iter()
        .map(Charged::into_untracked)
        .collect();
    Ok(discovery)
}

#[cfg(test)]
fn discovery_checkpoint(control: &mut Option<DiscoveryControl>, checkpoint: DiscoveryCheckpoint) {
    if let Some(hook) = control
        .as_mut()
        .and_then(|control| control.checkpoint_hook.as_mut())
    {
        hook(checkpoint);
    }
}

fn ensure_discovery_active(cancellation: &ProcessCancellation) -> Result<(), EffectFailed> {
    if cancellation.is_cancelled() {
        Err(discovery_cancelled())
    } else {
        Ok(())
    }
}

fn discovery_cancelled() -> EffectFailed {
    EffectFailed::other(EffectId(0), "analyzer.cancelled", "analyzer was cancelled")
}

impl AnalyzerHandler {
    /// Fixes the settings for every session without project I/O or store allocation.
    ///
    /// Source access and candidate-store creation are deferred until analysis; failures
    /// are reported by the operation that performs them.
    #[must_use]
    pub fn new(
        root: Utf8PathBuf,
        operators: MutationOperatorSelection,
        profile: MutationProfile,
        max_candidates: NonZeroUsize,
    ) -> Self {
        Self {
            payload: PayloadBudget::default(),
            exception_project: None,
            root_path: root,
            operators,
            profile,
            max_candidates,
            root: None,
            store: None,
            candidate_spool_owner: None,
            #[cfg(test)]
            analysis_hook: None,
            #[cfg(test)]
            checkpoint_hook: None,
        }
    }

    pub(crate) fn with_exception_project(mut self, config: &hoimin_core::RunConfig) -> Self {
        self.exception_project = ExceptionProject::from_config(config);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_candidate_spool_owner(mut self, owner: Arc<TempDir>) -> Self {
        self.candidate_spool_owner = Some(CandidateSpoolOwner::Temporary(owner));
        self
    }

    pub(crate) fn with_managed_candidate_spool_owner(mut self, owner: Arc<ManagedChild>) -> Self {
        self.candidate_spool_owner = Some(CandidateSpoolOwner::Managed(owner));
        self
    }

    pub(crate) fn release_candidate_spool(&mut self) {
        self.store = None;
        self.candidate_spool_owner = None;
    }

    /// Analyzes one target and either retains candidates or finalizes the session spool.
    ///
    /// Uses the constructor settings. `spool: None` means accumulation continues;
    /// `request.final_target` or truncation returns the full spool and ends it.
    /// See the [module contract](self) before retrying after an interruption or
    /// dropping this future: blocking work and spool cleanup have separate lifetimes.
    ///
    /// # Errors
    ///
    /// Returns an error when analysis is cancelled, the source cannot be read or decoded, a
    /// generated candidate is invalid, or the candidate store fails.
    pub async fn handle(&mut self, request: AnalyzeFile) -> Result<AnalysisFinished, EffectFailed> {
        self.handle_with_cancellation(request, ProcessCancellation::new())
            .await
    }

    #[tracing::instrument(name = "analyze_file", level = "debug", skip_all, fields(effect_id = request.id.0, path = ?request.target.path))]
    pub(crate) async fn handle_with_cancellation(
        &mut self,
        request: AnalyzeFile,
        cancellation: ProcessCancellation,
    ) -> Result<AnalysisFinished, EffectFailed> {
        let id = request.id;
        let candidate_limit = self.max_candidates.get() as u64;
        if self.store.is_none() {
            let store = self.candidate_spool_owner.as_ref().map_or_else(
                || CandidateStore::new(candidate_limit),
                |owner| CandidateStore::new_in(candidate_limit, owner.path()),
            );
            self.store = Some(
                store
                    .map(|store| store.with_payload(self.payload.clone()))
                    .map_err(|error| {
                        EffectFailed::other(id, "analyzer.store", error.to_string())
                    })?,
            );
        }
        let source = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                return Err(EffectFailed::other(id, "analyzer.cancelled", "analyzer was cancelled"));
            }
            result = async { self.read_source(&request.target.path) } => result
                .map_err(|error| EffectFailed::other(id, "analyzer.source.read", error.to_string()))?,
        };
        if self.exception_project.is_none()
            && self
                .operators
                .contains(hoimin_core::MutationOperator::ExceptionHierarchy)
        {
            self.exception_project = Some(ExceptionProject::new(&self.root_path));
        }
        let exception_project = self.exception_project.clone();
        let store = self.store.take().expect("store initialized");
        let remaining = candidate_limit.saturating_sub(store.count());
        let max_candidates = usize::try_from(remaining).unwrap_or(usize::MAX);
        let operators = self.operators.clone();
        let profile = self.profile;
        let payload = self.payload.clone();
        let task_cancellation = cancellation.clone();
        #[cfg(test)]
        let analysis_hook = self.analysis_hook.clone();
        #[cfg(test)]
        let checkpoint_hook = self.checkpoint_hook.clone();
        let candidate_spool_owner = self.candidate_spool_owner.clone();
        let span = tracing::Span::current();
        let task = tokio::task::spawn_blocking(move || {
            let _entered = span.enter();
            tracing::debug!("analyzing file");
            let _candidate_spool_owner = candidate_spool_owner;
            #[cfg(test)]
            if let Some(hook) = analysis_hook {
                hook();
            }
            analyze_and_store(BlockingAnalysis {
                payload,
                exception_project,
                request,
                operators,
                profile,
                source,
                store,
                max_candidates,
                cancellation: task_cancellation,
                #[cfg(test)]
                checkpoint_hook,
            })
        });
        tokio::pin!(task);
        let result = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                return Err(cancelled(id));
            }
            result = &mut task => result.map_err(|error| {
                EffectFailed::other(id, "analyzer.task", error.to_string())
            })?,
        }?;
        self.store = result.1;
        Ok(result.0)
    }

    fn read_source(
        &mut self,
        path: &Utf8Path,
    ) -> Result<Vec<u8>, crate::source_file::PortableFileReadError> {
        if self.root.is_none() {
            self.root = Some(
                PortableFileReader::open(&self.root_path)
                    .map_err(crate::source_file::PortableFileReadError::Other)?,
            );
        }
        self.root
            .as_ref()
            .expect("root reader initialized")
            .read(path)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiscoveryCheckpoint {
    BeforeSource,
    AfterSource,
    BeforeCandidate,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AnalysisCheckpoint {
    AfterAnalysis,
    BeforeFinalization,
}

struct BlockingAnalysis {
    payload: PayloadBudget,
    exception_project: Option<ExceptionProject>,
    request: AnalyzeFile,
    operators: MutationOperatorSelection,
    profile: MutationProfile,
    source: Vec<u8>,
    store: CandidateStore,
    max_candidates: usize,
    cancellation: ProcessCancellation,
    #[cfg(test)]
    checkpoint_hook: Option<Arc<dyn Fn(AnalysisCheckpoint) + Send + Sync>>,
}

#[allow(
    clippy::too_many_lines,
    reason = "inline invariant guards and test-only checkpoints preserve the established operation order"
)]
fn analyze_and_store(
    work: BlockingAnalysis,
) -> Result<(AnalysisFinished, Option<CandidateStore>), EffectFailed> {
    let BlockingAnalysis {
        payload,
        exception_project,
        request,
        operators,
        profile,
        source,
        mut store,
        max_candidates,
        cancellation,
        #[cfg(test)]
        checkpoint_hook,
    } = work;
    let id = request.id;
    let validation = source_validation(&source, &request.target.path, id)?;
    let module = validation
        .decoded_source()
        .expect("source was decoded")
        .text();
    let exceptions = exception_project
        .as_ref()
        .map(|p| p.load(&|| cancellation.is_cancelled()))
        .transpose()
        .map_err(|e| EffectFailed::other(id, "analyzer.exception_hierarchy", e))?;
    let output = hoimin_python::analyze_source_with_exceptions(
        &hoimin_python::AnalyzeRequest {
            payload: payload.clone(),
            path: &request.target.path,
            lines: &request.target.lines,
            symbols: &request.target.symbols,
            operators: &operators,
            profile,
            max_candidates,
        },
        module,
        &|| cancellation.is_cancelled(),
        exceptions.as_deref(),
    )
    .map_err(|error| analysis_failure(id, &request.target.path, error))?;
    #[cfg(test)]
    if let Some(hook) = checkpoint_hook.as_ref() {
        hook(AnalysisCheckpoint::AfterAnalysis);
    }
    for candidate in output.candidates {
        if cancellation.is_cancelled() {
            return Err(cancelled(id));
        }
        let Some(sequence) = store.count().checked_add(1) else {
            return {
                // The producer emits only within positive remaining capacity, proving store.count < max_candidates <= u64::MAX.
                #[cfg(coverage_nightly)]
                {
                    (#[coverage(off)]
                    || {
                        Err(EffectFailed::other(
                            id,
                            "analyzer.candidate",
                            "candidate sequence overflow",
                        ))
                    })()
                }
                #[cfg(not(coverage_nightly))]
                {
                    Err(EffectFailed::other(
                        id,
                        "analyzer.candidate",
                        "candidate sequence overflow",
                    ))
                }
            };
        };
        let candidate = mutation_candidate(id, &validation, candidate, sequence)?;
        store
            .push(&candidate)
            .map_err(|error| candidate_store_error(id, &candidate, &error))?;
    }
    #[cfg(test)]
    if let Some(hook) = checkpoint_hook.as_ref() {
        hook(AnalysisCheckpoint::BeforeFinalization);
    }
    if cancellation.is_cancelled() {
        return Err(cancelled(id));
    }
    let truncated = output.truncated;
    let diagnostics = output
        .diagnostics
        .into_iter()
        .map(map_analyzer_diagnostic)
        .collect();
    let (spool, store) =
        if request.final_target || truncated {
            (
                Some(store.finish().map_err(|error| {
                    EffectFailed::other(id, "analyzer.store", error.to_string())
                })?),
                None,
            )
        } else {
            (None, Some(store))
        };
    Ok((
        AnalysisFinished {
            id,
            spool,
            truncated,
            diagnostics,
        },
        store,
    ))
}

fn map_analyzer_diagnostic(diagnostic: AnalyzerDiagnostic) -> RunAnalysisDiagnostic {
    let default_message = match diagnostic.code {
        AnalyzerDiagnosticCode::UnsupportedEnumDefinition => "unsupported enum definition",
        AnalyzerDiagnosticCode::UnsupportedExceptionHierarchy => {
            "unsupported exception hierarchy reference"
        }
        AnalyzerDiagnosticCode::InvalidSyntax => "source could not be parsed",
        AnalyzerDiagnosticCode::CandidateLimitExceeded => {
            "candidate limit reached before analysis completed"
        }
    };
    let explanation = diagnostic
        .message
        .unwrap_or_else(|| default_message.to_owned());
    let mut location = diagnostic
        .path
        .map_or_else(String::new, |path| path.to_string());
    if let Some(line) = diagnostic.line {
        if location.is_empty() {
            location = format!("line {line}");
        } else {
            let _ = write!(location, ":{line}");
        }
    }
    if let Some(column) = diagnostic.column {
        if location.is_empty() {
            location = format!("column {column}");
        } else if diagnostic.line.is_some() {
            let _ = write!(location, ":{column}");
        } else {
            let _ = write!(location, " (column {column})");
        }
    }
    let message = if location.is_empty() {
        explanation
    } else {
        format!("{location}: {explanation}")
    };

    RunAnalysisDiagnostic {
        code: format!("analyzer.{}", diagnostic.code.canonical_name()),
        message,
    }
}

fn analysis_failure(
    id: EffectId,
    path: &camino::Utf8Path,
    error: hoimin_python::AnalysisError,
) -> EffectFailed {
    match error {
        hoimin_python::AnalysisError::Cancelled => cancelled(id),
        hoimin_python::AnalysisError::PayloadLimit { .. } => {
            EffectFailed::other(id, "analyzer.payload_limit", format!("{path}: {error}"))
        }
        hoimin_python::AnalysisError::HierarchyLimit
        | hoimin_python::AnalysisError::HierarchySourceChanged => {
            EffectFailed::other(id, "analyzer.exception_hierarchy", error.to_string())
        }
        hoimin_python::AnalysisError::DepthExceeded { .. } => {
            EffectFailed::other(id, "analyzer.depth", format!("{path}: {error}"))
        }
    }
}

fn cancelled(id: EffectId) -> EffectFailed {
    EffectFailed::other(id, "analyzer.cancelled", "analyzer was cancelled")
}

fn candidate_store_error(
    id: EffectId,
    candidate: &MutationCandidate,
    error: &StoreError,
) -> EffectFailed {
    if let StoreError::PayloadLimit { limit } = error {
        return analysis_failure(
            id,
            &candidate.path,
            hoimin_python::AnalysisError::PayloadLimit { limit: *limit },
        );
    }
    EffectFailed::other(
        id,
        "analyzer.store",
        format!(
            "{}:{} ({}): {error}",
            candidate.path, candidate.line, candidate.operator
        ),
    )
}

fn source_validation<'source>(
    source: &'source [u8],
    path: &Utf8Path,
    id: EffectId,
) -> Result<CandidateValidationContext<'source>, EffectFailed> {
    let failure = |error: String| {
        EffectFailed::other(id, "analyzer.source.encoding", format!("{path}: {error}"))
    };
    let validation = match CandidateValidationContext::new(source) {
        Ok(context) => context,
        Err(error) => {
            return {
                // Core separately checks the width boundary; reaching this propagation requires source or decoded text larger than 4 GiB.
                #[cfg(coverage_nightly)]
                {
                    (#[coverage(off)]
                    || Err(failure(error.to_string())))()
                }
                #[cfg(not(coverage_nightly))]
                {
                    Err(failure(error.to_string()))
                }
            };
        }
    };
    validation
        .decoded_source()
        .map_err(|error| failure(error.to_string()))?;
    Ok(validation)
}

fn mutation_candidate(
    effect_id: EffectId,
    validation: &CandidateValidationContext<'_>,
    charged: Charged<AnalyzerCandidate>,
    sequence: u64,
) -> Result<Charged<MutationCandidate>, EffectFailed> {
    let Charged {
        mut lease,
        value: candidate,
    } = charged;
    lease
        .grow([
            candidate.operator.as_str().len(),
            validation.file_hash().len(),
            hoimin_core::MUTANT_ID_BYTES,
        ])
        .map_err(|error| analysis_failure(effect_id, &candidate.path, error))?;
    let decoded = validation.decoded_source().map_err(|error| {
        EffectFailed::other(
            effect_id,
            "analyzer.source.encoding",
            format!("{}: {error}", candidate.path),
        )
    })?;
    let raw_span = raw_candidate_span(decoded, candidate.span).ok_or_else(|| {
        EffectFailed::other(
            effect_id,
            "analyzer.candidate",
            "decoded candidate span is not an original source boundary",
        )
    })?;
    let descriptor = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: candidate.path,
        span: raw_span,
        original: candidate.original,
        replacement: candidate.replacement,
        operator: candidate.operator.as_str().to_owned(),
        line: candidate.line,
        column: candidate.column,
        symbol: candidate.symbol,
        file_hash: validation.file_hash().to_owned(),
    };
    let Some(scratch_bytes) = hoimin_core::candidate_validation_scratch_bytes(&descriptor) else {
        return {
            // Producer fields are charged to a 16 MiB payload lease; duplicating each at most three times plus fixed metadata fits 32/64-bit usize.
            #[cfg(all(
                coverage_nightly,
                any(target_pointer_width = "32", target_pointer_width = "64")
            ))]
            {
                (#[coverage(off)]
                || {
                    Err(analysis_failure(
                        effect_id,
                        &descriptor.path,
                        lease.budget().exceeded(),
                    ))
                })()
            }
            #[cfg(not(all(
                coverage_nightly,
                any(target_pointer_width = "32", target_pointer_width = "64")
            )))]
            {
                Err(analysis_failure(
                    effect_id,
                    &descriptor.path,
                    lease.budget().exceeded(),
                ))
            }
        };
    };
    let _scratch = lease
        .budget()
        .reserve([scratch_bytes])
        .map_err(|error| analysis_failure(effect_id, &descriptor.path, error))?;
    let id = validate_candidate_with_context(validation, &descriptor)
        .map_err(|error| EffectFailed::other(effect_id, "analyzer.candidate", error.to_string()))?;
    Ok(Charged {
        lease,
        value: MutationCandidate {
            id: id.to_string(),
            sequence,
            path: descriptor.path,
            span: descriptor.span,
            original: descriptor.original,
            replacement: descriptor.replacement,
            operator: descriptor.operator,
            line: descriptor.line,
            column: descriptor.column,
            symbol: descriptor.symbol,
            file_hash: descriptor.file_hash,
        },
    })
}

fn raw_candidate_span(
    decoded: &hoimin_core::DecodedPythonSource<'_>,
    span: hoimin_core::ByteSpan,
) -> Option<hoimin_core::ByteSpan> {
    let Ok(start) = usize::try_from(span.start) else {
        return {
            // Every u64 offset fits 64-bit usize; 32-bit rejection remains instrumented.
            #[cfg(all(coverage_nightly, target_pointer_width = "64"))]
            {
                (#[coverage(off)]
                || None)()
            }
            #[cfg(not(all(coverage_nightly, target_pointer_width = "64")))]
            {
                None
            }
        };
    };
    let Ok(length) = usize::try_from(span.length) else {
        return {
            // Every u64 length fits 64-bit usize; 32-bit rejection remains instrumented.
            #[cfg(all(coverage_nightly, target_pointer_width = "64"))]
            {
                (#[coverage(off)]
                || None)()
            }
            #[cfg(not(all(coverage_nightly, target_pointer_width = "64")))]
            {
                None
            }
        };
    };
    let end = start.checked_add(length)?;
    let raw_start = decoded.utf8_to_raw(start)?;
    let raw_end = decoded.utf8_to_raw(end)?;
    let Some(raw_length) = raw_end.checked_sub(raw_start) else {
        return {
            // Decoded-source boundary mappings preserve order, and checked start+length proves start <= end.
            #[cfg(coverage_nightly)]
            {
                (#[coverage(off)]
                || None)()
            }
            #[cfg(not(coverage_nightly))]
            {
                None
            }
        };
    };
    let Ok(start) = u64::try_from(raw_start) else {
        return {
            // A source offset represented by 32/64-bit usize always fits u64.
            #[cfg(all(
                coverage_nightly,
                any(target_pointer_width = "32", target_pointer_width = "64")
            ))]
            {
                (#[coverage(off)]
                || None)()
            }
            #[cfg(not(all(
                coverage_nightly,
                any(target_pointer_width = "32", target_pointer_width = "64")
            )))]
            {
                None
            }
        };
    };
    let Ok(length) = u64::try_from(raw_length) else {
        return {
            // A source length represented by 32/64-bit usize always fits u64.
            #[cfg(all(
                coverage_nightly,
                any(target_pointer_width = "32", target_pointer_width = "64")
            ))]
            {
                (#[coverage(off)]
                || None)()
            }
            #[cfg(not(all(
                coverage_nightly,
                any(target_pointer_width = "32", target_pointer_width = "64")
            )))]
            {
                None
            }
        };
    };
    Some(hoimin_core::ByteSpan { start, length })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hoimin_core::{
        CandidateIdentity, CandidateValidationContext, EffectId, MutationProfile, TargetSlice,
        stable_mutant_id,
    };
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    fn request(id: u64) -> AnalyzeFile {
        AnalyzeFile {
            id: EffectId(id),
            target: TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
            final_target: true,
        }
    }

    fn analyzer_candidate() -> AnalyzerCandidate {
        AnalyzerCandidate {
            path: "src/calc.py".into(),
            span: hoimin_core::ByteSpan {
                start: 12,
                length: 2,
            },
            original: "==".into(),
            replacement: "!=".into(),
            operator: hoimin_core::MutationOperator::CompareEqNe,
            line: 2,
            column: 6,
            symbol: Some("compare".into()),
        }
    }

    #[test]
    fn batch_conversion_preserves_strict_candidate_identity() {
        let source = b"x = 1\nvalue == 2\n";
        let context = CandidateValidationContext::new(source).unwrap();

        let converted = mutation_candidate(
            EffectId(0),
            &context,
            PayloadBudget::default()
                .retain(analyzer_candidate())
                .unwrap(),
            7,
        )
        .unwrap();
        let identity = CandidateIdentity {
            schema_version: CANDIDATE_SCHEMA_VERSION,
            file_hash: blake3::hash(source).to_hex().to_string(),
            path: "src/calc.py".into(),
            span: hoimin_core::ByteSpan {
                start: 12,
                length: 2,
            },
            operator: "compare_eq_ne".into(),
            replacement: "!=".into(),
        };

        assert_eq!(converted.id, stable_mutant_id(&identity).to_string());
        assert_eq!(converted.sequence, 7);
        assert_eq!(converted.file_hash, context.file_hash());
        assert_eq!(converted.original, "==");
        assert_eq!(converted.line, 2);
        assert_eq!(converted.column, 6);
        assert_eq!(converted.symbol.as_deref(), Some("compare"));
    }

    #[test]
    fn batch_conversion_rejects_stale_source_metadata() {
        let source = b"x = 1\nvalue == 2\n";
        let context = CandidateValidationContext::new(source).unwrap();

        let mut stale_original = analyzer_candidate();
        stale_original.original = ">=".into();

        let error = mutation_candidate(
            EffectId(0),
            &context,
            PayloadBudget::default().retain(stale_original).unwrap(),
            1,
        )
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.candidate");
        assert_eq!(
            error.failure.message(),
            "candidate original text does not match the source span"
        );

        let mut stale_location = analyzer_candidate();
        stale_location.column = 5;

        let error = mutation_candidate(
            EffectId(0),
            &context,
            PayloadBudget::default().retain(stale_location).unwrap(),
            1,
        )
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.candidate");
        assert_eq!(
            error.failure.message(),
            "candidate line or column does not match its byte span"
        );
    }

    #[test]
    fn pre_cancelled_discovery_returns_before_opening_missing_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().join("missing-root")).unwrap();
        let cancellation = ProcessCancellation::new();
        cancellation.cancel();
        let work = discovery_work(
            &root,
            &[],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            10,
            cancellation,
            None,
        );

        let error = discover_targets_blocking(work).unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.cancelled");
        assert_eq!(error.failure.message(), "analyzer was cancelled");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn discovery_timeout_returns_before_detached_analysis_releases_resources() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::write(
            directory.path().join("src/calc.py"),
            "result = left == right\n",
        )
        .unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
        let targets = vec![TargetSlice {
            path: "src/calc.py".into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        }];
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let owned = Arc::new(tempfile::tempdir().unwrap());
        let owned_path = owned.path().to_owned();
        let control = DiscoveryControl::new(owned.clone(), move || {
            entered_tx.send(()).expect("test waits for discovery entry");
            release_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("test releases paused discovery");
        });
        let operators = MutationOperatorSelection::default();
        let task_targets = targets.clone();
        let operation = tokio::spawn(async move {
            discover_targets_with_control(
                &root,
                &task_targets,
                &operators,
                MutationProfile::Full,
                10,
                Duration::from_millis(20),
                Some(control),
            )
            .await
        });
        tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(2)))
            .await
            .expect("entry wait task must not panic")
            .expect("discovery must enter the test pause");

        let error = tokio::time::timeout(Duration::from_millis(200), operation)
            .await
            .expect("timeout must not await detached discovery")
            .unwrap()
            .unwrap_err();
        assert_eq!(error.failure.code(), "analyzer.timeout");
        assert_eq!(
            error.failure.message(),
            "--analyzer-timeout expired after 20ms"
        );
        drop(owned);
        assert!(owned_path.exists(), "detached discovery owns its resources");

        release_tx
            .send(())
            .expect("paused discovery still waits for release");
        tokio::time::timeout(Duration::from_secs(2), async {
            while owned_path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("detached discovery must release its resources");

        let recovered = discover_targets_with_timeout(
            &Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
            &targets,
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            10,
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        let expected = discover_targets(
            &Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
            &targets,
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            10,
        )
        .await
        .unwrap();
        assert_eq!(recovered, expected);
        assert_ne!(recovered.candidates, Vec::new());
    }

    pub(super) async fn wait_for_discovery_entry(
        receiver: &std::sync::mpsc::Receiver<()>,
    ) -> Result<(), std::sync::mpsc::TryRecvError> {
        loop {
            match receiver.try_recv() {
                Ok(()) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                Err(error) => return Err(error),
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn discovery_deadline_is_shared_across_targets() {
        // Arrange: handshakes keep virtual time fixed while the worker reaches each target.
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
        let targets = ["first.py", "second.py"].map(|name| {
            std::fs::write(root.join(name), "result = left + right\n").unwrap();
            TargetSlice {
                path: name.into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }
        });
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let control = DiscoveryControl::new(Arc::new(tempfile::tempdir().unwrap()), move || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let operation = tokio::spawn(async move {
            discover_targets_with_control(
                &root,
                &targets,
                &MutationOperatorSelection::default(),
                MutationProfile::Full,
                100,
                Duration::from_secs(1),
                Some(control),
            )
            .await
        });
        wait_for_discovery_entry(&entered_rx).await.unwrap();
        tokio::time::advance(Duration::from_millis(750)).await;
        release_tx.send(()).unwrap();
        // Yield without advancing time until target two reaches its handshake.
        wait_for_discovery_entry(&entered_rx).await.unwrap();
        // Act: expire the original deadline while the second target remains paused.
        // A reset at target two would still have 750 ms left on this frozen clock.
        tokio::time::advance(Duration::from_millis(250)).await;
        let result = operation.await;
        release_tx.send(()).unwrap();
        // Assert.
        let error = result.unwrap().unwrap_err();
        assert_eq!(error.failure.code(), "analyzer.timeout");
    }

    #[test]
    fn controlled_discovery_arms_its_deadline_after_blocking_worker_entry() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(1)
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir(directory.path().join("src")).unwrap();
            std::fs::write(
                directory.path().join("src/calc.py"),
                "result = left == right\n",
            )
            .unwrap();
            let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
            let targets = vec![TargetSlice {
                path: "src/calc.py".into(),
                lines: Vec::new(),
                symbols: Vec::new(),
            }];
            let (blocking_entered_tx, blocking_entered_rx) = mpsc::sync_channel(1);
            let (blocking_release_tx, blocking_release_rx) = mpsc::sync_channel(1);
            let blocker = tokio::task::spawn_blocking(move || {
                blocking_entered_tx.send(()).unwrap();
                blocking_release_rx
                    .recv_timeout(Duration::from_secs(2))
                    .expect("test releases the occupied blocking worker");
            });
            blocking_entered_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("blocking worker must be occupied before discovery starts");

            let (entered_tx, entered_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::sync_channel(1);
            let control =
                DiscoveryControl::new(Arc::new(tempfile::tempdir().unwrap()), move || {
                    entered_tx.send(()).expect("test waits for discovery entry");
                    release_rx
                        .recv_timeout(Duration::from_secs(2))
                        .expect("test releases paused discovery");
                });
            let (operation_started_tx, operation_started_rx) = tokio::sync::oneshot::channel();
            let operation = tokio::spawn(async move {
                operation_started_tx.send(()).unwrap();
                discover_targets_with_control(
                    &root,
                    &targets,
                    &MutationOperatorSelection::default(),
                    MutationProfile::Full,
                    10,
                    Duration::from_millis(20),
                    Some(control),
                )
                .await
            });
            operation_started_rx
                .await
                .expect("discovery operation must start");
            tokio::time::sleep(Duration::from_millis(60)).await;
            let expired_while_queued = operation.is_finished();

            blocking_release_tx
                .send(())
                .expect("occupied blocking worker still waits for release");
            blocker.await.expect("blocking worker must not panic");

            assert!(
                !expired_while_queued,
                "controlled deadline expired before the blocking worker entered"
            );

            tokio::time::timeout(Duration::from_secs(2), async {
                wait_for_discovery_entry(&entered_rx).await
            })
            .await
            .expect("controlled discovery must enter after the blocking worker is released")
            .expect("controlled discovery ended before entry");

            let error = tokio::time::timeout(Duration::from_millis(200), operation)
                .await
                .expect("entry-armed deadline must expire without awaiting release")
                .unwrap()
                .unwrap_err();

            assert_eq!(error.failure.code(), "analyzer.timeout");
            release_tx
                .send(())
                .expect("paused discovery still waits for release");
        });
    }

    #[rstest::rstest]
    #[case::analyzerdiagnostic_code_analyzerdiagnosticcode_unsupportedenumdef_1(0)]
    #[case::analyzerdiagnostic_code_analyzerdiagnosticcode_unsupportedexcepti_2(1)]
    #[case::analyzerdiagnostic_code_analyzerdiagnosticcode_invalidsyntax_path_3(2)]
    #[case::analyzerdiagnostic_code_analyzerdiagnosticcode_candidatelimitexce_4(3)]
    fn maps_every_analyzer_diagnostic_to_a_stable_run_diagnostic(#[case] scenario_index: usize) {
        let cases = [
            (
                AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::UnsupportedEnumDefinition,
                    path: None,
                    line: None,
                    column: None,
                    message: None,
                },
                "analyzer.enum_definition_skipped",
                "unsupported enum definition",
            ),
            (
                AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::UnsupportedExceptionHierarchy,
                    path: None,
                    line: None,
                    column: None,
                    message: None,
                },
                "analyzer.exception_hierarchy_skipped",
                "unsupported exception hierarchy reference",
            ),
            (
                AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::InvalidSyntax,
                    path: Some("src/broken.py".into()),
                    line: None,
                    column: None,
                    message: None,
                },
                "analyzer.invalid_syntax",
                "src/broken.py: source could not be parsed",
            ),
            (
                AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::CandidateLimitExceeded,
                    path: Some("src/calc.py".into()),
                    line: None,
                    column: None,
                    message: None,
                },
                "analyzer.candidate_limit",
                "src/calc.py: candidate limit reached before analysis completed",
            ),
        ];

        let (diagnostic, expected_code, expected_message) =
            cases.into_iter().nth(scenario_index).unwrap();

        let mapped = map_analyzer_diagnostic(diagnostic);

        assert_eq!(mapped.code, expected_code);
        assert_eq!(mapped.message, expected_message);
    }

    #[rstest::rstest]
    #[case::none_1(0)]
    #[case::some_src_calc_py_into_2(1)]
    fn analyzer_diagnostic_preserves_custom_message_with_or_without_location(
        #[case] scenario_index: usize,
    ) {
        let cases = [
            (None, None, None, "custom explanation"),
            (
                Some("src/calc.py".into()),
                Some(7),
                Some(12),
                "src/calc.py:7:12: custom explanation",
            ),
        ];

        let (path, line, column, expected_message) = cases.into_iter().nth(scenario_index).unwrap();

        let diagnostic = AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidSyntax,
            path,
            line,
            column,
            message: Some("custom explanation".to_owned()),
        };

        let mapped = map_analyzer_diagnostic(diagnostic);

        assert_eq!(mapped.message, expected_message);
    }

    #[tokio::test]
    async fn concrete_handler_cancellation_leaves_store_ready_for_final_spool() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::write(
            directory.path().join("src/calc.py"),
            "result = left == right\n",
        )
        .unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
        let mut handler = AnalyzerHandler::new(
            root,
            MutationOperatorSelection::default(),
            MutationProfile::Full,
            NonZeroUsize::new(10).unwrap(),
        );
        let mut first = request(81);
        first.final_target = false;
        handler.handle(first).await.unwrap();
        let cancellation = ProcessCancellation::new();
        cancellation.cancel();

        let cancelled = handler
            .handle_with_cancellation(request(82), cancellation)
            .await;

        assert!(matches!(
            cancelled,
            Err(error) if error.id == EffectId(82) && error.failure.code() == "analyzer.cancelled"
        ));
        assert_eq!(handler.store.as_ref().unwrap().count(), 1);

        let finished = handler.handle(request(83)).await.unwrap();
        let spool = finished.spool.unwrap();

        assert_eq!(spool.records, 2);
        assert!(handler.store.is_none());
    }

    #[tokio::test]
    async fn depth_failure_releases_candidate_store_and_preserves_handler_reuse() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("src")).unwrap();
        let source_path = directory.path().join("src/calc.py");
        let source = format!("value = {}\n", vec!["1"; 25_000].join("+"));
        std::fs::write(&source_path, &source).unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
        let owner = Arc::new(tempfile::tempdir().unwrap());
        let spool_path = owner.path().to_owned();
        let mut handler = AnalyzerHandler::new(
            root,
            MutationOperatorSelection::default(),
            MutationProfile::Full,
            NonZeroUsize::new(10).unwrap(),
        )
        .with_candidate_spool_owner(owner.clone());
        for id in 100..103 {
            let error = handler.handle(request(id)).await.unwrap_err();
            assert_eq!(error.id, EffectId(id));
            assert_eq!(error.failure.code(), "analyzer.depth");
            assert!(handler.store.is_none());
            assert_eq!(std::fs::read_dir(&spool_path).unwrap().count(), 0);
            assert_eq!(std::fs::read_to_string(&source_path).unwrap(), source);
        }
        std::fs::write(&source_path, "value = 1 + 2\n").unwrap();
        let output = handler.handle(request(103)).await.unwrap();
        assert!(output.spool.unwrap().records > 0);
        drop(handler);
        drop(owner);
        assert!(!spool_path.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn in_progress_analysis_is_cancellable() {
        use std::sync::{Arc, Barrier};
        use std::time::Duration;

        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::write(
            directory.path().join("src/calc.py"),
            "result = left == right\n",
        )
        .unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
        let entered = Arc::new(Barrier::new(2));
        let hook_entered = entered.clone();
        let spool_owner = Arc::new(tempfile::tempdir().unwrap());
        let spool_path = spool_owner.path().to_owned();
        let mut handler = AnalyzerHandler::new(
            root,
            MutationOperatorSelection::default(),
            MutationProfile::Full,
            NonZeroUsize::new(10).unwrap(),
        )
        .with_candidate_spool_owner(spool_owner.clone());
        handler.analysis_hook = Some(Arc::new(move || {
            hook_entered.wait();
            std::thread::sleep(Duration::from_millis(500));
        }));
        let cancellation = ProcessCancellation::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            handler
                .handle_with_cancellation(request(84), task_cancellation)
                .await
        });
        entered.wait();
        cancellation.cancel();

        let result = tokio::time::timeout(Duration::from_millis(200), task)
            .await
            .expect("cancellation must not wait for blocking analysis")
            .unwrap();

        assert!(matches!(
            result,
            Err(error) if error.id == EffectId(84) && error.failure.code() == "analyzer.cancelled"
        ));
        drop(spool_owner);
        assert!(
            spool_path.exists(),
            "detached analysis must retain the spool owner"
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while spool_path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("detached analysis must eventually release the spool owner");
        assert!(
            !spool_path.exists(),
            "spool owner must be released when detached analysis exits"
        );
    }
}

#[cfg(test)]
mod exception_project_tests {
    use super::*;
    #[tokio::test]
    async fn exception_hierarchy_discovery_reads_unselected_module() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("errors.py"),
            "class Root(Exception): pass\nclass Child(Root): pass\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("service.py"),
            "from errors import Child, Root\ndef f():\n    raise Child()\n",
        )
        .unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().into()).unwrap();
        let operators = serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();
        let output = discover_targets(
            &root,
            &[TargetSlice {
                path: "service.py".into(),
                lines: vec![],
                symbols: vec![],
            }],
            &operators,
            MutationProfile::Full,
            100,
        )
        .await
        .unwrap();
        assert_eq!(output.candidates.len(), 1);
        assert_eq!(output.candidates[0].replacement, "Root");
    }
}

#[cfg(test)]
mod exception_fingerprint_tests {
    #[test]
    fn exception_hierarchy_fingerprints_unselected_inputs_and_additions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("service.py"),
            "from errors import Child, Root\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("errors.py"),
            "class Root(Exception): pass\nclass Child(Root): pass\n",
        )
        .unwrap();
        let config = crate::cli::parse_config_from([
            "hoimin",
            "run",
            "--root",
            dir.path().to_str().unwrap(),
            "--file",
            "service.py",
            "--operators",
            "exception_hierarchy",
            "--",
            "python3",
            "-m",
            "unittest",
        ])
        .unwrap();
        let prepared = crate::shell::prepare_run_config(config.clone()).unwrap();

        assert_eq!(prepared.fingerprint_inputs.len(), 2);
        std::fs::write(
            dir.path().join("new_errors.py"),
            "class NewError(Exception): pass\n",
        )
        .unwrap();
        let next = crate::shell::prepare_run_config(config).unwrap();
        assert_eq!(next.fingerprint_inputs.len(), 3);
        assert_ne!(prepared.fingerprint_inputs, next.fingerprint_inputs);
    }
}

#[cfg(test)]
mod coverage_regressions {
    use super::*;

    #[tokio::test]
    async fn discovery_rejects_an_unrepresentable_timeout_before_work_starts() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();

        let error = discover_targets_with_timeout(
            root,
            &[],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            1,
            Duration::MAX,
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.timeout");
        assert!(
            error
                .failure
                .message()
                .contains("outside the supported deadline range")
        );
    }

    #[tokio::test]
    async fn controlled_discovery_rejects_an_unrepresentable_armed_timeout() {
        let directory = Arc::new(tempfile::tempdir().unwrap());
        let root = Utf8Path::from_path(directory.path()).unwrap();
        std::fs::write(root.join("source.py"), "result = left + right\n").unwrap();
        let control = DiscoveryControl::new(Arc::clone(&directory), discovery_noop);

        let error = discover_targets_with_control(
            root,
            &[TargetSlice {
                path: "source.py".into(),
                lines: vec![],
                symbols: vec![],
            }],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            1,
            Duration::MAX,
            Some(control),
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.timeout");
        assert!(
            error
                .failure
                .message()
                .contains("outside the supported deadline range")
        );
    }

    #[tokio::test]
    async fn discovery_worker_panic_is_a_typed_task_failure() {
        let directory = Arc::new(tempfile::tempdir().unwrap());
        let root = Utf8Path::from_path(directory.path()).unwrap();
        std::fs::write(root.join("source.py"), "result = left + right\n").unwrap();
        let control = DiscoveryControl::new(Arc::clone(&directory), || {
            panic!("controlled analyzer panic");
        });

        let error = discover_targets_with_control(
            root,
            &[TargetSlice {
                path: "source.py".into(),
                lines: vec![],
                symbols: vec![],
            }],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            1,
            Duration::from_secs(10),
            Some(control),
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.task");
        assert!(
            error
                .failure
                .message()
                .contains("controlled analyzer panic")
        );
    }

    #[rstest::rstest]
    #[case::line(None, Some(7), None, "line 7: detail")]
    #[case::column(None, None, Some(3), "column 3: detail")]
    #[case::path_column(Some("source.py"), None, Some(3), "source.py (column 3): detail")]
    fn diagnostic_projection_preserves_partial_location(
        #[case] path: Option<&str>,
        #[case] line: Option<u32>,
        #[case] column: Option<u32>,
        #[case] expected: &str,
    ) {
        let actual = map_analyzer_diagnostic(AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidSyntax,
            path: path.map(Utf8PathBuf::from),
            line,
            column,
            message: Some("detail".into()),
        });

        assert_eq!(actual.message, expected);
    }

    #[rstest::rstest]
    #[case::limit(hoimin_python::AnalysisError::HierarchyLimit)]
    #[case::changed(hoimin_python::AnalysisError::HierarchySourceChanged)]
    fn hierarchy_failure_retains_the_original_effect_identity(
        #[case] error: hoimin_python::AnalysisError,
    ) {
        let failure = analysis_failure(EffectId(91), Utf8Path::new("source.py"), error);

        let actual_1 = failure.failure.code();

        assert_eq!(failure.id, EffectId(91));
        assert_eq!(actual_1, "analyzer.exception_hierarchy");
    }

    fn converted_candidate(span: hoimin_core::ByteSpan) -> Charged<AnalyzerCandidate> {
        PayloadBudget::default()
            .retain(AnalyzerCandidate {
                path: "source.py".into(),
                span,
                original: "?".into(),
                replacement: "!".into(),
                operator: hoimin_core::MutationOperator::CompareEqNe,
                line: 1,
                column: 0,
                symbol: None,
            })
            .unwrap()
    }

    #[test]
    fn mutation_projection_reports_undecodable_source() {
        let validation = CandidateValidationContext::new(b"# coding: ascii\n# \xff\n").unwrap();

        let error = mutation_candidate(
            EffectId(92),
            &validation,
            converted_candidate(hoimin_core::ByteSpan {
                start: 0,
                length: 1,
            }),
            1,
        )
        .unwrap_err();

        assert_eq!(error.id, EffectId(92));
        assert_eq!(error.failure.code(), "analyzer.source.encoding");
        assert!(error.failure.message().starts_with("source.py:"));
    }

    #[rstest::rstest]
    #[case::start(1, 1)]
    #[case::end(0, 1)]
    #[case::overflow(u64::MAX, 1)]
    #[case::length(0, u64::MAX)]
    fn mutation_projection_rejects_a_non_boundary_decoded_span(
        #[case] start: u64,
        #[case] length: u64,
    ) {
        let validation = CandidateValidationContext::new("é\n".as_bytes()).unwrap();

        let error = mutation_candidate(
            EffectId(93),
            &validation,
            converted_candidate(hoimin_core::ByteSpan { start, length }),
            1,
        )
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.candidate");
        assert_eq!(
            error.failure.message(),
            "decoded candidate span is not an original source boundary"
        );
    }

    fn handler_fixture() -> (tempfile::TempDir, AnalyzerHandler) {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().join("project")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("source.py"), "result = left + right\n").unwrap();
        let handler = AnalyzerHandler::new(
            root,
            MutationOperatorSelection::default(),
            MutationProfile::Full,
            NonZeroUsize::new(100).unwrap(),
        )
        .with_candidate_spool_owner(Arc::new(tempfile::tempdir().unwrap()));
        (directory, handler)
    }

    fn handler_request(id: u64) -> AnalyzeFile {
        AnalyzeFile {
            id: EffectId(id),
            target: TargetSlice {
                path: "source.py".into(),
                lines: vec![],
                symbols: vec![],
            },
            final_target: true,
        }
    }

    fn cancel_at_checkpoint(
        cancellation: &ProcessCancellation,
        expected: AnalysisCheckpoint,
        actual: AnalysisCheckpoint,
    ) {
        if actual == expected {
            cancellation.cancel();
        }
    }

    #[rstest::rstest]
    #[case::after_analysis(AnalysisCheckpoint::AfterAnalysis)]
    #[case::before_finalization(AnalysisCheckpoint::BeforeFinalization)]
    #[tokio::test]
    async fn cancellation_at_an_analysis_checkpoint_is_reported_without_a_spool(
        #[case] checkpoint: AnalysisCheckpoint,
    ) {
        let (_directory, mut handler) = handler_fixture();
        let cancellation = ProcessCancellation::new();
        let worker_cancellation = cancellation.clone();
        handler.checkpoint_hook = Some(Arc::new(move |actual| {
            cancel_at_checkpoint(&worker_cancellation, checkpoint, actual);
        }));

        let error = handler
            .handle_with_cancellation(handler_request(97), cancellation)
            .await
            .unwrap_err();

        assert_eq!(error.id, EffectId(97));
        assert_eq!(error.failure.code(), "analyzer.cancelled");
        assert!(handler.store.is_none());
    }

    #[tokio::test]
    async fn discovery_reports_a_missing_root_before_analysis() {
        let directory = tempfile::tempdir().unwrap();
        let missing = Utf8PathBuf::from_path_buf(directory.path().join("absent")).unwrap();

        let error = discover_targets(
            &missing,
            &[],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.source.read");
    }

    #[tokio::test]
    async fn handler_reports_owned_spool_creation_failure_before_source_read() {
        let (_directory, handler) = handler_fixture();
        let owner = Arc::new(tempfile::tempdir().unwrap());
        std::fs::remove_dir(owner.path()).unwrap();
        let mut handler = handler.with_candidate_spool_owner(owner);

        let error = handler.handle(handler_request(98)).await.unwrap_err();

        assert_eq!(error.id, EffectId(98));
        assert_eq!(error.failure.code(), "analyzer.store");
        assert!(handler.root.is_none());
    }

    #[tokio::test]
    async fn handler_reports_worker_panic_without_retaining_the_store() {
        let (_directory, mut handler) = handler_fixture();
        handler.analysis_hook = Some(Arc::new(|| panic!("owned handler worker panic")));

        let error = handler.handle(handler_request(99)).await.unwrap_err();

        assert_eq!(error.id, EffectId(99));
        assert_eq!(error.failure.code(), "analyzer.task");
        assert!(
            error
                .failure
                .message()
                .contains("owned handler worker panic")
        );
        assert!(handler.store.is_none());
    }

    fn owned_store_with_handle(
        open: impl FnOnce(&std::path::Path) -> std::fs::File,
    ) -> CandidateStore {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let path = temporary.path().to_owned();
        let (_file, temp_path) = temporary.into_parts();
        let handle = open(&path);
        CandidateStore::with_test_file(100, tempfile::NamedTempFile::from_parts(handle, temp_path))
    }

    #[tokio::test]
    async fn handler_reports_spool_write_failure_without_a_completed_spool() {
        let (_directory, mut handler) = handler_fixture();
        handler.store = Some(owned_store_with_handle(|path| {
            std::fs::File::open(path).unwrap()
        }));

        let error = handler.handle(handler_request(100)).await.unwrap_err();

        assert_eq!(error.id, EffectId(100));
        assert_eq!(error.failure.code(), "analyzer.store");
        assert!(handler.store.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn handler_reports_spool_sync_failure_during_finalization() {
        let (_directory, mut handler) = handler_fixture();
        handler.store = Some(owned_store_with_handle(|_| {
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/null")
                .unwrap()
        }));

        let error = handler.handle(handler_request(101)).await.unwrap_err();

        assert_eq!(error.id, EffectId(101));
        assert_eq!(error.failure.code(), "analyzer.store");
        assert!(handler.store.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn handler_reports_project_disappearing_after_source_acquisition() {
        let (directory, mut handler) = handler_fixture();
        handler.operators = serde_json::from_str("[\"exception_hierarchy\"]").unwrap();
        handler.exception_project = Some(ExceptionProject::new(&handler.root_path));
        let root = handler.root_path.clone();
        let moved = directory.path().join("moved-project");
        handler.analysis_hook = Some(Arc::new(move || std::fs::rename(&root, &moved).unwrap()));

        let error = handler.handle(handler_request(102)).await.unwrap_err();

        assert_eq!(error.id, EffectId(102));
        assert_eq!(error.failure.code(), "analyzer.exception_hierarchy");
    }

    fn prepared_config(root: &Utf8Path, operator: &str) -> hoimin_core::RunConfig {
        let config = crate::cli::parse_config_from([
            "hoimin",
            "run",
            "--root",
            root.as_str(),
            "--file",
            "source.py",
            "--operators",
            operator,
            "--",
            "unused-test-command",
        ])
        .unwrap();
        crate::shell::prepare_run_config(config).unwrap()
    }

    #[tokio::test]
    async fn configured_discovery_preserves_the_selected_operator_and_target() {
        let (_directory, handler) = handler_fixture();
        let config = prepared_config(&handler.root_path, "binary_add_sub");

        let discovery = discover_config_targets(
            &config,
            &hoimin_core::DiscoveryConfig::new(Duration::from_secs(10)).unwrap(),
            &[handler_request(1).target],
            None,
        )
        .await
        .unwrap();

        assert_eq!(discovery.candidates.len(), 1);
        assert_eq!(discovery.candidates[0].path, Utf8Path::new("source.py"));
        assert_eq!(discovery.candidates[0].operator, "binary_add_sub");
    }

    #[tokio::test]
    async fn configured_discovery_reports_an_undecodable_hierarchy_input() {
        let (_directory, handler) = handler_fixture();
        std::fs::write(
            handler.root_path.join("errors.py"),
            b"# coding: ascii\n# \xff\n",
        )
        .unwrap();
        let config = prepared_config(&handler.root_path, "exception_hierarchy");

        let error = discover_config_targets(
            &config,
            &hoimin_core::DiscoveryConfig::new(Duration::from_secs(10)).unwrap(),
            &[handler_request(1).target],
            None,
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.exception_hierarchy");
        assert!(error.failure.message().starts_with("errors.py:"));
    }

    #[tokio::test]
    async fn discovery_reports_a_missing_selected_source() {
        let (_directory, handler) = handler_fixture();

        let error = discover_targets(
            &handler.root_path,
            &[TargetSlice {
                path: "absent.py".into(),
                lines: vec![],
                symbols: vec![],
            }],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.source.read");
    }

    #[tokio::test]
    async fn discovery_reports_selected_source_encoding_failure() {
        let (_directory, handler) = handler_fixture();
        std::fs::write(
            handler.root_path.join("source.py"),
            b"# coding: ascii\n# \xff\n",
        )
        .unwrap();

        let error = discover_targets(
            &handler.root_path,
            &[handler_request(1).target],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.source.encoding");
        assert!(error.failure.message().starts_with("source.py:"));
    }

    #[tokio::test]
    async fn discovery_marks_a_retained_prefix_as_truncated() {
        let (_directory, handler) = handler_fixture();
        std::fs::write(
            handler.root_path.join("source.py"),
            "first = left + right\nsecond = top + bottom\n",
        )
        .unwrap();
        let operators = serde_json::from_str("[\"binary_add_sub\"]").unwrap();

        let discovery = discover_targets(
            &handler.root_path,
            &[handler_request(1).target],
            &operators,
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap();

        assert!(discovery.truncated);
        assert_eq!(discovery.candidates.len(), 1);
    }

    #[tokio::test]
    async fn discovery_rejects_an_original_exceeding_the_spool_record_bound() {
        let (_directory, handler) = handler_fixture();
        let source = format!(
            "def f(s):\n    s.x = 1\n    #{}\n    s.y = 2\n",
            "x".repeat(usize::try_from(MAX_SPOOL_RECORD_BYTES).unwrap())
        );
        std::fs::write(handler.root_path.join("source.py"), source).unwrap();
        let operators = serde_json::from_str("[\"function_body_erase\"]").unwrap();

        let error = discover_targets(
            &handler.root_path,
            &[handler_request(1).target],
            &operators,
            MutationProfile::Full,
            1,
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.store");
        assert!(error.failure.message().contains("spool record exceeds"));
    }

    #[tokio::test]
    async fn handler_reports_missing_root_acquisition() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().join("absent")).unwrap();
        let mut handler = AnalyzerHandler::new(
            root,
            MutationOperatorSelection::default(),
            MutationProfile::Full,
            NonZeroUsize::new(1).unwrap(),
        )
        .with_candidate_spool_owner(Arc::new(tempfile::tempdir().unwrap()));

        let error = handler.handle(handler_request(111)).await.unwrap_err();

        assert_eq!(error.id, EffectId(111));
        assert_eq!(error.failure.code(), "analyzer.source.read");
    }

    #[tokio::test]
    async fn handler_reports_undecodable_selected_source() {
        let (_directory, mut handler) = handler_fixture();
        std::fs::write(
            handler.root_path.join("source.py"),
            b"# coding: ascii\n# \xff\n",
        )
        .unwrap();

        let error = handler.handle(handler_request(112)).await.unwrap_err();

        assert_eq!(error.id, EffectId(112));
        assert_eq!(error.failure.code(), "analyzer.source.encoding");
    }

    #[tokio::test]
    async fn bound_exception_snapshot_rejects_changed_auxiliary_input() {
        let (_directory, handler) = handler_fixture();
        let errors = handler.root_path.join("errors.py");
        std::fs::write(&errors, "class Root(Exception): pass\n").unwrap();
        let config = prepared_config(&handler.root_path, "exception_hierarchy");
        let mut handler = handler.with_exception_project(&config);
        handler.operators = config.operators.clone();
        std::fs::write(errors, "class Root(Exception): pass\n# changed\n").unwrap();

        let error = handler.handle(handler_request(113)).await.unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.exception_hierarchy");
        assert!(
            error
                .failure
                .message()
                .contains("changed after fingerprint preparation")
        );
    }

    #[tokio::test]
    async fn releasing_a_managed_spool_discards_the_accumulated_prefix() {
        let (_directory, handler) = handler_fixture();
        let parent = tempfile::tempdir().unwrap();
        let coordinator = crate::workspace::ManagedRootCoordinator::open(
            Utf8Path::from_path(parent.path()).unwrap(),
        )
        .unwrap();
        let root = crate::workspace::ManagedRunRoot::create(
            &coordinator,
            crate::workspace::OwnerKind::PublicExecution,
        )
        .unwrap();
        let child = Arc::new(root.create_child("candidate-spool-").unwrap());
        let before = std::fs::read_dir(child.path()).unwrap().count();
        let mut handler = handler.with_managed_candidate_spool_owner(Arc::clone(&child));
        let mut request = handler_request(114);
        request.final_target = false;

        let response = handler.handle(request).await.unwrap();
        let retained_count = handler.store.as_ref().unwrap().count();
        handler.release_candidate_spool();

        assert!(response.spool.is_none());
        assert!(retained_count > 0);
        assert!(handler.store.is_none());
        assert!(handler.candidate_spool_owner.is_none());
        assert_eq!(std::fs::read_dir(child.path()).unwrap().count(), before);
    }

    struct DebugEvents(Arc<std::sync::Mutex<Vec<String>>>);
    struct DebugValues(Arc<std::sync::Mutex<Vec<String>>>);
    impl tracing::field::Visit for DebugValues {
        fn record_debug(&mut self, _field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.lock().unwrap().push(format!("{value:?}"));
        }
    }
    impl tracing_subscriber::Layer<tracing_subscriber::Registry> for DebugEvents {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _context: tracing_subscriber::layer::Context<'_, tracing_subscriber::Registry>,
        ) {
            event.record(&mut DebugValues(Arc::clone(&self.0)));
        }
    }
    thread_local! {
        static WORKER_TRACING: std::cell::RefCell<Option<tracing::dispatcher::DefaultGuard>> = const { std::cell::RefCell::new(None) };
    }
    fn install_worker_diagnostics(events: Arc<std::sync::Mutex<Vec<String>>>) {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(DebugEvents(events));
        let guard = tracing::subscriber::set_default(subscriber);
        WORKER_TRACING.with(|slot| *slot.borrow_mut() = Some(guard));
    }

    #[test]
    fn handler_worker_emits_its_analysis_debug_event() {
        let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let worker_events = Arc::clone(&events);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .on_thread_start(move || install_worker_diagnostics(Arc::clone(&worker_events)))
            .build()
            .unwrap();
        let (_directory, mut handler) = handler_fixture();
        runtime
            .block_on(handler.handle(handler_request(115)))
            .unwrap();

        let actual_0 = events.lock();

        assert!(
            actual_0
                .unwrap()
                .iter()
                .any(|message| message.contains("analyzing file"))
        );
    }

    #[tokio::test]
    async fn unbounded_discovery_join_failure_preserves_worker_panic() {
        let (_directory, handler) = handler_fixture();
        let control = DiscoveryControl::new(Arc::new(tempfile::tempdir().unwrap()), || {
            panic!("unbounded worker panic")
        });

        let error = discover_unbounded(
            &handler.root_path,
            &[handler_request(1).target],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            100,
            Some(control),
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.task");
        assert!(error.failure.message().contains("unbounded worker panic"));
    }

    fn discovery_noop() {}

    #[tokio::test]
    async fn controlled_discovery_join_failure_before_entry_is_typed() {
        let (_directory, handler) = handler_fixture();
        let mut control =
            DiscoveryControl::new(Arc::new(tempfile::tempdir().unwrap()), discovery_noop);
        control.before_entry = Some(Box::new(|| panic!("pre-entry worker panic")));

        let error = discover_targets_with_control(
            &handler.root_path,
            &[handler_request(1).target],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            100,
            Duration::from_secs(10),
            Some(control),
        )
        .await
        .unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.task");
        assert!(error.failure.message().contains("pre-entry worker panic"));
    }

    fn cancel_discovery_at(
        cancellation: &ProcessCancellation,
        expected: DiscoveryCheckpoint,
        actual: DiscoveryCheckpoint,
    ) {
        if expected == actual {
            cancellation.cancel();
        }
    }

    #[rstest::rstest]
    #[case::before_source(DiscoveryCheckpoint::BeforeSource)]
    #[case::after_source(DiscoveryCheckpoint::AfterSource)]
    #[case::before_candidate(DiscoveryCheckpoint::BeforeCandidate)]
    fn discovery_cancellation_at_owned_checkpoint_prevents_retained_output(
        #[case] checkpoint: DiscoveryCheckpoint,
    ) {
        let (_directory, handler) = handler_fixture();
        let cancellation = ProcessCancellation::new();
        let callback_cancellation = cancellation.clone();
        let mut control =
            DiscoveryControl::new(Arc::new(tempfile::tempdir().unwrap()), discovery_noop);
        control.checkpoint_hook = Some(Box::new(move |actual| {
            cancel_discovery_at(&callback_cancellation, checkpoint, actual);
        }));
        let work = discovery_work(
            &handler.root_path,
            &[handler_request(1).target],
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
            100,
            cancellation,
            Some(control),
        );

        let error = discover_targets_blocking(work).unwrap_err();

        assert_eq!(error.failure.code(), "analyzer.cancelled");
    }

    #[tokio::test]
    async fn discovery_entry_wait_reports_a_disconnected_handshake() {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<()>(1);
        drop(sender);

        let actual_0 = tests::wait_for_discovery_entry(&receiver).await;

        assert_eq!(actual_0, Err(std::sync::mpsc::TryRecvError::Disconnected));
    }
}
