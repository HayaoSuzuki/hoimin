use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    CandidateDescriptor, FingerprintInputFile, MutationCandidate, OutputConfig, PlanConfig,
    RunConfig, TargetSlice, VerificationSelection, VerificationSelectionMode,
    VerificationSelectionPolicy, VerificationSelectionScope as ReportVerificationSelectionScope,
    normalized_relative_path, validate_candidate,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::analyzer::{AnalyzerDiagnostic, AnalyzerDiagnosticCode, discover_targets};
use crate::cli::{OutputFormat, TopSelectionPolicy, VerifySelection};
use crate::fingerprint_inputs;
use crate::resource::{self, ResourceError};
use crate::shell;
use crate::target::TargetHandler;

mod ranking;
#[cfg(test)]
mod ranking_tests;
mod selection;
#[cfg(test)]
mod selection_tests;

use ranking::{RANKING_RULE_VERSION, rank_candidates, validate_ranking, validate_ranking_against};
pub use ranking::{RankedPlanCandidate, RankingReason, RankingReasonCode};

pub const PLAN_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanManifest {
    pub schema_version: u32,
    pub kind: String,
    pub ranking_rule_version: u32,
    pub normalized_config: PlanConfig,
    pub sources: Vec<FingerprintInputFile>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub candidates: Vec<RankedPlanCandidate>,
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
    pub(crate) fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    pub selection: ResolvedVerifySelection,
    pub selection_scope: VerifySelectionScope,
    pub plan_truncated: bool,
    pub verification_selection: VerificationSelection,
}

impl VerifiedPlan {
    #[doc(hidden)]
    #[must_use]
    pub fn fingerprint_copy_inputs(&self) -> BTreeSet<Utf8PathBuf> {
        self.fingerprint_copy_inputs.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedVerifySelection {
    ExplicitCandidates(BTreeSet<String>),
    RankedCandidates(Vec<String>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifySelectionScope {
    ExplicitCandidates,
    RetainedCandidates,
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error(transparent)]
    ResourcePolicy(#[from] ResourceError),
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
    #[error("plan.workspace: {0}")]
    Workspace(String),
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
    resource::validate_plan_resource_policy(config.allow_best_effort_memory)?;
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
    let candidates = rank_candidates(&config.selection, &targets, discovery.candidates);
    let manifest = PlanManifest {
        schema_version: PLAN_SCHEMA_VERSION,
        kind: "plan".to_owned(),
        ranking_rule_version: RANKING_RULE_VERSION,
        normalized_config: config.clone().into_plan_config(),
        sources,
        fingerprint_inputs: config.fingerprint_inputs,
        candidates,
        truncated,
        diagnostics,
    };
    plan_output(manifest)
}

fn plan_output(manifest: PlanManifest) -> Result<PlanOutput, PlanError> {
    let exit_code = if manifest.truncated { 4 } else { 0 };
    validate_header(&manifest)?;
    Ok(PlanOutput {
        manifest,
        exit_code,
    })
}

/// Validates a plan manifest and prepares its immutable run configuration for verification.
///
/// # Errors
///
/// Returns an error when the manifest is malformed or has invalid normalized configuration, the
/// current target or fingerprint-input records differ, or a requested candidate no longer has the
/// exact planned descriptor.
pub async fn prepare_verify(
    manifest_path: impl AsRef<Path>,
    requested_ids: &[String],
    format: OutputFormat,
) -> Result<VerifiedPlan, PlanError> {
    prepare_verify_selection(
        manifest_path,
        &VerifySelection::CandidateIds(requested_ids.to_vec()),
        format,
    )
    .await
}

/// Validates a plan and resolves an explicit-ID or ranked top-N verification selection.
///
/// # Errors
///
/// Returns an error when the manifest, selection, sources, or planned candidates are invalid.
pub async fn prepare_verify_selection(
    manifest_path: impl AsRef<Path>,
    requested_selection: &VerifySelection,
    format: OutputFormat,
) -> Result<VerifiedPlan, PlanError> {
    let manifest_path = manifest_path.as_ref();
    let bytes = std::fs::read(manifest_path).map_err(|error| {
        PlanError::ManifestInvalid(format!("{}: {error}", manifest_path.display()))
    })?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    let schema_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PlanError::ManifestInvalid("missing schema version".to_owned()))?;
    if schema_version != u64::from(PLAN_SCHEMA_VERSION) {
        return Err(PlanError::ManifestInvalid(format!(
            "unsupported schema version {schema_version}; regenerate the plan with this hoimin version"
        )));
    }
    let manifest: PlanManifest = serde_json::from_value(value)
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    validate_header(&manifest)?;
    manifest
        .normalized_config
        .validate()
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;

    let (selection, selection_scope, verification_selection) =
        resolve_verify_selection(&manifest, requested_selection)?;
    let candidate_ids = match &selection {
        ResolvedVerifySelection::ExplicitCandidates(candidate_ids) => candidate_ids.clone(),
        ResolvedVerifySelection::RankedCandidates(candidate_ids) => {
            candidate_ids.iter().cloned().collect()
        }
    };
    let mut config = manifest
        .normalized_config
        .clone()
        .into_run_config(output_config(format));
    let targets = TargetHandler::resolve(&config.selection)
        .await
        .map_err(|error| PlanError::SourceChanged(error.to_string()))?;
    validate_ranking_against(
        &manifest.normalized_config.selection,
        &targets,
        &manifest.candidates,
    )
    .map_err(PlanError::ManifestInvalid)?;
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
    let copy_options = crate::workspace::CopyOptions {
        includes: config.selection.includes.clone(),
        excludes: config.selection.excludes.clone(),
    };
    let copy_manifest = crate::workspace::build_validation_manifest(&config.root, &copy_options)
        .map_err(|error| PlanError::Workspace(error.to_string()))?;
    let fingerprint_copy_inputs = config
        .fingerprint_inputs
        .iter()
        .filter(|record| copy_manifest.entry(&record.path).is_some())
        .map(|record| record.path.clone())
        .collect();
    validate_requested_candidates(&manifest, &candidate_ids, &config, &targets).await?;

    Ok(VerifiedPlan {
        config,
        fingerprint_copy_inputs,
        verification_selection,
        selection,
        selection_scope,
        plan_truncated: manifest.truncated,
    })
}

fn resolve_verify_selection(
    manifest: &PlanManifest,
    requested_selection: &VerifySelection,
) -> Result<
    (
        ResolvedVerifySelection,
        VerifySelectionScope,
        VerificationSelection,
    ),
    PlanError,
> {
    let max_mutants = manifest.normalized_config.limits.max_mutants.get();
    let (selection, selection_scope, mode, policy, requested) = match requested_selection {
        VerifySelection::CandidateIds(requested_ids) => (
            ResolvedVerifySelection::ExplicitCandidates(normalize_requested_ids(
                requested_ids,
                max_mutants,
            )?),
            VerifySelectionScope::ExplicitCandidates,
            VerificationSelectionMode::CandidateIds,
            VerificationSelectionPolicy::ExplicitCandidates,
            requested_ids.iter().collect::<BTreeSet<_>>().len(),
        ),
        VerifySelection::Top { count, policy } => {
            let candidate_ids =
                selection::select_top_candidate_ids(&manifest.candidates, *count, *policy);
            let report_policy = match policy {
                TopSelectionPolicy::Strict => VerificationSelectionPolicy::Strict,
                TopSelectionPolicy::Diverse => VerificationSelectionPolicy::FileRoundRobinV1,
            };
            if candidate_ids.len() > max_mutants {
                return Err(PlanError::CandidateInvalid(format!(
                    "selected {} candidates exceeds max_mutants {max_mutants}",
                    candidate_ids.len()
                )));
            }
            (
                ResolvedVerifySelection::RankedCandidates(candidate_ids),
                VerifySelectionScope::RetainedCandidates,
                VerificationSelectionMode::Top,
                report_policy,
                count.get(),
            )
        }
    };
    let selected = match &selection {
        ResolvedVerifySelection::ExplicitCandidates(ids) => ids.len(),
        ResolvedVerifySelection::RankedCandidates(ids) => ids.len(),
    };
    let scope = match selection_scope {
        VerifySelectionScope::ExplicitCandidates => {
            ReportVerificationSelectionScope::ExplicitCandidates
        }
        VerifySelectionScope::RetainedCandidates => {
            ReportVerificationSelectionScope::RetainedCandidates
        }
    };
    Ok((
        selection,
        selection_scope,
        VerificationSelection {
            mode,
            policy,
            requested,
            selected,
            scope,
            plan_truncated: manifest.truncated,
        },
    ))
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
            "unsupported schema version {}; regenerate the plan with this hoimin version",
            manifest.schema_version
        )));
    }
    if manifest.kind != "plan" {
        return Err(PlanError::ManifestInvalid(format!(
            "unexpected kind {}",
            manifest.kind
        )));
    }
    if manifest.ranking_rule_version != RANKING_RULE_VERSION {
        return Err(PlanError::ManifestInvalid(format!(
            "unsupported ranking rule version {}",
            manifest.ranking_rule_version
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

    validate_ranking(&manifest.candidates).map_err(PlanError::ManifestInvalid)?;
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
        .map(|candidate| (candidate.id.as_str(), &candidate.candidate))
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

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use hoimin_core::{ByteSpan, CommandArg, MutationCandidate, RawRunConfig, RunConfig};

    use super::{
        PLAN_SCHEMA_VERSION, PlanError, PlanManifest, RANKING_RULE_VERSION, RankedPlanCandidate,
        plan_output,
    };

    fn manifest() -> PlanManifest {
        let normalized_config = RunConfig::try_from(RawRunConfig {
            files: vec![Utf8PathBuf::from("src/calc.py")],
            test_argv: vec![CommandArg::Unix(b"test".to_vec())],
            ..RawRunConfig::default()
        })
        .expect("test config is valid")
        .into_plan_config();
        PlanManifest {
            schema_version: PLAN_SCHEMA_VERSION,
            kind: "plan".to_owned(),
            ranking_rule_version: RANKING_RULE_VERSION,
            normalized_config,
            sources: Vec::new(),
            fingerprint_inputs: Vec::new(),
            candidates: Vec::new(),
            truncated: true,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn plan_output_validates_ranking_and_preserves_truncated_exit_code() {
        let output = plan_output(manifest()).expect("empty candidate ranking is valid");
        assert_eq!(output.exit_code, 4);

        let mut malformed = output.manifest;
        malformed.candidates.push(RankedPlanCandidate {
            candidate: MutationCandidate {
                id: format!("m1_{}", "0".repeat(64)),
                sequence: 1,
                path: Utf8PathBuf::from("src/calc.py"),
                span: ByteSpan {
                    start: 0,
                    length: 1,
                },
                original: "x".to_owned(),
                replacement: "y".to_owned(),
                operator: "binary_add_sub".to_owned(),
                line: 1,
                column: 0,
                symbol: None,
                file_hash: "0".repeat(64),
            },
            rank: 1,
            score: 0,
            ranking_reasons: Vec::new(),
        });

        let error = plan_output(malformed).expect_err("missing operator reason must be rejected");
        assert!(
            matches!(error, PlanError::ManifestInvalid(message) if message == "candidate ranking must contain exactly one operator reason, got 0")
        );
    }
}
