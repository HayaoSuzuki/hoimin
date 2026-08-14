use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    BindingFlowTestMutation, BindingFlowTestSnapshot, binding_flow_try_exit_snapshot,
    binding_flow_try_exit_snapshot_with_mutation,
};

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../formal/HoiminOracle/corpus/except-star-flow.jsonl"
));

const CORPUS_FIELDS: [&str; 17] = [
    "candidate",
    "candidate_count",
    "candidate_length",
    "candidate_operator",
    "candidate_original",
    "candidate_path",
    "candidate_replacement",
    "candidate_start",
    "candidate_symbol",
    "expected_exits",
    "family",
    "id",
    "marker",
    "mode",
    "observation_kind",
    "schema",
    "source",
];

const EXPECTED_KEYS: [(&str, &str, &str, &str); 8] = [
    (
        "starred_summary_preserves_common",
        "internal-fixture",
        "try_exit",
        "collapsed-summary",
    ),
    (
        "starred_summary_meets_disagreement",
        "internal-fixture",
        "try_exit",
        "collapsed-summary",
    ),
    (
        "starred_target_cleanup",
        "internal-fixture",
        "try_exit",
        "target-cleanup",
    ),
    (
        "starred_unhandled_remainder",
        "internal-fixture",
        "try_exit",
        "unhandled-remainder",
    ),
    (
        "two_matching_siblings_exact_route",
        "model-only",
        "model_witness",
        "sibling-order",
    ),
    (
        "raised_handler_allows_later_sibling",
        "model-only",
        "model_witness",
        "delayed-raise",
    ),
    (
        "starred_public_candidate_present",
        "strict",
        "public_candidate",
        "collapsed-summary",
    ),
    (
        "starred_public_candidate_absent",
        "strict",
        "public_candidate",
        "collapsed-summary",
    ),
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedExits {
    fallthrough: Vec<Vec<String>>,
    breaks: Vec<Vec<String>>,
    continues: Vec<Vec<String>>,
    terminates: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    observation_kind: String,
    family: String,
    source: String,
    marker: String,
    expected_exits: ExpectedExits,
    candidate: String,
    candidate_count: usize,
    candidate_path: Option<String>,
    candidate_start: Option<u64>,
    candidate_length: Option<u64>,
    candidate_operator: Option<String>,
    candidate_original: Option<String>,
    candidate_replacement: Option<String>,
    candidate_symbol: Option<String>,
}

fn owned_source(id: &str) -> Option<&'static str> {
    match id {
        "starred_summary_preserves_common" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "try:  # starred_summary_preserves_common\n",
            "    work()\n",
            "except* FirstError:\n",
            "    from typing import Sequence\n",
            "except* SecondError:\n",
            "    from typing import Sequence\n",
        )),
        "starred_summary_meets_disagreement" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "try:  # starred_summary_meets_disagreement\n",
            "    work()\n",
            "except* FirstError:\n",
            "    from typing import Sequence\n",
            "except* SecondError:\n",
            "    Sequence = object\n",
        )),
        "starred_target_cleanup" => Some(concat!(
            "def run():\n",
            "    from typing import Mapping, Sequence\n",
            "    try:  # starred_target_cleanup\n",
            "        work()\n",
            "    except* FirstError as Sequence:\n",
            "        from typing import Sequence\n",
            "    except* SecondError as Sequence:\n",
            "        from typing import Sequence\n",
        )),
        "starred_unhandled_remainder" => Some(concat!(
            "def run():\n",
            "    from typing import Mapping, Sequence\n",
            "    try:  # starred_unhandled_remainder\n",
            "        raise UnknownError\n",
            "    except* FirstError:\n",
            "        pass\n",
            "    except* SecondError:\n",
            "        pass\n",
        )),
        "two_matching_siblings_exact_route" | "raised_handler_allows_later_sibling" => Some(""),
        "starred_public_candidate_present" => Some(concat!(
            "from typing import Sequence\n",
            "try:\n",
            "    work()\n",
            "except* FirstError:\n",
            "    from typing import Sequence\n",
            "except* SecondError:\n",
            "    from typing import Sequence\n",
            "starred_present: Sequence[int]\n",
        )),
        "starred_public_candidate_absent" => Some(concat!(
            "from typing import Sequence\n",
            "try:\n",
            "    work()\n",
            "except* FirstError:\n",
            "    from typing import Sequence\n",
            "except* SecondError:\n",
            "    Sequence = object\n",
            "starred_absent: Sequence[int]\n",
        )),
        _ => None,
    }
}

fn public_fields_valid(item: &OracleCase) -> bool {
    match item.candidate.as_str() {
        "not_observed" | "absent" => {
            item.candidate_count == 0
                && item.candidate_path.is_none()
                && item.candidate_start.is_none()
                && item.candidate_length.is_none()
                && item.candidate_operator.is_none()
                && item.candidate_original.is_none()
                && item.candidate_replacement.is_none()
                && item.candidate_symbol.is_none()
        }
        "present" => {
            item.candidate_count == 1
                && item.candidate_path.as_deref() == Some("target.py")
                && item.candidate_start.is_some()
                && item.candidate_length == Some(item.marker.len() as u64)
                && item.candidate_operator.as_deref() == Some("type_list_sequence")
                && item.candidate_original.as_deref() == Some("Sequence[int]")
                && item.candidate_replacement.as_deref() == Some("list[int]")
                && item.candidate_symbol.is_none()
        }
        _ => false,
    }
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1 || item.id.is_empty() || item.family.is_empty() {
        return Err(format!("{} has invalid identity fields", item.id));
    }
    if !matches!(
        item.mode.as_str(),
        "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
    ) {
        return Err(format!("{} has unknown mode", item.id));
    }
    if !matches!(
        item.observation_kind.as_str(),
        "try_exit" | "public_candidate" | "model_witness"
    ) {
        return Err(format!("{} has unknown observation kind", item.id));
    }
    if owned_source(&item.id) != Some(item.source.as_str()) {
        return Err(format!("{} has changed source premises", item.id));
    }
    if item.mode == "model-only" {
        if item.observation_kind != "model_witness"
            || !item.source.is_empty()
            || !item.marker.is_empty()
        {
            return Err(format!("{} has a production-backed model witness", item.id));
        }
    } else if item.marker.is_empty() || item.source.match_indices(&item.marker).count() != 1 {
        return Err(format!("{} marker is not unique", item.id));
    }
    if !public_fields_valid(item) {
        return Err(format!("{} has inconsistent candidate fields", item.id));
    }
    Ok(())
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let expected_fields = BTreeSet::from(CORPUS_FIELDS);
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        let object = value
            .as_object()
            .ok_or_else(|| format!("line {} is not an object", index + 1))?;
        let fields = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if fields != expected_fields {
            return Err(format!("line {} has an inexact field set", index + 1));
        }
        let item: OracleCase = serde_json::from_value(value)
            .map_err(|error| format!("line {} violates schema: {error}", index + 1))?;
        validate_case(&item)?;
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        cases.push(item);
    }
    let actual = cases
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.mode.as_str(),
                item.observation_kind.as_str(),
                item.family.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    if actual != EXPECTED_KEYS.into_iter().collect::<BTreeSet<_>>() {
        return Err("corpus does not contain the exact owned case set".to_owned());
    }
    Ok(cases)
}

fn selected_internal_cases() -> Result<Vec<OracleCase>, String> {
    let cases = parse_corpus(CORPUS)?
        .into_iter()
        .filter(|item| item.mode == "internal-fixture")
        .collect::<Vec<_>>();
    let Some(selected) = std::env::var_os("HOIMIN_EXCEPT_STAR_CASE") else {
        return Ok(cases);
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_EXCEPT_STAR_CASE is not UTF-8".to_owned())?;
    let matching = cases
        .into_iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "HOIMIN_EXCEPT_STAR_CASE={selected} selected {} internal rows",
            matching.len()
        ));
    }
    Ok(matching)
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
fn except_star_corpus_is_closed_and_rejects_premise_drift() {
    let cases = parse_corpus(CORPUS).expect("valid Lean except-star corpus");
    assert_eq!(cases.len(), 8);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        4
    );

    let first = CORPUS.lines().next().unwrap();
    let mut unknown: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(
        parse_corpus(&format!("{unknown}\n"))
            .unwrap_err()
            .contains("inexact field set")
    );

    let mut changed: serde_json::Value = serde_json::from_str(first).unwrap();
    changed["source"] = serde_json::json!("try:\n    pass\n");
    assert!(
        parse_corpus(&format!("{changed}\n"))
            .unwrap_err()
            .contains("changed source premises")
    );
}

#[test]
fn internal_rows_match_complete_production_try_exits() {
    let cases = selected_internal_cases().expect("valid single-case filter");
    assert!(!cases.is_empty());
    for item in cases {
        let actual = binding_flow_try_exit_snapshot(&item.source, &item.marker)
            .unwrap_or_else(|error| panic!("{error}\ncase={}\nsource:\n{}", item.id, item.source));
        assert_eq!(
            actual,
            expected_snapshot(&item),
            "same-premise mismatch for {}\nsource:\n{}",
            item.id,
            item.source
        );
    }
}

#[test]
fn internal_rows_detect_production_transfer_mutations() {
    let cases = parse_corpus(CORPUS).expect("valid Lean except-star corpus");
    let case = |id: &str| cases.iter().find(|item| item.id == id).unwrap();
    for (id, mutation) in [
        (
            "starred_summary_meets_disagreement",
            BindingFlowTestMutation::KeepFirstHandlerOnly,
        ),
        (
            "starred_target_cleanup",
            BindingFlowTestMutation::OmitHandlerFallthroughCleanup,
        ),
        (
            "starred_unhandled_remainder",
            BindingFlowTestMutation::DropBodyTerminates,
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
