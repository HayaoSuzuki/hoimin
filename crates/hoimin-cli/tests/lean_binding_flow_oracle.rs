use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/binding-flow-joins.jsonl");

const CORPUS_FIELDS: [&str; 10] = [
    "expected_present",
    "expected_replacement",
    "expected_symbol",
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
    expected_present: bool,
    expected_replacement: Option<String>,
    expected_symbol: Option<String>,
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

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1
        || item.id.is_empty()
        || item.source.is_empty()
        || item.site_marker.is_empty()
        || !item.source.contains(&item.site_marker)
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
    let premise_matches = match item.family.as_str() {
        "typing-import" => item.source.contains("from typing import Sequence"),
        "control-flow" => ["if ", "for ", "while ", "try:", "match "]
            .iter()
            .any(|keyword| item.source.contains(keyword)),
        "scope" => ["def ", "class ", "global ", "nonlocal "]
            .iter()
            .any(|keyword| item.source.contains(keyword)),
        "builtin-pair" => item.source.contains("list"),
        "exception-pair" => item.source.contains("ValueError"),
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
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    present: bool,
    replacement: Option<String>,
    operator: Option<String>,
    symbol: Option<String>,
}

impl Observation {
    fn absent() -> Self {
        Self {
            present: false,
            replacement: None,
            operator: None,
            symbol: None,
        }
    }
}

fn expected_observation(item: &OracleCase) -> Observation {
    Observation {
        present: item.expected_present,
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
            let candidate_end = candidate.span.start + candidate.span.length;
            candidate.operator == item.operator
                && candidate_start < marker_end
                && marker_start < candidate_end
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(Observation::absent()),
        [candidate] => Ok(Observation {
            present: true,
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
}
