use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    CandidateDescriptor, CandidateValidationContext, FingerprintInputFile, MutationCandidate,
    OutputConfig, PlanConfig, RunConfig, TargetSlice, VerificationSelection,
    VerificationSelectionMode, VerificationSelectionPolicy,
    VerificationSelectionScope as ReportVerificationSelectionScope, normalized_relative_path,
    validate_candidate_with_context,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(not(test))]
use crate::analyzer::discover_targets_with_timeout;
use crate::analyzer::{AnalyzerDiagnostic, AnalyzerDiagnosticCode, Discovery};
#[cfg(test)]
use crate::analyzer::{DiscoveryControl, discover_targets_with_control};
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
#[cfg(test)]
mod validation_tests;

use ranking::{RANKING_RULE_VERSION, rank_candidates, validate_ranking, validate_ranking_against};
pub use ranking::{RankedPlanCandidate, RankingReason, RankingReasonCode};

pub const PLAN_SCHEMA_VERSION: u32 = 4;

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

#[derive(Clone, Copy)]
enum DiscoveryCaller {
    Create,
    Verify,
}

fn map_discovery_error(error: &hoimin_core::EffectFailed, caller: DiscoveryCaller) -> PlanError {
    let message = error.failure.message();
    if error.failure.code() == "analyzer.timeout" {
        return PlanError::Discovery(format!("analyzer.timeout: {message}"));
    }
    match caller {
        DiscoveryCaller::Create => PlanError::Discovery(message),
        DiscoveryCaller::Verify => PlanError::CandidateInvalid(message),
    }
}

async fn discover_plan_targets(
    root: &Utf8Path,
    targets: &[TargetSlice],
    operators: &hoimin_core::MutationOperatorSelection,
    profile: hoimin_core::MutationProfile,
    max_candidates: usize,
    analyzer_timeout: std::time::Duration,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<Discovery, hoimin_core::EffectFailed> {
    #[cfg(test)]
    {
        discover_targets_with_control(
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
    #[cfg(not(test))]
    {
        discover_targets_with_timeout(
            root,
            targets,
            operators,
            profile,
            max_candidates,
            analyzer_timeout,
        )
        .await
    }
}

/// Creates a read-only, versioned mutation candidate plan.
///
/// # Errors
///
/// Returns an error before manifest serialization when fingerprint inputs, targets, sources, or
/// analysis cannot be resolved successfully.
pub async fn create(config: RunConfig) -> Result<PlanOutput, PlanError> {
    create_inner(
        config,
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(test)]
async fn create_with_discovery_control(
    config: RunConfig,
    control: Option<DiscoveryControl>,
) -> Result<PlanOutput, PlanError> {
    create_inner(config, control).await
}

async fn create_inner(
    config: RunConfig,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<PlanOutput, PlanError> {
    resource::validate_plan_resource_policy(config.allow_best_effort_memory)?;
    let config = shell::prepare_run_config(config)
        .map_err(|error| PlanError::FingerprintInput(error.to_string()))?;
    let targets = TargetHandler::resolve(&config.selection)
        .await
        .map_err(|error| PlanError::TargetResolution(error.to_string()))?;
    let sources = source_records(&config.root, &targets).await?;
    let discovery = discover_plan_targets(
        &config.root,
        &targets,
        &config.operators,
        config.profile,
        config.limits.max_candidates.get(),
        config.limits.analyzer_timeout.get(),
        #[cfg(test)]
        control,
    )
    .await
    .map_err(|error| map_discovery_error(&error, DiscoveryCaller::Create))?;
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
/// Returns an error when the manifest, selection, sources, or planned candidates are invalid,
/// including a top-N selection from a plan with no retained candidates.
pub async fn prepare_verify_selection(
    manifest_path: impl AsRef<Path>,
    requested_selection: &VerifySelection,
    format: OutputFormat,
) -> Result<VerifiedPlan, PlanError> {
    prepare_verify_selection_inner(
        manifest_path.as_ref(),
        requested_selection,
        format,
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(test)]
async fn prepare_verify_with_discovery_control(
    manifest_path: impl AsRef<Path>,
    requested_ids: Vec<String>,
    format: OutputFormat,
    control: Option<DiscoveryControl>,
) -> Result<VerifiedPlan, PlanError> {
    prepare_verify_selection_inner(
        manifest_path.as_ref(),
        &VerifySelection::CandidateIds(requested_ids),
        format,
        control,
    )
    .await
}

async fn prepare_verify_selection_inner(
    manifest_path: &Path,
    requested_selection: &VerifySelection,
    format: OutputFormat,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<VerifiedPlan, PlanError> {
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
    validate_ranking_against(
        &manifest.normalized_config.selection,
        &targets,
        &manifest.candidates,
    )
    .map_err(PlanError::ManifestInvalid)?;
    config.fingerprint_inputs = current_inputs;
    let copy_options = crate::workspace::CopyOptions {
        includes: config.selection.includes.clone(),
        excludes: config.selection.excludes.clone(),
        literal_exclusions: Vec::new(),
    };
    let copy_manifest = crate::workspace::build_validation_manifest(&config.root, &copy_options)
        .map_err(|error| PlanError::Workspace(error.to_string()))?;
    let fingerprint_copy_inputs = config
        .fingerprint_inputs
        .iter()
        .filter(|record| copy_manifest.entry(&record.path).is_some())
        .map(|record| record.path.clone())
        .collect();
    validate_requested_candidates(
        &manifest,
        &candidate_ids,
        &config,
        &targets,
        #[cfg(test)]
        control,
        #[cfg(test)]
        None,
    )
    .await?;

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
            if candidate_ids.is_empty() {
                return Err(PlanError::CandidateInvalid(
                    "--top requires at least one candidate; the plan has no retained candidates"
                        .to_owned(),
                ));
            }
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
            "unsupported ranking rule version {}; regenerate the plan with this hoimin version",
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
    validate_candidate_sequences(&manifest.candidates)?;
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
        crate::analyzer::CandidateStore::record_size(&candidate.candidate).map_err(|error| {
            PlanError::CandidateInvalid(format!(
                "{}:{} ({}): {error}",
                candidate.path, candidate.line, candidate.operator
            ))
        })?;
    }
    Ok(())
}

fn validate_candidate_sequences(candidates: &[RankedPlanCandidate]) -> Result<(), PlanError> {
    let mut sequences = candidates
        .iter()
        .map(|candidate| candidate.sequence)
        .collect::<Vec<_>>();
    sequences.sort_unstable();
    for (index, sequence) in sequences.into_iter().enumerate() {
        let expected = u64::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or_else(|| {
                PlanError::CandidateInvalid("candidate sequence count overflow".to_owned())
            })?;
        if sequence != expected {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate sequences must be a permutation of 1..={}; expected {expected}, got {sequence}",
                candidates.len()
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

#[cfg(test)]
#[derive(Default, Debug)]
struct ValidationStats {
    contexts: usize,
    source_bytes: usize,
}

async fn validate_requested_candidates(
    manifest: &PlanManifest,
    requested_ids: &BTreeSet<String>,
    config: &RunConfig,
    targets: &[TargetSlice],
    #[cfg(test)] control: Option<DiscoveryControl>,
    #[cfg(test)] stats: Option<&mut ValidationStats>,
) -> Result<(), PlanError> {
    let candidates = manifest
        .candidates
        .iter()
        .map(|candidate| (candidate.id.as_str(), &candidate.candidate))
        .collect::<BTreeMap<_, _>>();
    let requested_paths = validate_requested_descriptors(
        &candidates,
        requested_ids,
        config,
        targets,
        #[cfg(test)]
        stats,
    )
    .await?;

    let discovery_targets = requested_discovery_targets(targets, &requested_paths);
    let discovery = discover_plan_targets(
        &config.root,
        &discovery_targets,
        &config.operators,
        config.profile,
        config.limits.max_candidates.get(),
        config.limits.analyzer_timeout.get(),
        #[cfg(test)]
        control,
    )
    .await
    .map_err(|error| map_discovery_error(&error, DiscoveryCaller::Verify))?;
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
        if candidate_descriptor(current) != candidate_descriptor(planned) {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate descriptor differs for {candidate_id}"
            )));
        }
    }
    Ok(())
}

async fn validate_requested_descriptors(
    candidates: &BTreeMap<&str, &MutationCandidate>,
    requested_ids: &BTreeSet<String>,
    config: &RunConfig,
    targets: &[TargetSlice],
    #[cfg(test)] mut stats: Option<&mut ValidationStats>,
) -> Result<BTreeSet<Utf8PathBuf>, PlanError> {
    let mut by_path: BTreeMap<&Utf8Path, Vec<&MutationCandidate>> = BTreeMap::new();
    for id in requested_ids {
        if let Some(candidate) = candidates.get(id.as_str()) {
            by_path.entry(&candidate.path).or_default().push(candidate);
        }
    }
    let mut requested_paths = BTreeSet::new();
    let mut results = BTreeMap::new();
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
        requested_paths.insert(candidate.path.clone());
        if let Some(file_candidates) = by_path.remove(candidate.path.as_path()) {
            let source = tokio::fs::read(config.root.join(&candidate.path))
                .await
                .map_err(|error| {
                    PlanError::CandidateInvalid(format!("{}: {error}", candidate.path))
                })?;
            #[cfg(test)]
            if let Some(stats) = stats.as_deref_mut() {
                stats.contexts += 1;
                stats.source_bytes += source.len();
            }
            let context = CandidateValidationContext::new(&source)
                .map_err(|error| PlanError::CandidateInvalid(error.to_string()))?;
            for candidate in file_candidates {
                let result =
                    validate_candidate_with_context(&context, &candidate_descriptor(candidate))
                        .map_err(|error| PlanError::CandidateInvalid(error.to_string()))
                        .and_then(|stable_id| {
                            if stable_id.as_str() == candidate.id {
                                Ok(())
                            } else {
                                Err(PlanError::CandidateInvalid(format!(
                                    "candidate stable id differs for {}",
                                    candidate.id
                                )))
                            }
                        });
                results.insert(candidate.id.as_str(), result);
            }
            // The source and its borrowed index are released before reading another file.
        }
        // Preserve requested-ID error ordering even when files are interleaved.
        results
            .remove(candidate_id.as_str())
            .expect("requested candidate was validated")?;
    }
    Ok(requested_paths)
}

fn requested_discovery_targets(
    targets: &[TargetSlice],
    requested_paths: &BTreeSet<Utf8PathBuf>,
) -> Vec<TargetSlice> {
    targets
        .iter()
        .filter(|target| requested_paths.contains(&target.path))
        .cloned()
        .collect()
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
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::fmt::Write as _;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use camino::Utf8PathBuf;
    use hoimin_core::{
        ByteSpan, CommandArg, EffectFailed, EffectId, LineRange, MutationCandidate, RawRunConfig,
        RunConfig, TargetSlice,
    };

    use crate::analyzer::DiscoveryControl;
    use crate::cli::{OutputFormat, ParsedCommand, parse_from};

    use super::{
        DiscoveryCaller, PLAN_SCHEMA_VERSION, PlanError, PlanManifest, RANKING_RULE_VERSION,
        RankedPlanCandidate, create_with_discovery_control, map_discovery_error, plan_output,
        prepare_verify_with_discovery_control, requested_discovery_targets,
    };

    pub(super) struct Project {
        _directory: tempfile::TempDir,
        pub(super) root: Utf8PathBuf,
        marker: std::path::PathBuf,
    }

    impl Project {
        pub(super) fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir(directory.path().join("src")).unwrap();
            std::fs::write(
                directory.path().join("src/calc.py"),
                "def calc(left, right):\n    return left + right\n",
            )
            .unwrap();
            Self {
                root: Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
                marker: directory.path().join("test-command-ran"),
                _directory: directory,
            }
        }

        pub(super) fn config(&self, analyzer_timeout: &str) -> RunConfig {
            let command = format!(
                "from pathlib import Path; Path({:?}).write_text('ran')",
                self.marker.to_string_lossy()
            );
            let ParsedCommand::Plan(args) = parse_from([
                OsString::from("hoimin"),
                OsString::from("plan"),
                OsString::from("--root"),
                self.root.as_os_str().to_owned(),
                OsString::from("--file"),
                OsString::from("src/calc.py"),
                OsString::from("--analyzer-timeout"),
                OsString::from(analyzer_timeout),
                OsString::from("--allow-best-effort-memory"),
                OsString::from("--"),
                OsString::from("python"),
                OsString::from("-c"),
                OsString::from(command),
            ])
            .unwrap() else {
                panic!("plan arguments parsed as another command")
            };
            args.into_run_config().unwrap()
        }
    }

    fn paused_control() -> (DiscoveryControl, mpsc::Receiver<()>, mpsc::SyncSender<()>) {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let owner = Arc::new(tempfile::tempdir().unwrap());
        let control = DiscoveryControl::new(owner, move || {
            entered_tx
                .try_send(())
                .expect("entry channel has one empty slot");
            release_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("test releases paused plan discovery");
        });
        (control, entered_rx, release_tx)
    }

    async fn wait_for_discovery_entry(entered: mpsc::Receiver<()>) {
        tokio::task::spawn_blocking(move || entered.recv_timeout(Duration::from_secs(2)))
            .await
            .expect("entry wait task must not panic")
            .expect("plan discovery must enter its controlled pause");
    }

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

    #[tokio::test]
    async fn requested_validation_preprocesses_each_file_once() {
        for count in [1, 2, 4] {
            let project = Project::new();
            let source = "x = 1 + 2\n".repeat(count);
            std::fs::write(project.root.join("src/calc.py"), &source).unwrap();
            let config = project.config("30s");
            let targets = crate::target::TargetHandler::resolve(&config.selection)
                .await
                .unwrap();
            let output = super::create(config.clone()).await.unwrap();
            assert_eq!(output.manifest.candidates.len(), count);
            let ids = output
                .manifest
                .candidates
                .iter()
                .map(|c| c.id.clone())
                .collect();
            let mut stats = super::ValidationStats::default();
            super::validate_requested_candidates(
                &output.manifest,
                &ids,
                &config,
                &targets,
                None,
                Some(&mut stats),
            )
            .await
            .unwrap();
            assert_eq!(stats.contexts, 1, "{count} candidates: {stats:?}");
            assert_eq!(stats.source_bytes, source.len());
        }
    }

    #[tokio::test]
    #[ignore = "benchmark harness; run explicitly in release mode"]
    async fn benchmark_candidate_conversion_and_manifest() {
        let project = Project::new();
        let mut source = String::with_capacity(307_200);
        for index in 0..7_680 {
            writeln!(
                source,
                "result_{index:05} = left_{index:05} + right_{index:05}"
            )
            .expect("writing to String cannot fail");
        }
        assert_eq!(source.len(), 307_200);
        std::fs::write(project.root.join("src/calc.py"), &source).unwrap();

        let started = std::time::Instant::now();
        let output = super::create(project.config("30s")).await.unwrap();
        let manifest_json = serde_json::to_vec(&output.manifest).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(output.manifest.candidates.len(), 7_680);
        assert!(!manifest_json.is_empty());
        std::hint::black_box(&manifest_json);
        println!(
            "source_bytes={} candidates={} manifest_bytes={} elapsed_ms={}",
            source.len(),
            output.manifest.candidates.len(),
            manifest_json.len(),
            elapsed.as_secs_f64() * 1_000.0,
        );
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

    #[test]
    fn verification_discovery_retains_only_requested_candidate_files_and_their_slices() {
        let first_requested = TargetSlice {
            path: Utf8PathBuf::from("src/z_requested.py"),
            lines: vec![LineRange { start: 4, end: 9 }],
            symbols: vec!["z_selected".to_owned()],
        };
        let second_requested = TargetSlice {
            path: Utf8PathBuf::from("src/a_requested.py"),
            lines: vec![LineRange { start: 12, end: 18 }],
            symbols: vec!["a_selected".to_owned()],
        };
        let targets = vec![
            first_requested.clone(),
            TargetSlice {
                path: Utf8PathBuf::from("src/omitted.py"),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
            second_requested.clone(),
        ];

        let actual = requested_discovery_targets(
            &targets,
            &BTreeSet::from([first_requested.path.clone(), second_requested.path.clone()]),
        );

        assert_eq!(actual, vec![first_requested, second_requested]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn create_timeout_uses_the_common_plan_discovery_identity_without_output() {
        let project = Project::new();
        let (control, entered, release) = paused_control();
        let operation = tokio::spawn(create_with_discovery_control(
            project.config("20ms"),
            Some(control),
        ));
        wait_for_discovery_entry(entered).await;

        let result = tokio::time::timeout(Duration::from_millis(200), operation)
            .await
            .expect("plan create timeout must not await detached discovery")
            .unwrap();
        let mut stdout = Vec::new();
        let error = match result {
            Ok(output) => {
                serde_json::to_writer(&mut stdout, &output.manifest).unwrap();
                panic!("paused discovery unexpectedly created a plan")
            }
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            "plan.discovery: analyzer.timeout: --analyzer-timeout expired after 20ms"
        );
        assert!(stdout.is_empty());
        release.send(()).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn verify_timeout_uses_the_common_identity_before_the_test_command() {
        let project = Project::new();
        let mut output = super::create(project.config("1s")).await.unwrap();
        output.manifest.normalized_config.limits.analyzer_timeout =
            project.config("20ms").limits.analyzer_timeout;
        let manifest_path = project.root.join("plan.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&output.manifest).unwrap(),
        )
        .unwrap();
        let requested = vec![output.manifest.candidates[0].id.clone()];
        let (control, entered, release) = paused_control();
        let operation = tokio::spawn(prepare_verify_with_discovery_control(
            manifest_path.into_std_path_buf(),
            requested,
            OutputFormat::Json,
            Some(control),
        ));
        wait_for_discovery_entry(entered).await;

        let error = tokio::time::timeout(Duration::from_millis(200), operation)
            .await
            .expect("verify timeout must not await detached rediscovery")
            .unwrap()
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "plan.discovery: analyzer.timeout: --analyzer-timeout expired after 20ms"
        );
        assert!(!project.marker.exists(), "verify launched the test command");
        release.send(()).unwrap();
    }

    #[tokio::test]
    async fn successful_create_and_verify_preserve_the_normalized_analyzer_timeout() {
        let project = Project::new();
        let timeout = Duration::from_millis(250);
        let output = create_with_discovery_control(
            project.config("250ms"),
            Some(DiscoveryControl::new(
                Arc::new(tempfile::tempdir().unwrap()),
                || {},
            )),
        )
        .await
        .unwrap();
        assert_eq!(
            output
                .manifest
                .normalized_config
                .limits
                .analyzer_timeout
                .get(),
            timeout
        );
        let manifest_path = project.root.join("plan.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&output.manifest).unwrap(),
        )
        .unwrap();
        let verified = prepare_verify_with_discovery_control(
            manifest_path.into_std_path_buf(),
            vec![output.manifest.candidates[0].id.clone()],
            OutputFormat::Json,
            Some(DiscoveryControl::new(
                Arc::new(tempfile::tempdir().unwrap()),
                || {},
            )),
        )
        .await
        .unwrap();

        assert_eq!(verified.config.limits.analyzer_timeout.get(), timeout);
        assert!(!project.marker.exists());
    }

    #[test]
    fn non_timeout_discovery_errors_keep_caller_specific_mapping() {
        let failure = EffectFailed::other(EffectId(0), "analyzer.source.read", "read failed");
        let create = map_discovery_error(&failure, DiscoveryCaller::Create);
        let verify = map_discovery_error(&failure, DiscoveryCaller::Verify);

        assert!(matches!(create, PlanError::Discovery(message) if message == "read failed"));
        assert!(matches!(verify, PlanError::CandidateInvalid(message) if message == "read failed"));
    }
}
