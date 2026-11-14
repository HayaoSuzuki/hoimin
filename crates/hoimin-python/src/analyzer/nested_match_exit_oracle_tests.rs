use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    BindingFlowTestMutation, binding_flow_loop_head_snapshot_at_marker,
    binding_flow_try_exit_snapshot, binding_flow_try_exit_snapshot_with_mutation,
};

const CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/nested-match-exits.jsonl");

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ObservationKind {
    TryExit,
    LoopHead,
    PublicCandidate,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    observation_kind: ObservationKind,
    family: String,
    source: String,
    marker: String,
    expected_exits: ExpectedExits,
    expected_facts: Vec<String>,
    candidate: CandidateExpectation,
    candidate_count: usize,
    candidate_path: Option<String>,
    candidate_operator: Option<String>,
    candidate_original: Option<String>,
    candidate_replacement: Option<String>,
    candidate_symbol: Option<String>,
}

fn expected_source(id: &str) -> Option<&'static str> {
    match id {
        "handler_match_break_continue_categories" => Some(
            "from typing import Mapping, Sequence\n\nwhile condition:\n    try:  # handler_match_break_continue\n        risky()\n    except Error as Sequence:\n        match value:\n            case 0:\n                from typing import Sequence\n                break\n            case _:\n                Mapping = object\n                continue\n    finally:\n        from typing import Sequence\n",
        ),
        "handler_match_fallthrough_terminate_categories" => Some(
            "def f():\n    from typing import Mapping, Sequence\n    try:  # handler_match_fallthrough_terminate\n        risky()\n    except Error as Sequence:\n        match value:\n            case 0:\n                from typing import Sequence\n                return None\n            case 1:\n                from typing import Sequence\n                raise RuntimeError\n            case _:\n                from typing import Sequence\n    finally:\n        from typing import Sequence\n",
        ),
        "irrefutable_terminate_excludes_later_break" => Some(
            "def f():\n    from typing import Mapping, Sequence\n    while condition:\n        try:  # irrefutable_terminate\n            risky()\n        except Error as Sequence:\n            match value:\n                case _:\n                    from typing import Sequence\n                    return None\n                case 1:\n                    break\n        finally:\n            from typing import Sequence\n",
        ),
        "nested_continue_reaches_loop_head" | "nested_continue_suppresses_public_candidate" => {
            Some(
                "from typing import Sequence\n\nwhile condition:  # nested_continue_loop\n    match value:\n        case 0:\n            observed: Sequence[int]\n            Sequence = object\n            continue\n        case _:\n            break\n",
            )
        }
        "post_loop_meets_break_and_natural_exit" => Some(
            "from typing import Sequence\n\nwhile condition:\n    match value:\n        case 0:\n            Sequence = object\n            break\n        case _:\n            continue\nelse:\n    from typing import Sequence\n\npost_loop: Sequence[int]\n",
        ),
        _ => None,
    }
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
    if item.marker.is_empty() || item.source.matches(&item.marker).count() != 1 {
        return Err(format!("{} does not have one unique marker", item.id));
    }
    if expected_source(&item.id) != Some(item.source.as_str()) {
        return Err(format!("{} has changed source premises", item.id));
    }
    let public_fields_match = match item.candidate {
        CandidateExpectation::Present => {
            item.candidate_count == 1
                && item.candidate_path.is_some()
                && item.candidate_operator.is_some()
                && item.candidate_original.is_some()
                && item.candidate_replacement.is_some()
        }
        CandidateExpectation::Absent | CandidateExpectation::NotObserved => {
            item.candidate_count == 0
                && item.candidate_path.is_none()
                && item.candidate_operator.is_none()
                && item.candidate_original.is_none()
                && item.candidate_replacement.is_none()
                && item.candidate_symbol.is_none()
        }
    };
    if !public_fields_match {
        return Err(format!("{} has inconsistent public observation", item.id));
    }
    let identity_matches = match item.id.as_str() {
        "handler_match_break_continue_categories" => internal_identity(
            item,
            ObservationKind::TryExit,
            "nested-handler-categories",
            "# handler_match_break_continue",
        ),
        "handler_match_fallthrough_terminate_categories" => internal_identity(
            item,
            ObservationKind::TryExit,
            "nested-handler-categories",
            "# handler_match_fallthrough_terminate",
        ),
        "irrefutable_terminate_excludes_later_break" => internal_identity(
            item,
            ObservationKind::TryExit,
            "nested-reachability",
            "# irrefutable_terminate",
        ),
        "nested_continue_reaches_loop_head" => internal_identity(
            item,
            ObservationKind::LoopHead,
            "loop-back-edge",
            "# nested_continue_loop",
        ),
        "nested_continue_suppresses_public_candidate" => strict_identity(item, "loop-back-edge"),
        "post_loop_meets_break_and_natural_exit" => strict_identity(item, "loop-consumption"),
        _ => {
            // expected_source above accepts only the IDs enumerated in this match.
            // Retain a fail-closed fallback for future corpus edits.
            #[cfg(coverage_nightly)]
            {
                (#[coverage(off)]
                || false)()
            }
            #[cfg(not(coverage_nightly))]
            {
                false
            }
        }
    };
    if !identity_matches {
        return Err(format!("{} has inconsistent closed identity", item.id));
    }
    Ok(())
}

fn internal_identity(item: &OracleCase, kind: ObservationKind, family: &str, marker: &str) -> bool {
    item.mode == "internal-fixture"
        && item.observation_kind == kind
        && item.family == family
        && item.marker == marker
        && item.candidate == CandidateExpectation::NotObserved
}

fn strict_identity(item: &OracleCase, family: &str) -> bool {
    item.mode == "strict"
        && item.observation_kind == ObservationKind::PublicCandidate
        && item.family == family
        && item.marker == "Sequence[int]"
        && item.candidate == CandidateExpectation::Absent
}

fn selected_cases(cases: &[OracleCase]) -> Result<Vec<&OracleCase>, String> {
    apply_case_filter(cases, std::env::var_os("HOIMIN_NESTED_MATCH_EXIT_CASE"))
}

fn apply_case_filter(
    cases: &[OracleCase],
    filter: Option<std::ffi::OsString>,
) -> Result<Vec<&OracleCase>, String> {
    let Some(selected) = filter else {
        return Ok(cases.iter().collect());
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_NESTED_MATCH_EXIT_CASE is not UTF-8".to_owned())?;
    let matches = cases
        .iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "HOIMIN_NESTED_MATCH_EXIT_CASE={selected} selected {} rows",
            matches.len()
        ));
    }
    Ok(matches)
}

#[test]
fn nested_match_exit_corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");
    assert_eq!(cases.len(), 6);
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 2);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        4
    );
}

#[rstest::rstest]
#[case::schema_1(0)]
#[case::mode_2(1)]
#[case::observation_kind_3(2)]
fn nested_match_exit_corpus_rejects_inexact_and_crossed_rows_scenario_1(
    #[case] scenario_index: usize,
) {
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
        "duplicate case id handler_match_break_continue_categories"
    );

    let (field, value, expected) = [
        ("schema", serde_json::json!(2), "unsupported schema"),
        ("mode", serde_json::json!("report"), "unknown mode"),
        (
            "observation_kind",
            serde_json::json!("handler_exit"),
            "unknown variant",
        ),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let mut invalid: serde_json::Value = serde_json::from_str(first).unwrap();
    invalid[field] = value;
    assert!(
        parse_corpus(&format!("{invalid}\n"))
            .unwrap_err()
            .contains(expected)
    );
}

#[test]
fn nested_match_exit_corpus_rejects_inexact_and_crossed_rows() {
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
        "duplicate case id handler_match_break_continue_categories"
    );

    let mut crossed: serde_json::Value = serde_json::from_str(first).unwrap();
    crossed["family"] = serde_json::json!("loop-consumption");
    assert!(
        parse_corpus(&format!("{crossed}\n"))
            .unwrap_err()
            .contains("inconsistent closed identity")
    );
    let mut changed_source: serde_json::Value = serde_json::from_str(first).unwrap();
    changed_source["source"] = serde_json::json!(
        changed_source["source"]
            .as_str()
            .unwrap()
            .replace("risky()", "other_risky()")
    );
    assert!(
        parse_corpus(&format!("{changed_source}\n"))
            .unwrap_err()
            .contains("changed source premises")
    );
}

#[test]
fn internal_rows_match_complete_production_observations() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    let selected = selected_cases(&cases).expect("valid single-case filter");
    for item in selected
        .iter()
        .filter(|item| item.mode == "internal-fixture")
    {
        assert!(!matches!(
            item.observation_kind,
            ObservationKind::PublicCandidate
        ));
    }
    for item in selected.iter().filter(|item| {
        item.mode == "internal-fixture" && matches!(item.observation_kind, ObservationKind::TryExit)
    }) {
        let actual = binding_flow_try_exit_snapshot(&item.source, &item.marker).expect(&item.id);
        assert_eq!(
            actual.fallthrough, item.expected_exits.fallthrough,
            "{}",
            item.id
        );
        assert_eq!(actual.breaks, item.expected_exits.breaks, "{}", item.id);
        assert_eq!(
            actual.continues, item.expected_exits.continues,
            "{}",
            item.id
        );
        assert_eq!(
            actual.terminates, item.expected_exits.terminates,
            "{}",
            item.id
        );
    }
    for item in selected.iter().filter(|item| {
        item.mode == "internal-fixture"
            && matches!(item.observation_kind, ObservationKind::LoopHead)
    }) {
        let actual = binding_flow_loop_head_snapshot_at_marker(&item.source, &item.marker, None)
            .expect(&item.id);
        assert_eq!(actual, item.expected_facts, "{}", item.id);
    }
}

#[test]
fn fixtures_detect_flattened_match_categories_and_omitted_continue_edges() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    let try_item = cases
        .iter()
        .find(|item| item.id == "handler_match_break_continue_categories")
        .unwrap();
    let flattened = binding_flow_try_exit_snapshot_with_mutation(
        &try_item.source,
        &try_item.marker,
        BindingFlowTestMutation::FlattenMatchAbruptToFallthrough,
    )
    .expect("mutation premise must remain observable");
    assert_ne!(flattened.breaks, try_item.expected_exits.breaks);
    assert_ne!(flattened.continues, try_item.expected_exits.continues);

    let loop_item = cases
        .iter()
        .find(|item| item.id == "nested_continue_reaches_loop_head")
        .unwrap();
    let omitted = binding_flow_loop_head_snapshot_at_marker(
        &loop_item.source,
        &loop_item.marker,
        Some(BindingFlowTestMutation::OmitLoopContinueBackEdge),
    )
    .expect("mutation premise must remain observable");
    assert_ne!(omitted, loop_item.expected_facts);
}

#[rstest::rstest]
#[case("schema", serde_json::json!(0), "unsupported schema")]
#[case("mode", serde_json::json!("unknown"), "unknown mode")]
#[case("marker", serde_json::json!(""), "one unique marker")]
#[case("candidate_count", serde_json::json!(999), "inconsistent public observation")]
#[case("id", serde_json::json!("unknown"), "changed source premises")]
#[case("family", serde_json::json!("unknown"), "inconsistent closed identity")]
fn oracle_validation_rejects_inconsistent_case_fields(
    #[case] field: &str,
    #[case] replacement: serde_json::Value,
    #[case] expected: &str,
) {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    row[field] = replacement;
    let item: OracleCase = serde_json::from_value(row).unwrap();

    let actual = validate_case(&item).unwrap_err().contains(expected);

    assert!(actual);
}

#[rstest::rstest]
#[case("{", "invalid JSON")]
#[case("", "no cases")]
fn oracle_validation_rejects_invalid_corpus_structure(#[case] input: &str, #[case] expected: &str) {
    let actual = parse_corpus(input).unwrap_err().contains(expected);

    assert!(actual);
}

#[test]
fn oracle_validation_rejects_duplicate_case_id() {
    let duplicated = format!("{CORPUS}\n{CORPUS}");

    let actual = parse_corpus(&duplicated)
        .unwrap_err()
        .contains("duplicate case id");

    assert!(actual);
}

#[test]
fn oracle_validation_accepts_blank_lines_between_valid_rows() {
    let with_blanks = format!("\n{CORPUS}\n\n");

    let actual = parse_corpus(&with_blanks).unwrap().len();

    assert_eq!(actual, parse_corpus(CORPUS).unwrap().len());
}

#[test]
fn oracle_validation_rejects_field_with_wrong_json_type() {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    row["schema"] = serde_json::json!(false);

    let actual = parse_corpus(&row.to_string())
        .unwrap_err()
        .contains("invalid JSON");

    assert!(actual);
}

#[test]
fn oracle_case_filter_selects_the_requested_fixture() {
    let cases = parse_corpus(CORPUS).unwrap();
    let id = cases[0].id.clone();
    let selected = apply_case_filter(&cases, Some(id.clone().into())).unwrap();

    let actual = selected.len();

    assert_eq!(actual, 1);
    assert_eq!(selected[0].id, id);
}

#[test]
fn oracle_case_filter_rejects_an_unknown_fixture() {
    let cases = parse_corpus(CORPUS).unwrap();
    let error = apply_case_filter(&cases, Some("unknown-fixture".into())).unwrap_err();

    let actual = error.contains("selected 0");

    assert!(actual);
}

#[cfg(unix)]
#[test]
fn oracle_case_filter_rejects_non_utf8_fixture_names() {
    use std::os::unix::ffi::OsStringExt as _;
    let cases = parse_corpus(CORPUS).unwrap();
    let error =
        apply_case_filter(&cases, Some(std::ffi::OsString::from_vec(vec![0xff]))).unwrap_err();

    let actual = error.contains("not UTF-8");

    assert!(actual);
}

#[test]
fn oracle_validation_rejects_a_well_formed_candidate_for_an_absent_candidate_identity() {
    let mut item: OracleCase = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    item.candidate = CandidateExpectation::Present;
    item.candidate_count = 1;
    item.candidate_path = Some("target.py".into());
    item.candidate_operator = Some("type_list_sequence".into());
    item.candidate_original = Some("Sequence[int]".into());
    item.candidate_replacement = Some("list[int]".into());

    let actual = validate_case(&item)
        .unwrap_err()
        .contains("inconsistent closed identity");

    assert!(actual);
}
