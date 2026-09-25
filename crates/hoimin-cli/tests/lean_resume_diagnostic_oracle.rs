use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{BeginSession, EffectId, FinishSession, LoadSession, RunFingerprint};
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
    Ok(Observation {
        selected_run: loaded.resume.map(|resume| resume.run_id),
        reason: loaded.fresh_reason.map(|reason| reason.code().to_owned()),
    })
}

#[test]
fn generated_corpus_is_closed_complete_and_unique() {
    assert_eq!(parse(CORPUS).unwrap().len(), 81);
    let first = CORPUS.lines().next().unwrap();
    for field in ["selected_run", "reason", "rows", "phase"] {
        let mut row: serde_json::Value = serde_json::from_str(first).unwrap();
        row.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Case>(row).is_err(), "{field}");
    }
    for (field, value) in [
        ("schema", serde_json::json!(2)),
        ("mode", serde_json::json!("report")),
        ("phase", serde_json::json!("unknown")),
        ("reason", serde_json::json!("no_compatible_run")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut row: serde_json::Value = serde_json::from_str(first).unwrap();
        row[field] = value;
        let result = serde_json::from_value::<Case>(row)
            .map_err(|error| error.to_string())
            .and_then(|case| validate(&case));
        assert!(result.is_err(), "{field}");
    }
    assert!(parse(&format!("{CORPUS}{first}\n")).is_err());
    let mut duplicate: serde_json::Value = serde_json::from_str(first).unwrap();
    duplicate["id"] = serde_json::json!("another-id");
    assert!(parse(&format!("{CORPUS}{duplicate}\n")).is_err());
    assert!(parse(&CORPUS.lines().skip(1).collect::<Vec<_>>().join("\n")).is_err());
}

#[test]
fn fixture_rows_must_match_declared_categories_and_order() {
    let original: serde_json::Value = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|case| case["mode"] == "strict" && case["rows"].as_array().unwrap().len() == 6)
        .unwrap();
    for field in ["compatible", "complete", "budget", "name"] {
        let mut changed = original.clone();
        changed["rows"][0][field] = match field {
            "compatible" | "complete" => {
                serde_json::json!(!original["rows"][0][field].as_bool().unwrap())
            }
            "budget" => serde_json::json!(99),
            _ => serde_json::json!("unknown"),
        };
        assert!(
            validate(&serde_json::from_value::<Case>(changed).unwrap()).is_err(),
            "{field}"
        );
    }
    for field in [
        "eligible",
        "budget_decreased",
        "matching_complete",
        "other_incomplete",
        "other_complete",
        "reverse",
    ] {
        let mut changed = original.clone();
        changed[field] = serde_json::json!(!original[field].as_bool().unwrap());
        assert!(
            validate(&serde_json::from_value::<Case>(changed).unwrap()).is_err(),
            "{field}"
        );
    }
    for remove in [false, true] {
        let mut changed = original.clone();
        let rows = changed["rows"].as_array_mut().unwrap();
        if remove {
            rows.pop();
        } else {
            rows.swap(0, 1);
        }
        assert!(validate(&serde_json::from_value::<Case>(changed).unwrap()).is_err());
    }
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
