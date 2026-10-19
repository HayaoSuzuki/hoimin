//! Run-report decoding shared by progress and review.
//!
//! Progress reads current-schema JSON/JSONL and reports unusable runs without
//! rejecting their valid structure. Review requires a complete current-schema JSON
//! document with a passing baseline. Callers own plan matching, source freshness,
//! duplicate handling and command output.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};

use std::path::{Path, PathBuf};

use hoimin_core::{
    ExitPolicy, MutantFinished, MutationSummary, OutputEvent, ProcessTermination,
    REPORT_SCHEMA_VERSION, ReportVersions, ResourceControl, VerificationSelection, exit_code_for,
    summarize,
};
use serde::Deserialize;
use thiserror::Error;

mod dispatch;
mod jsonl;

#[derive(Debug)]
pub enum InputReport {
    Usable(UsableReport),
    Unusable {
        source: PathBuf,
        reason: UnusableReason,
    },
}

#[derive(Debug)]
pub struct UsableReport {
    pub source: PathBuf,
    pub mutants: Vec<MutantFinished>,
}

pub(crate) struct ReviewReport {
    pub project: camino::Utf8PathBuf,
    pub run_id: String,
    pub mutants: Vec<MutantFinished>,
}

/// Decode a complete current-schema JSON report with a passing baseline.
///
/// Validates report structure and resolves the embedded configuration's project
/// root through the filesystem. Missing or non-UTF-8 roots are errors. `path`
/// identifies the report in diagnostics; the caller supplies already-bounded
/// bytes and checks the resulting project and candidates against its plan.
///
/// # Errors
///
/// Returns an error for invalid JSON, unsupported schema, invalid configuration
/// or structure, an unusable run, or failure to resolve the project root.
pub(crate) fn read_review_json(path: &Path, bytes: &[u8]) -> Result<ReviewReport, ProgressError> {
    if let Some(header) = dispatch::probe(bytes) {
        check_schema(path, header.schema_version)?;
    }
    let document: RunReportDocument<hoimin_core::RunConfig> = serde_json::from_slice(bytes)
        .map_err(|source| ProgressError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    check_schema(path, document.schema_version)?;
    let run_id = document.run.run_id().to_owned();
    let project = std::fs::canonicalize(&document.run.value().normalized_config.selection.root)
        .map_err(|source| ProgressError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    let project = camino::Utf8PathBuf::from_path_buf(project)
        .map_err(|_| invalid_structure(path, "review project root is not UTF-8"))?;
    match validate_document(path, document)? {
        InputReport::Usable(report) => Ok(ReviewReport {
            project,
            run_id,
            mutants: report.mutants,
        }),
        InputReport::Unusable { .. } => Err(invalid_structure(
            path,
            "review requires a complete report with a passing baseline",
        )),
    }
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
        "unsupported schema version {found} in progress report {path}; expected {REPORT_SCHEMA_VERSION}; read old reports with their producing binary or rerun with this version"
    )]
    UnsupportedSchema { path: PathBuf, found: u32 },
    #[error("invalid structure in progress report {path}: {message}")]
    InvalidStructure {
        path: PathBuf,
        message: &'static str,
    },
    #[error("invalid structure in progress report {path}: {source}")]
    InvalidResult {
        path: PathBuf,
        #[source]
        source: hoimin_core::ReportSequenceError,
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
enum ReportRunEvent<C> {
    RunStarted(ReportRunStarted<C>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportRunStarted<C> {
    schema_version: u32,
    sequence: u64,
    run_id: String,
    normalized_config: C,
    #[serde(rename = "versions")]
    _versions: ReportVersions,
    resource_control: ResourceControl,
    #[serde(default, rename = "verification_selection")]
    _verification_selection: Option<VerificationSelection>,
    #[serde(default, rename = "resume", deserialize_with = "present_resume")]
    _resume: Option<hoimin_core::ResumeOutcome>,
}

fn present_resume<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<hoimin_core::ResumeOutcome>, D::Error> {
    hoimin_core::ResumeOutcome::deserialize(deserializer).map(Some)
}

impl<C> ReportRunEvent<C> {
    fn value(&self) -> &ReportRunStarted<C> {
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

// Decode configuration with the consumer's type before any Value map can discard
// duplicate fields. Event structure validation is shared independently of that type.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunReportDocument<C> {
    schema_version: u32,
    run: ReportRunEvent<C>,
    baseline: Option<OutputEvent>,
    mutants: Vec<OutputEvent>,
    summary: OutputEvent,
}

/// Reads and validates one mutation run report.
///
/// Accepts only current-schema JSON documents or JSONL reports.
/// Structurally valid runs with no baseline, a failing baseline or incomplete
/// results return `InputReport::Unusable`. This reader does not apply review's
/// byte limit or resolve the embedded project root.
///
/// # Errors
///
/// Returns an error when the report cannot be read, parsed, or structurally validated.
pub fn read_report(path: &Path) -> Result<InputReport, ProgressError> {
    read_buffered_report(path, BufReader::new(open_report(path)?))
}

/// Read with the same acceptance rules as `read_report`, hashing the bytes read.
/// The fingerprint describes this read; callers own subsequent duplicate checks.
pub(crate) fn read_report_with_fingerprint(
    path: &Path,
) -> Result<(InputReport, blake3::Hash), ProgressError> {
    let mut reader = FingerprintReader::new(open_report(path)?);
    let report = read_buffered_report(path, BufReader::new(&mut reader))?;
    Ok((report, reader.fingerprint()))
}

fn open_report(path: &Path) -> Result<File, ProgressError> {
    crate::artifacts::open_regular_file(path)
        .and_then(|file| {
            file.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "input is not a regular file")
            })
        })
        .map_err(|source| ProgressError::Read {
            path: path.to_path_buf(),
            source,
        })
}

fn read_buffered_report(
    path: &Path,
    mut reader: impl BufRead,
) -> Result<InputReport, ProgressError> {
    let mut bytes = Vec::new();
    jsonl::read_line(path, &mut reader, &mut bytes)?;
    let header = dispatch::probe(&bytes);
    if let Some(header) = header {
        check_schema(path, header.schema_version)?;
    }
    let document = if header.is_some_and(|header| header.format == dispatch::Format::Jsonl) {
        jsonl::read_document(path, reader, bytes)?
    } else {
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| ProgressError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        // Pretty documents may not expose both discriminators on the first line.
        let header = header.or_else(|| dispatch::probe(&bytes));
        if let Some(header) = header {
            check_schema(path, header.schema_version)?;
        }
        serde_json::from_slice::<RunReportDocument<serde_json::Value>>(&bytes).map_err(
            |source| ProgressError::Parse {
                path: path.to_path_buf(),
                source,
            },
        )?
    };
    // Progress accepts opaque configurations in both formats, but not other JSON shapes.
    let config = &document.run.value().normalized_config;
    if !config.is_null() && !config.is_object() {
        return Err(invalid_structure(
            path,
            "normalized_config must be null or an object",
        ));
    }

    validate_document(path, document)
}

fn validate_document<C>(
    path: &Path,
    document: RunReportDocument<C>,
) -> Result<InputReport, ProgressError> {
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

fn check_schema(path: &Path, found: u32) -> Result<(), ProgressError> {
    if found == REPORT_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ProgressError::UnsupportedSchema {
            path: path.to_path_buf(),
            found,
        })
    }
}

fn validate_schema_versions<C>(
    path: &Path,
    document: &RunReportDocument<C>,
) -> Result<(), ProgressError> {
    let versions = std::iter::once(document.schema_version)
        .chain(std::iter::once(document.run.schema_version()))
        .chain(document.baseline.iter().map(OutputEvent::schema_version))
        .chain(document.mutants.iter().map(OutputEvent::schema_version))
        .chain(std::iter::once(document.summary.schema_version()));
    for found in versions {
        check_schema(path, found)?;
    }
    Ok(())
}

fn validate_structure<C>(
    path: &Path,
    document: &RunReportDocument<C>,
) -> Result<(), ProgressError> {
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
            OutputEvent::MutantFinished(mutant) => mutant.outcome.status(),
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
    // Keep structural diagnostics first, then guarantee identity for every consumer.
    for event in &document.mutants {
        let OutputEvent::MutantFinished(mutant) = event else {
            unreachable!("mutant event kinds were validated above");
        };
        if !hoimin_core::CandidateIdentityRef::from(&mutant.candidate)
            .matches_id(&mutant.candidate.id)
        {
            return Err(invalid_structure(
                path,
                "candidate identity conflict: stable ID does not match identity fields",
            ));
        }
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

struct FingerprintReader<R> {
    inner: R,
    hasher: blake3::Hasher,
}

impl<R> FingerprintReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
        }
    }

    fn fingerprint(&self) -> blake3::Hash {
        self.hasher.finalize()
    }
}

impl<R: Read> Read for FingerprintReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..count]);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_rejects_old_root_shape_with_schema_migration_guidance() {
        let bytes = include_bytes!("../../tests/golden/reports/schema-v6-current.json");

        let error = read_review_json(Path::new("old.json"), bytes)
            .err()
            .unwrap();

        assert!(matches!(
            error,
            ProgressError::UnsupportedSchema { found: 6, .. }
        ));
        assert!(error.to_string().contains("rerun with this version"));
    }

    fn current_report_json() -> String {
        let document: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/golden/reports/schema-v8-current.json"
        ))
        .unwrap();
        serde_json::to_string(&document).unwrap()
    }

    fn reject_config_before_root_read(change: fn(&mut serde_json::Value), expected: &str) {
        // Arrange: the valid control reaches canonicalization and fails there.
        let project = tempfile::tempdir().unwrap();
        let mut document: serde_json::Value = serde_json::from_str(&current_report_json()).unwrap();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path().join("missing-project"));
        change(&mut document["run"]["normalized_config"]);

        // Act
        let result = read_review_json(
            Path::new("report.json"),
            &serde_json::to_vec(&document).unwrap(),
        );

        // Assert
        let error = result.err().expect("invalid config must fail");
        assert!(matches!(error, ProgressError::Parse { .. }), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
    }

    macro_rules! invalid_review_config_cases {
        ($($name:ident: $change:expr => $expected:literal;)*) => {
            $(#[test]
            fn $name() {
                reject_config_before_root_read($change, $expected);
            })*
        };
    }

    invalid_review_config_cases! {
        review_rejects_jobs_above_absolute_cap: |config| {
            config["limits"]["jobs"] = serde_json::json!(257);
            config["limits"]["max_processes"] = serde_json::json!(300);
        } => "--jobs 257 exceeds the supported maximum 256";
        review_rejects_jobs_above_process_cap: |config| config["limits"]["jobs"] = serde_json::json!(65)
            => "--jobs 65 exceeds --max-processes 64";
        review_rejects_empty_selector: |config| {
            config["selection"]["sources"] = serde_json::json!([]);
            config["selection"]["files"] = serde_json::json!([]);
            config["selection"]["lines"] = serde_json::json!([]);
            config["selection"]["symbols"] = serde_json::json!([]);
            config["selection"]["changed"] = serde_json::Value::Null;
        } => "at least one target selector is required";
        review_rejects_empty_operators: |config| config["operators"] = serde_json::json!([])
            => "mutation operator selection is empty";
        review_rejects_empty_test_argv: |config| config["test_argv"] = serde_json::json!([])
            => "at least one test argv element is required";
        review_rejects_escaping_import_root: |config| config["import_roots"] = serde_json::json!(["../escape"])
            => "invalid --import-root ../escape";
    }

    #[test]
    fn review_valid_config_reaches_project_root_read() {
        // Arrange
        let project = tempfile::tempdir().unwrap();
        let mut document: serde_json::Value = serde_json::from_str(&current_report_json()).unwrap();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path().join("missing-project"));

        // Act
        let result = read_review_json(
            Path::new("report.json"),
            &serde_json::to_vec(&document).unwrap(),
        );

        // Assert
        assert!(matches!(result, Err(ProgressError::Read { .. })));
    }

    #[test]
    fn review_preserves_required_and_known_config_fields() {
        for (original, replacement) in [
            (r#""import_roots":[],"#, ""),
            (r#""import_roots":[]"#, r#""import_roots":null"#),
            (r#""fingerprint_files":[],"#, ""),
            (r#""fingerprint_files":[]"#, r#""fingerprint_files":null"#),
            (
                r#""import_roots":[]"#,
                r#""import_roots":[],"future_config":true"#,
            ),
            (r#""jobs":1"#, r#""jobs":0"#),
        ] {
            // Arrange
            let original_json = current_report_json();
            assert_eq!(original_json.matches(original).count(), 1);
            let raw = original_json.replacen(original, replacement, 1);

            // Act
            let result = read_review_json(Path::new("report.json"), raw.as_bytes());

            // Assert
            assert!(
                matches!(result, Err(ProgressError::Parse { .. })),
                "accepted {replacement}"
            );
        }
    }

    #[test]
    fn review_rejects_raw_duplicate_config_fields() {
        for (original, duplicate) in [
            (
                r#""import_roots":[]"#,
                r#""import_roots":[],"import_roots":[]"#,
            ),
            (
                r#""import_roots":[]"#,
                r#""import_roots":[],"import_roots":["src"]"#,
            ),
            (
                r#""import_roots":[]"#,
                r#""import_roots":["src"],"import_roots":[]"#,
            ),
            (
                r#""import_roots":[]"#,
                r#""import_roots":null,"import_roots":[]"#,
            ),
            (
                r#""import_roots":[]"#,
                r#""import_roots":[],"import_roots":null"#,
            ),
            (
                r#""fingerprint_files":[]"#,
                r#""fingerprint_files":[],"fingerprint_files":[]"#,
            ),
            (
                r#""allow_best_effort_memory":false"#,
                r#""allow_best_effort_memory":true,"allow_best_effort_memory":false"#,
            ),
            (r#""jobs":1"#, r#""jobs":1,"jobs":1"#),
            (r#""jobs":1"#, r#""jobs":1,"jobs":2"#),
            (r#""jobs":1"#, r#""jobs":2,"jobs":1"#),
            (r#""secs":60}"#, r#""secs":60,"secs":61}"#),
            (
                r#""metrics":"metrics.json""#,
                r#""metrics":null,"metrics":null"#,
            ),
            (
                r#""path":"session.sqlite3""#,
                r#""path":"other.sqlite3","path":"session.sqlite3""#,
            ),
            (r#""mode":"resume""#, r#""mode":"resume","mode":"resume""#),
        ] {
            // Arrange: insert duplicates after serialization so no Value map can erase them.
            let original_json = current_report_json();
            assert_eq!(original_json.matches(original).count(), 1);
            let raw = original_json.replacen(original, duplicate, 1);

            // Act
            let result = read_review_json(Path::new("report.json"), raw.as_bytes());

            // Assert
            assert!(
                matches!(result, Err(ProgressError::Parse { .. })),
                "accepted {duplicate}"
            );
        }
    }

    proptest::proptest! {
        #[test]
        fn review_rejects_duplicate_nested_jobs_with_equivalent_key_spellings(
            first in 1_u16..=256,
            second in 1_u16..=256,
            escaped_first in proptest::bool::ANY,
            whitespace in "[ \\t\\r\\n]{0,8}",
        ) {
            // Arrange: both values are valid in isolation; escaped names denote the same key.
            let (first_key, second_key) = if escaped_first {
                (r#""\u006aobs""#, r#""jobs""#)
            } else {
                (r#""jobs""#, r#""\u006aobs""#)
            };
            let duplicate = format!("{first_key}:{first},{whitespace}{second_key}:{second}");
            let original = current_report_json().replace(r#""max_processes":64"#, r#""max_processes":256"#);
            let raw = original.replacen(r#""jobs":1"#, &duplicate, 1);

            // Act
            let result = read_review_json(Path::new("report.json"), raw.as_bytes());

            // Assert
            let error = result.map(|_| ()).expect_err("duplicate jobs must be rejected");
            proptest::prop_assert!(error.to_string().contains("duplicate field `jobs`"));
        }
    }

    #[test]
    fn fingerprint_hashes_only_bytes_returned_by_reads() {
        let bytes = b"partial final read";
        let mut reader = FingerprintReader::new(bytes.as_slice());
        let mut buffer = [0xdd; 7];

        while reader.read(&mut buffer).unwrap() != 0 {}

        assert_eq!(reader.fingerprint(), blake3::hash(bytes));
    }
}
