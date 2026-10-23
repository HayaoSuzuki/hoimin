//! Run-report decoding shared by progress and review.
//!
//! Current-schema JSONL decoding and structural validation have one owner.
//! Consumers own usability, project resolution, plan matching, source freshness,
//! duplicate handling and command output.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, Read};

use std::path::{Path, PathBuf};

use hoimin_core::{
    BaselineFinished, ExitPolicy, MutantFinished, MutationSummary, REPORT_SCHEMA_VERSION,
    ReportVersions, ResourceControl, RunSummary, VerificationSelection, exit_code_for, summarize,
};
use serde::Deserialize;
use thiserror::Error;

mod jsonl;

/// Structurally checked report data. Consumers own usability and project policy.
#[derive(Debug)]
pub(crate) struct DecodedReport<C> {
    pub(crate) normalized_config: C,
    pub(crate) run_id: String,
    pub(crate) baseline: Option<BaselineFinished>,
    pub(crate) mutants: Vec<MutantFinished>,
    pub(crate) summary: RunSummary,
}

/// Decode bounded JSONL bytes with the complete typed configuration.
pub(crate) fn read_typed_jsonl(
    path: &Path,
    bytes: &[u8],
) -> Result<DecodedReport<hoimin_core::RunConfig>, ReportReadError> {
    read_stream(path, bytes)
}

#[derive(Debug, Error)]
pub enum ReportReadError {
    #[error("could not read report {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse report {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "unsupported schema version {found} in report {path}; expected {REPORT_SCHEMA_VERSION}; read old reports with their producing binary or rerun with this version"
    )]
    UnsupportedSchema { path: PathBuf, found: u32 },
    #[error("invalid structure in report {path}: {message}")]
    InvalidStructure {
        path: PathBuf,
        message: &'static str,
    },
    #[error("invalid structure in report {path}: {source}")]
    InvalidResult {
        path: PathBuf,
        #[source]
        source: hoimin_core::ReportSequenceError,
    },
}

// A single-variant tagged enum would buffer the entire configuration before
// ReportConfigProjection can ignore opaque descendants. Parse the tag as a struct field
// instead, preserving duplicate/unknown-field checks on the original stream.
#[derive(Deserialize)]
#[serde(transparent)]
struct ReportRunEvent<C>(ReportRunStarted<C>);

enum RunStartedKind {
    RunStarted,
}

impl<'de> Deserialize<'de> for RunStartedKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // An externally tagged enum also accepts {"run_started": null}. The wire
        // discriminator is strictly a JSON string, including in review inputs.
        let kind = String::deserialize(deserializer)?;
        match kind.as_str() {
            "run_started" => Ok(Self::RunStarted),
            other => Err(serde::de::Error::unknown_variant(other, &["run_started"])),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportRunStarted<C> {
    #[serde(rename = "kind")]
    _kind: RunStartedKind,
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

// Project only closed configuration descendants for readers that do not need
// the complete RunConfig. Other configuration fields stay opaque, without
// buffering known children into Value maps that would discard duplicate keys.
#[derive(Debug)]
pub(crate) struct ReportConfigProjection;

impl<'de> Deserialize<'de> for ReportConfigProjection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Config {
            #[serde(default, rename = "environment")]
            _environment: Option<hoimin_core::EnvironmentSnapshot>,
            #[serde(default, rename = "session")]
            _session: Option<hoimin_core::SessionMode>,
            #[serde(default, rename = "output")]
            _output: Option<hoimin_core::OutputConfig>,
            #[serde(default, rename = "selection")]
            _selection: Option<Selection>,
        }

        #[derive(Deserialize)]
        struct Selection {
            #[serde(default, rename = "changed")]
            _changed: Option<hoimin_core::ChangedSelection>,
        }

        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ReportConfigProjection;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a normalized_config object or null")
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(ReportConfigProjection)
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                Config::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(ReportConfigProjection)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

fn present_resume<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<hoimin_core::ResumeOutcome>, D::Error> {
    hoimin_core::ResumeOutcome::deserialize(deserializer).map(Some)
}

impl<C> ReportRunEvent<C> {
    fn value(&self) -> &ReportRunStarted<C> {
        &self.0
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

// Streaming decoding retains only payloads needed by report consumers.
struct ReportPayloads<C> {
    run: ReportRunEvent<C>,
    baseline: Option<BaselineFinished>,
    mutants: Vec<MutantFinished>,
    summary: RunSummary,
}

/// Reads and validates one mutation run report.
///
/// Accepts only current-schema JSONL reports.
/// Returns structurally checked data without applying consumer usability policy.
/// This reader does not apply review's byte limit or resolve the project root.
///
/// # Errors
///
/// Returns an error when the report cannot be read, parsed, or structurally validated.
pub(crate) fn read_report(
    path: &Path,
) -> Result<DecodedReport<ReportConfigProjection>, ReportReadError> {
    let reader = report_reader(path)?;
    read_stream(path, BufReader::new(reader))
}

/// Read with the same acceptance rules as `read_report`, hashing the bytes read.
/// The fingerprint describes this read; callers own subsequent duplicate checks.
pub(crate) fn read_report_with_fingerprint(
    path: &Path,
) -> Result<(DecodedReport<ReportConfigProjection>, blake3::Hash), ReportReadError> {
    let reader = report_reader(path)?;
    let mut reader = FingerprintReader::new(reader);
    let report = read_stream(path, BufReader::new(&mut reader))?;
    Ok((report, reader.fingerprint()))
}

fn open_report(path: &Path) -> Result<File, ReportReadError> {
    crate::artifacts::open_regular_file(path)
        .and_then(|file| {
            file.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "input is not a regular file")
            })
        })
        .map_err(|source| ReportReadError::Read {
            path: path.to_path_buf(),
            source,
        })
}

// Bound the streaming pass to the descriptor's initial size. SnapshotReader
// rejects growth and truncation rather than accepting a valid report prefix.
fn report_reader(path: &Path) -> Result<crate::finite_file::SnapshotReader, ReportReadError> {
    crate::finite_file::SnapshotReader::new(open_report(path)?).map_err(|source| {
        ReportReadError::Read {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn read_stream<C: serde::de::DeserializeOwned>(
    path: &Path,
    reader: impl std::io::BufRead,
) -> Result<DecodedReport<C>, ReportReadError> {
    let payloads = jsonl::read_payloads(path, reader)?;
    validate_structure(path, &payloads)?;
    Ok(DecodedReport {
        run_id: payloads.run.run_id().to_owned(),
        normalized_config: payloads.run.0.normalized_config,
        baseline: payloads.baseline,
        mutants: payloads.mutants,
        summary: payloads.summary,
    })
}

#[cfg(test)]
fn read_buffered_report(
    path: &Path,
    bytes: &[u8],
) -> Result<DecodedReport<ReportConfigProjection>, ReportReadError> {
    read_stream(path, bytes)
}

fn check_schema(path: &Path, found: u32) -> Result<(), ReportReadError> {
    if found == REPORT_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ReportReadError::UnsupportedSchema {
            path: path.to_path_buf(),
            found,
        })
    }
}

fn validate_structure<C>(path: &Path, document: &ReportPayloads<C>) -> Result<(), ReportReadError> {
    let mut stable_identities = BTreeMap::new();
    for mutant in &document.mutants {
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
        .map(|mutant| mutant.outcome.status())
        .collect::<Vec<_>>();
    let summary = &document.summary;
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
        .map(|event| (event.run_id.as_str(), event.sequence))
        .chain(
            document
                .mutants
                .iter()
                .map(|event| (event.run_id.as_str(), event.sequence)),
        )
        .chain(std::iter::once((summary.run_id.as_str(), summary.sequence)));
    let mut previous_sequence = Some(document.run.sequence());
    for (event_run_id, sequence) in events {
        if event_run_id != run_id {
            return Err(invalid_structure(
                path,
                "all present events must share run.run_id",
            ));
        }
        if previous_sequence.is_some_and(|previous| sequence <= previous) {
            return Err(invalid_structure(
                path,
                "event sequences must be strictly increasing in document order",
            ));
        }
        previous_sequence = Some(sequence);
    }
    // Keep structural diagnostics first, then guarantee identity for every consumer.
    for mutant in &document.mutants {
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
) -> Result<(), ReportReadError> {
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

fn invalid_structure(path: &Path, message: &'static str) -> ReportReadError {
    ReportReadError::InvalidStructure {
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
pub(crate) mod test_support {
    pub(crate) fn report_view(bytes: &[u8]) -> serde_json::Value {
        let events: Vec<serde_json::Value> = std::str::from_utf8(bytes)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let first = events.first().unwrap();
        serde_json::json!({
            "schema_version": first["schema_version"],
            "run": first,
            "baseline": events.iter().find(|event| event["kind"] == "baseline_finished"),
            "mutants": events.iter().filter(|event| event["kind"] == "mutant_finished").collect::<Vec<_>>(),
            "summary": events.iter().find(|event| event["kind"] == "run_finished").unwrap(),
        })
    }

    pub(crate) fn current_report_view() -> serde_json::Value {
        report_view(include_bytes!(
            "../../tests/golden/events/schema-v10-current.jsonl"
        ))
    }

    // Tests edit payloads through a named view, then serialize the real event
    // protocol including the starts that must match each finished candidate.
    pub(crate) fn jsonl_bytes(view: &serde_json::Value) -> Vec<u8> {
        let mut events = vec![view["run"].clone()];
        if !view["baseline"].is_null() {
            events.push(view["baseline"].clone());
        }
        for mutant in view["mutants"].as_array().unwrap() {
            events.push(serde_json::json!({
                "kind": "mutant_started", "schema_version": mutant["schema_version"],
                "run_id": mutant["run_id"], "sequence": mutant["sequence"].as_u64().unwrap() - 1,
                "mutant_id": mutant["candidate"]["id"], "mutant_sequence": mutant["candidate"]["sequence"],
            }));
            events.push(mutant.clone());
        }
        events.push(view["summary"].clone());
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn aggregate_json_reports_are_rejected() {
        let bytes = serde_json::to_vec(&super::test_support::current_report_view()).unwrap();
        assert!(super::read_buffered_report(std::path::Path::new("legacy.json"), &bytes).is_err());
        assert!(super::read_typed_jsonl(std::path::Path::new("legacy.json"), &bytes).is_err());
    }

    macro_rules! required_janitor_case {
        ($name:ident, $present:expr) => {
            #[test]
            fn $name() {
                // Arrange: exercise the actual shared JSONL artifact decoder.
                let mut document = super::test_support::current_report_view();
                if !$present {
                    document["summary"]["disk"]
                        .as_object_mut()
                        .unwrap()
                        .remove("janitor");
                }
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("report.json");
                std::fs::write(&path, jsonl_bytes(&document)).unwrap();
                // Act.
                let result = super::read_report(&path);
                // Assert.
                assert_eq!(result.is_ok(), $present, "{result:?}");
            }
        };
    }
    required_janitor_case!(artifact_reader_rejects_missing_janitor, false);
    required_janitor_case!(artifact_reader_accepts_explicit_null_janitor, true);

    use super::test_support::{current_report_view, jsonl_bytes};
    use super::*;

    #[test]
    fn complete_report_with_survivors_requires_success_exit() {
        let mut counts = MutationSummary::default();
        counts.record(hoimin_core::MutationStatus::Survived);

        assert!(validate_summary_coherence(Path::new("report.json"), &counts, true, 0).is_ok());
        assert!(validate_summary_coherence(Path::new("report.json"), &counts, true, 1).is_err());
        assert!(validate_summary_coherence(Path::new("report.json"), &counts, false, 0).is_err());
    }

    #[test]
    fn review_decode_errors_identify_report_without_progress_context() {
        let error = read_typed_jsonl(Path::new("review-input.json"), b"{")
            .expect_err("malformed report must fail");
        let diagnostic = error.to_string();
        assert!(
            diagnostic.contains("report review-input.json"),
            "{diagnostic}"
        );
        assert!(!diagnostic.contains("progress"), "{diagnostic}");
    }

    #[test]
    fn review_decoder_leaves_completion_policy_to_caller() {
        let project = tempfile::tempdir().unwrap();
        let mut document = current_report_view();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path());
        document["summary"]["complete"] = serde_json::json!(false);
        document["summary"]["exit_code"] = serde_json::json!(4);
        let result = read_typed_jsonl(Path::new("review-input.json"), &jsonl_bytes(&document));
        assert!(
            result.is_ok(),
            "structurally valid incomplete report must decode"
        );
    }

    #[test]
    fn review_rejects_old_root_shape() {
        let bytes = include_bytes!("../../tests/golden/reports/schema-v6-current.json");

        assert!(read_typed_jsonl(Path::new("old.json"), bytes).is_err());
    }

    #[test]
    fn review_rejects_old_shape_during_decode_before_project_root_read() {
        let project = tempfile::tempdir().unwrap();
        let mut document: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../tests/golden/reports/schema-v6-current.json"
        ))
        .unwrap();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path().join("missing-project"));
        document.as_object_mut().unwrap().remove("schema_version");
        let payload = serde_json::to_string(&document).unwrap();
        let wire = format!(r#"{{"schema_version":6,{}"#, &payload[1..]);

        let error = read_typed_jsonl(Path::new("old.json"), wire.as_bytes())
            .expect_err("old shape must fail during typed decode");

        assert!(matches!(error, ReportReadError::Parse { .. }), "{error}");
    }

    fn current_report_jsonl() -> String {
        String::from_utf8(jsonl_bytes(&current_report_view())).unwrap()
    }

    #[test]
    fn review_accepts_current_schema_first_and_last() {
        let project = tempfile::tempdir().unwrap();
        let mut document = current_report_view();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path());
        let mut events: Vec<serde_json::Value> = std::str::from_utf8(&jsonl_bytes(&document))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let schema = events[0]
            .as_object_mut()
            .unwrap()
            .remove("schema_version")
            .unwrap();
        let payload = serde_json::to_string(&events[0]).unwrap();
        let rest = events[1..]
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        for first in [
            format!(r#"{{"schema_version":{schema},{}"#, &payload[1..]),
            format!(
                r#"{},"schema_version":{schema}}}"#,
                &payload[..payload.len() - 1]
            ),
        ] {
            let wire = format!("{first}\n{rest}");
            let report = read_typed_jsonl(Path::new("report.json"), wire.as_bytes())
                .unwrap_or_else(|error| panic!("{error}"));

            assert_eq!(
                report.normalized_config.selection.root.as_std_path(),
                project.path()
            );
            assert_eq!(report.run_id, "schema-v10-current");
            assert_eq!(report.mutants.len(), 1);
        }
    }

    #[test]
    fn review_checks_event_schema_before_project_root_read() {
        let project = tempfile::tempdir().unwrap();
        let mut document = current_report_view();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path().join("missing-project"));
        document["run"]["schema_version"] = serde_json::json!(9);

        let result = read_typed_jsonl(Path::new("report.json"), &jsonl_bytes(&document));

        assert!(matches!(
            result,
            Err(ReportReadError::UnsupportedSchema { found: 9, .. })
        ));
    }

    fn reject_config_before_root_read(change: fn(&mut serde_json::Value), expected: &str) {
        // Arrange: project availability must not affect typed configuration validation.
        let project = tempfile::tempdir().unwrap();
        let mut document = current_report_view();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path().join("missing-project"));
        change(&mut document["run"]["normalized_config"]);

        // Act
        let result = read_typed_jsonl(Path::new("report.json"), &jsonl_bytes(&document));

        // Assert
        let error = result.expect_err("invalid config must fail");
        assert!(matches!(error, ReportReadError::Parse { .. }), "{error}");
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
        review_rejects_unknown_nested_config_before_project_root_read: |config| {
            config["limits"]["future_limit"] = serde_json::json!(1);
        } => "unknown field `future_limit`";
        review_rejects_jobs_above_absolute_cap: |config| {
            config["limits"]["jobs"] = serde_json::json!(257);
            config["limits"]["max_processes"] = serde_json::json!(300);
        } => "--jobs 257 exceeds the supported maximum 256";
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
    fn review_decoder_does_not_resolve_project_root() {
        // Arrange
        let project = tempfile::tempdir().unwrap();
        let mut document = current_report_view();
        document["run"]["normalized_config"]["selection"]["root"] =
            serde_json::json!(project.path().join("missing-project"));
        document["run"]["normalized_config"]["limits"]["jobs"] = serde_json::json!(4);
        document["run"]["normalized_config"]["limits"]["max_processes"] = serde_json::json!(1);

        // Act
        let result = read_typed_jsonl(Path::new("report.json"), &jsonl_bytes(&document));

        // Assert
        assert!(result.is_ok(), "decoder must not resolve project roots");
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
            let original_json = current_report_jsonl();
            assert_eq!(original_json.matches(original).count(), 1);
            let raw = original_json.replacen(original, replacement, 1);

            // Act
            let result = read_typed_jsonl(Path::new("report.json"), raw.as_bytes());

            // Assert
            assert!(
                matches!(result, Err(ReportReadError::Parse { .. })),
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
            let original_json = current_report_jsonl();
            assert_eq!(original_json.matches(original).count(), 1);
            let raw = original_json.replacen(original, duplicate, 1);

            // Act
            let result = read_typed_jsonl(Path::new("report.json"), raw.as_bytes());

            // Assert
            assert!(
                matches!(result, Err(ReportReadError::Parse { .. })),
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
            whitespace in "[ \\t\\r]{0,8}",
        ) {
            // Arrange: both values are valid in isolation; escaped names denote the same key.
            let (first_key, second_key) = if escaped_first {
                (r#""\u006aobs""#, r#""jobs""#)
            } else {
                (r#""jobs""#, r#""\u006aobs""#)
            };
            let duplicate = format!("{first_key}:{first},{whitespace}{second_key}:{second}");
            let original = current_report_jsonl().replace(r#""max_processes":64"#, r#""max_processes":256"#);
            let raw = original.replacen(r#""jobs":1"#, &duplicate, 1);

            // Act
            let result = read_typed_jsonl(Path::new("report.json"), raw.as_bytes());

            // Assert
            let error = result.map(|_| ()).expect_err("duplicate jobs must be rejected");
            proptest::prop_assert!(error.to_string().contains("duplicate field `jobs`"));
        }
    }

    #[test]
    fn fingerprint_covers_complete_jsonl_reads_including_whitespace() {
        let directory = tempfile::tempdir().unwrap();
        for input in [
            current_report_jsonl(),
            include_str!("../../tests/golden/events/schema-v10-current.jsonl").to_owned(),
        ] {
            // More than one BufReader buffer, including a final blank line.
            let bytes = format!("\n{}{}\n", input.trim_end(), " ".repeat(32 * 1024));
            let path = directory.path().join("report");
            std::fs::write(&path, &bytes).unwrap();
            let (_, fingerprint) = read_report_with_fingerprint(&path).unwrap();
            assert_eq!(fingerprint, blake3::hash(bytes.as_bytes()));
        }
    }

    #[test]
    fn progress_rejects_appends_instead_of_accepting_a_valid_prefix() {
        use std::io::Write;
        let directory = tempfile::tempdir().unwrap();
        for input in [
            current_report_jsonl(),
            include_str!("../../tests/golden/events/schema-v10-current.jsonl").to_owned(),
        ] {
            let path = directory.path().join("report");
            std::fs::write(&path, &input).unwrap();
            let reader = report_reader(&path).unwrap();
            let mut appended = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            appended.write_all(&vec![b' '; 32 * 1024]).unwrap();
            let error =
                read_stream::<ReportConfigProjection>(&path, BufReader::new(reader)).unwrap_err();
            assert!(
                error.to_string().contains("grew while being read"),
                "{error}"
            );
        }
    }

    #[test]
    fn jsonl_streaming_keeps_physical_line_boundaries() {
        let events = include_str!("../../tests/golden/events/schema-v10-current.jsonl");
        let mut lines = events.lines();
        let first = lines.next().unwrap();
        let second = lines.next().unwrap();
        let rest = lines.collect::<Vec<_>>().join("\n");
        for malformed in [
            format!("{first} {second}\n{rest}"),
            format!("{}\n{}\n{second}\n{rest}", &first[..1], &first[1..]),
        ] {
            assert!(read_buffered_report(Path::new("report"), malformed.as_bytes()).is_err());
        }
        let valid = format!("\n  \n\t{events}\n  \t");
        assert!(read_buffered_report(Path::new("report"), valid.as_bytes()).is_ok());
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

#[cfg(test)]
mod closed_wire_tests;
