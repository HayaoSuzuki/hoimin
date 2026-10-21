use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    CandidateDescriptor, CandidateValidationContext, FingerprintInputFile, MutationCandidate,
    OutputConfig, PlanConfig, RunConfig, RunSelection, TargetSlice, VerificationSelection,
    VerificationTopPolicy, normalized_relative_path, validate_candidate_with_context,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::analyzer::Discovery;
#[cfg(test)]
use crate::analyzer::DiscoveryControl;
use crate::cli::{OutputFormat, TopSelectionPolicy, VerifySelection};
use crate::fingerprint_inputs;
use crate::resource::{self, ResourceError};
use crate::shell;
use crate::target::TargetHandler;
use hoimin_python::{AnalyzerDiagnostic, AnalyzerDiagnosticCode};

mod preview;
pub use preview::VerifyPreview;

mod ranking;
#[cfg(test)]
mod ranking_tests;
#[cfg(test)]
mod review_tests;
mod sampling;
mod schema;
#[cfg(test)]
mod schema_tests;
mod selection;
#[cfg(test)]
mod selection_tests;
mod source;
#[cfg(test)]
mod validation_tests;

use ranking::{RANKING_RULE_VERSION, rank_candidates, validate_ranking, validate_ranking_against};
pub use ranking::{RankedPlanCandidate, RankingReason, RankingReasonCode};
use schema::decode_manifest;

pub const PLAN_SCHEMA_VERSION: u32 = 10;

pub(crate) struct ReadOnlyPlan {
    pub manifest: PlanManifest,
    pub canonical_path: PathBuf,
    pub content_hash: String,
    pub project: Utf8PathBuf,
}

pub(crate) fn read_for_review(path: &Path) -> Result<ReadOnlyPlan, PlanError> {
    let invalid = |error: String| PlanError::ManifestInvalid(error);
    let bytes = crate::artifacts::bounded_read(path, crate::artifacts::READ_ONLY_ARTIFACT_LIMIT)
        .map_err(|error| {
            // Preserve the historical read-only-plan diagnostics at this boundary.
            invalid(match error {
                crate::artifacts::ArtifactReadError::Io(error) => {
                    format!("review I/O failed: {error}")
                }
                crate::artifacts::ArtifactReadError::Invalid(message) => {
                    format!("invalid review input: {message}")
                }
            })
        })?;
    let manifest = decode_manifest(&bytes)?;
    validate_header(&manifest)?;
    // Header validation rejects duplicate paths before this borrowed index is built.
    #[cfg(test)]
    review_tests::SOURCE_WORK.with_borrow_mut(|work| work.index_builds += 1);
    let sources: BTreeMap<&Utf8Path, &str> = manifest
        .sources
        .iter()
        .map(|source| {
            #[cfg(test)]
            review_tests::SOURCE_WORK.with_borrow_mut(|work| work.source_visits += 1);
            (source.path.as_path(), source.hash.as_str())
        })
        .collect();
    let project = std::fs::canonicalize(&manifest.normalized_config.selection.root)
        .map_err(|error| invalid(error.to_string()))?;
    let project = Utf8PathBuf::from_path_buf(project)
        .map_err(|_| invalid("project root is not UTF-8".into()))?;
    for candidate in &manifest.candidates {
        let descriptor = CandidateDescriptor::from(&candidate.candidate);
        let id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&descriptor));
        if id.as_str() != candidate.id {
            return Err(invalid("candidate id does not match its descriptor".into()));
        }
        #[cfg(test)]
        review_tests::SOURCE_WORK.with_borrow_mut(|work| work.membership_queries += 1);
        if sources.get(candidate.path.as_path()).copied() != Some(candidate.file_hash.as_str()) {
            return Err(invalid(
                "candidate source record is missing or inconsistent".into(),
            ));
        }
    }
    Ok(ReadOnlyPlan {
        manifest,
        canonical_path: std::fs::canonicalize(path).map_err(|error| invalid(error.to_string()))?,
        content_hash: blake3::hash(&bytes).to_hex().to_string(),
        project,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanManifest {
    pub schema_version: u32,
    pub kind: String,
    pub ranking_rule_version: u32,
    pub normalized_config: PlanConfig,
    pub sources: Vec<FingerprintInputFile>,
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

/// A checked plan whose execution settings and selected candidates stay coupled.
/// Inspect shared references; use `with_metrics` for the permitted output override.
/// Execution still rechecks protected inputs that can change after preparation.
///
/// Validated execution settings cannot be changed through public fields:
///
/// ```compile_fail
/// fn enable_resume(mut plan: hoimin_cli::plan::VerifiedPlan) {
///     plan.config.session = hoimin_core::SessionMode::Disabled;
/// }
/// ```
///
/// Read-only inspection cannot be used as a mutable configuration handle:
///
/// ```compile_fail
/// fn enable_resume(plan: hoimin_cli::plan::VerifiedPlan) {
///     plan.config().session = hoimin_core::SessionMode::Disabled;
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPlan {
    manifest_inputs: Vec<PathBuf>,
    config: RunConfig,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    selection: RunSelection,
}

impl VerifiedPlan {
    /// Execution settings as validated against the saved plan and current project.
    #[must_use]
    pub const fn config(&self) -> &RunConfig {
        &self.config
    }

    /// Candidate membership or ordered dispatch IDs; no mutable access is exposed.
    #[must_use]
    pub const fn selection(&self) -> &RunSelection {
        &self.selection
    }

    /// Overrides only the metrics destination; execution validates destination safety.
    /// Candidate order, execution limits and protected inputs are unchanged.
    #[must_use]
    pub fn with_metrics(mut self, destination: Option<Utf8PathBuf>) -> Self {
        let mut output = self.config.output.clone();
        output.metrics = destination;
        self.config = self.config.with_output(output);
        self
    }

    /// Executes the checked selection with owned deadline-aware output writers.
    ///
    /// # Errors
    /// Owns preparation and infrastructure diagnostics; callers may inspect errors
    /// but must not print them again. Deadline expiry can omit a final diagnostic.
    pub async fn execute<Stdout, Stderr>(
        self,
        stdout: Stdout,
        stderr: Stderr,
        presentation: crate::report::ReportPresentation,
    ) -> Result<i32, String>
    where
        Stdout: std::io::Write + Send + 'static,
        Stderr: std::io::Write + Send + 'static,
    {
        shell::run_verified(self, stdout, stderr, presentation).await
    }

    pub(crate) fn into_execution(self) -> VerifiedExecution {
        VerifiedExecution {
            manifest_inputs: self.manifest_inputs,
            config: self.config,
            fingerprint_copy_inputs: self.fingerprint_copy_inputs,
            selection: self.selection,
        }
    }
}

/// One owned handoff keeps checked selection, configuration and protected inputs together.
pub(crate) struct VerifiedExecution {
    pub config: RunConfig,
    pub selection: RunSelection,
    pub manifest_inputs: Vec<PathBuf>,
    pub fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
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
    #[error(
        "plan.fingerprint_env.changed: selected inherited environment differs; regenerate the plan"
    )]
    FingerprintEnvChanged,
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
    config: &RunConfig,
    discovery: &hoimin_core::DiscoveryConfig,
    targets: &[TargetSlice],
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<Discovery, hoimin_core::EffectFailed> {
    crate::live_progress::stage("analyzing sources");
    crate::analyzer::discover_config_targets(
        config,
        discovery,
        targets,
        #[cfg(test)]
        control,
    )
    .await
}

/// Captures the request once and creates a read-only candidate plan from its validated snapshot.
///
/// # Errors
///
/// Returns an error before manifest serialization when fingerprint inputs, targets, sources, or
/// analysis cannot be resolved successfully.
#[tracing::instrument(name = "plan", level = "debug", skip_all)]
pub async fn create(
    config: RunConfig<hoimin_core::EnvironmentRequest>,
    discovery_config: hoimin_core::DiscoveryConfig,
) -> Result<PlanOutput, PlanError> {
    create_inner(
        config,
        discovery_config,
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(test)]
async fn create_with_discovery_control(
    config: RunConfig<hoimin_core::EnvironmentRequest>,
    discovery_config: hoimin_core::DiscoveryConfig,
    control: Option<DiscoveryControl>,
) -> Result<PlanOutput, PlanError> {
    create_inner(config, discovery_config, control).await
}

async fn create_inner(
    config: RunConfig<hoimin_core::EnvironmentRequest>,
    discovery_config: hoimin_core::DiscoveryConfig,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<PlanOutput, PlanError> {
    discovery_config
        .validate()
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    crate::live_progress::stage("preparing plan");
    resource::validate_plan_resource_policy(config.allow_best_effort_memory)?;
    let config = shell::prepare_run_config(config)
        .map_err(|error| PlanError::FingerprintInput(error.to_string()))?;
    let targets = TargetHandler::resolve(&config.selection)
        .await
        .map_err(|error| PlanError::TargetResolution(error.to_string()))?;
    crate::live_progress::stage("reading sources");
    let sources = source_records(&config.selection.root, &targets).await?;
    let discovery = discover_plan_targets(
        &config,
        &discovery_config,
        &targets,
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
    crate::live_progress::stage("ranking candidates");
    tracing::debug!(
        candidates = discovery.candidates.len(),
        truncated = discovery.truncated,
        "analysis completed"
    );
    let diagnostics = discovery.diagnostics.iter().map(plan_diagnostic).collect();
    let truncated = discovery.truncated;
    let candidates = rank_candidates(&config.selection, &targets, discovery.candidates);
    let manifest = PlanManifest {
        schema_version: PLAN_SCHEMA_VERSION,
        kind: "plan".to_owned(),
        ranking_rule_version: RANKING_RULE_VERSION,
        normalized_config: config.into_plan_config(discovery_config),
        sources,
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
#[tracing::instrument(name = "verify_plan", level = "debug", skip_all)]
pub async fn prepare_verify_selection(
    manifest_path: impl AsRef<Path>,
    requested_selection: &VerifySelection,
    format: OutputFormat,
) -> Result<VerifiedPlan, PlanError> {
    crate::live_progress::stage("validating saved plan");
    prepare_verify_selection_inner(
        manifest_path.as_ref(),
        requested_selection,
        format,
        #[cfg(test)]
        None,
    )
    .await
    .map(|checked| checked.verified)
}

/// Validates the same inputs and selection as execution, then builds display data.
///
/// # Errors
/// Returns the same manifest, selection, source and candidate validation errors
/// as [`prepare_verify_selection`].
#[tracing::instrument(name = "verify_plan_preview", level = "debug", skip_all)]
pub async fn prepare_verify_preview_selection(
    manifest_path: impl AsRef<Path>,
    requested_selection: &VerifySelection,
    format: OutputFormat,
) -> Result<VerifyPreview, PlanError> {
    crate::live_progress::stage("validating saved plan");
    let checked = prepare_verify_selection_inner(
        manifest_path.as_ref(),
        requested_selection,
        format,
        #[cfg(test)]
        None,
    )
    .await?;
    Ok(VerifyPreview::new(
        &checked.manifest,
        requested_selection,
        &checked.verified.selection,
        &checked.discovered_ids,
    ))
}

struct CheckedPlan {
    verified: VerifiedPlan,
    manifest: PlanManifest,
    discovered_ids: Vec<String>,
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
    .map(|checked| checked.verified)
}

fn read_manifest(path: &Path) -> Result<(Vec<PathBuf>, PlanManifest), PlanError> {
    // Protect both the requested entry (which may be a final symlink) and
    // the file read through it. Keep invocation paths out of the saved config.
    let open = || -> std::io::Result<_> {
        let inputs = vec![std::path::absolute(path)?, std::fs::canonicalize(path)?];
        let file = crate::artifacts::open_regular_file(path)?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "input is not a regular file",
            )
        })?;
        Ok((inputs, crate::artifacts::SnapshotReader::new(file)?))
    };
    let (inputs, reader) = open()
        .map_err(|error| PlanError::ManifestInvalid(format!("{}: {error}", path.display())))?;
    let manifest = schema::decode_reader(BufReader::new(reader))?;
    Ok((inputs, manifest))
}

async fn prepare_verify_selection_inner(
    manifest_path: &Path,
    requested_selection: &VerifySelection,
    format: OutputFormat,
    #[cfg(test)] control: Option<DiscoveryControl>,
) -> Result<CheckedPlan, PlanError> {
    let (manifest_inputs, manifest) = read_manifest(manifest_path)?;
    validate_header(&manifest)?;
    validate_current_fingerprint_env(&manifest.normalized_config)?;

    let selection = resolve_verify_selection(&manifest, requested_selection)?;
    let candidate_ids = selection
        .candidate_ids()
        .expect("verified selection has membership");
    let mut config = manifest
        .normalized_config
        .clone()
        .into_run_config(output_config(format))
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    let targets = TargetHandler::resolve(&config.selection)
        .await
        .map_err(|error| PlanError::SourceChanged(error.to_string()))?;
    let current_sources = source_records(&config.selection.root, &targets)
        .await
        .map_err(|error| PlanError::SourceChanged(error.to_string()))?;
    ensure_exact_records(&manifest.sources, &current_sources, RecordMismatch::Source)?;
    let current_inputs = fingerprint_inputs::resolve_config(&config)
        .map_err(|error| PlanError::FingerprintInputChanged(error.to_string()))?;
    ensure_exact_records(
        &manifest.normalized_config.fingerprint_inputs,
        &current_inputs,
        RecordMismatch::FingerprintInput,
    )?;
    validate_ranking_against(
        &manifest.normalized_config.selection,
        &targets,
        &manifest.candidates,
    )
    .map_err(PlanError::ManifestInvalid)?;
    config = config.with_fingerprint_inputs(current_inputs);
    let copy_options = crate::workspace::CopyOptions {
        includes: config.selection.includes.clone(),
        excludes: config.selection.excludes.clone(),
        literal_exclusions: Vec::new(),
    };
    let copy_manifest =
        crate::workspace::build_validation_manifest(&config.selection.root, &copy_options)
            .map_err(|error| PlanError::Workspace(error.to_string()))?;
    let fingerprint_copy_inputs = config
        .fingerprint_inputs
        .iter()
        .filter(|record| copy_manifest.entry(&record.path).is_some())
        .map(|record| record.path.clone())
        .collect();
    let discovered_ids = validate_requested_candidates(
        &manifest,
        candidate_ids,
        &config,
        &targets,
        #[cfg(test)]
        control,
        #[cfg(test)]
        None,
    )
    .await?;

    Ok(CheckedPlan {
        verified: VerifiedPlan {
            manifest_inputs,
            config,
            fingerprint_copy_inputs,
            selection,
        },
        manifest,
        discovered_ids,
    })
}

fn validate_current_fingerprint_env(config: &hoimin_core::PlanConfig) -> Result<(), PlanError> {
    let request = hoimin_core::EnvironmentRequest::new(config.environment.names())
        .map_err(|error| PlanError::FingerprintInput(error.to_string()))?;
    let current = crate::fingerprint_env::capture(&request)
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    if current == config.environment {
        Ok(())
    } else {
        Err(PlanError::FingerprintEnvChanged)
    }
}

fn resolve_verify_selection(
    manifest: &PlanManifest,
    requested_selection: &VerifySelection,
) -> Result<RunSelection, PlanError> {
    let max_mutants = manifest.normalized_config.limits.max_mutants.get();
    let selection = match requested_selection {
        VerifySelection::Sample { count, seed } => {
            let sample = resolve_sample(manifest, count.get(), *seed)?;
            let ids = sample.selected_ids().to_vec();
            let metadata = VerificationSelection::sample(count.get(), sample)
                .map_err(|error| PlanError::CandidateInvalid(error.to_string()))?;
            RunSelection::ordered(ids, metadata)
        }
        VerifySelection::CandidateIds(requested_ids) => {
            let ids = normalize_requested_ids(requested_ids, max_mutants)?;
            let metadata = VerificationSelection::candidate_ids(
                requested_ids.iter().collect::<BTreeSet<_>>().len(),
                ids.len(),
                manifest.truncated,
            );
            RunSelection::explicit(
                ids,
                metadata.map_err(|error| PlanError::CandidateInvalid(error.to_string()))?,
            )
        }
        VerifySelection::Top {
            count,
            policy,
            offset,
        } => {
            if *offset > 0 && *offset >= manifest.candidates.len() {
                return Err(PlanError::CandidateInvalid(format!(
                    "--offset {offset} is outside the {} retained candidates",
                    manifest.candidates.len()
                )));
            }
            let candidate_ids = selection::select_top_candidate_ids_at(
                &manifest.candidates,
                *count,
                *policy,
                *offset,
            );
            if candidate_ids.is_empty() {
                return Err(PlanError::CandidateInvalid(
                    "--top requires at least one candidate; the plan has no retained candidates"
                        .to_owned(),
                ));
            }
            let policy = match policy {
                TopSelectionPolicy::Strict => VerificationTopPolicy::Strict,
                TopSelectionPolicy::Diverse => VerificationTopPolicy::FileRoundRobinV1,
                TopSelectionPolicy::LineDiverse => VerificationTopPolicy::LineRoundRobinV1,
            };
            if candidate_ids.len() > max_mutants {
                return Err(PlanError::CandidateInvalid(format!(
                    "selected {} candidates exceeds max_mutants {max_mutants}",
                    candidate_ids.len(),
                )));
            }
            let metadata = VerificationSelection::top(
                count.get(),
                candidate_ids.len(),
                policy,
                manifest.truncated,
            );
            RunSelection::ordered(
                candidate_ids,
                metadata.map_err(|error| PlanError::CandidateInvalid(error.to_string()))?,
            )
        }
    };
    selection.map_err(|error| PlanError::CandidateInvalid(error.to_string()))
}

fn resolve_sample(
    manifest: &PlanManifest,
    requested: usize,
    seed: u64,
) -> Result<hoimin_core::VerificationSampling, PlanError> {
    if manifest.truncated {
        return Err(PlanError::CandidateInvalid(
            "--sample requires a complete plan; truncated plans cannot be sampled".to_owned(),
        ));
    }
    let population = manifest.candidates.len();
    if population == 0 {
        return Err(PlanError::CandidateInvalid(
            "--sample requires at least one candidate; the plan is empty".to_owned(),
        ));
    }
    let actual = requested.min(population);
    let max_mutants = manifest.normalized_config.limits.max_mutants.get();
    if actual > max_mutants {
        return Err(PlanError::CandidateInvalid(format!(
            "selected {actual} candidates exceeds max_mutants {max_mutants}"
        )));
    }
    hoimin_core::VerificationSampling::new(
        population,
        seed,
        sampling::sample_indices(population, actual, seed)
            .into_iter()
            .map(|index| manifest.candidates[index].id.clone())
            .collect(),
    )
    .map_err(|error| PlanError::CandidateInvalid(error.to_string()))
}

async fn source_records(
    root: &Utf8Path,
    targets: &[TargetSlice],
) -> Result<Vec<FingerprintInputFile>, PlanError> {
    let mut records = BTreeMap::new();
    let mut reader = source::SourceReader::new(root);
    for target in targets {
        let path = target.path.clone();
        let hash = reader.hash(&path).await.map_err(PlanError::SourceRead)?;
        records.insert(
            path.clone(),
            FingerprintInputFile {
                path,
                hash: hash.to_hex().to_string(),
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
    manifest
        .normalized_config
        .validate()
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    validate_records("sources", &manifest.sources)?;
    validate_records(
        "normalized_config.fingerprint_inputs",
        &manifest.normalized_config.fingerprint_inputs,
    )?;

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
    let expected = record_map(expected);
    let current = record_map(current);
    if expected == current {
        return Ok(());
    }
    let details = record_change_details(&expected, &current);
    match kind {
        RecordMismatch::Source => Err(PlanError::SourceChanged(format!(
            "planned target source records do not match the current workspace: {details}"
        ))),
        RecordMismatch::FingerprintInput => Err(PlanError::FingerprintInputChanged(format!(
            "planned fingerprint input records do not match the current workspace: {details}"
        ))),
    }
}

fn record_change_details(
    expected: &BTreeMap<Utf8PathBuf, String>,
    current: &BTreeMap<Utf8PathBuf, String>,
) -> String {
    const DISPLAY_LIMIT: usize = 10;
    let changes: BTreeMap<_, _> = expected
        .iter()
        .filter_map(|(path, hash)| match current.get(path) {
            None => Some((path, "removed")),
            Some(current_hash) if current_hash != hash => Some((path, "modified")),
            Some(_) => None,
        })
        .chain(
            current
                .keys()
                .filter(|path| !expected.contains_key(*path))
                .map(|path| (path, "added")),
        )
        .collect();
    let mut details: Vec<_> = changes
        .iter()
        .take(DISPLAY_LIMIT)
        .map(|(path, change)| format!("{change} {:?}", path.as_str()))
        .collect();
    if changes.len() > DISPLAY_LIMIT {
        details.push(format!(
            "{} additional paths omitted",
            changes.len() - DISPLAY_LIMIT
        ));
    }
    details.join("; ")
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
    target_path_visits: usize,
    membership_queries: usize,
}

async fn validate_requested_candidates(
    manifest: &PlanManifest,
    requested_ids: &BTreeSet<String>,
    config: &RunConfig,
    targets: &[TargetSlice],
    #[cfg(test)] control: Option<DiscoveryControl>,
    #[cfg(test)] stats: Option<&mut ValidationStats>,
) -> Result<Vec<String>, PlanError> {
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
        config,
        &manifest.normalized_config.discovery,
        &discovery_targets,
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
        if CandidateDescriptor::from(*current) != CandidateDescriptor::from(*planned) {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate descriptor differs for {candidate_id}"
            )));
        }
    }
    Ok(discovery
        .candidates
        .iter()
        .filter(|candidate| requested_ids.contains(&candidate.id))
        .map(|candidate| candidate.id.clone())
        .collect())
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
    // Keep component-based path equality, including equivalent path spellings.
    let selected_paths: HashSet<&Utf8Path> = targets
        .iter()
        .map(|target| {
            #[cfg(test)]
            if let Some(stats) = stats.as_deref_mut() {
                stats.target_path_visits += 1;
            }
            target.path.as_path()
        })
        .collect();
    let mut requested_paths = BTreeSet::new();
    let mut results = BTreeMap::new();
    let mut reader = source::SourceReader::new(&config.selection.root);
    for candidate_id in requested_ids {
        let candidate = candidates.get(candidate_id.as_str()).ok_or_else(|| {
            PlanError::CandidateInvalid(format!("candidate id is not in the plan: {candidate_id}"))
        })?;
        #[cfg(test)]
        if let Some(stats) = stats.as_deref_mut() {
            stats.membership_queries += 1;
        }
        if !selected_paths.contains(candidate.path.as_path()) {
            return Err(PlanError::CandidateInvalid(format!(
                "candidate target is not selected: {}",
                candidate.path
            )));
        }
        requested_paths.insert(candidate.path.clone());
        if let Some(file_candidates) = by_path.remove(candidate.path.as_path()) {
            let source = reader
                .read(&candidate.path)
                .await
                .map_err(PlanError::CandidateInvalid)?;
            #[cfg(test)]
            if let Some(stats) = stats.as_deref_mut() {
                stats.contexts += 1;
                stats.source_bytes += source.len();
            }
            let context = CandidateValidationContext::new(&source)
                .map_err(|error| PlanError::CandidateInvalid(error.to_string()))?;
            for candidate in file_candidates {
                let result = validate_candidate_with_context(
                    &context,
                    &CandidateDescriptor::from(candidate),
                )
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
        code: diagnostic.code.canonical_name().to_owned(),
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
        ByteSpan, CommandArg, EffectFailed, EffectId, FingerprintInputFile, LineRange,
        MutationCandidate, RawRunConfig, RunConfig, TargetSlice,
    };

    use crate::analyzer::DiscoveryControl;
    use crate::cli::{OutputFormat, ParsedCommand, parse_from};

    use super::{
        DiscoveryCaller, PLAN_SCHEMA_VERSION, PlanError, PlanManifest, RANKING_RULE_VERSION,
        RankedPlanCandidate, create_with_discovery_control, map_discovery_error, plan_output,
        prepare_verify_with_discovery_control, requested_discovery_targets, validate_header,
    };

    #[test]
    fn analyzer_diagnostics_preserve_plan_names_and_structured_fields() {
        use hoimin_python::{AnalyzerDiagnostic, AnalyzerDiagnosticCode};

        let cases = [
            (AnalyzerDiagnosticCode::InvalidSyntax, "invalid_syntax"),
            (
                AnalyzerDiagnosticCode::UnsupportedExceptionHierarchy,
                "exception_hierarchy_skipped",
            ),
            (
                AnalyzerDiagnosticCode::UnsupportedEnumDefinition,
                "enum_definition_skipped",
            ),
            (
                AnalyzerDiagnosticCode::CandidateLimitExceeded,
                "candidate_limit",
            ),
        ];
        for (code, expected_name) in cases {
            let diagnostic = AnalyzerDiagnostic {
                code,
                path: Some("src/example.py".into()),
                line: Some(7),
                column: Some(12),
                message: None,
            };

            let mapped = super::plan_diagnostic(&diagnostic);

            assert_eq!(
                serde_json::to_value(mapped).unwrap(),
                serde_json::json!({
                    "code": expected_name,
                    "path": "src/example.py",
                    "line": 7,
                    "column": 12,
                    "message": "",
                }),
            );
        }
    }

    #[test]
    fn analyzer_diagnostic_preserves_plan_custom_message_without_location() {
        use hoimin_python::{AnalyzerDiagnostic, AnalyzerDiagnosticCode};

        let diagnostic = AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidSyntax,
            path: None,
            line: None,
            column: None,
            message: Some("custom explanation".to_owned()),
        };

        let mapped = super::plan_diagnostic(&diagnostic);

        assert_eq!(
            serde_json::to_value(mapped).unwrap(),
            serde_json::json!({
                "code": "invalid_syntax",
                "path": null,
                "line": null,
                "column": null,
                "message": "custom explanation",
            }),
        );
    }

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

        pub(super) fn config(&self) -> RunConfig<hoimin_core::EnvironmentRequest> {
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
                OsString::from("--allow-best-effort-memory"),
                OsString::from("--"),
                OsString::from("python"),
                OsString::from("-c"),
                OsString::from(command),
            ])
            .unwrap() else {
                panic!("plan arguments parsed as another command")
            };
            args.into_configs().unwrap().0
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
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
        .into_plan_config(
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        );
        PlanManifest {
            schema_version: PLAN_SCHEMA_VERSION,
            kind: "plan".to_owned(),
            ranking_rule_version: RANKING_RULE_VERSION,
            normalized_config,
            sources: Vec::new(),
            candidates: Vec::new(),
            truncated: true,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn header_preserves_version_precedence_over_invalid_selection() {
        let mut manifest = manifest();
        manifest.normalized_config.selection.files.clear();
        manifest.schema_version = PLAN_SCHEMA_VERSION + 1;

        let result = validate_header(&manifest);

        assert!(matches!(
            result,
            Err(PlanError::ManifestInvalid(message))
                if message.starts_with("unsupported schema version")
        ));
    }

    #[test]
    fn header_validates_normalized_config_before_manifest_records() {
        let mut manifest = manifest();
        manifest.normalized_config.selection.files.clear();
        manifest.sources.push(FingerprintInputFile {
            path: "../outside.py".into(),
            hash: "invalid".into(),
        });
        assert!(matches!(
            validate_header(&manifest),
            Err(PlanError::ManifestInvalid(message))
                if message == "at least one target selector is required"
        ));
    }

    #[tokio::test]
    async fn requested_validation_preprocesses_each_file_once() {
        for count in [1, 2, 4] {
            let project = Project::new();
            let source = "x = 1 + 2\n".repeat(count);
            std::fs::write(project.root.join("src/calc.py"), &source).unwrap();
            let config = project.config();
            let targets = crate::target::TargetHandler::resolve(&config.selection)
                .await
                .unwrap();
            let output = super::create(
                config.clone(),
                hoimin_core::DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
            )
            .await
            .unwrap();
            // Each line has one binary and four integer-neighbor candidates.
            assert_eq!(output.manifest.candidates.len(), 5 * count);
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
                &crate::shell::prepare_run_config(config).unwrap(),
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
        let output = super::create(
            project.config(),
            hoimin_core::DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
        )
        .await
        .unwrap();
        let manifest_json = serde_json::to_vec(&output.manifest).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(output.manifest.candidates.len(), 7_680);
        assert_ne!(manifest_json, Vec::<u8>::new());
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

    #[tokio::test]
    async fn create_rejects_oversized_saved_discovery_before_project_work() {
        // Arrange: deserialization checks positivity; semantic validation owns the ceiling.
        let project = Project::new();
        let mut config = project.config().into_data();
        config.selection.root = project.root.join("missing");
        let config = RunConfig::try_from(config).unwrap();
        let discovery = serde_json::from_value(serde_json::json!({
            "analyzer_timeout": {"secs": hoimin_core::MAX_TIMEOUT.as_secs(), "nanos": 1}
        }))
        .unwrap();
        // Act.
        let error = super::create(config, discovery).await.unwrap_err();
        // Assert.
        assert_eq!(
            error.to_string(),
            "plan.manifest.invalid: invalid zero or overflowing limit: --analyzer-timeout"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn create_timeout_uses_the_common_plan_discovery_identity_without_output() {
        let project = Project::new();
        let (control, entered, release) = paused_control();
        let operation = tokio::spawn(create_with_discovery_control(
            project.config(),
            hoimin_core::DiscoveryConfig::new(Duration::from_millis(20)).unwrap(),
            Some(control),
        ));
        wait_for_discovery_entry(entered).await;

        let result = tokio::time::timeout(Duration::from_millis(200), operation)
            .await
            .expect("plan create timeout must not await detached discovery")
            .unwrap();
        let stdout = Vec::<u8>::new();
        let error = result
            .map(|_| ())
            .expect_err("paused discovery unexpectedly created a plan");
        assert_eq!(
            error.to_string(),
            "plan.discovery: analyzer.timeout: --analyzer-timeout expired after 20ms"
        );
        assert_eq!(stdout, Vec::<u8>::new());
        release.send(()).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn verify_timeout_uses_the_common_identity_before_the_test_command() {
        let project = Project::new();
        let mut output = super::create(
            project.config(),
            hoimin_core::DiscoveryConfig::new(Duration::from_secs(1)).unwrap(),
        )
        .await
        .unwrap();
        output.manifest.normalized_config.discovery =
            hoimin_core::DiscoveryConfig::new(Duration::from_millis(20)).unwrap();
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
            project.config(),
            hoimin_core::DiscoveryConfig::new(timeout).unwrap(),
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
                .discovery
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

        let execution = serde_json::to_value(&verified.config).unwrap();
        assert!(execution.get("discovery").is_none());
        assert!(execution["limits"].get("analyzer_timeout").is_none());
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
