use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/binding-flow-joins.jsonl");

const CORPUS_FIELDS: [&str; 18] = [
    "expected_breaks",
    "expected_continues",
    "expected_destination_target",
    "expected_fallthrough",
    "expected_loop_head",
    "expected_original",
    "expected_present",
    "expected_replacement",
    "expected_source_target",
    "expected_symbol",
    "expected_terminates",
    "family",
    "id",
    "mode",
    "operator",
    "schema",
    "site_marker",
    "source",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    family: String,
    operator: String,
    source: String,
    site_marker: String,
    expected_original: String,
    expected_source_target: String,
    expected_destination_target: String,
    expected_present: bool,
    expected_replacement: Option<String>,
    expected_symbol: Option<String>,
    expected_fallthrough: Vec<Vec<String>>,
    expected_breaks: Vec<Vec<String>>,
    expected_continues: Vec<Vec<String>>,
    expected_terminates: Vec<Vec<String>>,
    expected_loop_head: Option<Vec<String>>,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
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
        if fields != BTreeSet::from(CORPUS_FIELDS) {
            return Err(format!("line {} has an inexact field set", index + 1));
        }
        let item: OracleCase = serde_json::from_value(value)
            .map_err(|error| format!("line {} violates the schema: {error}", index + 1))?;
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

type ScenarioContract = (&'static str, &'static str, &'static [&'static str]);

fn typing_scenario_contract(id: &str) -> Option<ScenarioContract> {
    Some(match id {
        "typing_if_identical" => ("strict", "typing-import", &["if flag:", "else:"]),
        "typing_if_disagrees" => (
            "strict",
            "typing-import",
            &["if flag:", "Sequence = local_sequence", "else:"],
        ),
        "typing_loop_zero_iteration" => (
            "internal-fixture",
            "control-flow",
            &["for item in items:", "Sequence = local_sequence"],
        ),
        "typing_loop_continue_backedge" => (
            "internal-fixture",
            "control-flow",
            &["while condition:", "Sequence = local_sequence", "continue"],
        ),
        "typing_loop_break_exit" => (
            "internal-fixture",
            "control-flow",
            &["while condition:", "Sequence = local_sequence", "break"],
        ),
        "typing_try_handler_join" => (
            "strict",
            "control-flow",
            &["try:", "except Error:", "Sequence = local_sequence"],
        ),
        "typing_finally_restores" => (
            "strict",
            "control-flow",
            &["try:", "finally:", "Sequence = local_sequence"],
        ),
        "typing_abrupt_finally" => (
            "model-only",
            "control-flow",
            &["try:", "finally:", "raise Error"],
        ),
        "typing_match_unmatched_path" => (
            "strict",
            "control-flow",
            &["match value:", "case 0:", "Sequence = local_sequence"],
        ),
        "typing_match_irrefutable_reimport" => (
            "strict",
            "control-flow",
            &["match value:", "case _:", "from typing import Sequence"],
        ),
        "typing_match_guard_binding" => (
            "internal-fixture",
            "control-flow",
            &["match value:", "if (Sequence := local_sequence)"],
        ),
        "typing_function_whole_block_local" => (
            "strict",
            "scope",
            &["def local_scope():", "Sequence = local_sequence"],
        ),
        "typing_method_skips_class" => ("strict", "scope", &["class Box:", "def method(self):"]),
        "typing_class_before_binding" => (
            "strict",
            "scope",
            &["class Before:", "Sequence = local_sequence"],
        ),
        "typing_class_after_binding" => (
            "strict",
            "scope",
            &["class After:", "Sequence = local_sequence"],
        ),
        "typing_global_unknown" => (
            "model-only",
            "scope",
            &["def global_scope():", "global Sequence"],
        ),
        "typing_nonlocal_unknown" => (
            "model-only",
            "scope",
            &["def outer():", "def inner():", "nonlocal Sequence"],
        ),
        "typing_wildcard_unknown" => ("strict", "typing-import", &["from helpers import *"]),
        "typing_unconditional_reimport" => (
            "strict",
            "typing-import",
            &["Sequence = local_sequence", "restored: list[str]"],
        ),
        _ => return None,
    })
}

fn pair_scenario_contract(id: &str) -> Option<ScenarioContract> {
    Some(match id {
        "builtin_pair_clean" => ("strict", "builtin-pair", &["clean_marker = list(items)"]),
        "builtin_source_shadowed" => (
            "strict",
            "builtin-pair",
            &["list = local_list", "hidden_source = list(items)"],
        ),
        "builtin_destination_shadowed" => (
            "strict",
            "builtin-pair",
            &["tuple = local_tuple", "hidden_destination = list(items)"],
        ),
        "exception_pair_clean" => ("strict", "exception-pair", &["raise ValueError"]),
        "exception_source_shadowed" => (
            "strict",
            "exception-pair",
            &["ValueError = CustomValueError", "raise ValueError"],
        ),
        "exception_destination_shadowed" => (
            "strict",
            "exception-pair",
            &["TypeError = CustomTypeError", "raise ValueError"],
        ),
        _ => return None,
    })
}

fn scenario_contract(id: &str) -> Option<ScenarioContract> {
    typing_scenario_contract(id).or_else(|| pair_scenario_contract(id))
}

fn validate_normalized_states(item: &OracleCase) -> Result<(), String> {
    let states = item
        .expected_fallthrough
        .iter()
        .chain(&item.expected_breaks)
        .chain(&item.expected_continues)
        .chain(&item.expected_terminates)
        .chain(item.expected_loop_head.iter());
    if states
        .flatten()
        .any(|fact| fact != "direct:Sequence=typing.Sequence")
    {
        return Err(format!(
            "{} has an unknown normalized binding fact",
            item.id
        ));
    }
    if item.expected_loop_head.is_some() && item.mode != "internal-fixture" {
        return Err(format!(
            "{} exposes a loop head outside an internal fixture",
            item.id
        ));
    }
    Ok(())
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1
        || item.id.is_empty()
        || item.source.is_empty()
        || item.site_marker.is_empty()
        || item.expected_original.is_empty()
        || !item.source.contains(&item.site_marker)
        || !item.site_marker.contains(&item.expected_original)
    {
        return Err(format!("{} has an invalid identity or marker", item.id));
    }
    if !matches!(
        item.mode.as_str(),
        "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
    ) {
        return Err(format!("{} has unknown mode {}", item.id, item.mode));
    }
    if !matches!(
        (item.family.as_str(), item.operator.as_str()),
        (
            "typing-import" | "control-flow" | "scope",
            "type_list_sequence"
        ) | ("builtin-pair", "collection_list_tuple")
            | ("exception-pair", "exception_type_pair")
    ) {
        return Err(format!(
            "{} has mismatched family/operator {}/{}",
            item.id, item.family, item.operator
        ));
    }
    let Some((scenario_mode, scenario_family, required_source)) = scenario_contract(&item.id)
    else {
        return Err(format!(
            "{} is not a recognized binding-flow scenario",
            item.id
        ));
    };
    if item.mode != scenario_mode
        || item.family != scenario_family
        || required_source
            .iter()
            .any(|required| !item.source.contains(required))
    {
        return Err(format!("{} violates its typed scenario contract", item.id));
    }
    let premise_matches = match item.operator.as_str() {
        "type_list_sequence" => {
            item.expected_original == "list[str]"
                && item.expected_source_target == "builtin"
                && item.expected_destination_target == "typing"
                && item.source.contains("from typing import Sequence")
        }
        "collection_list_tuple" => {
            item.expected_original == "list"
                && item.expected_source_target == "builtin"
                && item.expected_destination_target == "builtin"
                && item.source.contains("list")
        }
        "exception_type_pair" => {
            item.expected_original == "ValueError"
                && item.expected_source_target == "builtin"
                && item.expected_destination_target == "builtin"
                && item.source.contains("ValueError")
        }
        _ => false,
    };
    if !premise_matches {
        return Err(format!("{} does not establish its family premise", item.id));
    }
    if item.expected_present != item.expected_replacement.is_some()
        || (!item.expected_present && item.expected_symbol.is_some())
        || item.expected_replacement.as_deref() == Some("")
        || item.expected_symbol.as_deref() == Some("")
    {
        return Err(format!(
            "{} has inconsistent nullable expectations",
            item.id
        ));
    }
    validate_normalized_states(item)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    present: bool,
    original: Option<String>,
    replacement: Option<String>,
    operator: Option<String>,
    symbol: Option<String>,
}

impl Observation {
    fn absent() -> Self {
        Self {
            present: false,
            original: None,
            replacement: None,
            operator: None,
            symbol: None,
        }
    }
}

fn expected_observation(item: &OracleCase) -> Observation {
    Observation {
        present: item.expected_present,
        original: item
            .expected_present
            .then(|| item.expected_original.clone()),
        replacement: item.expected_replacement.clone(),
        operator: item.expected_present.then(|| item.operator.clone()),
        symbol: item.expected_symbol.clone(),
    }
}

fn observe_manifest(item: &OracleCase, manifest: &PlanManifest) -> Result<Observation, String> {
    let marker_offsets = item
        .source
        .match_indices(&item.site_marker)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let [marker_start] = marker_offsets.as_slice() else {
        return Err(format!(
            "infrastructure-error: {} marker occurs {} times",
            item.id,
            marker_offsets.len()
        ));
    };
    let marker_start = u64::try_from(*marker_start)
        .map_err(|_| format!("infrastructure-error: {} marker offset overflow", item.id))?;
    let marker_end = marker_start
        + u64::try_from(item.site_marker.len())
            .map_err(|_| format!("infrastructure-error: {} marker length overflow", item.id))?;
    let matching = manifest
        .candidates
        .iter()
        .filter(|candidate| {
            let candidate_start = candidate.span.start;
            let Some(candidate_end) = candidate.span.start.checked_add(candidate.span.length)
            else {
                return false;
            };
            let exact_source = usize::try_from(candidate_start)
                .ok()
                .zip(usize::try_from(candidate_end).ok())
                .and_then(|(start, end)| item.source.get(start..end))
                == Some(candidate.original.as_str());
            candidate.operator == item.operator
                && candidate.original == item.expected_original
                && exact_source
                && marker_start <= candidate_start
                && candidate_end <= marker_end
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(Observation::absent()),
        [candidate] => Ok(Observation {
            present: true,
            original: Some(candidate.original.clone()),
            replacement: Some(candidate.replacement.clone()),
            operator: Some(candidate.operator.clone()),
            symbol: candidate.symbol.clone(),
        }),
        _ => Err(format!(
            "infrastructure-error: {} has {} matching candidates at marker",
            item.id,
            matching.len()
        )),
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let root = directory.path().join("project");
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/case.py"), source).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"binding-flow-case\"\nversion = \"0.0.0\"\n",
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            _directory: directory,
            root,
        })
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace crates")
        .parent()
        .expect("workspace root")
        .to_owned()
}

fn python_executable() -> PathBuf {
    if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    }
}

fn plan_args(fixture: &Fixture, operator: &str) -> Vec<OsString> {
    vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        fixture.root.as_os_str().to_owned(),
        "--source".into(),
        "src".into(),
        "--file".into(),
        "src/case.py".into(),
        "--operators".into(),
        operator.into(),
        "--jobs".into(),
        "1".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python_executable().into_os_string(),
        "-c".into(),
        "pass".into(),
    ]
}

async fn public_plan(item: &OracleCase) -> Result<PlanManifest, String> {
    let fixture = Fixture::new(&item.source)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_io(
            plan_args(&fixture, &item.operator),
            &mut stdout,
            &mut stderr,
        ),
    )
    .await
    .map_err(|_| format!("infrastructure-error: {} public plan timed out", item.id))?;
    if code != 0 || !stderr.is_empty() {
        return Err(format!(
            "infrastructure-error: {} public plan exit={code}, stderr={}",
            item.id,
            String::from_utf8_lossy(&stderr)
        ));
    }
    serde_json::from_slice(&stdout)
        .map_err(|error| format!("infrastructure-error: {} malformed plan: {error}", item.id))
}

#[tokio::test(flavor = "current_thread")]
async fn strict_public_plan_observations_match_lean() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    for item in cases.iter().filter(|item| item.mode == "strict") {
        let manifest = public_plan(item)
            .await
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        let actual = observe_manifest(item, &manifest)
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        let expected = expected_observation(item);
        assert_eq!(
            actual, expected,
            "same-premise mismatch for {}\nsource:\n{}",
            item.id, item.source
        );
    }
}

#[test]
fn corpus_accepts_the_lean_binding_flow_schema() {
    let cases = parse_corpus(CORPUS).expect("Lean binding-flow corpus must be valid");

    assert_eq!(cases.len(), 25);
    assert!(cases.iter().any(|item| item.mode == "strict"));
    assert!(cases.iter().any(|item| item.mode == "internal-fixture"));
    assert!(cases.iter().any(|item| item.mode == "model-only"));
}

#[test]
fn corpus_rejects_unknown_fields_duplicate_ids_and_bad_modes() {
    let first = CORPUS.lines().next().expect("first corpus line");
    let mut unknown_field: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown_field["unexpected"] = serde_json::json!(true);
    assert!(parse_corpus(&format!("{unknown_field}\n")).is_err());

    assert!(parse_corpus(&format!("{first}\n{first}\n")).is_err());

    let mut bad_mode: serde_json::Value = serde_json::from_str(first).unwrap();
    bad_mode["mode"] = serde_json::json!("speculative");
    assert!(parse_corpus(&format!("{bad_mode}\n")).is_err());
}

#[test]
fn corpus_rejects_operator_family_and_premise_mismatches() {
    let first = CORPUS.lines().next().expect("first corpus line");
    let mut wrong_operator: serde_json::Value = serde_json::from_str(first).unwrap();
    wrong_operator["operator"] = serde_json::json!("exception_type_pair");
    assert!(parse_corpus(&format!("{wrong_operator}\n")).is_err());

    let mut missing_marker: serde_json::Value = serde_json::from_str(first).unwrap();
    missing_marker["site_marker"] = serde_json::json!("not in source");
    assert!(parse_corpus(&format!("{missing_marker}\n")).is_err());

    let mut inconsistent_absence: serde_json::Value = serde_json::from_str(first).unwrap();
    inconsistent_absence["expected_present"] = serde_json::json!(false);
    assert!(parse_corpus(&format!("{inconsistent_absence}\n")).is_err());

    let mut wrong_source_target: serde_json::Value = serde_json::from_str(first).unwrap();
    wrong_source_target["expected_source_target"] = serde_json::json!("typing");
    assert!(parse_corpus(&format!("{wrong_source_target}\n")).is_err());

    let mut wrong_original: serde_json::Value = serde_json::from_str(first).unwrap();
    wrong_original["expected_original"] = serde_json::json!("list");
    assert!(parse_corpus(&format!("{wrong_original}\n")).is_err());

    let try_case = CORPUS
        .lines()
        .find(|line| line.contains("typing_try_handler_join"))
        .expect("try scenario");
    let mut wrong_scenario: serde_json::Value = serde_json::from_str(try_case).unwrap();
    wrong_scenario["source"] =
        serde_json::json!("from typing import Sequence\nsimple: list[str]\n");
    wrong_scenario["site_marker"] = serde_json::json!("list[str]");
    assert!(parse_corpus(&format!("{wrong_scenario}\n")).is_err());
}
