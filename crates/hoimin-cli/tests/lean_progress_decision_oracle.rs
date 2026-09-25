use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use hoimin_cli::progress::{
    InputReport, ProgressResult, ProgressState, UsableReport, compare_reports,
};
use serde::Deserialize;
use serde_json::{Value, json};

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/progress-decision.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    patience: u64,
    reports: Vec<OracleReport>,
    expected: ExpectedHistory,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleReport {
    usable: bool,
    reason: Option<String>,
    mutants: Vec<OracleMutant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleMutant {
    candidate_id: u64,
    content_key: u64,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedHistory {
    latest_state: String,
    default_exit_code: i32,
    regression_exit_code: i32,
    error_exit_code: i32,
    consecutive_stalls: u64,
    saturated: bool,
    comparisons: Vec<ExpectedComparison>,
    details: ExpectedDetails,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedDetails {
    available: bool,
    eligibility: Option<String>,
    previous_input: usize,
    current_input: usize,
    omitted: usize,
    unidentified: usize,
    transitions: Vec<ExpectedTransition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedTransition {
    candidate_id: u64,
    previous_status: String,
    current_status: String,
    classification: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedComparison {
    eligibility: String,
    common: u64,
    added: u64,
    removed: u64,
    ambiguous: u64,
    inconclusive: u64,
    improvements: u64,
    regressions: u64,
    carried_survivors: u64,
    previous_score: Option<ExpectedScore>,
    current_score: Option<ExpectedScore>,
    score_delta: Option<ExpectedDelta>,
    state: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedScore {
    killed: u64,
    decidable: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedDelta {
    numerator: i64,
    denominator: u64,
}

fn parse_corpus() -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in CORPUS.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&item)?;
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        cases.push(item);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 3 || item.id.is_empty() || item.patience == 0 {
        return Err(format!(
            "{} has an invalid schema, id, or patience",
            item.id
        ));
    }
    if !matches!(
        item.mode.as_str(),
        "strict" | "internal-fixture" | "model-only"
    ) {
        return Err(format!("{} has unknown mode {}", item.id, item.mode));
    }
    if item.reports.is_empty() {
        return Err(format!("{} has no reports", item.id));
    }
    validate_state(&item.id, &item.expected.latest_state, true)?;
    if item.expected.saturated != (item.expected.latest_state == "saturated") {
        return Err(format!("{} has inconsistent saturation fields", item.id));
    }
    for report in &item.reports {
        match (report.usable, report.reason.as_deref()) {
            (true, None) | (false, Some("missing_baseline" | "baseline_failed" | "incomplete")) => {
            }
            _ => return Err(format!("{} has an inconsistent report reason", item.id)),
        }
        let mut candidate_ids = BTreeSet::new();
        for mutant in &report.mutants {
            if mutant.candidate_id > 1 || mutant.content_key > 1 {
                return Err(format!("{} uses a role outside 0..=1", item.id));
            }
            if !matches!(
                mutant.status.as_str(),
                "killed"
                    | "survived"
                    | "timeout"
                    | "out_of_memory"
                    | "process_limit"
                    | "error"
                    | "not_run"
            ) {
                return Err(format!("{} has unknown status {}", item.id, mutant.status));
            }
            if !candidate_ids.insert(mutant.candidate_id) && item.mode == "strict" {
                return Err(format!("{} repeats a candidate ID in strict mode", item.id));
            }
        }
    }
    for comparison in &item.expected.comparisons {
        if !matches!(
            comparison.eligibility.as_str(),
            "matching" | "different" | "duplicate"
        ) {
            return Err(format!("{} has unknown eligibility", item.id));
        }
        validate_state(&item.id, &comparison.state, false)?;
        validate_score(&item.id, comparison.previous_score.as_ref())?;
        validate_score(&item.id, comparison.current_score.as_ref())?;
        validate_delta(&item.id, comparison.score_delta.as_ref())?;
    }
    Ok(())
}

fn validate_state(id: &str, state: &str, latest: bool) -> Result<(), String> {
    let valid = matches!(
        state,
        "improving" | "regressing" | "stalled" | "indeterminate"
    ) || latest && state == "saturated";
    valid
        .then_some(())
        .ok_or_else(|| format!("{id} has invalid state {state}"))
}

fn validate_score(id: &str, score: Option<&ExpectedScore>) -> Result<(), String> {
    let Some(score) = score else {
        return Ok(());
    };
    if score.decidable == 0 || score.killed > score.decidable {
        return Err(format!("{id} has an invalid score fraction"));
    }
    if !matches!((score.killed, score.decidable), (0 | 1, 1) | (1, 2)) {
        return Err(format!(
            "{id} has a score outside the exact adapter boundary"
        ));
    }
    Ok(())
}

fn validate_delta(id: &str, delta: Option<&ExpectedDelta>) -> Result<(), String> {
    let Some(delta) = delta else {
        return Ok(());
    };
    if delta.denominator == 0 {
        return Err(format!("{id} has a zero score-delta denominator"));
    }
    if exact_delta(delta).is_none() {
        return Err(format!("{id} has a score delta outside the exact boundary"));
    }
    Ok(())
}

fn base_report() -> Value {
    serde_json::from_str(include_str!("golden/reports/schema-v2-current.json"))
        .expect("owned schema-v2 golden report must remain valid JSON")
}

fn build_report(case_id: &str, report_index: usize, input: &OracleReport) -> Value {
    let mut document = base_report();
    let run_id = format!("lean-{case_id}-{report_index}");
    document["run"]["run_id"] = json!(run_id);
    document["run"]["normalized_config"] = Value::Null;
    document["baseline"]["run_id"] = json!(run_id);

    let mutants = input
        .mutants
        .iter()
        .enumerate()
        .map(|(index, mutant)| {
            let sequence = u64::try_from(index).unwrap() + 3;
            let content = mutant.content_key;
            json!({
                "kind": "mutant_finished",
                "schema_version": 2,
                "sequence": sequence,
                "run_id": run_id,
                "candidate": {
                    "id": format!("candidate-{}", mutant.candidate_id),
                    "sequence": mutant.candidate_id + 1,
                    "path": format!("src/content-{content}.py"),
                    "span": { "start": content, "length": 1 },
                    "original": format!("original-{content}"),
                    "replacement": format!("replacement-{content}"),
                    "operator": format!("operator-{content}"),
                    "line": content + 1,
                    "column": content,
                    "symbol": format!("symbol-{content}"),
                    "file_hash": format!("hash-{content}")
                },
                "status": mutant.status,
                "termination": null,
                "elapsed_ms": 1,
                "resource_mode": "hard",
                "output": null
            })
        })
        .collect::<Vec<_>>();
    document["mutants"] = Value::Array(mutants);

    let counts = summary_counts(&input.mutants);
    let summary_sequence = u64::try_from(input.mutants.len()).unwrap() + 3;
    document["summary"]["sequence"] = json!(summary_sequence);
    document["summary"]["run_id"] = json!(run_id);
    document["summary"]["counts"] = counts;
    document["summary"]["complete"] = json!(true);
    document["summary"]["exit_code"] = json!(i32::from(
        document["summary"]["counts"]["survived"].as_u64().unwrap() > 0
    ));

    if !input.usable {
        match input.reason.as_deref() {
            Some("missing_baseline") => document["baseline"] = Value::Null,
            Some("baseline_failed") => {
                document["baseline"]["termination"] = json!({ "Exit": 1 });
            }
            Some("incomplete") => {
                document["summary"]["complete"] = json!(false);
                document["summary"]["exit_code"] = json!(4);
            }
            _ => unreachable!("validated unusable reason"),
        }
    }
    document
}

fn summary_counts(mutants: &[OracleMutant]) -> Value {
    let mut killed = 0_u64;
    let mut survived = 0_u64;
    let mut timeout = 0_u64;
    let mut out_of_memory = 0_u64;
    let mut process_limit = 0_u64;
    let mut error = 0_u64;
    let mut not_run = 0_u64;
    for mutant in mutants {
        match mutant.status.as_str() {
            "killed" => killed += 1,
            "survived" => survived += 1,
            "timeout" => timeout += 1,
            "out_of_memory" => out_of_memory += 1,
            "process_limit" => process_limit += 1,
            "error" => error += 1,
            "not_run" => not_run += 1,
            _ => unreachable!("validated status"),
        }
    }
    let decidable = killed + survived;
    let score = (decidable > 0)
        .then(|| exact_score(killed, decidable).expect("two-mutant fixture score is exact"));
    json!({
        "killed": killed,
        "survived": survived,
        "timeout": timeout,
        "out_of_memory": out_of_memory,
        "process_limit": process_limit,
        "error": error,
        "not_run": not_run,
        "inconclusive": timeout + out_of_memory + process_limit + error + not_run,
        "score": score
    })
}

fn write_reports(fixture: &tempfile::TempDir, item: &OracleCase) -> Vec<PathBuf> {
    item.reports
        .iter()
        .enumerate()
        .map(|(index, report)| {
            let path = fixture.path().join(format!("report-{index}.json"));
            let document = build_report(&item.id, index, report);
            std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
            path
        })
        .collect()
}

async fn public_output(
    item: &OracleCase,
    details: bool,
    gate: bool,
    malformed_suffix: bool,
) -> Result<std::process::Output, String> {
    let fixture = tempfile::tempdir().map_err(|error| error.to_string())?;
    let mut reports = write_reports(&fixture, item);
    if malformed_suffix {
        let path = fixture.path().join("malformed.json");
        std::fs::write(&path, "{").map_err(|error| error.to_string())?;
        reports.push(path);
    }
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["progress", "--format", "json", "--patience"])
        .arg(item.patience.to_string())
        .args(&reports)
        .kill_on_drop(true);
    if details {
        command.args(["--details", "--details-limit", "1"]);
    }
    if gate {
        command.arg("--fail-on-regression");
    }
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .map_err(|_| format!("infrastructure-error case={}: CLI timed out", item.id))?
        .map_err(|error| {
            format!(
                "infrastructure-error case={}: spawn failed: {error}",
                item.id
            )
        })
}

async fn public_progress(item: &OracleCase, details: bool, gate: bool) -> Result<Value, String> {
    let output = public_output(item, details, gate, false).await?;
    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(format!(
            "infrastructure-error case={}: exit={:?} stderr={}",
            item.id,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let expected = if gate {
        item.expected.regression_exit_code
    } else {
        item.expected.default_exit_code
    };
    assert_eq!(
        output.status.code(),
        Some(expected),
        "semantic mismatch case={} gate={gate}",
        item.id
    );
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "infrastructure-error case={}: invalid JSON: {error}; stdout={}",
            item.id,
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn expected_score(score: Option<&ExpectedScore>) -> Option<f64> {
    score.map(|value| {
        exact_score(value.killed, value.decidable).expect("validated exact score fraction")
    })
}

fn expected_delta(delta: Option<&ExpectedDelta>) -> Option<f64> {
    delta.map(|value| exact_delta(value).expect("validated exact score delta"))
}

fn exact_score(killed: u64, decidable: u64) -> Option<f64> {
    if killed == 0 && decidable > 0 {
        Some(0.0)
    } else if killed == decidable && decidable > 0 {
        Some(1.0)
    } else if killed.checked_mul(2) == Some(decidable) {
        Some(0.5)
    } else {
        None
    }
}

fn exact_delta(delta: &ExpectedDelta) -> Option<f64> {
    let denominator = i64::try_from(delta.denominator).ok()?;
    if delta.numerator == -denominator {
        Some(-1.0)
    } else if delta.numerator.checked_mul(2) == Some(-denominator) {
        Some(-0.5)
    } else if delta.numerator == 0 {
        Some(0.0)
    } else if delta.numerator.checked_mul(2) == Some(denominator) {
        Some(0.5)
    } else if delta.numerator == denominator {
        Some(1.0)
    } else {
        None
    }
}

fn assert_case_matches(item: &OracleCase, actual: &Value) {
    let context = format!("case={}", item.id);
    assert_eq!(actual["patience"], item.patience, "{context}");
    assert_eq!(
        actual["consecutive_stalls"], item.expected.consecutive_stalls,
        "{context} top-level consecutive stalls"
    );
    assert_eq!(
        actual["latest"]["state"], item.expected.latest_state,
        "{context}"
    );
    assert_eq!(
        actual["latest"]["consecutive_stalls"], item.expected.consecutive_stalls,
        "{context} latest consecutive stalls"
    );
    assert_eq!(actual["latest"]["patience"], item.patience, "{context}");
    assert_eq!(
        actual["latest"]["saturated"], item.expected.saturated,
        "{context}"
    );

    let comparisons = actual["comparisons"]
        .as_array()
        .unwrap_or_else(|| panic!("{context}: comparisons must be an array"));
    assert_eq!(
        comparisons.len(),
        item.expected.comparisons.len(),
        "{context}"
    );
    for (index, (actual, expected)) in comparisons
        .iter()
        .zip(&item.expected.comparisons)
        .enumerate()
    {
        let comparison_context = format!("{context} comparison={index}");
        for (field, value) in [
            ("common", expected.common),
            ("added", expected.added),
            ("removed", expected.removed),
            ("ambiguous", expected.ambiguous),
            ("inconclusive", expected.inconclusive),
            ("improvements", expected.improvements),
            ("regressions", expected.regressions),
            ("carried_survivors", expected.carried_survivors),
        ] {
            assert_eq!(actual[field], value, "{comparison_context} field={field}");
        }
        assert_eq!(actual["state"], expected.state, "{comparison_context}");
        assert_eq!(
            actual["previous_score"].as_f64(),
            expected_score(expected.previous_score.as_ref()),
            "{comparison_context} previous_score"
        );
        assert_eq!(
            actual["current_score"].as_f64(),
            expected_score(expected.current_score.as_ref()),
            "{comparison_context} current_score"
        );
        assert_eq!(
            actual["score_delta"].as_f64(),
            expected_delta(expected.score_delta.as_ref()),
            "{comparison_context} score_delta"
        );
    }
}

#[test]
fn lean_progress_decision_corpus_is_valid() {
    let cases = parse_corpus().expect("Lean corpus must satisfy the strict adapter schema");
    assert_eq!(cases.len(), 26);
    assert_eq!(
        cases.iter().filter(|item| item.mode == "strict").count(),
        19
    );
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        6
    );
    for required in [
        "simultaneous_regression_wins",
        "inconclusive_resets_stalls",
        "unusable_gap_resets_stalls",
        "matching_ids_ignore_duplicate_content",
        "different_ids_duplicate_content_is_ambiguous",
        "duplicate_candidate_id_model_only",
    ] {
        assert!(
            cases.iter().any(|item| item.id == required),
            "missing {required}"
        );
    }
    for status in [
        "timeout",
        "out_of_memory",
        "process_limit",
        "error",
        "not_run",
    ] {
        assert!(
            cases.iter().any(|item| item.mode == "internal-fixture"
                && item
                    .reports
                    .iter()
                    .flat_map(|report| &report.mutants)
                    .any(|mutant| mutant.status == status)),
            "missing internal comparison status {status}"
        );
    }
}

#[tokio::test]
async fn public_progress_matches_every_strict_lean_case() {
    let cases = parse_corpus().expect("valid Lean corpus");
    for item in cases.iter().filter(|item| item.mode == "strict") {
        let actual = public_progress(item, false, false)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_case_matches(item, &actual);
    }
}

fn state_name(state: ProgressState) -> &'static str {
    match state {
        ProgressState::Improving => "improving",
        ProgressState::Regressing => "regressing",
        ProgressState::Stalled => "stalled",
        ProgressState::Saturated => "saturated",
        ProgressState::Indeterminate => "indeterminate",
    }
}

fn inject_usable_reports_without_reader_validation(item: &OracleCase) -> Vec<InputReport> {
    item.reports
        .iter()
        .enumerate()
        .map(|(index, report)| {
            assert!(
                report.usable,
                "internal fixture must explicitly inject usable reports"
            );
            let doc = build_report(&item.id, index, report);
            InputReport::Usable(UsableReport {
                source: PathBuf::from(format!("internal-{index}.json")),
                mutants: doc["mutants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|event| serde_json::from_value(event.clone()).unwrap())
                    .collect(),
            })
        })
        .collect()
}

fn comparison_result_as_cli_json(actual: &ProgressResult) -> Value {
    let comparisons: Vec<Value> = actual
        .comparisons
        .iter()
        .map(|comparison| {
            json!({
                "common": comparison.common,
                "added": comparison.added,
                "removed": comparison.removed,
                "ambiguous": comparison.ambiguous,
                "inconclusive": comparison.inconclusive,
                "improvements": comparison.improvements,
                "regressions": comparison.regressions,
                "carried_survivors": comparison.carried_survivors,
                "state": state_name(comparison.state),
                "previous_score": comparison.previous_score,
                "current_score": comparison.current_score,
                "score_delta": comparison.score_delta
            })
        })
        .collect();
    json!({
        "patience": actual.patience.get(),
        "consecutive_stalls": actual.consecutive_stalls,
        "latest": {
            "state": state_name(actual.latest),
            "consecutive_stalls": actual.consecutive_stalls,
            "patience": actual.patience.get(),
            "saturated": actual.latest == ProgressState::Saturated
        },
        "comparisons": comparisons
    })
}

#[test]
fn internal_comparison_fixtures_preserve_inconclusive_semantics() {
    for item in parse_corpus()
        .unwrap()
        .iter()
        .filter(|item| item.mode == "internal-fixture")
    {
        let inputs = inject_usable_reports_without_reader_validation(item);
        let patience = NonZeroUsize::new(usize::try_from(item.patience).unwrap()).unwrap();
        let actual = compare_reports(&inputs, patience);
        assert_case_matches(item, &comparison_result_as_cli_json(&actual));
    }
}

#[tokio::test]
async fn public_progress_details_match_generated_latest_pair_expectations() {
    for item in parse_corpus()
        .unwrap()
        .iter()
        .filter(|item| item.mode == "strict")
    {
        let actual = public_progress(item, true, false)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_case_matches(item, &actual);
        assert_eq!(actual["schema_version"], 2);
        let detail = &actual["details"];
        let expected = &item.expected.details;
        assert_eq!(detail["available"], expected.available, "{}", item.id);
        assert_eq!(
            detail["eligibility"],
            json!(expected.eligibility),
            "{}",
            item.id
        );
        for (key, value) in [
            ("previous_input", expected.previous_input),
            ("current_input", expected.current_input),
            ("omitted", expected.omitted),
            ("unidentified", expected.unidentified),
        ] {
            assert_eq!(detail[key], value, "{} {key}", item.id);
        }
        let entries = detail["transitions"].as_array().unwrap();
        assert_eq!(entries.len(), expected.transitions.len(), "{}", item.id);
        for (entry, transition) in entries.iter().zip(&expected.transitions) {
            assert_eq!(
                entry["id"],
                format!("candidate-{}", transition.candidate_id),
                "{}",
                item.id
            );
            assert_eq!(
                entry["previous_status"], transition.previous_status,
                "{}",
                item.id
            );
            assert_eq!(
                entry["current_status"], transition.current_status,
                "{}",
                item.id
            );
            assert_eq!(
                entry["classification"], transition.classification,
                "{}",
                item.id
            );
        }
    }
}

#[tokio::test]
async fn public_regression_exit_matches_lean_without_changing_output() {
    for item in parse_corpus()
        .unwrap()
        .iter()
        .filter(|item| item.mode == "strict")
    {
        for details in [false, true] {
            let default = public_progress(item, details, false).await.unwrap();
            let gated = public_progress(item, details, true).await.unwrap();
            // Each invocation has temporary input paths; compare semantic output.
            assert_case_matches(item, &gated);
            assert_eq!(default["latest"], gated["latest"], "{}", item.id);
            assert_eq!(default["details"], gated["details"], "{}", item.id);
            let failed = public_output(item, details, true, true).await.unwrap();
            assert_eq!(
                failed.status.code(),
                Some(item.expected.error_exit_code),
                "{}",
                item.id
            );
            assert!(failed.stdout.is_empty(), "{}", item.id);
            assert!(!failed.stderr.is_empty(), "{}", item.id);
        }
    }
}
