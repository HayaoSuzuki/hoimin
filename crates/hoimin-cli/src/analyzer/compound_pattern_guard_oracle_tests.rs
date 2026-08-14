use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    BindingFlowTestMutation, binding_flow_marker_snapshot,
    binding_flow_marker_snapshot_with_mutation,
};

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../formal/HoiminOracle/corpus/compound-pattern-guards.jsonl"
));

const CORPUS_FIELDS: [&str; 16] = [
    "candidate_count",
    "candidate_length",
    "candidate_operator",
    "candidate_original",
    "candidate_path",
    "candidate_replacement",
    "candidate_start",
    "candidate_symbol",
    "expected_facts",
    "family",
    "id",
    "marker",
    "mode",
    "observation_kind",
    "schema",
    "source",
];

const EXPECTED_KEYS: [(&str, &str, &str, &str); 11] = [
    (
        "or_success_meets_arms",
        "internal-fixture",
        "case_entry",
        "or-success-meet",
    ),
    (
        "or_failure_meets_arms",
        "internal-fixture",
        "next_case_entry",
        "or-failure-meet",
    ),
    (
        "as_child_failure_precedes_alias",
        "internal-fixture",
        "next_case_entry",
        "as-binding-point",
    ),
    (
        "mapping_child_failure_precedes_rest",
        "internal-fixture",
        "next_case_entry",
        "mapping-rest-binding-point",
    ),
    (
        "class_early_failure_precedes_capture",
        "internal-fixture",
        "next_case_entry",
        "class-capture-binding-point",
    ),
    (
        "false_guard_uses_post_guard",
        "internal-fixture",
        "next_case_entry",
        "false-guard",
    ),
    (
        "compound_preserves_mapping",
        "internal-fixture",
        "next_case_entry",
        "unrelated-fact",
    ),
    (
        "as_failure_public_candidate",
        "strict",
        "public_candidate",
        "as-binding-point",
    ),
    (
        "mapping_failure_public_candidate",
        "strict",
        "public_candidate",
        "mapping-rest-binding-point",
    ),
    (
        "class_failure_public_candidate",
        "strict",
        "public_candidate",
        "class-capture-binding-point",
    ),
    (
        "unequal_or_capture_sets",
        "model-only",
        "model_witness",
        "or-success-meet",
    ),
];

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
    expected_facts: Vec<String>,
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
        "or_success_meets_arms" | "or_failure_meets_arms" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "match value:\n",
            "    case [0, Sequence] | {\"item\": Sequence}:\n",
            "        or_body_marker: list[str]\n",
            "    case _:\n",
            "        or_failure_marker: list[str]\n",
        )),
        "as_child_failure_precedes_alias" | "as_failure_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "match value:\n",
            "    case [0] as Sequence:\n",
            "        pass\n",
            "    case _:\n",
            "        as_failure_marker: list[int]\n",
        )),
        "mapping_child_failure_precedes_rest" | "mapping_failure_public_candidate" => {
            Some(concat!(
                "from typing import Sequence\n",
                "match value:\n",
                "    case {\"tag\": 0, **Sequence}:\n",
                "        pass\n",
                "    case _:\n",
                "        mapping_failure_marker: list[int]\n",
            ))
        }
        "class_early_failure_precedes_capture" | "class_failure_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "match value:\n",
            "    case Point(0, tail=Sequence):\n",
            "        pass\n",
            "    case _:\n",
            "        class_failure_marker: list[int]\n",
        )),
        "false_guard_uses_post_guard" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "match value:\n",
            "    case [Mapping] if ((Sequence := local_sequence) and False):\n",
            "        pass\n",
            "    case _:\n",
            "        false_guard_marker: list[str]\n",
        )),
        "compound_preserves_mapping" => Some(concat!(
            "from typing import Mapping, Sequence\n",
            "match value:\n",
            "    case [0] as Sequence:\n",
            "        pass\n",
            "    case _:\n",
            "        preserved_mapping_marker: tuple[Mapping]\n",
        )),
        "unequal_or_capture_sets" => Some(""),
        _ => None,
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
        "case_entry" | "next_case_entry" | "public_candidate" | "model_witness"
    ) {
        return Err(format!("{} has unknown observation kind", item.id));
    }
    if owned_source(&item.id) != Some(item.source.as_str()) {
        return Err(format!("{} has changed source premises", item.id));
    }
    if !item.expected_facts.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(format!("{} has unsorted or duplicate facts", item.id));
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

    let public_fields = if item.mode == "strict" {
        item.observation_kind == "public_candidate"
            && item.expected_facts.is_empty()
            && item.candidate_count == 1
            && item.candidate_path.as_deref() == Some("target.py")
            && item.candidate_start.is_some()
            && item.candidate_length == Some(item.marker.len() as u64)
            && item.candidate_operator.as_deref() == Some("type_list_sequence")
            && item.candidate_original.as_deref() == Some("list[int]")
            && item.candidate_replacement.as_deref() == Some("Sequence[int]")
            && item.candidate_symbol.is_none()
    } else {
        item.candidate_count == 0
            && item.candidate_path.is_none()
            && item.candidate_start.is_none()
            && item.candidate_length.is_none()
            && item.candidate_operator.is_none()
            && item.candidate_original.is_none()
            && item.candidate_replacement.is_none()
            && item.candidate_symbol.is_none()
    };
    if !public_fields {
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
    let expected = EXPECTED_KEYS.into_iter().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err("corpus does not contain the exact owned case set".to_owned());
    }
    Ok(cases)
}

fn selected_internal_cases() -> Result<Vec<OracleCase>, String> {
    let cases = parse_corpus(CORPUS)?
        .into_iter()
        .filter(|item| item.mode == "internal-fixture")
        .collect::<Vec<_>>();
    let Some(selected) = std::env::var_os("HOIMIN_COMPOUND_PATTERN_CASE") else {
        return Ok(cases);
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_COMPOUND_PATTERN_CASE is not UTF-8".to_owned())?;
    let matching = cases
        .into_iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "HOIMIN_COMPOUND_PATTERN_CASE={selected} selected {} internal rows",
            matching.len()
        ));
    }
    Ok(matching)
}

#[test]
fn corpus_schema_rejects_unknown_fields_and_changed_sources() {
    let mut rows = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut unknown: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    rows[0] = unknown.to_string();
    assert!(
        parse_corpus(&rows.join("\n"))
            .unwrap_err()
            .contains("inexact field set")
    );

    let mut rows = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut changed: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    changed["source"] = serde_json::json!("match changed:\n    case _: pass\n");
    rows[0] = changed.to_string();
    assert!(
        parse_corpus(&rows.join("\n"))
            .unwrap_err()
            .contains("changed source premises")
    );
}

#[test]
fn compound_pattern_guard_internal_corpus() {
    let cases = selected_internal_cases().unwrap();
    assert!(!cases.is_empty());
    for item in cases {
        let actual = binding_flow_marker_snapshot(&item.source, &item.marker)
            .unwrap_or_else(|error| panic!("{error}\ncase={}\nsource:\n{}", item.id, item.source));
        assert_eq!(
            actual, item.expected_facts,
            "same-premise mismatch for {}",
            item.id
        );
    }
}

#[test]
fn internal_rows_detect_broken_compound_pattern_transfers() {
    let cases = parse_corpus(CORPUS).unwrap();
    for id in [
        "or_failure_meets_arms",
        "as_child_failure_precedes_alias",
        "mapping_child_failure_precedes_rest",
        "class_early_failure_precedes_capture",
    ] {
        let item = cases.iter().find(|item| item.id == id).unwrap();
        let correct = binding_flow_marker_snapshot(&item.source, &item.marker).unwrap();
        let broken = binding_flow_marker_snapshot_with_mutation(
            &item.source,
            &item.marker,
            BindingFlowTestMutation::UseMatchedPatternFailureEnvironment,
        )
        .unwrap();
        assert_eq!(correct, item.expected_facts, "{id}");
        assert_ne!(broken, correct, "{id} must detect late capture on failure");
    }

    let unrelated = cases
        .iter()
        .find(|item| item.id == "compound_preserves_mapping")
        .unwrap();
    let correct = binding_flow_marker_snapshot(&unrelated.source, &unrelated.marker).unwrap();
    let broken = binding_flow_marker_snapshot_with_mutation(
        &unrelated.source,
        &unrelated.marker,
        BindingFlowTestMutation::OverbroadPatternCleanup,
    )
    .unwrap();
    assert_eq!(correct, unrelated.expected_facts);
    assert_ne!(broken, correct);

    let or_failure_order = concat!(
        "from typing import Sequence\n",
        "match value:\n",
        "    case [Sequence, 0] | [0, Sequence]:\n",
        "        pass\n",
        "    case _:\n",
        "        ordered_or_failure_marker: list[str]\n",
    );
    let correct =
        binding_flow_marker_snapshot(or_failure_order, "ordered_or_failure_marker").unwrap();
    let broken = binding_flow_marker_snapshot_with_mutation(
        or_failure_order,
        "ordered_or_failure_marker",
        BindingFlowTestMutation::KeepLastOrPatternFailure,
    )
    .unwrap();
    assert!(correct.is_empty());
    assert_eq!(broken, vec!["direct:Sequence=typing.Sequence"]);
}
