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
    expected_operator: Option<String>,
    expected_original: Option<String>,
    expected_replacement: Option<String>,
    expected_present: bool,
}

const EXPECTED_CASE_KEYS: &[(&str, &str, &str, &str)] = &[
    (
        "global_before_site",
        "internal-fixture",
        "global-before",
        "annotation",
    ),
    (
        "global_after_write_site",
        "internal-fixture",
        "global-after-write",
        "annotation",
    ),
    (
        "global_restored_site",
        "internal-fixture",
        "global-restored",
        "annotation",
    ),
    (
        "global_unaffected_function_site",
        "internal-fixture",
        "global-unaffected",
        "annotation",
    ),
    (
        "nonlocal_before_site",
        "internal-fixture",
        "nonlocal-before",
        "annotation",
    ),
    (
        "nonlocal_after_write_site",
        "internal-fixture",
        "nonlocal-after-write",
        "annotation",
    ),
    (
        "nonlocal_restored_site",
        "internal-fixture",
        "nonlocal-restored",
        "annotation",
    ),
    (
        "nonlocal_unaffected_module_site",
        "internal-fixture",
        "nonlocal-unaffected",
        "annotation",
    ),
    (
        "class_global_site",
        "internal-fixture",
        "class-global",
        "annotation",
    ),
    (
        "class_nonlocal_site",
        "internal-fixture",
        "class-nonlocal",
        "annotation",
    ),
    (
        "class_global_unaffected_function_site",
        "internal-fixture",
        "class-global-unaffected",
        "annotation",
    ),
    (
        "class_nonlocal_unaffected_module_site",
        "internal-fixture",
        "class-nonlocal-unaffected",
        "annotation",
    ),
    (
        "list_outer_before_site",
        "internal-fixture",
        "list-outer-before",
        "annotation",
    ),
    (
        "list_outer_after_site",
        "internal-fixture",
        "list-outer-after",
        "annotation",
    ),
    (
        "list_after_resolution",
        "internal-fixture",
        "list-after",
        "resolution",
    ),
    (
        "list_first_iterable_resolution",
        "internal-fixture",
        "list-first-iterable",
        "resolution",
    ),
    (
        "list_body_resolution",
        "internal-fixture",
        "list-body",
        "resolution",
    ),
    (
        "set_body_resolution",
        "internal-fixture",
        "set-body",
        "resolution",
    ),
    (
        "dict_body_resolution",
        "internal-fixture",
        "dict-body",
        "resolution",
    ),
    (
        "generator_body_resolution",
        "internal-fixture",
        "generator-body",
        "resolution",
    ),
    (
        "global_before_public",
        "strict",
        "global-before",
        "public-candidate",
    ),
    (
        "global_after_write_public",
        "strict",
        "global-after-write",
        "public-candidate",
    ),
    (
        "global_restored_public",
        "strict",
        "global-restored",
        "public-candidate",
    ),
    (
        "nonlocal_before_public",
        "strict",
        "nonlocal-before",
        "public-candidate",
    ),
    (
        "nonlocal_after_write_public",
        "strict",
        "nonlocal-after-write",
        "public-candidate",
    ),
    (
        "nonlocal_restored_public",
        "strict",
        "nonlocal-restored",
        "public-candidate",
    ),
    (
        "list_after_public",
        "strict",
        "list-after",
        "public-candidate",
    ),
    (
        "list_first_iterable_public",
        "strict",
        "list-first-iterable",
        "public-candidate",
    ),
    (
        "list_body_public",
        "strict",
        "list-body",
        "public-candidate",
    ),
];

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
    let actual_keys = cases
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.mode.as_str(),
                item.scenario.as_str(),
                item.observation_kind.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    let expected_keys = EXPECTED_CASE_KEYS.iter().copied().collect::<BTreeSet<_>>();
    if actual_keys != expected_keys {
        return Err("corpus does not contain the exact schema-owned case set".to_owned());
    }
    Ok(cases)
}

fn common_fields_are_valid(item: &OracleCase) -> bool {
    const SCENARIOS: [&str; 20] = [
        "global-before",
        "global-after-write",
        "global-restored",
        "global-unaffected",
        "nonlocal-before",
        "nonlocal-after-write",
        "nonlocal-restored",
        "nonlocal-unaffected",
        "class-global",
        "class-nonlocal",
        "class-global-unaffected",
        "class-nonlocal-unaffected",
        "list-outer-before",
        "list-outer-after",
        "list-after",
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
                        | "global-unaffected"
                        | "nonlocal-before"
                        | "nonlocal-after-write"
                        | "nonlocal-restored"
                        | "nonlocal-unaffected"
                        | "class-global"
                        | "class-nonlocal"
                        | "class-global-unaffected"
                        | "class-nonlocal-unaffected"
                        | "list-outer-before"
                        | "list-outer-after"
                )
                && matches!(
                    (
                        item.expected_scope.as_deref(),
                        item.expected_symbol.as_deref()
                    ),
                    (Some("function"), Some(_)) | (Some("module"), None)
                )
                && item.expected_resolution.is_none()
                && item.expected_operator.is_none()
                && item.expected_original.is_none()
                && item.expected_replacement.is_none()
                && item.expected_present != item.expected_facts.is_empty()
        }
        "resolution" => {
            item.mode == "internal-fixture"
                && matches!(
                    item.scenario.as_str(),
                    "list-after"
                        | "list-first-iterable"
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
                && item.expected_operator.is_none()
                && item.expected_original.is_none()
                && item.expected_replacement.is_none()
                && item.expected_present
                    == (item.expected_resolution.as_deref() == Some("definitely-builtin"))
        }
        "public-candidate" => {
            item.mode == "strict"
                && item.expected_facts.is_empty()
                && item.expected_scope.is_none()
                && item.expected_resolution.is_none()
                && item.expected_operator.is_some()
                && item.expected_original.is_some()
                && item.expected_replacement.is_some()
                && item.expected_present == item.expected_symbol.is_some()
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
    count: usize,
    present: bool,
    original: Option<String>,
    replacement: Option<String>,
    operator: Option<String>,
    symbol: Option<String>,
}

fn expected_public_observation(item: &OracleCase) -> PublicObservation {
    PublicObservation {
        count: usize::from(item.expected_present),
        present: item.expected_present,
        original: if item.expected_present {
            item.expected_original.clone()
        } else {
            None
        },
        replacement: if item.expected_present {
            item.expected_replacement.clone()
        } else {
            None
        },
        operator: if item.expected_present {
            item.expected_operator.clone()
        } else {
            None
        },
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
    let operator = item
        .expected_operator
        .as_deref()
        .ok_or_else(|| format!("infrastructure-error: {} operator missing", item.id))?;
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
            start < marker_end && marker_start < end
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(PublicObservation {
            count: 0,
            present: false,
            original: None,
            replacement: None,
            operator: None,
            symbol: None,
        }),
        [candidate] => Ok(PublicObservation {
            count: 1,
            present: true,
            original: Some(candidate.original.clone()),
            replacement: Some(candidate.replacement.clone()),
            operator: Some(candidate.operator.clone()),
            symbol: candidate.symbol.clone(),
        }),
        _ => Ok(PublicObservation {
            count: matching.len(),
            present: true,
            original: None,
            replacement: None,
            operator: None,
            symbol: None,
        }),
    }
}

#[test]
fn annotation_scope_corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean annotation-scope corpus must be valid");
    assert_eq!(cases.len(), 29);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        20
    );
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 9);
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

    let mut substituted = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut replacement: serde_json::Value = serde_json::from_str(&substituted[0]).unwrap();
    replacement["id"] = serde_json::json!("allowed_but_unowned_case");
    substituted[0] = replacement.to_string();
    assert!(parse_corpus(&format!("{}\n", substituted.join("\n"))).is_err());
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

#[tokio::test(flavor = "current_thread")]
async fn negative_observation_cannot_hide_a_candidate_with_unexpected_identity() {
    let mut item = parse_corpus(CORPUS)
        .expect("valid Lean corpus")
        .into_iter()
        .find(|item| item.id == "global_before_public")
        .expect("positive public fixture");
    item.expected_present = false;
    item.expected_symbol = None;
    item.expected_original = Some("deliberately-wrong-original".to_owned());

    let actual = public_plan_observation(&item).await.unwrap();
    assert!(actual.present);
    assert_ne!(actual, expected_public_observation(&item));
}
