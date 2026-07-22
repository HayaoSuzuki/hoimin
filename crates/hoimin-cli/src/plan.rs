use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    CandidateDescriptor, FingerprintInputFile, MutationCandidate, OutputConfig, PlanConfig,
    RunConfig, TargetSlice, normalized_relative_path, validate_candidate,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::analyzer::{AnalyzerDiagnostic, AnalyzerDiagnosticCode, discover_targets};
use crate::cli::OutputFormat;
use crate::fingerprint_inputs;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPlan {
    pub config: RunConfig,
    pub candidate_ids: BTreeSet<String>,
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("plan.manifest.invalid: {0}")]
    ManifestInvalid(String),
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
    #[error("plan.source.changed: {0}")]
    SourceChanged(String),
    #[error("plan.fingerprint_input.changed: {0}")]
    FingerprintInputChanged(String),
    #[error("plan.candidate.invalid: {0}")]
    CandidateInvalid(String),
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

/// Validates a plan manifest and prepares its immutable run configuration for verification.
///
/// # Errors
///
/// Returns an error when the manifest is malformed, the current target or fingerprint-input
/// records differ, or a requested candidate no longer has the exact planned descriptor.
pub async fn prepare_verify(
    manifest_path: impl AsRef<Path>,
    requested_ids: &[String],
    format: OutputFormat,
) -> Result<VerifiedPlan, PlanError> {
    let manifest_path = manifest_path.as_ref();
    let bytes = std::fs::read(manifest_path).map_err(|error| {
        PlanError::ManifestInvalid(format!("{}: {error}", manifest_path.display()))
    })?;
    let manifest: PlanManifest = serde_json::from_slice(&bytes)
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    validate_header(&manifest)?;

    let candidate_ids = normalize_requested_ids(
        requested_ids,
        manifest.normalized_config.limits.max_mutants.get(),
    )?;
    let mut config = manifest
        .normalized_config
        .clone()
        .into_run_config(output_config(format));
    let targets = TargetHandler::resolve(&config.selection)
        .await
        .map_err(|error| PlanError::SourceChanged(error.to_string()))?;
    let current_sources = source_records(&config.root, &targets)
        .await
        .map_err(|error| PlanError::SourceChanged(error.to_string()))?;
    ensure_exact_records(&manifest.sources, &current_sources, RecordMismatch::Source)?;
    let current_inputs = fingerprint_inputs::resolve(
        &config.root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
    )
    .map_err(|error| PlanError::FingerprintInputChanged(error.to_string()))?;
    ensure_exact_records(
        &manifest.fingerprint_inputs,
        &current_inputs,
        RecordMismatch::FingerprintInput,
    )?;
    config.fingerprint_inputs = current_inputs;
    validate_requested_candidates(&manifest, &candidate_ids, &config, &targets).await?;

    Ok(VerifiedPlan {
        config,
        candidate_ids,
    })
}

async fn source_records(
    root: &Utf8Path,
    targets: &[TargetSlice],
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

fn validate_header(manifest: &PlanManifest) -> Result<(), PlanError> {
    if manifest.schema_version != PLAN_SCHEMA_VERSION {
        return Err(PlanError::ManifestInvalid(format!(
            "unsupported schema version {}",
            manifest.schema_version
        )));
    }
    if manifest.kind != "plan" {
        return Err(PlanError::ManifestInvalid(format!(
            "unexpected kind {}",
            manifest.kind
        )));
    }
    if manifest.normalized_config.selection.root != manifest.normalized_config.root {
        return Err(PlanError::ManifestInvalid(
            "selection root differs from normalized config root".to_owned(),
        ));
    }
    validate_records("sources", &manifest.sources)?;
    validate_records("fingerprint_inputs", &manifest.fingerprint_inputs)?;
    validate_records(
        "normalized_config.fingerprint_inputs",
        &manifest.normalized_config.fingerprint_inputs,
    )?;
    if record_map(&manifest.fingerprint_inputs)
        != record_map(&manifest.normalized_config.fingerprint_inputs)
    {
        return Err(PlanError::ManifestInvalid(
            "fingerprint inputs differ from normalized config".to_owned(),
        ));
    }

    let mut candidate_ids = BTreeSet::new();
    for candidate in &manifest.candidates {
        if !valid_candidate_id(&candidate.id) {
            return Err(PlanError::CandidateInvalid(format!(
                "invalid candidate id {}",
                candidate.id
            )));
        }
        if !candidate_ids.insert(candidate.id.clone()) {
            return Err(PlanError::CandidateInvalid(format!(
                "duplicate candidate id {}",
                candidate.id
            )));
        }
        if !normalized_relative_path(candidate.path.as_str()) {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate path is not root-relative: {}",
                candidate.path
            )));
        }
    }
    Ok(())
}

fn validate_records(name: &str, records: &[FingerprintInputFile]) -> Result<(), PlanError> {
    let mut paths = BTreeSet::new();
    for record in records {
        if !normalized_relative_path(record.path.as_str()) {
            return Err(PlanError::ManifestInvalid(format!(
                "{name} path is not root-relative: {}",
                record.path
            )));
        }
        if !valid_hash(&record.hash) {
            return Err(PlanError::ManifestInvalid(format!(
                "{name} hash is not a BLAKE3 hex digest: {}",
                record.path
            )));
        }
        if !paths.insert(record.path.clone()) {
            return Err(PlanError::ManifestInvalid(format!(
                "{name} contains duplicate path: {}",
                record.path
            )));
        }
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_candidate_id(value: &str) -> bool {
    value.strip_prefix("m1_").is_some_and(valid_hash)
}

fn normalize_requested_ids(
    requested_ids: &[String],
    max_mutants: usize,
) -> Result<BTreeSet<String>, PlanError> {
    let mut candidate_ids = BTreeSet::new();
    for id in requested_ids {
        if !valid_candidate_id(id) {
            return Err(PlanError::CandidateInvalid(format!(
                "invalid requested candidate id {id}"
            )));
        }
        candidate_ids.insert(id.clone());
    }
    if candidate_ids.is_empty() {
        return Err(PlanError::CandidateInvalid(
            "at least one candidate id is required".to_owned(),
        ));
    }
    if candidate_ids.len() > max_mutants {
        return Err(PlanError::CandidateInvalid(format!(
            "requested {} candidates exceeds max_mutants {max_mutants}",
            candidate_ids.len()
        )));
    }
    Ok(candidate_ids)
}

#[derive(Clone, Copy)]
enum RecordMismatch {
    Source,
    FingerprintInput,
}

fn ensure_exact_records(
    expected: &[FingerprintInputFile],
    current: &[FingerprintInputFile],
    kind: RecordMismatch,
) -> Result<(), PlanError> {
    if record_map(expected) == record_map(current) {
        return Ok(());
    }
    match kind {
        RecordMismatch::Source => Err(PlanError::SourceChanged(
            "planned target source records do not match the current workspace".to_owned(),
        )),
        RecordMismatch::FingerprintInput => Err(PlanError::FingerprintInputChanged(
            "planned fingerprint input records do not match the current workspace".to_owned(),
        )),
    }
}

fn record_map(records: &[FingerprintInputFile]) -> BTreeMap<Utf8PathBuf, String> {
    records
        .iter()
        .map(|record| (record.path.clone(), record.hash.clone()))
        .collect()
}

async fn validate_requested_candidates(
    manifest: &PlanManifest,
    requested_ids: &BTreeSet<String>,
    config: &RunConfig,
    targets: &[TargetSlice],
) -> Result<(), PlanError> {
    let candidates = manifest
        .candidates
        .iter()
        .map(|candidate| (candidate.id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut source_bytes = BTreeMap::new();
    for candidate_id in requested_ids {
        let candidate = candidates.get(candidate_id.as_str()).ok_or_else(|| {
            PlanError::CandidateInvalid(format!("candidate id is not in the plan: {candidate_id}"))
        })?;
        if !targets.iter().any(|target| target.path == candidate.path) {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate target is not selected: {}",
                candidate.path
            )));
        }
        let source = if let Some(source) = source_bytes.get(&candidate.path) {
            source
        } else {
            let source = tokio::fs::read(config.root.join(&candidate.path))
                .await
                .map_err(|error| {
                    PlanError::CandidateInvalid(format!("{}: {error}", candidate.path))
                })?;
            source_bytes.insert(candidate.path.clone(), source);
            source_bytes
                .get(&candidate.path)
                .expect("source bytes inserted for candidate path")
        };
        let descriptor = candidate_descriptor(candidate);
        let stable_id = validate_candidate(source, &descriptor)
            .map_err(|error| PlanError::CandidateInvalid(error.to_string()))?;
        if stable_id.as_str() != candidate.id {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate stable id differs for {}",
                candidate.id
            )));
        }
    }

    let discovery = discover_targets(
        &config.root,
        targets,
        &config.operators,
        config.profile,
        config.limits.max_candidates.get(),
    )
    .await
    .map_err(|error| PlanError::CandidateInvalid(error.failure.message()))?;
    let discovered = discovery
        .candidates
        .iter()
        .map(|candidate| (candidate.id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    for candidate_id in requested_ids {
        let planned = candidates
            .get(candidate_id.as_str())
            .expect("requested candidate was validated as present");
        let current = discovered.get(candidate_id.as_str()).ok_or_else(|| {
            PlanError::CandidateInvalid(format!(
                "candidate is not discoverable under the planned configuration: {candidate_id}"
            ))
        })?;
        if *current != *planned {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate descriptor differs for {candidate_id}"
            )));
        }
    }
    Ok(())
}

fn candidate_descriptor(candidate: &MutationCandidate) -> CandidateDescriptor {
    CandidateDescriptor {
        schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
        path: candidate.path.clone(),
        span: candidate.span,
        original: candidate.original.clone(),
        replacement: candidate.replacement.clone(),
        operator: candidate.operator.clone(),
        line: candidate.line,
        column: candidate.column,
        symbol: candidate.symbol.clone(),
        file_hash: candidate.file_hash.clone(),
    }
}

fn output_config(format: OutputFormat) -> OutputConfig {
    OutputConfig {
        format: match format {
            OutputFormat::Json => hoimin_core::OutputFormat::Json,
            OutputFormat::Jsonl => hoimin_core::OutputFormat::Jsonl,
            OutputFormat::Human => hoimin_core::OutputFormat::Human,
        },
        metrics: None,
    }
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
