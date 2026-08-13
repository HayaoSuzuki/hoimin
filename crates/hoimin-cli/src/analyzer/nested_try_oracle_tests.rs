use std::collections::BTreeSet;

use serde::Deserialize;

use super::binding_flow_try_exit_snapshot;

const CORPUS: &str = include_str!("../../../../formal/HoiminOracle/corpus/nested-try-flow.jsonl");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedExits {
    fallthrough: Vec<Vec<String>>,
    breaks: Vec<Vec<String>>,
    continues: Vec<Vec<String>>,
    terminates: Vec<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CandidateExpectation {
    NotObserved,
    Present,
    Absent,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    family: String,
    source: String,
    marker: String,
    entry_category: String,
    expected: ExpectedExits,
    candidate: CandidateExpectation,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
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
    if item.schema != 1 {
        return Err(format!("{} has unsupported schema", item.id));
    }
    if !matches!(
        item.mode.as_str(),
        "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
    ) {
        return Err(format!("{} has unknown mode", item.id));
    }
    if !matches!(
        item.entry_category.as_str(),
        "fallthrough" | "break" | "continue" | "terminate"
    ) {
        return Err(format!("{} has unknown entry category", item.id));
    }
    if item.marker.is_empty() || item.source.matches(&item.marker).count() != 1 {
        return Err(format!("{} does not have one unique marker", item.id));
    }
    let identity_matches = match item.id.as_str() {
        "finally_annotation_meets_normal_and_raise" => {
            item.mode == "strict"
                && item.family == "finally-annotation"
                && item.marker == "value: Sequence[int]"
                && item.candidate == CandidateExpectation::Absent
        }
        "post_finally_uses_only_fallthrough" => {
            item.mode == "strict"
                && item.family == "post-finally"
                && item.marker == "value: Sequence[int]"
                && item.candidate == CandidateExpectation::Present
        }
        "falling_finally_preserves_break" => {
            internal_identity(item, "category-routing", "# break_exit", "break")
        }
        "falling_finally_preserves_continue" => {
            internal_identity(item, "category-routing", "# continue_exit", "continue")
        }
        "falling_finally_preserves_return_terminate" => {
            internal_identity(item, "category-routing", "# return_exit", "terminate")
        }
        "falling_finally_preserves_raise_terminate" => {
            internal_identity(item, "category-routing", "# raise_exit", "terminate")
        }
        "abrupt_finally_replaces_fallthrough" => internal_identity(
            item,
            "abrupt-finally",
            "# abrupt_fallthrough",
            "fallthrough",
        ),
        "abrupt_finally_replaces_break" => {
            internal_identity(item, "abrupt-finally", "# abrupt_break", "break")
        }
        "unreachable_post_return_excluded" => {
            internal_identity(item, "reachability", "# unreachable_return", "terminate")
        }
        "nonselected_handler_meet" => {
            item.mode == "model-only"
                && item.family == "handler-meet"
                && item.marker == "# handler_meet"
                && item.candidate == CandidateExpectation::NotObserved
        }
        _ => false,
    };
    if !identity_matches {
        return Err(format!("{} has inconsistent closed identity", item.id));
    }
    Ok(())
}

fn internal_identity(item: &OracleCase, family: &str, marker: &str, category: &str) -> bool {
    item.mode == "internal-fixture"
        && item.family == family
        && item.marker == marker
        && item.entry_category == category
        && item.candidate == CandidateExpectation::NotObserved
}

fn selected_cases<'a>(cases: &'a [OracleCase]) -> Result<Vec<&'a OracleCase>, String> {
    let Some(selected) = std::env::var_os("HOIMIN_NESTED_TRY_CASE") else {
        return Ok(cases.iter().collect());
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_NESTED_TRY_CASE is not UTF-8".to_owned())?;
    let matches = cases
        .iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "HOIMIN_NESTED_TRY_CASE={selected} selected {} rows",
            matches.len()
        ));
    }
    Ok(matches)
}

#[test]
fn nested_try_internal_rows_match_post_finally_production_exits() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");
    let selected =
        selected_cases(&cases).expect("single-case filter must select a closed identity");
    if std::env::var_os("HOIMIN_NESTED_TRY_CASE").is_some()
        && selected[0].mode != "internal-fixture"
    {
        panic!(
            "single-case internal adapter cannot execute mode {}",
            selected[0].mode
        );
    }
    for item in selected
        .into_iter()
        .filter(|item| item.mode == "internal-fixture")
    {
        let actual = binding_flow_try_exit_snapshot(&item.source, &item.marker)
            .unwrap_or_else(|error| panic!("{}: {error}", item.id));
        assert_eq!(
            actual.fallthrough, item.expected.fallthrough,
            "case={}",
            item.id
        );
        assert_eq!(actual.breaks, item.expected.breaks, "case={}", item.id);
        assert_eq!(
            actual.continues, item.expected.continues,
            "case={}",
            item.id
        );
        assert_eq!(
            actual.terminates, item.expected.terminates,
            "case={}",
            item.id
        );
    }
}

#[test]
fn nested_try_corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");
    assert_eq!(cases.len(), 10);
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 2);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        7
    );
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "model-only")
            .count(),
        1
    );
}

#[test]
fn nested_try_corpus_rejects_inexact_and_crossed_rows() {
    let first = CORPUS.lines().next().expect("first corpus row");

    let mut unknown: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(
        parse_corpus(&format!("{unknown}\n"))
            .unwrap_err()
            .contains("unknown field")
    );

    assert_eq!(
        parse_corpus(&format!("{first}\n{first}\n")).unwrap_err(),
        "duplicate case id finally_annotation_meets_normal_and_raise"
    );

    for (field, value, expected) in [
        ("schema", serde_json::json!(2), "unsupported schema"),
        ("mode", serde_json::json!("report"), "unknown mode"),
        (
            "entry_category",
            serde_json::json!("return"),
            "unknown entry category",
        ),
    ] {
        let mut invalid: serde_json::Value = serde_json::from_str(first).unwrap();
        invalid[field] = value;
        assert!(
            parse_corpus(&format!("{invalid}\n"))
                .unwrap_err()
                .contains(expected)
        );
    }

    let mut crossed: serde_json::Value = serde_json::from_str(first).unwrap();
    crossed["family"] = serde_json::json!("post-finally");
    assert!(
        parse_corpus(&format!("{crossed}\n"))
            .unwrap_err()
            .contains("inconsistent closed identity")
    );

    let mut duplicate_marker: serde_json::Value = serde_json::from_str(first).unwrap();
    let source = duplicate_marker["source"].as_str().unwrap();
    let marker = duplicate_marker["marker"].as_str().unwrap();
    duplicate_marker["source"] = serde_json::json!(format!("{source}\n{marker}\n"));
    assert!(
        parse_corpus(&format!("{duplicate_marker}\n"))
            .unwrap_err()
            .contains("one unique marker")
    );
}
