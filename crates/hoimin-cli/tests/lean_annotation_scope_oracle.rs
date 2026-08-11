use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/annotation-scope-correspondence.jsonl");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    observation_kind: String,
    source: String,
    marker: String,
    expected_facts: Vec<String>,
    expected_symbol: Option<String>,
    expected_scope: Option<String>,
    expected_resolution: Option<String>,
    expected_present: bool,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid: {error}", index + 1))?;
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

fn common_fields_are_valid(item: &OracleCase) -> bool {
    const SCENARIOS: [&str; 15] = [
        "global-before",
        "global-after-write",
        "global-restored",
        "nonlocal-before",
        "nonlocal-after-write",
        "nonlocal-restored",
        "class-global",
        "class-nonlocal",
        "list-outer-before",
        "list-outer-after",
        "list-first-iterable",
        "list-body",
        "set-body",
        "dict-body",
        "generator-body",
    ];
    let facts_are_valid = item.expected_facts.is_empty()
        || matches!(
            item.expected_facts.as_slice(),
            [fact] if fact == "direct:Sequence=typing.Sequence"
        );
    item.schema == 1
        && !item.id.is_empty()
        && !item.source.is_empty()
        && !item.marker.is_empty()
        && item.source.match_indices(&item.marker).count() == 1
        && SCENARIOS.contains(&item.scenario.as_str())
        && matches!(
            item.mode.as_str(),
            "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
        )
        && matches!(
            item.observation_kind.as_str(),
            "annotation" | "resolution" | "public-candidate"
        )
        && facts_are_valid
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if !common_fields_are_valid(item) {
        return Err(format!("{} violates the closed corpus schema", item.id));
    }
    let fields_match = match item.observation_kind.as_str() {
        "annotation" => {
            item.mode == "internal-fixture"
                && matches!(
                    item.scenario.as_str(),
                    "global-before"
                        | "global-after-write"
                        | "global-restored"
                        | "nonlocal-before"
                        | "nonlocal-after-write"
                        | "nonlocal-restored"
                        | "class-global"
                        | "class-nonlocal"
                        | "list-outer-before"
                        | "list-outer-after"
                )
                && item.expected_symbol.is_some()
                && item.expected_scope.as_deref() == Some("function")
                && item.expected_resolution.is_none()
                && item.expected_present != item.expected_facts.is_empty()
        }
        "resolution" => {
            item.mode == "internal-fixture"
                && matches!(
                    item.scenario.as_str(),
                    "list-first-iterable"
                        | "list-body"
                        | "set-body"
                        | "dict-body"
                        | "generator-body"
                )
                && item.expected_facts.is_empty()
                && item.expected_symbol.is_none()
                && item.expected_scope.is_none()
                && matches!(
                    item.expected_resolution.as_deref(),
                    Some("definitely-builtin" | "shadowed" | "unknown")
                )
                && item.expected_present
                    == (item.expected_resolution.as_deref() == Some("definitely-builtin"))
        }
        "public-candidate" => {
            item.mode == "strict"
                && matches!(
                    (item.scenario.as_str(), item.expected_present),
                    (
                        "global-before"
                            | "global-restored"
                            | "nonlocal-before"
                            | "nonlocal-restored"
                            | "list-first-iterable",
                        true
                    ) | (
                        "global-after-write" | "nonlocal-after-write" | "list-body",
                        false
                    )
                )
                && item.expected_facts.is_empty()
                && item.expected_scope.is_none()
                && item.expected_resolution.is_none()
                && (item.expected_present || item.expected_symbol.is_none())
        }
        _ => false,
    };
    if !fields_match {
        return Err(format!("{} has fields incompatible with its mode", item.id));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublicObservation {
    present: bool,
    original: Option<String>,
    replacement: Option<String>,
    operator: Option<String>,
    symbol: Option<String>,
}

fn public_contract(item: &OracleCase) -> (&'static str, &'static str, &'static str) {
    if item.scenario.starts_with("list-") {
        ("collection_list_tuple", "list", "tuple")
    } else {
        ("type_list_sequence", "list[str]", "Sequence[str]")
    }
}

fn expected_public_observation(item: &OracleCase) -> PublicObservation {
    let (operator, original, replacement) = public_contract(item);
    PublicObservation {
        present: item.expected_present,
        original: item.expected_present.then(|| original.to_owned()),
        replacement: item.expected_present.then(|| replacement.to_owned()),
        operator: item.expected_present.then(|| operator.to_owned()),
        symbol: item.expected_symbol.clone(),
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
            "[project]\nname = \"annotation-scope-case\"\nversion = \"0.0.0\"\n",
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

async fn public_plan_observation(item: &OracleCase) -> Result<PublicObservation, String> {
    let fixture = Fixture::new(&item.source)?;
    let (operator, original, _) = public_contract(item);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_io(plan_args(&fixture, operator), &mut stdout, &mut stderr),
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
    let manifest: PlanManifest = serde_json::from_slice(&stdout)
        .map_err(|error| format!("infrastructure-error: {} malformed plan: {error}", item.id))?;
    let marker_start = item
        .source
        .find(&item.marker)
        .ok_or_else(|| format!("infrastructure-error: {} marker missing", item.id))?;
    let marker_end = marker_start + item.marker.len();
    let matching = manifest
        .candidates
        .iter()
        .filter(|candidate| {
            let Ok(start) = usize::try_from(candidate.span.start) else {
                return false;
            };
            let Some(end_u64) = candidate.span.start.checked_add(candidate.span.length) else {
                return false;
            };
            let Ok(end) = usize::try_from(end_u64) else {
                return false;
            };
            candidate.operator == operator
                && candidate.original == original
                && item.source.get(start..end) == Some(candidate.original.as_str())
                && marker_start <= start
                && end <= marker_end
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(PublicObservation {
            present: false,
            original: None,
            replacement: None,
            operator: None,
            symbol: None,
        }),
        [candidate] => Ok(PublicObservation {
            present: true,
            original: Some(candidate.original.clone()),
            replacement: Some(candidate.replacement.clone()),
            operator: Some(candidate.operator.clone()),
            symbol: candidate.symbol.clone(),
        }),
        _ => Err(format!(
            "infrastructure-error: {} has {} matching candidates",
            item.id,
            matching.len()
        )),
    }
}

#[test]
fn annotation_scope_corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean annotation-scope corpus must be valid");
    assert_eq!(cases.len(), 23);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        15
    );
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 8);
}

#[test]
fn annotation_scope_corpus_rejects_crossed_scenarios_and_expectations() {
    let annotation = CORPUS.lines().next().expect("annotation case");
    let mut crossed: serde_json::Value = serde_json::from_str(annotation).unwrap();
    crossed["scenario"] = serde_json::json!("list-body");
    assert!(parse_corpus(&format!("{crossed}\n")).is_err());

    let resolution = CORPUS
        .lines()
        .find(|line| line.contains("list_first_iterable_resolution"))
        .expect("resolution case");
    let mut inconsistent: serde_json::Value = serde_json::from_str(resolution).unwrap();
    inconsistent["expected_present"] = serde_json::json!(false);
    assert!(parse_corpus(&format!("{inconsistent}\n")).is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn strict_public_annotation_scope_observations_match_lean() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    for item in cases.iter().filter(|item| item.mode == "strict") {
        let actual = public_plan_observation(item)
            .await
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        assert_eq!(
            actual,
            expected_public_observation(item),
            "same-premise mismatch for {}\nsource:\n{}",
            item.id,
            item.source
        );
    }
}
