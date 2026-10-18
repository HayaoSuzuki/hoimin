use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    BindingFlowTestMutation, BindingFlowTestSnapshot, binding_flow_try_exit_snapshot,
    binding_flow_try_exit_snapshot_with_mutation,
};

const CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/multiple-handler-joins.jsonl");

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
    candidate: CandidateExpectation,
    candidate_count: usize,
    candidate_path: Option<String>,
    candidate_start: Option<u64>,
    candidate_length: Option<u64>,
    candidate_operator: Option<String>,
    candidate_original: Option<String>,
    candidate_replacement: Option<String>,
    candidate_symbol: Option<String>,
}

fn expected_source(id: &str) -> Option<&'static str> {
    match id {
        "two_handlers_disagree_fallthrough" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "try:  # two_handlers_disagree_fallthrough\n",
            "    work()\n",
            "except FirstError:\n",
            "    from typing import Sequence\n",
            "except SecondError:\n",
            "    Sequence = object\n",
            "after_disagreement: Sequence[int]\n",
            "preserved_mapping: Mapping[str, int]\n",
        )),
        "three_handlers_preserve_mapping" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "try:  # three_handlers_preserve_mapping\n",
            "    work()\n",
            "except FirstError:\n",
            "    Sequence = object\n",
            "except SecondError:\n",
            "    from typing import Sequence\n",
            "except ThirdError:\n",
            "    pass\n",
            "after_three: Mapping[str, int]\n",
        )),
        "per_handler_cleanup_categories" => Some(concat!(
            "def run(flag):\n",
            "    from typing import Mapping, Sequence\n",
            "    try:  # per_handler_cleanup_categories\n",
            "        work()\n",
            "    except FirstError as Sequence:\n",
            "        from typing import Sequence\n",
            "    except SecondError as Sequence:\n",
            "        from typing import Sequence\n",
            "        return flag\n",
        )),
        "different_handler_break_continue" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "while active:\n",
            "    from typing import Sequence\n",
            "    try:  # different_handler_break_continue\n",
            "        work()\n",
            "    except FirstError as Sequence:\n",
            "        from typing import Sequence\n",
            "        break\n",
            "    except SecondError as Sequence:\n",
            "        from typing import Sequence\n",
            "        continue\n",
        )),
        "unhandled_remainder_terminates" => Some(concat!(
            "def run():\n",
            "    from typing import Mapping, Sequence\n",
            "    try:  # unhandled_remainder_terminates\n",
            "        raise UnknownError\n",
            "    except FirstError:\n",
            "        pass\n",
            "    except SecondError:\n",
            "        pass\n",
        )),
        "all_handlers_preserve_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "try:\n",
            "    work()\n",
            "except FirstError:\n",
            "    from typing import Sequence\n",
            "except SecondError:\n",
            "    from typing import Sequence\n",
            "all_handlers_preserve: Sequence[int]\n",
        )),
        "one_handler_shadows_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "try:\n",
            "    work()\n",
            "except FirstError:\n",
            "    from typing import Sequence\n",
            "except SecondError:\n",
            "    Sequence = object\n",
            "one_handler_shadows: Sequence[int]\n",
        )),
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
                && item.candidate_start.is_some()
                && item.candidate_length.is_some()
                && item.candidate_operator.is_some()
                && item.candidate_original.is_some()
                && item.candidate_replacement.is_some()
        }
        CandidateExpectation::Absent | CandidateExpectation::NotObserved => {
            item.candidate_count == 0
                && item.candidate_path.is_none()
                && item.candidate_start.is_none()
                && item.candidate_length.is_none()
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
        "two_handlers_disagree_fallthrough" => internal_identity(item, "handler-fallthrough-meet"),
        "three_handlers_preserve_mapping" => internal_identity(item, "unrelated-fact-preservation"),
        "per_handler_cleanup_categories" => internal_identity(item, "per-handler-cleanup"),
        "different_handler_break_continue" => internal_identity(item, "handler-abrupt-categories"),
        "unhandled_remainder_terminates" => internal_identity(item, "unhandled-remainder"),
        "all_handlers_preserve_public_candidate" | "one_handler_shadows_public_candidate" => {
            item.mode == "strict"
                && item.observation_kind == ObservationKind::PublicCandidate
                && item.family == "handler-fallthrough-meet"
                && item.marker == "Sequence[int]"
                && item.candidate != CandidateExpectation::NotObserved
        }
        _ => false,
    };
    if !identity_matches {
        return Err(format!("{} has inconsistent closed identity", item.id));
    }
    Ok(())
}

fn internal_identity(item: &OracleCase, family: &str) -> bool {
    item.mode == "internal-fixture"
        && item.observation_kind == ObservationKind::TryExit
        && item.family == family
        && item.candidate == CandidateExpectation::NotObserved
}

fn selected_cases(cases: &[OracleCase]) -> Result<Vec<&OracleCase>, String> {
    let Some(selected) = std::env::var_os("HOIMIN_MULTIPLE_HANDLER_CASE") else {
        return Ok(cases.iter().collect());
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_MULTIPLE_HANDLER_CASE is not UTF-8".to_owned())?;
    let matches = cases
        .iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "HOIMIN_MULTIPLE_HANDLER_CASE={selected} selected {} rows",
            matches.len()
        ));
    }
    Ok(matches)
}

fn expected_snapshot(item: &OracleCase) -> BindingFlowTestSnapshot {
    BindingFlowTestSnapshot {
        fallthrough: item.expected_exits.fallthrough.clone(),
        breaks: item.expected_exits.breaks.clone(),
        continues: item.expected_exits.continues.clone(),
        terminates: item.expected_exits.terminates.clone(),
    }
}

#[test]
fn multiple_handler_corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("valid Lean multiple-handler corpus");
    assert_eq!(cases.len(), 7);
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 2);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        5
    );
}

#[test]
fn multiple_handler_corpus_rejects_inexact_and_crossed_rows() {
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
        "duplicate case id two_handlers_disagree_fallthrough"
    );
    for (field, value, expected) in [
        ("schema", serde_json::json!(2), "unsupported schema"),
        ("mode", serde_json::json!("report"), "unknown mode"),
        (
            "observation_kind",
            serde_json::json!("handler_exit"),
            "unknown variant",
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
    crossed["family"] = serde_json::json!("unhandled-remainder");
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
            .replace("work()", "other_work()")
    );
    assert!(
        parse_corpus(&format!("{changed_source}\n"))
            .unwrap_err()
            .contains("changed source premises")
    );
}

#[test]
fn internal_rows_match_complete_production_observations() {
    let cases = parse_corpus(CORPUS).expect("valid Lean multiple-handler corpus");
    let selected = selected_cases(&cases).expect("valid single-case filter");
    for item in selected
        .into_iter()
        .filter(|item| item.mode == "internal-fixture")
    {
        let actual = binding_flow_try_exit_snapshot(&item.source, &item.marker)
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        assert_eq!(
            actual,
            expected_snapshot(item),
            "same-premise mismatch for {}\nsource:\n{}",
            item.id,
            item.source
        );
    }
}

#[test]
fn internal_fixtures_detect_multiple_handler_routing_mutations() {
    let cases = parse_corpus(CORPUS).expect("valid Lean multiple-handler corpus");
    let case = |id: &str| cases.iter().find(|item| item.id == id).unwrap();
    for (id, mutation) in [
        (
            "two_handlers_disagree_fallthrough",
            BindingFlowTestMutation::KeepFirstHandlerOnly,
        ),
        (
            "three_handlers_preserve_mapping",
            BindingFlowTestMutation::KeepLastHandlerOnly,
        ),
        (
            "different_handler_break_continue",
            BindingFlowTestMutation::FlattenHandlerAbruptToFallthrough,
        ),
        (
            "unhandled_remainder_terminates",
            BindingFlowTestMutation::DropBodyTerminates,
        ),
        (
            "per_handler_cleanup_categories",
            BindingFlowTestMutation::OmitHandlerFallthroughCleanup,
        ),
        (
            "per_handler_cleanup_categories",
            BindingFlowTestMutation::OmitHandlerTerminateCleanup,
        ),
    ] {
        let item = case(id);
        let mutated =
            binding_flow_try_exit_snapshot_with_mutation(&item.source, &item.marker, mutation)
                .expect("mutation premise must remain observable");
        assert_ne!(
            mutated,
            expected_snapshot(item),
            "{id} must detect {mutation:?}"
        );
    }
}
