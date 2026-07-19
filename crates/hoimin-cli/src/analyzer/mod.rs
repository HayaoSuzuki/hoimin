mod protocol;
#[allow(dead_code)]
mod rust;
mod store;

pub use protocol::*;
pub use store::*;

use camino::Utf8PathBuf;
use hoimin_core::{
    AnalysisFinished, AnalyzeFile, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, EffectFailed,
    MutationCandidate, validate_candidate,
};

use crate::process::ProcessCancellation;
use crate::resource::ResourceBackend;

pub struct AnalyzerHandler {
    root: Utf8PathBuf,
    store: Option<CandidateStore>,
}

impl AnalyzerHandler {
    pub fn new(
        root: Utf8PathBuf,
        _python: Utf8PathBuf,
        _timeout: std::time::Duration,
    ) -> Result<Self, std::io::Error> {
        Ok(Self { root, store: None })
    }

    pub fn with_backend(
        root: Utf8PathBuf,
        _python: Utf8PathBuf,
        _timeout: std::time::Duration,
        _backend: ResourceBackend,
        _max_memory_bytes: u64,
        _max_processes: u32,
    ) -> Result<Self, std::io::Error> {
        Ok(Self { root, store: None })
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
        let source = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                return Err(EffectFailed::other(id, "analyzer.cancelled", "analyzer was cancelled"));
            }
            result = tokio::fs::read(self.root.join(&request.target.path)) => result
                .map_err(|error| EffectFailed::other(id, "analyzer.source.read", error.to_string()))?,
        };
        let module = String::from_utf8(source.clone())
            .map_err(|error| EffectFailed::other(id, "analyzer.source.utf8", error.to_string()))?;
        let store = self.store.as_mut().expect("store initialized");
        let remaining = request.max_candidates.saturating_sub(store.count());
        if remaining == 0 {
            return Err(EffectFailed::other(
                id,
                "analyzer.candidate_limit",
                "candidate limit reached before analysis completed",
            ));
        }
        let max_candidates = usize::try_from(remaining).unwrap_or(usize::MAX);
        let output = rust::analyze_source(
            &rust::AnalyzeRequest {
                path: &request.target.path,
                lines: &request.target.lines,
                symbols: &request.target.symbols,
                max_candidates,
            },
            &module,
        );
        for candidate in output.candidates {
            accept_candidate(store, &source, id, candidate)?;
        }
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
            truncated: output.truncated,
        })
    }
}

fn accept_candidate(
    store: &mut CandidateStore,
    source: &[u8],
    id: hoimin_core::EffectId,
    candidate: AnalyzerCandidate,
) -> Result<(), EffectFailed> {
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
