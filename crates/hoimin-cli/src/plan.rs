use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{FingerprintInputFile, MutationCandidate, PlanConfig, RunConfig};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::analyzer::{AnalyzerDiagnostic, AnalyzerDiagnosticCode, discover_targets};
use crate::shell;
use crate::target::TargetHandler;

pub const PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanManifest {
    pub schema_version: u32,
    pub kind: String,
    pub normalized_config: PlanConfig,
    pub sources: Vec<FingerprintInputFile>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub candidates: Vec<MutationCandidate>,
    pub truncated: bool,
    pub diagnostics: Vec<PlanDiagnostic>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanDiagnostic {
    pub code: String,
    pub path: Option<Utf8PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct PlanOutput {
    pub manifest: PlanManifest,
    pub exit_code: i32,
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("plan.fingerprint_input: {0}")]
    FingerprintInput(String),
    #[error("plan.target.resolve: {0}")]
    TargetResolution(String),
    #[error("plan.source.read: {0}")]
    SourceRead(String),
    #[error("plan.discovery: {0}")]
    Discovery(String),
    #[error("plan.invalid_syntax: {0}")]
    InvalidSyntax(String),
}

/// Creates a read-only, versioned mutation candidate plan.
///
/// # Errors
///
/// Returns an error before manifest serialization when fingerprint inputs, targets, sources, or
/// analysis cannot be resolved successfully.
pub async fn create(config: RunConfig) -> Result<PlanOutput, PlanError> {
    let config = shell::prepare_run_config(config)
        .map_err(|error| PlanError::FingerprintInput(error.to_string()))?;
    let targets = TargetHandler::resolve(&config.selection)
        .await
        .map_err(|error| PlanError::TargetResolution(error.to_string()))?;
    let sources = source_records(&config.root, &targets).await?;
    let discovery = discover_targets(
        &config.root,
        &targets,
        &config.operators,
        config.profile,
        config.limits.max_candidates.get(),
    )
    .await
    .map_err(|error| PlanError::Discovery(error.failure.message()))?;
    if let Some(diagnostic) = discovery
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == AnalyzerDiagnosticCode::InvalidSyntax)
    {
        return Err(PlanError::InvalidSyntax(
            diagnostic
                .path
                .as_deref()
                .map_or_else(|| "unknown source".to_owned(), ToString::to_string),
        ));
    }
    let diagnostics = discovery.diagnostics.iter().map(plan_diagnostic).collect();
    let truncated = discovery.truncated;
    let manifest = PlanManifest {
        schema_version: PLAN_SCHEMA_VERSION,
        kind: "plan".to_owned(),
        normalized_config: config.clone().into_plan_config(),
        sources,
        fingerprint_inputs: config.fingerprint_inputs,
        candidates: discovery.candidates,
        truncated,
        diagnostics,
    };
    Ok(PlanOutput {
        manifest,
        exit_code: if truncated { 4 } else { 0 },
    })
}

async fn source_records(
    root: &Utf8Path,
    targets: &[hoimin_core::TargetSlice],
) -> Result<Vec<FingerprintInputFile>, PlanError> {
    let mut records = BTreeMap::new();
    for target in targets {
        let path = target.path.clone();
        let bytes = tokio::fs::read(root.join(&path))
            .await
            .map_err(|error| PlanError::SourceRead(format!("{path}: {error}")))?;
        records.insert(
            path.clone(),
            FingerprintInputFile {
                path,
                hash: blake3::hash(&bytes).to_hex().to_string(),
            },
        );
    }
    Ok(records.into_values().collect())
}

fn plan_diagnostic(diagnostic: &AnalyzerDiagnostic) -> PlanDiagnostic {
    PlanDiagnostic {
        code: match diagnostic.code {
            AnalyzerDiagnosticCode::InvalidSyntax => "invalid_syntax",
            AnalyzerDiagnosticCode::UnreconstructableSpan => "unreconstructable_span",
            AnalyzerDiagnosticCode::UnparseableReplacement => "unparseable_replacement",
            AnalyzerDiagnosticCode::CandidateLimitExceeded => "candidate_limit",
            AnalyzerDiagnosticCode::InvalidRequest => "invalid_request",
        }
        .to_owned(),
        path: diagnostic.path.clone(),
        line: diagnostic.line,
        column: diagnostic.column,
        message: diagnostic.message.clone().unwrap_or_default(),
    }
}
