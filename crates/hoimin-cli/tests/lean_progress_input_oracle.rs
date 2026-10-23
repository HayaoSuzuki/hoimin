#[path = "support/candidate_identity.rs"]
mod candidate_identity;
#[path = "support/mutation_outcome.rs"]
mod mutation_outcome;
use std::{collections::BTreeSet, process::Stdio, time::Duration};

use serde::Deserialize;
use serde_json::{Value, json};

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/progress-input.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeanOracleCase {
    schema: u32,
    id: String,
    mode: String,
    statuses: Vec<String>,
    baseline: String,
    #[serde(default)]
    result_fields: Option<ResultFields>,
    complete: bool,
    exit_code: i32,
    counts: Value,
    score: Option<Score>,
    expected_disposition: String,
    expected_history: Option<History>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultFields {
    termination: Value,
    output_state: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Score {
    numerator: u32,
    denominator: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct History {
    latest_state: String,
    consecutive_stalls: u64,
    comparisons: usize,
}

#[derive(Debug)]
struct CliObservation {
    disposition: String,
    history: Option<History>,
}

fn parse_lean_oracle_cases() -> Vec<LeanOracleCase> {
    let cases: Vec<LeanOracleCase> = CORPUS
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut ids = BTreeSet::new();
    assert_eq!(cases.len(), 282, "closed boundary matrix changed");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.result_fields.is_none())
            .count(),
        184
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.result_fields.is_some())
            .count(),
        98
    );
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert!(ids.insert(&case.id), "duplicate id {}", case.id);
        assert!(matches!(
            case.expected_disposition.as_str(),
            "invalid" | "usable" | "incomplete" | "baseline_failed" | "missing_baseline"
        ));
        assert_eq!(
            case.expected_disposition == "invalid",
            case.expected_history.is_none()
        );
    }
    cases
}

fn report_json_from_lean_case(case: &LeanOracleCase) -> Value {
    let mut doc: Value =
        serde_json::from_str(include_str!("golden/reports/schema-v10-current.json")).unwrap();
    doc["run"]["normalized_config"] = Value::Null;
    match case.baseline.as_str() {
        "passed" => doc["baseline"]["termination"] = json!({"Exit": 0}),
        "failed" => doc["baseline"]["termination"] = json!({"Exit": 1}),
        "missing" => doc["baseline"] = Value::Null,
        other => panic!("unknown baseline {other}"),
    }
    let template = doc["mutants"][0].clone();
    doc["mutants"] = Value::Array(
        case.statuses
            .iter()
            .enumerate()
            .map(|(index, status)| {
                let mut event = template.clone();
                event["sequence"] = json!(index + 3);
                event["candidate"]["sequence"] = json!(index);
                event["candidate"]["span"]["start"] = json!(index);
                candidate_identity::assign_id(&mut event["candidate"]);
                let status: hoimin_core::MutationStatus =
                    serde_json::from_value(json!(status)).unwrap();
                event["outcome"] =
                    serde_json::to_value(mutation_outcome::executed(status)).unwrap();
                event["status"] = serde_json::to_value(status).unwrap();
                if let Some(fields) = &case.result_fields {
                    event["outcome"]["executed"]["termination"] = fields.termination.clone();
                    event["outcome"]["executed"]["output_state"] = json!(fields.output_state);
                }
                event
            })
            .collect(),
    );
    let summary = &mut doc["summary"];
    summary["sequence"] = json!(case.statuses.len() + 3);
    summary["counts"] = case.counts.clone();
    summary["counts"]["score"] = case.score.as_ref().map_or(Value::Null, |score| {
        assert!(score.denominator > 0);
        json!(f64::from(score.numerator) / f64::from(score.denominator))
    });
    summary["complete"] = json!(case.complete);
    summary["exit_code"] = json!(case.exit_code);
    summary["disk"] = serde_json::to_value(hoimin_core::DiskRunSummary::unmeasured(8, 10)).unwrap();
    doc
}

async fn observe_cli_self_comparison(case: &LeanOracleCase) -> Result<CliObservation, String> {
    observe_cli_encoding(case, false).await
}

async fn observe_cli_encoding(
    case: &LeanOracleCase,
    jsonl: bool,
) -> Result<CliObservation, String> {
    let fixture = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = fixture.path().join(format!("{}.report", case.id));
    std::fs::write(
        &path,
        encode_report(&report_json_from_lean_case(case), jsonl),
    )
    .map_err(|error| error.to_string())?;
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["progress", "--patience", "1"])
        .arg(&path)
        .arg(&path)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| "CLI timeout".to_owned())?
        .map_err(|error| error.to_string())?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    match output.status.code() {
        Some(2)
            if stderr.contains("invalid structure in report")
                || stderr.contains("could not parse report") =>
        {
            if !output.stdout.is_empty() || !stderr.contains(path.to_str().unwrap()) {
                return Err(format!(
                    "invalid-input diagnostic/output contract failed: {stderr}"
                ));
            }
            Ok(CliObservation {
                disposition: "invalid".to_owned(),
                history: None,
            })
        }
        Some(0) => {
            let result: Value = serde_json::from_slice(&output.stdout)
                .map_err(|error| format!("CLI JSON: {error}"))?;
            let inputs = result["inputs"].as_array().ok_or("missing inputs")?;
            if inputs.len() != 2
                || inputs[0]["usable"] != inputs[1]["usable"]
                || inputs[0]["reason"] != inputs[1]["reason"]
            {
                return Err("inconsistent CLI input observations".to_owned());
            }
            let disposition = if inputs[0]["usable"].as_bool().ok_or("missing usable")? {
                "usable"
            } else {
                inputs[0]["reason"]
                    .as_str()
                    .ok_or("missing unusable reason")?
            };
            Ok(CliObservation {
                disposition: disposition.to_owned(),
                history: Some(History {
                    latest_state: result["latest"]["state"]
                        .as_str()
                        .ok_or("missing state")?
                        .to_owned(),
                    consecutive_stalls: result["consecutive_stalls"]
                        .as_u64()
                        .ok_or("missing stalls")?,
                    comparisons: result["comparisons"]
                        .as_array()
                        .ok_or("missing comparisons")?
                        .len(),
                }),
            })
        }
        code => Err(format!("CLI failed with {code:?}: {stderr}")),
    }
}

#[tokio::test]
async fn current_summaries_follow_lean_before_entering_stall_history() {
    let mut mismatches = Vec::new();
    for case in parse_lean_oracle_cases() {
        let schema = hoimin_core::REPORT_SCHEMA_VERSION;
        let observed = observe_cli_self_comparison(&case)
            .await
            .unwrap_or_else(|error| panic!("{} v{schema} infrastructure-error: {error}", case.id));
        mismatches.extend(
            (observed.disposition != case.expected_disposition
                || observed.history != case.expected_history)
                .then(|| {
                    format!(
                        "{} v{schema}: expected {:?}/{:?}, observed {observed:?}",
                        case.id, case.expected_disposition, case.expected_history
                    )
                }),
        );
    }
    assert!(
        mismatches.is_empty(),
        "{} strict mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[tokio::test]
async fn killed_exit_zero_minimal_regression_follows_lean() {
    let case = parse_lean_oracle_cases()
        .into_iter()
        .find(|case| case.id == "result_killed_exit_zero_complete")
        .expect("retain the minimal result-validation witness");
    let schema = hoimin_core::REPORT_SCHEMA_VERSION;
    let observed = observe_cli_self_comparison(&case)
        .await
        .unwrap_or_else(|error| panic!("v{schema} infrastructure-error: {error}"));
    assert_eq!(observed.disposition, case.expected_disposition, "v{schema}");
    assert_eq!(observed.history, case.expected_history, "v{schema}");
}

fn encode_report(document: &Value, jsonl: bool) -> Vec<u8> {
    if !jsonl {
        return serde_json::to_vec(document).unwrap();
    }
    let mut events = vec![document["run"].clone()];
    if !document["baseline"].is_null() {
        events.push(document["baseline"].clone());
    }
    for mutant in document["mutants"].as_array().unwrap() {
        events.push(json!({
            "kind": "mutant_started", "schema_version": document["schema_version"],
            "sequence": 0, "run_id": document["run"]["run_id"],
            "mutant_id": mutant["candidate"]["id"],
            "mutant_sequence": mutant["candidate"]["sequence"]
        }));
        events.push(mutant.clone());
    }
    events.push(document["summary"].clone());
    let mut bytes = Vec::new();
    for (index, event) in events.iter_mut().enumerate() {
        event["sequence"] = json!(index + 1);
        serde_json::to_writer(&mut bytes, event).unwrap();
        bytes.push(b'\n');
    }
    bytes
}

fn classify_observation(
    case: &LeanOracleCase,
    observed: &Result<CliObservation, String>,
) -> (&'static str, Option<String>) {
    match observed {
        Ok(value)
            if value.disposition == case.expected_disposition
                && value.history == case.expected_history =>
        {
            ("match", None)
        }
        Ok(value) => (
            "mismatch",
            Some(format!(
                "expected {:?}/{:?}, observed {value:?}",
                case.expected_disposition, case.expected_history
            )),
        ),
        Err(error) => ("infrastructure-error", Some(error.clone())),
    }
}

#[tokio::test]
async fn shared_reader_progress_formats_follow_lean() {
    let mut failures = Vec::new();
    let selected = std::env::var("HOIMIN_BOUNDARY_CASE").ok();
    let mut selected_cases = 0;
    for case in parse_lean_oracle_cases()
        .into_iter()
        .filter(|case| selected.as_ref().is_none_or(|id| *id == case.id))
    {
        for gap in std::iter::once(&case)
            .filter(|case| case.baseline == "missing" && !case.statuses.is_empty())
        {
            println!(
                "BOUNDARY_OBSERVATION {}",
                json!({
                    "fixture": gap.id, "encoding": "jsonl-v6", "boundary": "reader-progress",
                    "mode": "report", "evidence": "real-cli", "status": "unexecuted",
                    "reason": "document permits missing baseline with mutant projections; full JSONL lifecycle requires preceding baseline"
                })
            );
        }
        selected_cases += 1;
        for (encoding, jsonl) in [("json-v6", false), ("jsonl-v6", true)]
            .into_iter()
            .filter(|(_, jsonl)| !jsonl || case.baseline != "missing" || case.statuses.is_empty())
        {
            let observed = observe_cli_encoding(&case, jsonl).await;
            let (status, reason) = classify_observation(&case, &observed);
            println!(
                "BOUNDARY_OBSERVATION {}",
                json!({
                    "fixture": case.id, "encoding": encoding, "boundary": "reader-progress",
                    "mode": "strict", "evidence": "real-cli", "status": status, "reason": reason,
                    "expected_disposition": case.expected_disposition
                })
            );
            failures
                .extend((status != "match").then(|| format!("{} {encoding}: {reason:?}", case.id)));
        }
    }
    assert!(selected_cases > 0, "unknown HOIMIN_BOUNDARY_CASE");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn shared_reader_fixture_encodings_preserve_fields_and_projection_premises() {
    let cases = parse_lean_oracle_cases();
    let mut applicable = 0;
    let mut projection_gaps = 0;
    for case in cases {
        let document = report_json_from_lean_case(&case);
        let encoded = encode_report(&document, true);
        let events: Vec<Value> = encoded
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        let mutants: Vec<_> = events
            .iter()
            .filter(|event| event["kind"] == "mutant_finished")
            .collect();
        assert_eq!(mutants.len(), case.statuses.len());
        assert_eq!(
            events.last().unwrap()["counts"],
            document["summary"]["counts"]
        );
        for (event, original) in mutants.iter().zip(document["mutants"].as_array().unwrap()) {
            for field in ["candidate", "outcome", "status", "diagnostics"] {
                assert_eq!(event[field], original[field], "{} {field}", case.id);
            }
        }
        let has_projection_gap = case.baseline == "missing" && !case.statuses.is_empty();
        projection_gaps += usize::from(has_projection_gap);
        applicable += usize::from(!has_projection_gap);
    }
    assert_eq!(applicable + projection_gaps, 282);
    assert_eq!(applicable, 282);
    assert_eq!(
        projection_gaps, 0,
        "current Lean missing-baseline cases have no mutants"
    );
}
