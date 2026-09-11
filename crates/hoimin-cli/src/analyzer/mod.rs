mod protocol;
#[allow(dead_code)]
mod rust;
mod store;

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

pub use protocol::*;
pub use store::*;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    AnalysisDiagnostic as RunAnalysisDiagnostic, AnalysisFinished, AnalyzeFile,
    CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateValidationContext, EffectFailed,
    EffectId, MutationCandidate, MutationOperatorSelection, MutationProfile, TargetSlice,
    validate_candidate_with_context,
};

use crate::process::ProcessCancellation;
use crate::resource::ResourceBackend;
use crate::workspace::ManagedChild;
use crate::workspace::RootRelativeReader;
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

pub struct AnalyzerHandler {
    root_path: Utf8PathBuf,
    root: Option<RootRelativeReader>,
    store: Option<CandidateStore>,
    candidate_spool_owner: Option<CandidateSpoolOwner>,
    #[cfg(test)]
    analysis_hook: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
    pub candidates: Vec<MutationCandidate>,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub truncated: bool,
}

struct DiscoveryWork {
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
    before_analysis: Box<dyn FnOnce() + Send>,
    deadline_arm: Option<tokio::sync::oneshot::Sender<tokio::time::Instant>>,
}

#[cfg(test)]
impl DiscoveryControl {
    pub(crate) fn new(
        owner: Arc<TempDir>,
        before_analysis: impl FnOnce() + Send + 'static,
    ) -> Self {
        Self {
            _owner: owner,
            before_analysis: Box::new(before_analysis),
            deadline_arm: None,
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
    let cancellation = ProcessCancellation::new();
    let work = discovery_work(
        root,
        targets,
        operators,
        profile,
        max_candidates,
        cancellation,
        #[cfg(test)]
        None,
    );
    tokio::task::spawn_blocking(move || discover_targets_blocking(work))
        .await
        .map_err(|error| EffectFailed::other(EffectId(0), "analyzer.task", error.to_string()))?
}

/// Discovers targets within one timeout covering the complete discovery operation.
///
/// # Errors
///
/// Returns `analyzer.timeout` when the deadline expires, or an analyzer error when discovery
/// cannot complete successfully.
pub(crate) async fn discover_targets_with_timeout(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &MutationOperatorSelection,
    profile: MutationProfile,
    max_candidates: usize,
    analyzer_timeout: Duration,
) -> Result<Discovery, EffectFailed> {
    discover_targets_inner(
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

async fn discover_targets_inner(
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
    let work = discovery_work(
        root,
        targets,
        operators,
        profile,
        max_candidates,
        task_cancellation,
        #[cfg(test)]
        control,
    );
    let mut task = tokio::task::spawn_blocking(move || {
        let discovery = discover_targets_blocking(work);
        (tokio::time::Instant::now(), discovery)
    });
    #[cfg(test)]
    let deadline = if let Some(deadline_arm) = deadline_arm {
        let armed_at = match tokio::time::timeout(Duration::from_secs(2), deadline_arm).await {
            Ok(Ok(armed_at)) => armed_at,
            Ok(Err(_)) => {
                return tokio::time::timeout(Duration::from_secs(2), &mut task)
                    .await
                    .map_err(|_| {
                        EffectFailed::other(
                            EffectId(0),
                            "analyzer.test_control",
                            "controlled discovery did not finish within 2s after ending before entry",
                        )
                    })?
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

fn discover_targets_blocking(work: DiscoveryWork) -> Result<Discovery, EffectFailed> {
    let DiscoveryWork {
        root,
        targets,
        operators,
        profile,
        max_candidates,
        cancellation,
        #[cfg(test)]
        mut control,
    } = work;
    ensure_discovery_active(&cancellation)?;
    let root = RootRelativeReader::open(root.clone()).map_err(|error| {
        EffectFailed::other(EffectId(0), "analyzer.source.read", error.to_string())
    })?;
    let mut discovery = Discovery {
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        truncated: false,
    };
    for target in &targets {
        ensure_discovery_active(&cancellation)?;
        let source = root.read(&target.path).map_err(|error| {
            EffectFailed::other(EffectId(0), "analyzer.source.read", error.to_string())
        })?;
        ensure_discovery_active(&cancellation)?;
        let module = String::from_utf8(source.clone()).map_err(|error| {
            EffectFailed::other(EffectId(0), "analyzer.source.utf8", error.to_string())
        })?;
        #[cfg(test)]
        if let Some(mut control) = control.take() {
            if let Some(deadline_arm) = control.deadline_arm.take() {
                let _ = deadline_arm.send(tokio::time::Instant::now());
            }
            (control.before_analysis)();
        }
        let output = rust::analyze_source_cancellable(
            &rust::AnalyzeRequest {
                path: &target.path,
                lines: &target.lines,
                symbols: &target.symbols,
                operators: &operators,
                profile,
                max_candidates: max_candidates.saturating_sub(discovery.candidates.len()),
            },
            &module,
            || cancellation.is_cancelled(),
        )
        .map_err(|error| analysis_failure(EffectId(0), &target.path, error))?;
        let validation = CandidateValidationContext::new(&source).map_err(|error| {
            EffectFailed::other(EffectId(0), "analyzer.source", error.to_string())
        })?;
        for candidate in output.candidates {
            ensure_discovery_active(&cancellation)?;
            let sequence = u64::try_from(discovery.candidates.len())
                .ok()
                .and_then(|count| count.checked_add(1))
                .ok_or_else(|| {
                    EffectFailed::other(
                        EffectId(0),
                        "analyzer.candidate",
                        "candidate sequence overflow",
                    )
                })?;
            discovery
                .candidates
                .push(mutation_candidate(&validation, candidate, sequence)?);
        }
        discovery.diagnostics.extend(output.diagnostics);
        if output.truncated {
            discovery.truncated = true;
            break;
        }
    }
    Ok(discovery)
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
    #[allow(
        clippy::missing_errors_doc,
        reason = "The preserved fallible API currently has no error-producing path."
    )]
    pub fn new(root: Utf8PathBuf) -> Result<Self, std::io::Error> {
        Ok(Self {
            root_path: root,
            root: None,
            store: None,
            candidate_spool_owner: None,
            #[cfg(test)]
            analysis_hook: None,
        })
    }

    #[allow(
        clippy::missing_errors_doc,
        reason = "The preserved fallible API currently has no error-producing path."
    )]
    pub fn with_backend(
        root: Utf8PathBuf,
        _backend: ResourceBackend,
        _max_memory_bytes: u64,
        _max_processes: u32,
    ) -> Result<Self, std::io::Error> {
        Self::new(root)
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

    /// # Errors
    ///
    /// Returns an error when analysis is cancelled, the source cannot be read or decoded, a
    /// generated candidate is invalid, or the candidate store fails.
    pub async fn handle(
        &mut self,
        request: AnalyzeFile,
        operators: &MutationOperatorSelection,
        profile: MutationProfile,
    ) -> Result<AnalysisFinished, EffectFailed> {
        self.handle_with_cancellation(request, operators, profile, ProcessCancellation::new())
            .await
    }

    pub(crate) async fn handle_with_cancellation(
        &mut self,
        request: AnalyzeFile,
        operators: &MutationOperatorSelection,
        profile: MutationProfile,
        cancellation: ProcessCancellation,
    ) -> Result<AnalysisFinished, EffectFailed> {
        let id = request.id;
        if self.store.is_none() {
            let store = self.candidate_spool_owner.as_ref().map_or_else(
                || CandidateStore::new(request.max_candidates),
                |owner| CandidateStore::new_in(request.max_candidates, owner.path()),
            );
            self.store =
                Some(store.map_err(|error| {
                    EffectFailed::other(id, "analyzer.store", error.to_string())
                })?);
        }
        let source = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                return Err(EffectFailed::other(id, "analyzer.cancelled", "analyzer was cancelled"));
            }
            result = async { self.read_source(&request.target.path) } => result
                .map_err(|error| EffectFailed::other(id, "analyzer.source.read", error.to_string()))?,
        };
        let module = String::from_utf8(source.clone())
            .map_err(|error| EffectFailed::other(id, "analyzer.source.utf8", error.to_string()))?;
        let store = self.store.take().expect("store initialized");
        let remaining = request.max_candidates.saturating_sub(store.count());
        let max_candidates = usize::try_from(remaining).unwrap_or(usize::MAX);
        let operators = operators.clone();
        let task_cancellation = cancellation.clone();
        #[cfg(test)]
        let analysis_hook = self.analysis_hook.clone();
        let candidate_spool_owner = self.candidate_spool_owner.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _candidate_spool_owner = candidate_spool_owner;
            #[cfg(test)]
            if let Some(hook) = analysis_hook {
                hook();
            }
            analyze_and_store(BlockingAnalysis {
                request,
                operators,
                profile,
                source,
                module,
                store,
                max_candidates,
                cancellation: task_cancellation,
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
    ) -> Result<Vec<u8>, crate::workspace::RootRelativeReadError> {
        if self.root.is_none() {
            self.root = Some(
                RootRelativeReader::open(self.root_path.clone())
                    .map_err(crate::workspace::RootRelativeReadError::Other)?,
            );
        }
        self.root
            .as_ref()
            .expect("root reader initialized")
            .read(path)
    }
}

struct BlockingAnalysis {
    request: AnalyzeFile,
    operators: MutationOperatorSelection,
    profile: MutationProfile,
    source: Vec<u8>,
    module: String,
    store: CandidateStore,
    max_candidates: usize,
    cancellation: ProcessCancellation,
}

fn analyze_and_store(
    work: BlockingAnalysis,
) -> Result<(AnalysisFinished, Option<CandidateStore>), EffectFailed> {
    let BlockingAnalysis {
        request,
        operators,
        profile,
        source,
        module,
        mut store,
        max_candidates,
        cancellation,
    } = work;
    let id = request.id;
    let output = rust::analyze_source_cancellable(
        &rust::AnalyzeRequest {
            path: &request.target.path,
            lines: &request.target.lines,
            symbols: &request.target.symbols,
            operators: &operators,
            profile,
            max_candidates,
        },
        &module,
        || cancellation.is_cancelled(),
    )
    .map_err(|error| analysis_failure(id, &request.target.path, error))?;
    let validation = CandidateValidationContext::new(&source)
        .map_err(|error| EffectFailed::other(id, "analyzer.source", error.to_string()))?;
    for candidate in output.candidates {
        if cancellation.is_cancelled() {
            return Err(cancelled(id));
        }
        let sequence = store.count().checked_add(1).ok_or_else(|| {
            EffectFailed::other(id, "analyzer.candidate", "candidate sequence overflow")
        })?;
        let candidate = mutation_candidate(&validation, candidate, sequence)?;
        store
            .push(&candidate)
            .map_err(|error| EffectFailed::other(id, "analyzer.store", error.to_string()))?;
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
    let (code, default_message) = match diagnostic.code {
        AnalyzerDiagnosticCode::InvalidSyntax => {
            ("analyzer.invalid_syntax", "source could not be parsed")
        }
        AnalyzerDiagnosticCode::UnreconstructableSpan => (
            "analyzer.unreconstructable_span",
            "source span could not be reconstructed",
        ),
        AnalyzerDiagnosticCode::UnparseableReplacement => (
            "analyzer.unparseable_replacement",
            "generated replacement could not be parsed",
        ),
        AnalyzerDiagnosticCode::CandidateLimitExceeded => (
            "analyzer.candidate_limit",
            "candidate limit reached before analysis completed",
        ),
        AnalyzerDiagnosticCode::InvalidRequest => {
            ("analyzer.invalid_request", "analyzer request was invalid")
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
        code: code.to_owned(),
        message,
    }
}

fn analysis_failure(
    id: EffectId,
    path: &camino::Utf8Path,
    error: rust::AnalysisError,
) -> EffectFailed {
    match error {
        rust::AnalysisError::Cancelled => cancelled(id),
        rust::AnalysisError::DepthExceeded { .. } => {
            EffectFailed::other(id, "analyzer.depth", format!("{path}: {error}"))
        }
    }
}

fn cancelled(id: EffectId) -> EffectFailed {
    EffectFailed::other(id, "analyzer.cancelled", "analyzer was cancelled")
}

fn mutation_candidate(
    validation: &CandidateValidationContext<'_>,
    candidate: AnalyzerCandidate,
    sequence: u64,
) -> Result<MutationCandidate, EffectFailed> {
    let descriptor = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: candidate.path,
        span: candidate.span,
        original: candidate.original,
        replacement: candidate.replacement,
        operator: candidate.operator,
        line: candidate.line,
        column: candidate.column,
        symbol: candidate.symbol,
        file_hash: validation.file_hash().to_owned(),
    };
    let id = validate_candidate_with_context(validation, &descriptor).map_err(|error| {
        EffectFailed::other(EffectId(0), "analyzer.candidate", error.to_string())
    })?;
    Ok(MutationCandidate {
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
    })
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
            max_candidates: 10,
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
            operator: "compare_eq_ne".into(),
            line: 2,
            column: 6,
            symbol: Some("compare".into()),
        }
    }

    #[test]
    fn batch_conversion_preserves_strict_candidate_identity() {
        let source = b"x = 1\nvalue == 2\n";
        let context = CandidateValidationContext::new(source).unwrap();

        let converted = mutation_candidate(&context, analyzer_candidate(), 7).unwrap();
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
        let error = mutation_candidate(&context, stale_original, 1).unwrap_err();
        assert_eq!(error.failure.code(), "analyzer.candidate");
        assert_eq!(
            error.failure.message(),
            "candidate original text does not match the source span"
        );

        let mut stale_location = analyzer_candidate();
        stale_location.column = 5;
        let error = mutation_candidate(&context, stale_location, 1).unwrap_err();
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
        assert!(!recovered.candidates.is_empty());
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
            if expired_while_queued {
                let _ = operation.await;
                panic!("controlled deadline expired before the blocking worker entered");
            }

            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    match entered_rx.try_recv() {
                        Ok(()) => break,
                        Err(mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            panic!("controlled discovery ended before entry")
                        }
                    }
                }
            })
            .await
            .expect("controlled discovery must enter after the blocking worker is released");
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

    #[test]
    fn maps_every_analyzer_diagnostic_to_a_stable_run_diagnostic() {
        let cases = [
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
                    code: AnalyzerDiagnosticCode::UnreconstructableSpan,
                    path: Some("src/calc.py".into()),
                    line: Some(7),
                    column: Some(12),
                    message: None,
                },
                "analyzer.unreconstructable_span",
                "src/calc.py:7:12: source span could not be reconstructed",
            ),
            (
                AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::UnparseableReplacement,
                    path: Some("src/calc.py".into()),
                    line: Some(9),
                    column: Some(3),
                    message: None,
                },
                "analyzer.unparseable_replacement",
                "src/calc.py:9:3: generated replacement could not be parsed",
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
            (
                AnalyzerDiagnostic {
                    code: AnalyzerDiagnosticCode::InvalidRequest,
                    path: None,
                    line: None,
                    column: None,
                    message: None,
                },
                "analyzer.invalid_request",
                "analyzer request was invalid",
            ),
        ];

        for (diagnostic, expected_code, expected_message) in cases {
            let mapped = map_analyzer_diagnostic(diagnostic);
            assert_eq!(mapped.code, expected_code);
            assert_eq!(mapped.message, expected_message);
        }

        let mapped = map_analyzer_diagnostic(AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidRequest,
            path: None,
            line: None,
            column: None,
            message: Some("operators must not be empty".to_owned()),
        });
        assert_eq!(mapped.message, "operators must not be empty");
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
        let mut handler = AnalyzerHandler::new(root).unwrap();
        let operators = MutationOperatorSelection::default();
        let cancellation = ProcessCancellation::new();
        cancellation.cancel();

        let cancelled = handler
            .handle_with_cancellation(request(82), &operators, MutationProfile::Full, cancellation)
            .await;

        assert!(matches!(
            cancelled,
            Err(error) if error.id == EffectId(82) && error.failure.code() == "analyzer.cancelled"
        ));
        assert_eq!(handler.store.as_ref().unwrap().count(), 0);

        let finished = handler
            .handle(request(83), &operators, MutationProfile::Full)
            .await
            .unwrap();
        let spool = finished.spool.unwrap();

        assert_eq!(spool.records, 1);
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
        let mut handler = AnalyzerHandler::new(root)
            .unwrap()
            .with_candidate_spool_owner(owner.clone());
        let operators = MutationOperatorSelection::default();
        for id in 100..103 {
            let error = handler
                .handle(request(id), &operators, MutationProfile::Full)
                .await
                .unwrap_err();
            assert_eq!(error.id, EffectId(id));
            assert_eq!(error.failure.code(), "analyzer.depth");
            assert!(handler.store.is_none());
            assert_eq!(std::fs::read_dir(&spool_path).unwrap().count(), 0);
            assert_eq!(std::fs::read_to_string(&source_path).unwrap(), source);
        }
        std::fs::write(&source_path, "value = 1 + 2\n").unwrap();
        let output = handler
            .handle(request(103), &operators, MutationProfile::Full)
            .await
            .unwrap();
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
        let mut handler = AnalyzerHandler::new(root)
            .unwrap()
            .with_candidate_spool_owner(spool_owner.clone());
        handler.analysis_hook = Some(Arc::new(move || {
            hook_entered.wait();
            std::thread::sleep(Duration::from_millis(500));
        }));
        let operators = MutationOperatorSelection::default();
        let cancellation = ProcessCancellation::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            handler
                .handle_with_cancellation(
                    request(84),
                    &operators,
                    MutationProfile::Full,
                    task_cancellation,
                )
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
