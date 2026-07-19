mod protocol;
#[allow(dead_code)]
mod rust;
mod store;

pub use protocol::*;
pub use store::*;

use std::process::Stdio;

use camino::Utf8PathBuf;
use hoimin_core::{
    AnalysisFinished, AnalyzeFile, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, EffectFailed,
    MutationCandidate, ProcessLimits, validate_candidate,
};
use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::process::{
    ProcessCancellation, exit_termination, terminate_supervised, terminate_unattached_child,
    wait_after_termination,
};
use crate::resource::{ProcessSupervisor, ResourceBackend};

pub struct AnalyzerHandler {
    root: Utf8PathBuf,
    python: Utf8PathBuf,
    helper: Utf8PathBuf,
    _helper_dir: TempDir,
    store: Option<CandidateStore>,
    timeout: std::time::Duration,
    backend: ResourceBackend,
    limits: ProcessLimits,
}

struct AnalyzerBuild<'a> {
    timeout: std::time::Duration,
    backend: ResourceBackend,
    max_memory_bytes: u64,
    max_processes: u32,
    max_output_bytes: u64,
    helper_source: &'a str,
}

impl AnalyzerHandler {
    pub fn new(
        root: Utf8PathBuf,
        python: Utf8PathBuf,
        timeout: std::time::Duration,
    ) -> Result<Self, std::io::Error> {
        Self::build(
            root,
            python,
            AnalyzerBuild {
                timeout,
                backend: ResourceBackend::Portable(crate::resource::PortableBackend::for_tests()),
                max_memory_bytes: 1024 * 1024 * 1024,
                max_processes: 64,
                max_output_bytes: DEFAULT_MAX_ANALYZER_OUTPUT_BYTES as u64,
                helper_source: include_str!("../../../../python/hoimin_analyzer.py"),
            },
        )
    }

    pub fn with_backend(
        root: Utf8PathBuf,
        python: Utf8PathBuf,
        timeout: std::time::Duration,
        backend: ResourceBackend,
        max_memory_bytes: u64,
        max_processes: u32,
    ) -> Result<Self, std::io::Error> {
        Self::build(
            root,
            python,
            AnalyzerBuild {
                timeout,
                backend,
                max_memory_bytes,
                max_processes,
                max_output_bytes: DEFAULT_MAX_ANALYZER_OUTPUT_BYTES as u64,
                helper_source: include_str!("../../../../python/hoimin_analyzer.py"),
            },
        )
    }

    #[doc(hidden)]
    pub fn with_helper_source_for_tests(
        root: Utf8PathBuf,
        python: Utf8PathBuf,
        timeout: std::time::Duration,
        max_output_bytes: u64,
        helper_source: &str,
    ) -> Result<Self, std::io::Error> {
        Self::build(
            root,
            python,
            AnalyzerBuild {
                timeout,
                backend: ResourceBackend::Portable(crate::resource::PortableBackend::for_tests()),
                max_memory_bytes: 1024 * 1024 * 1024,
                max_processes: 8,
                max_output_bytes,
                helper_source,
            },
        )
    }

    fn build(
        root: Utf8PathBuf,
        python: Utf8PathBuf,
        options: AnalyzerBuild<'_>,
    ) -> Result<Self, std::io::Error> {
        let AnalyzerBuild {
            timeout,
            backend,
            max_memory_bytes,
            max_processes,
            max_output_bytes,
            helper_source,
        } = options;
        let helper_dir = tempfile::tempdir()?;
        let helper = Utf8PathBuf::from_path_buf(helper_dir.path().join("hoimin_analyzer.py"))
            .expect("temporary paths must be UTF-8");
        std::fs::write(&helper, helper_source)?;
        Ok(Self {
            root,
            python,
            helper,
            _helper_dir: helper_dir,
            store: None,
            timeout,
            backend,
            limits: ProcessLimits {
                timeout,
                max_output_bytes,
                max_memory_bytes,
                max_processes,
            },
        })
    }

    pub async fn handle(&mut self, request: AnalyzeFile) -> Result<AnalysisFinished, EffectFailed> {
        self.handle_with_cancellation(request, ProcessCancellation::new())
            .await
    }

    pub(crate) async fn handle_with_cancellation(
        &mut self,
        request: AnalyzeFile,
        cancellation: ProcessCancellation,
    ) -> Result<AnalysisFinished, EffectFailed> {
        let id = request.id;
        if self.store.is_none() {
            self.store = Some(
                CandidateStore::new(request.max_candidates).map_err(|error| {
                    EffectFailed::other(id, "analyzer.store", error.to_string())
                })?,
            );
        }
        let source = tokio::fs::read(self.root.join(&request.target.path))
            .await
            .map_err(|error| EffectFailed::other(id, "analyzer.source.read", error.to_string()))?;
        let module = String::from_utf8(source.clone())
            .map_err(|error| EffectFailed::other(id, "analyzer.source.utf8", error.to_string()))?;
        let remaining = request
            .max_candidates
            .saturating_sub(self.store.as_ref().map_or(0, CandidateStore::count));
        if remaining == 0 {
            return Err(EffectFailed::other(
                id,
                "analyzer.candidate_limit",
                "candidate limit reached before analysis completed",
            ));
        }
        let payload = json!({
            "effect_id": id.0,
            "path": request.target.path.as_str(),
            "module": module,
            "lines": request.target.lines.iter().map(|line| [line.start, line.end]).collect::<Vec<_>>(),
            "symbols": request.target.symbols,
            "max_candidates": remaining,
        });
        let mut command = Command::new(&self.python);
        command
            .arg(&self.helper)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut supervisor = self
            .backend
            .prepare(&mut command, self.limits)
            .map_err(|error| EffectFailed::other(id, "analyzer.resource", error.to_string()))?;
        let mut child = command
            .spawn()
            .map_err(|error| EffectFailed::other(id, "analyzer.spawn", error.to_string()))?;
        if let Err(error) = supervisor.attach(&child) {
            let cleanup = terminate_unattached_child(&mut child).await.err();
            return Err(EffectFailed::other(
                id,
                "analyzer.resource.attach",
                match cleanup {
                    Some(cleanup) => format!("{error}; child cleanup failed: {cleanup}"),
                    None => error.to_string(),
                },
            ));
        }
        let deadline = tokio::time::Instant::now() + self.timeout;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let encoded = match serde_json::to_vec(&payload) {
            Ok(encoded) => encoded,
            Err(error) => {
                return Err(abort_analyzer(
                    id,
                    &mut supervisor,
                    &mut child,
                    EffectFailed::other(id, "analyzer.request", error.to_string()),
                )
                .await);
            }
        };
        let delivery = async {
            stdin.write_all(&encoded).await?;
            stdin.write_all(b"\n").await
        };
        let delivery_result = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                return Err(abort_analyzer(
                    id,
                    &mut supervisor,
                    &mut child,
                    EffectFailed::other(id, "analyzer.cancelled", "analyzer was cancelled"),
                ).await);
            }
            () = tokio::time::sleep_until(deadline) => {
                return Err(abort_analyzer(
                    id,
                    &mut supervisor,
                    &mut child,
                    EffectFailed::other(id, "analyzer.timeout", "analyzer timed out"),
                ).await);
            }
            result = delivery => result,
        };
        if let Err(error) = delivery_result {
            return Err(abort_analyzer(
                id,
                &mut supervisor,
                &mut child,
                EffectFailed::other(id, "analyzer.stdin", error.to_string()),
            )
            .await);
        }
        drop(stdin);
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let mut protocol = AnalyzerProtocol::new(id);
        let store = self.store.as_mut().expect("store initialized");
        let joined = async {
            tokio::join!(
                child.wait(),
                drain_stdout(stdout, &mut protocol, store, &source, id),
                drain_bounded(stderr, self.limits.max_output_bytes as usize, id),
            )
        };
        let (status, stdout_result, stderr_result) = match tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                return Err(abort_analyzer(
                    id,
                    &mut supervisor,
                    &mut child,
                    EffectFailed::other(
                        id,
                        "analyzer.cancelled",
                        "analyzer was cancelled",
                    ),
                ).await);
            }
            result = tokio::time::timeout_at(deadline, joined) => result,
        } {
            Ok(values) => values,
            Err(_) => {
                return Err(abort_analyzer(
                    id,
                    &mut supervisor,
                    &mut child,
                    EffectFailed::other(id, "analyzer.timeout", "analyzer timed out"),
                )
                .await);
            }
        };
        let status =
            status.map_err(|error| EffectFailed::other(id, "analyzer.wait", error.to_string()))?;
        let classified = supervisor
            .classify(exit_termination(status))
            .map_err(|error| {
                EffectFailed::other(id, "analyzer.resource.classify", error.to_string())
            });
        let terminated = terminate_supervised(id, &mut supervisor, false);
        let termination = classified?;
        terminated?;
        stdout_result?;
        let stderr = stderr_result?;
        if termination != hoimin_core::ProcessTermination::Exit(0) {
            return Err(EffectFailed::other(
                id,
                "analyzer.failed",
                String::from_utf8_lossy(&stderr).into_owned(),
            ));
        }
        let summary = protocol
            .finish()
            .map_err(|error| EffectFailed::other(id, "analyzer.protocol", error.to_string()))?;
        let spool = if request.final_target {
            Some(
                self.store
                    .take()
                    .expect("store initialized")
                    .finish()
                    .map_err(|error| {
                        EffectFailed::other(id, "analyzer.store", error.to_string())
                    })?,
            )
        } else {
            None
        };
        Ok(AnalysisFinished {
            id,
            spool,
            truncated: summary.truncated,
        })
    }
}

async fn abort_analyzer(
    id: hoimin_core::EffectId,
    supervisor: &mut ProcessSupervisor,
    child: &mut tokio::process::Child,
    primary: EffectFailed,
) -> EffectFailed {
    let cleanup = match terminate_supervised(id, supervisor, true) {
        Ok(()) => wait_after_termination(id, child).await,
        Err(supervisor_error) => match terminate_unattached_child(child).await {
            Ok(()) => Err(supervisor_error),
            Err(child_error) => Err(EffectFailed::other(
                id,
                supervisor_error.failure.code(),
                format!(
                    "{}; fallback child cleanup failed: {}",
                    supervisor_error.failure.message(),
                    child_error
                ),
            )),
        },
    };
    match cleanup {
        Ok(()) => primary,
        Err(error) => EffectFailed::other(
            id,
            primary.failure.code(),
            format!(
                "{}; analyzer cleanup failed: {}",
                primary.failure.message(),
                error.failure.message()
            ),
        ),
    }
}

async fn drain_stdout(
    mut stdout: impl AsyncRead + Unpin,
    protocol: &mut AnalyzerProtocol,
    store: &mut CandidateStore,
    source: &[u8],
    id: hoimin_core::EffectId,
) -> Result<(), EffectFailed> {
    let mut chunk = [0_u8; 8192];
    let mut line = Vec::new();
    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .map_err(|error| EffectFailed::other(id, "analyzer.stdout", error.to_string()))?;
        if read == 0 {
            break;
        }
        for byte in &chunk[..read] {
            line.push(*byte);
            if line.len() > DEFAULT_MAX_ANALYZER_LINE_BYTES {
                return Err(EffectFailed::other(
                    id,
                    "analyzer.protocol",
                    format!("analyzer JSONL line exceeds {DEFAULT_MAX_ANALYZER_LINE_BYTES} bytes"),
                ));
            }
            if *byte == b'\n' {
                accept_line(protocol, store, source, id, &line)?;
                line.clear();
            }
        }
    }
    if !line.is_empty() {
        accept_line(protocol, store, source, id, &line)?;
    }
    Ok(())
}

fn accept_line(
    protocol: &mut AnalyzerProtocol,
    store: &mut CandidateStore,
    source: &[u8],
    id: hoimin_core::EffectId,
    line: &[u8],
) -> Result<(), EffectFailed> {
    let record = protocol
        .receive_line(line)
        .map_err(|error| EffectFailed::other(id, "analyzer.protocol", error.to_string()))?;
    let Some(AnalyzerRecord::Candidate(candidate)) = record else {
        return Ok(());
    };
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
        file_hash: blake3::hash(source).to_hex().to_string(),
    };
    let mutant_id = validate_candidate(source, &descriptor)
        .map_err(|error| EffectFailed::other(id, "analyzer.candidate", error.to_string()))?;
    let sequence = store.count() + 1;
    store
        .push(&MutationCandidate {
            id: mutant_id.to_string(),
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
        .map_err(|error| EffectFailed::other(id, "analyzer.store", error.to_string()))
}

async fn drain_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
    id: hoimin_core::EffectId,
) -> Result<Vec<u8>, EffectFailed> {
    let mut result = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|error| EffectFailed::other(id, "analyzer.stderr", error.to_string()))?;
        if read == 0 {
            return Ok(result);
        }
        if result.len().saturating_add(read) > limit {
            return Err(EffectFailed::other(
                id,
                "analyzer.stderr.limit",
                format!("analyzer stderr exceeds {limit} bytes"),
            ));
        }
        result.extend_from_slice(&chunk[..read]);
    }
}
