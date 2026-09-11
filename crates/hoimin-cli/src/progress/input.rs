use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hoimin_core::{
    BaselineFinished, ExitPolicy, MutantFinished, MutationSummary, OutputEvent, ProcessTermination,
    REPORT_SCHEMA_VERSION, ReportVersions, ResourceControl, VerificationSelection, exit_code_for,
    summarize,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug)]
pub enum InputReport {
    Usable(UsableReport),
    Unusable {
        source: PathBuf,
        reason: UnusableReason,
    },
}

#[derive(Debug)]
pub(super) struct InputDisposition {
    pub(super) source: PathBuf,
    pub(super) reason: Option<UnusableReason>,
}

impl From<&InputReport> for InputDisposition {
    fn from(input: &InputReport) -> Self {
        match input {
            InputReport::Usable(report) => Self {
                source: report.source.clone(),
                reason: None,
            },
            InputReport::Unusable { source, reason } => Self {
                source: source.clone(),
                reason: Some(*reason),
            },
        }
    }
}

impl InputDisposition {
    pub(super) fn is_usable(&self) -> bool {
        self.reason.is_none()
    }
}

#[derive(Debug)]
pub struct UsableReport {
    pub source: PathBuf,
    pub mutants: Vec<MutantFinished>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnusableReason {
    MissingBaseline,
    BaselineFailed,
    Incomplete,
}

#[derive(Debug, Error)]
pub enum ProgressError {
    #[error("could not read progress report {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse progress report {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "unsupported schema version {found} in progress report {path}; expected {REPORT_SCHEMA_VERSION}"
    )]
    UnsupportedSchema { path: PathBuf, found: u32 },
    #[error("invalid structure in progress report {path}: {message}")]
    InvalidStructure {
        path: PathBuf,
        message: &'static str,
    },
    #[error("could not serialize progress output: {source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },
    #[error("could not write progress output: {source}")]
    Write {
        #[source]
        source: std::io::Error,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ProgressRunEvent {
    RunStarted(ProgressRunStarted),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressRunStarted {
    schema_version: u32,
    sequence: u64,
    run_id: String,
    normalized_config: serde_json::Value,
    #[serde(rename = "versions")]
    _versions: ReportVersions,
    #[serde(rename = "resource_control")]
    _resource_control: ResourceControl,
    #[serde(default, rename = "verification_selection")]
    _verification_selection: Option<VerificationSelection>,
}

impl ProgressRunEvent {
    fn value(&self) -> &ProgressRunStarted {
        match self {
            Self::RunStarted(value) => value,
        }
    }

    fn schema_version(&self) -> u32 {
        self.value().schema_version
    }

    fn sequence(&self) -> u64 {
        self.value().sequence
    }

    fn run_id(&self) -> &str {
        &self.value().run_id
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunReportDocument {
    schema_version: u32,
    run: ProgressRunEvent,
    baseline: Option<OutputEvent>,
    mutants: Vec<OutputEvent>,
    summary: OutputEvent,
}

#[derive(Deserialize)]
struct ReportHeader {
    schema_version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyV2ReportDocument {
    schema_version: u32,
    run: serde_json::Value,
    baseline: Option<serde_json::Value>,
    mutants: Vec<serde_json::Value>,
    summary: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyV2RunSummary {
    schema_version: u32,
    sequence: u64,
    run_id: String,
    counts: MutationSummary,
    complete: bool,
    exit_code: i32,
    #[serde(default, rename = "verification_selection")]
    _verification_selection: Option<VerificationSelection>,
}

/// Reads and validates one mutation run report.
///
/// # Errors
///
/// Returns an error when the report cannot be read, parsed, or structurally validated.
pub fn read_report(path: &Path) -> Result<InputReport, ProgressError> {
    let bytes = fs::read(path).map_err(|source| ProgressError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let header =
        serde_json::from_slice::<ReportHeader>(&bytes).map_err(|source| ProgressError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    if header.schema_version == 2 {
        return read_legacy_v2(path, &bytes);
    }
    let document = serde_json::from_slice::<RunReportDocument>(&bytes).map_err(|source| {
        ProgressError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })?;

    validate_schema_versions(path, &document)?;
    validate_structure(path, &document)?;

    let source = path.to_path_buf();
    let Some(OutputEvent::BaselineFinished(baseline)) = document.baseline else {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::MissingBaseline,
        });
    };
    if baseline.termination != ProcessTermination::Exit(0) {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::BaselineFailed,
        });
    }
    let OutputEvent::RunFinished(summary) = document.summary else {
        unreachable!("validate_structure requires a run_finished summary");
    };
    if !summary.complete {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::Incomplete,
        });
    }

    let mutants = document
        .mutants
        .into_iter()
        .map(|event| match event {
            OutputEvent::MutantFinished(mutant) => mutant,
            _ => unreachable!("validate_structure requires only mutant_finished events"),
        })
        .collect();
    Ok(InputReport::Usable(UsableReport { source, mutants }))
}

fn read_legacy_v2(path: &Path, bytes: &[u8]) -> Result<InputReport, ProgressError> {
    let document = serde_json::from_slice::<LegacyV2ReportDocument>(bytes).map_err(|source| {
        ProgressError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })?;
    if document.schema_version != 2 {
        return Err(ProgressError::UnsupportedSchema {
            path: path.to_path_buf(),
            found: document.schema_version,
        });
    }
    let run = decode_legacy_event::<ProgressRunStarted>(path, &document.run, "run_started")?;
    validate_legacy_version(path, run.schema_version)?;
    if !run.normalized_config.is_null() && !run.normalized_config.is_object() {
        return Err(invalid_structure(
            path,
            "normalized_config must be null or an object",
        ));
    }
    let baseline = document
        .baseline
        .map(|value| decode_legacy_event::<BaselineFinished>(path, &value, "baseline_finished"))
        .transpose()?;
    let mutants = document
        .mutants
        .into_iter()
        .map(|value| decode_legacy_event::<MutantFinished>(path, &value, "mutant_finished"))
        .collect::<Result<Vec<_>, _>>()?;
    let summary =
        decode_legacy_event::<LegacyV2RunSummary>(path, &document.summary, "run_finished")?;

    for version in baseline
        .iter()
        .map(|value| value.schema_version)
        .chain(mutants.iter().map(|value| value.schema_version))
        .chain(std::iter::once(summary.schema_version))
    {
        validate_legacy_version(path, version)?;
    }
    validate_legacy_structure(path, &run, baseline.as_ref(), &mutants, &summary)?;

    let source = path.to_path_buf();
    let Some(baseline) = baseline else {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::MissingBaseline,
        });
    };
    if baseline.termination != ProcessTermination::Exit(0) {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::BaselineFailed,
        });
    }
    if !summary.complete {
        return Ok(InputReport::Unusable {
            source,
            reason: UnusableReason::Incomplete,
        });
    }
    Ok(InputReport::Usable(UsableReport { source, mutants }))
}

fn decode_legacy_event<T: for<'de> Deserialize<'de>>(
    path: &Path,
    value: &serde_json::Value,
    expected_kind: &'static str,
) -> Result<T, ProgressError> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| invalid_structure(path, "legacy report events must be JSON objects"))?;
    let kind = object
        .remove("kind")
        .and_then(|value| value.as_str().map(str::to_owned));
    if kind.as_deref() != Some(expected_kind) {
        return Err(invalid_structure(
            path,
            "legacy report event kind is invalid",
        ));
    }
    serde_json::from_value(serde_json::Value::Object(object)).map_err(|source| {
        ProgressError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn validate_legacy_version(path: &Path, found: u32) -> Result<(), ProgressError> {
    if found == 2 {
        Ok(())
    } else {
        Err(ProgressError::UnsupportedSchema {
            path: path.to_path_buf(),
            found,
        })
    }
}

fn validate_legacy_structure(
    path: &Path,
    run: &ProgressRunStarted,
    baseline: Option<&BaselineFinished>,
    mutants: &[MutantFinished],
    summary: &LegacyV2RunSummary,
) -> Result<(), ProgressError> {
    let mut stable_identities = BTreeMap::new();
    for mutant in mutants {
        if let Some(expected_sequence) =
            stable_identities.insert(&mutant.candidate.id, mutant.candidate.sequence)
        {
            let message = if expected_sequence == mutant.candidate.sequence {
                "mutant stable IDs must appear at most once"
            } else {
                "mutant stable IDs must map to one candidate sequence"
            };
            return Err(invalid_structure(path, message));
        }
    }
    if summarize(&mutants.iter().map(|value| value.status).collect::<Vec<_>>()) != summary.counts {
        return Err(invalid_structure(
            path,
            "summary counts must match mutant events",
        ));
    }
    let mut previous = run.sequence;
    for (run_id, sequence) in baseline
        .map(|value| (value.run_id.as_str(), value.sequence))
        .into_iter()
        .chain(
            mutants
                .iter()
                .map(|value| (value.run_id.as_str(), value.sequence)),
        )
        .chain(std::iter::once((summary.run_id.as_str(), summary.sequence)))
    {
        if run_id != run.run_id {
            return Err(invalid_structure(
                path,
                "all present events must share run.run_id",
            ));
        }
        if sequence <= previous {
            return Err(invalid_structure(
                path,
                "event sequences must be strictly increasing in document order",
            ));
        }
        previous = sequence;
    }
    validate_summary_coherence(path, &summary.counts, summary.complete, summary.exit_code)
}

fn validate_schema_versions(
    path: &Path,
    document: &RunReportDocument,
) -> Result<(), ProgressError> {
    let versions = std::iter::once(document.schema_version)
        .chain(std::iter::once(document.run.schema_version()))
        .chain(document.baseline.iter().map(OutputEvent::schema_version))
        .chain(document.mutants.iter().map(OutputEvent::schema_version))
        .chain(std::iter::once(document.summary.schema_version()));
    for found in versions {
        if found != REPORT_SCHEMA_VERSION {
            return Err(ProgressError::UnsupportedSchema {
                path: path.to_path_buf(),
                found,
            });
        }
    }
    Ok(())
}

fn validate_structure(path: &Path, document: &RunReportDocument) -> Result<(), ProgressError> {
    let started = document.run.value();
    if !started.normalized_config.is_null() && !started.normalized_config.is_object() {
        return Err(invalid_structure(
            path,
            "normalized_config must be null or an object",
        ));
    }
    if !matches!(
        document.baseline,
        None | Some(OutputEvent::BaselineFinished(_))
    ) {
        return Err(invalid_structure(
            path,
            "baseline must be null or a baseline_finished event",
        ));
    }
    if document
        .mutants
        .iter()
        .any(|event| !matches!(event, OutputEvent::MutantFinished(_)))
    {
        return Err(invalid_structure(
            path,
            "mutants must contain only mutant_finished events",
        ));
    }
    if !matches!(document.summary, OutputEvent::RunFinished(_)) {
        return Err(invalid_structure(
            path,
            "summary must be a run_finished event",
        ));
    }
    let mut stable_identities = BTreeMap::new();
    for event in &document.mutants {
        let OutputEvent::MutantFinished(mutant) = event else {
            unreachable!("mutant event kinds were validated above");
        };
        if let Some(expected_sequence) =
            stable_identities.insert(&mutant.candidate.id, mutant.candidate.sequence)
        {
            let message = if expected_sequence == mutant.candidate.sequence {
                "mutant stable IDs must appear at most once"
            } else {
                "mutant stable IDs must map to one candidate sequence"
            };
            return Err(invalid_structure(path, message));
        }
    }
    let statuses = document
        .mutants
        .iter()
        .map(|event| match event {
            OutputEvent::MutantFinished(mutant) => mutant.status,
            _ => unreachable!("mutant event kinds were validated above"),
        })
        .collect::<Vec<_>>();
    let OutputEvent::RunFinished(summary) = &document.summary else {
        unreachable!("summary event kind was validated above");
    };
    if summarize(&statuses) != summary.counts {
        return Err(invalid_structure(
            path,
            "summary counts must match mutant events",
        ));
    }
    validate_summary_coherence(path, &summary.counts, summary.complete, summary.exit_code)?;

    let run_id = document.run.run_id();
    let events = document
        .baseline
        .iter()
        .chain(document.mutants.iter())
        .chain(std::iter::once(&document.summary));
    let mut previous_sequence = Some(document.run.sequence());
    for event in events {
        if event.run_id() != run_id {
            return Err(invalid_structure(
                path,
                "all present events must share run.run_id",
            ));
        }
        if previous_sequence.is_some_and(|previous| event.sequence() <= previous) {
            return Err(invalid_structure(
                path,
                "event sequences must be strictly increasing in document order",
            ));
        }
        previous_sequence = Some(event.sequence());
    }
    Ok(())
}

fn validate_summary_coherence(
    path: &Path,
    counts: &MutationSummary,
    complete: bool,
    reported_exit_code: i32,
) -> Result<(), ProgressError> {
    let counts_policy = ExitPolicy::from_summary(counts);
    let exit_matches_completion = if complete {
        counts.inconclusive == 0 && reported_exit_code == exit_code_for(counts_policy)
    } else {
        exit_can_result_from_run_failure(counts_policy, reported_exit_code)
    };
    if !exit_matches_completion {
        return Err(invalid_structure(
            path,
            "summary complete and exit_code must be consistent with counts and exit policy",
        ));
    }
    Ok(())
}

fn exit_can_result_from_run_failure(counts_policy: ExitPolicy, reported_exit_code: i32) -> bool {
    let possible_failure_policies = [
        ExitPolicy {
            interrupted: true,
            ..counts_policy
        },
        ExitPolicy {
            infrastructure_error: true,
            ..counts_policy
        },
        ExitPolicy {
            baseline_failed: true,
            ..counts_policy
        },
        ExitPolicy {
            incomplete: true,
            ..counts_policy
        },
    ];
    possible_failure_policies
        .into_iter()
        .any(|policy| exit_code_for(policy) == reported_exit_code)
}

fn invalid_structure(path: &Path, message: &'static str) -> ProgressError {
    ProgressError::InvalidStructure {
        path: path.to_path_buf(),
        message,
    }
}
