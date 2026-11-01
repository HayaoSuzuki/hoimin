use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{
    BeginSession, EffectId, FinishSession, LoadSession, RunFingerprint, SessionLoadOutcome,
};
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/resume-diagnostic.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    name: String,
    compatible: bool,
    complete: bool,
    budget: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent finite history predicates from the generated corpus"
)]
struct Case {
    schema: u32,
    id: String,
    mode: String,
    phase: String,
    eligible: bool,
    budget_decreased: bool,
    matching_complete: bool,
    other_incomplete: bool,
    other_complete: bool,
    reverse: bool,
    rows: Vec<Row>,
    #[serde(deserialize_with = "required_option")]
    selected_run: Option<String>,
    #[serde(deserialize_with = "required_option")]
    reason: Option<String>,
}

fn required_option<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::deserialize(deserializer)
}

fn validate(case: &Case) -> Result<(), String> {
    if case.schema != 1 || case.id.is_empty() {
        return Err("unknown schema or empty id".into());
    }
    match (case.phase.as_str(), case.mode.as_str()) {
        ("normal", "strict") => {}
        ("none_then_eligible", "model-only") if case.eligible && !case.reverse => {}
        ("lost_eligibility", "model-only")
            if !case.eligible
                && !case.budget_decreased
                && !case.matching_complete
                && !case.other_incomplete
                && !case.other_complete
                && !case.reverse => {}
        _ => return Err("unknown phase/mode or invalid race domain".into()),
    }
    validate_rows(case)?;
    let names = case
        .rows
        .iter()
        .map(|row| row.name.as_str())
        .collect::<BTreeSet<_>>();
    if case
        .selected_run
        .as_deref()
        .is_some_and(|name| !names.contains(name))
        || case.selected_run.is_some() == case.reason.is_some()
    {
        return Err("invalid outcome shape".into());
    }
    if case.reason.as_deref().is_some_and(|reason| {
        !matches!(
            reason,
            "no_prior_run"
                | "matching_run_complete"
                | "budget_decreased"
                | "fingerprint_mismatch"
                | "no_incomplete_run"
                | "candidate_changed"
        )
    }) {
        return Err("unknown reason".into());
    }
    Ok(())
}

// Validate fixture input coverage only; expected selection/reasons remain Lean-owned.
fn validate_rows(case: &Case) -> Result<(), String> {
    let domain = [
        ("eligible_older", true, false, 1, case.eligible),
        ("eligible_newer", true, false, 2, case.eligible),
        ("budget", true, false, 3, case.budget_decreased),
        ("matching_complete", true, true, 2, case.matching_complete),
        ("other_incomplete", false, false, 2, case.other_incomplete),
        ("other_complete", false, true, 2, case.other_complete),
    ];
    let mut expected = domain
        .into_iter()
        .filter(|row| row.4)
        .map(|(name, compatible, complete, budget, _)| (name, compatible, complete, budget))
        .collect::<Vec<_>>();
    if case.reverse {
        expected.reverse();
    }
    let actual = case
        .rows
        .iter()
        .map(|row| (row.name.as_str(), row.compatible, row.complete, row.budget))
        .collect::<Vec<_>>();
    if actual != expected {
        return Err("rows do not match the declared history categories and order".into());
    }
    Ok(())
}

fn parse(input: &str) -> Result<Vec<Case>, String> {
    let mut ids = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    let mut cases = Vec::new();
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).map_err(|error| error.to_string())?;
        validate(&case)?;
        if !ids.insert(case.id.clone())
            || !inputs.insert((
                case.phase.clone(),
                case.eligible,
                case.budget_decreased,
                case.matching_complete,
                case.other_incomplete,
                case.other_complete,
                case.reverse,
            ))
        {
            return Err("duplicate id or input".into());
        }
        cases.push(case);
    }
    if cases.len() != 81
        || cases.iter().filter(|case| case.mode == "strict").count() != 64
        || cases
            .iter()
            .filter(|case| case.phase == "none_then_eligible")
            .count()
            != 16
    {
        return Err("incomplete corpus domain".into());
    }
    Ok(cases)
}

fn fingerprint(compatible: bool) -> RunFingerprint {
    RunFingerprint::from_bytes([if compatible { 1 } else { 2 }; 32])
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    selected_run: Option<String>,
    reason: Option<String>,
}

fn execute(case: &Case) -> Result<Observation, String> {
    let temporary = tempfile::tempdir().map_err(|error| format!("tempdir: {error}"))?;
    let database = temporary.path().join("session.sqlite");
    for row in &case.rows {
        let mut writer =
            SessionHandler::open(&database).map_err(|error| format!("fixture open: {error:?}"))?;
        writer
            .begin(BeginSession {
                id: EffectId(1),
                run_id: row.name.clone(),
                fingerprint: fingerprint(row.compatible),
                max_mutants: NonZeroUsize::new(row.budget).unwrap(),
            })
            .map_err(|error| format!("fixture begin: {error:?}"))?;
        writer
            .finish(FinishSession {
                id: EffectId(2),
                run_id: row.name.clone(),
                complete: row.complete,
            })
            .map_err(|error| format!("fixture finish: {error:?}"))?;
    }
    let mut reader =
        SessionHandler::open(&database).map_err(|error| format!("reader open: {error:?}"))?;
    let loaded = reader
        .load(&LoadSession {
            id: EffectId(3),
            fingerprint: fingerprint(true),
            max_mutants: NonZeroUsize::new(2).unwrap(),
        })
        .map_err(|error| format!("reader load: {error:?}"))?;
    if loaded.id != EffectId(3) {
        return Err("reader returned unexpected effect id".into());
    }
    Ok(match loaded.outcome {
        SessionLoadOutcome::Resumed(resume) => Observation {
            selected_run: Some(resume.run_id),
            reason: None,
        },
        SessionLoadOutcome::Fresh(reason) => Observation {
            selected_run: None,
            reason: Some(reason.code().to_owned()),
        },
    })
}

#[test]
fn generated_corpus_is_complete() {
    let cases = parse(CORPUS).unwrap();

    assert_eq!(cases.len(), 81);
}

#[rstest::rstest]
#[case::selected_run("selected_run")]
#[case::reason("reason")]
#[case::rows("rows")]
#[case::phase("phase")]
fn generated_corpus_rejects_absent_required_fields(#[case] field: &str) {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    row.as_object_mut().unwrap().remove(field);

    let result = serde_json::from_value::<Case>(row);

    assert!(result.is_err(), "{field}");
}

#[rstest::rstest]
#[case::schema("schema",serde_json::json!(2))]
#[case::mode("mode",serde_json::json!("report"))]
#[case::phase("phase",serde_json::json!("unknown"))]
#[case::reason("reason",serde_json::json!("no_compatible_run"))]
#[case::unexpected_field("unexpected",serde_json::json!(true))]
fn generated_corpus_rejects_invalid_metadata(
    #[case] field: &str,
    #[case] value: serde_json::Value,
) {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    row[field] = value;

    let result = serde_json::from_value::<Case>(row)
        .map_err(|error| error.to_string())
        .and_then(|case| validate(&case));

    assert!(result.is_err(), "{field}");
}

#[rstest::rstest]
#[case::duplicate_id(format!("{CORPUS}{}\n",CORPUS.lines().next().unwrap()))]
#[case::duplicate_coordinate(duplicate_coordinate_corpus())]
#[case::missing_case(CORPUS.lines().skip(1).collect::<Vec<_>>().join("\n"))]
fn generated_corpus_rejects_inexact_case_sets(#[case] input: String) {
    let result = parse(&input);

    assert!(result.is_err());
}

fn duplicate_coordinate_corpus() -> String {
    let mut duplicate: serde_json::Value =
        serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    duplicate["id"] = serde_json::json!("another-id");
    format!("{CORPUS}{duplicate}\n")
}

#[rstest::rstest]
#[case::compatibility("compatible", |original: &serde_json::Value| serde_json::json!(!original["rows"][0]["compatible"].as_bool().unwrap()))]
#[case::completion("complete", |original: &serde_json::Value| serde_json::json!(!original["rows"][0]["complete"].as_bool().unwrap()))]
#[case::budget("budget", |_: &serde_json::Value| serde_json::json!(99))]
#[case::name("name", |_: &serde_json::Value| serde_json::json!("unknown"))]
fn fixture_rows_must_match_declared_categories(
    #[case] field: &str,
    #[case] make_value: fn(&serde_json::Value) -> serde_json::Value,
) {
    let original: serde_json::Value = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|case| case["mode"] == "strict" && case["rows"].as_array().unwrap().len() == 6)
        .unwrap();

    let value = make_value(&original);

    let mut changed = original.clone();
    changed["rows"][0][field] = value;
    assert!(
        validate(&serde_json::from_value::<Case>(changed).unwrap()).is_err(),
        "{field}"
    );
}

#[rstest::rstest]
#[case::eligible_1("eligible")]
#[case::budget_decreased_2("budget_decreased")]
#[case::matching_complete_3("matching_complete")]
#[case::other_incomplete_4("other_incomplete")]
#[case::other_complete_5("other_complete")]
#[case::reverse_6("reverse")]
fn fixture_case_flags_must_match_declared_categories_and_order(#[case] field: &str) {
    let original: serde_json::Value = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|case| case["mode"] == "strict" && case["rows"].as_array().unwrap().len() == 6)
        .unwrap();

    let mut changed = original.clone();
    changed[field] = serde_json::json!(!original[field].as_bool().unwrap());
    assert!(
        validate(&serde_json::from_value::<Case>(changed).unwrap()).is_err(),
        "{field}"
    );
}

#[test]
fn real_session_history_matches_lean_diagnostics() {
    let cases = parse(CORPUS).unwrap();
    let selected = std::env::var("HOIMIN_RESUME_DIAGNOSTIC_ORACLE_CASE").ok();
    let strict = cases
        .iter()
        .filter(|case| case.mode == "strict")
        .filter(|case| selected.as_ref().is_none_or(|id| *id == case.id))
        .collect::<Vec<_>>();
    assert!(!strict.is_empty(), "unknown or model-only selected case");
    for case in strict {
        let actual = execute(case)
            .unwrap_or_else(|detail| panic!("{} infrastructure error: {detail}", case.id));
        let expected = Observation {
            selected_run: case.selected_run.clone(),
            reason: case.reason.clone(),
        };
        assert_eq!(actual, expected, "{} semantic mismatch", case.id);
    }
}

#[rstest::rstest]
#[case::missing_row(|rows: &mut Vec<serde_json::Value>| { rows.pop(); })]
#[case::swapped_rows(|rows: &mut Vec<serde_json::Value>| rows.swap(0,1))]
fn fixture_rows_must_preserve_count_and_order(#[case] mutate: fn(&mut Vec<serde_json::Value>)) {
    let original: serde_json::Value = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|case| case["mode"] == "strict" && case["rows"].as_array().unwrap().len() == 6)
        .unwrap();

    let mut changed = original.clone();
    mutate(changed["rows"].as_array_mut().unwrap());
    let case = serde_json::from_value::<Case>(changed).unwrap();

    let result = validate(&case);

    assert!(result.is_err());
}
