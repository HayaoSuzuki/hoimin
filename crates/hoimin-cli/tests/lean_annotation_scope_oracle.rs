#![cfg_attr(coverage_nightly, feature(coverage_attribute, stmt_expr_attributes))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

#[cfg(windows)]
const VENV_PYTHON_PATH: &str = ".venv/Scripts/python.exe";
#[cfg(not(windows))]
const VENV_PYTHON_PATH: &str = ".venv/bin/python";

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
        _ => {
            // common_fields_are_valid already rejected every other observation kind.
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
        Self::from_directory(tempfile::tempdir(), source)
    }

    fn from_directory(
        directory: std::io::Result<tempfile::TempDir>,
        source: &str,
    ) -> Result<Self, String> {
        let directory = directory.map_err(|error| error.to_string())?;
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
    repo_root().join(VENV_PYTHON_PATH)
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
    normalize_manifest(item, &public_plan(item).await?)
}

async fn public_plan(item: &OracleCase) -> Result<PlanManifest, String> {
    public_plan_from_fixture(item, Fixture::new(&item.source)).await
}

async fn public_plan_from_fixture(
    item: &OracleCase,
    fixture: Result<Fixture, String>,
) -> Result<PlanManifest, String> {
    let fixture = fixture?;
    let operator = item
        .expected_operator
        .as_deref()
        .ok_or_else(|| format!("infrastructure-error: {} operator missing", item.id))?;
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_writers(
            plan_args(&fixture, operator),
            stdout.clone(),
            stderr.clone(),
        ),
    )
    .await;
    decode_public_plan(item, result, &stdout.snapshot(), &stderr.snapshot())
}

fn decode_public_plan(
    item: &OracleCase,
    result: Result<i32, tokio::time::error::Elapsed>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<PlanManifest, String> {
    let code =
        result.map_err(|_| format!("infrastructure-error: {} public plan timed out", item.id))?;
    if code != 0 || !stderr.is_empty() {
        return Err(format!(
            "infrastructure-error: {} public plan exit={code}, stderr={}",
            item.id,
            String::from_utf8_lossy(stderr)
        ));
    }
    serde_json::from_slice(stdout)
        .map_err(|error| format!("infrastructure-error: {} malformed plan: {error}", item.id))
}

fn normalize_manifest(
    item: &OracleCase,
    manifest: &PlanManifest,
) -> Result<PublicObservation, String> {
    let marker_start = item
        .source
        .find(&item.marker)
        .ok_or_else(|| format!("infrastructure-error: {} marker missing", item.id))?;
    let marker_end = marker_start + item.marker.len();
    let matching = manifest
        .candidates
        .iter()
        .filter(|candidate| {
            #[cfg(target_pointer_width = "32")]
            let Ok(start) = usize::try_from(candidate.span.start) else {
                return false;
            };
            #[cfg(target_pointer_width = "64")]
            let start = usize::try_from(candidate.span.start).expect("u64 fits 64-bit usize");
            let Some(end_u64) = candidate.span.start.checked_add(candidate.span.length) else {
                return false;
            };
            #[cfg(target_pointer_width = "32")]
            let Ok(end) = usize::try_from(end_u64) else {
                return false;
            };
            #[cfg(target_pointer_width = "64")]
            let end = usize::try_from(end_u64).expect("u64 fits 64-bit usize");
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
        let actual = public_plan_observation(item).await.unwrap_or_else(
            // Missing public observations abort the correspondence assertion.
            #[cfg_attr(coverage_nightly, coverage(off))]
            |error| panic!("{error}\nsource:\n{}", item.source),
        );
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

#[rstest::rstest]
#[case::schema("/schema", serde_json::json!(2))]
#[case::identity("/id", serde_json::json!(""))]
#[case::source("/source", serde_json::json!(""))]
#[case::marker("/marker", serde_json::json!(""))]
#[case::scenario("/scenario", serde_json::json!("unknown"))]
#[case::mode("/mode", serde_json::json!("unknown"))]
#[case::observation("/observation_kind", serde_json::json!("unknown"))]
#[case::facts("/expected_facts", serde_json::json!(["unowned-fact"]))]
fn annotation_schema_rejects_one_invalid_coordinate(
    #[case] pointer: &str,
    #[case] value: serde_json::Value,
) {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    *row.pointer_mut(pointer).unwrap() = value;

    let actual_0 = parse_corpus(&row.to_string());

    assert!(
        actual_0
            .unwrap_err()
            .contains("violates the closed corpus schema")
    );
}

#[rstest::rstest]
#[case::empty(&String::new(), "corpus contains no cases")]
#[case::invalid(&"{".to_owned(), "line 1 is invalid")]
#[case::duplicate(&format!("{CORPUS}\n{}", CORPUS.lines().next().unwrap()), "duplicate case id")]
fn annotation_parser_reports_invalid_corpus(#[case] input: &str, #[case] diagnostic: &str) {
    assert!(parse_corpus(input).unwrap_err().contains(diagnostic));
}

#[test]
fn annotation_parser_ignores_blank_lines() {
    let actual_0 = parse_corpus(&format!("\n{CORPUS}\n"));

    assert_eq!(actual_0.unwrap().len(), 29);
}

#[tokio::test]
async fn annotation_public_plan_reports_missing_operator() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);

    let actual_0 = public_plan_observation(&item).await;

    assert!(actual_0.unwrap_err().contains("operator missing"));
}

#[test]
fn annotation_fixture_reports_tempdir_failure() {
    let result = Fixture::from_directory(Err(std::io::Error::other("denied")), "pass");

    assert_eq!(result.err().unwrap(), "denied");
}

#[test]
fn annotation_fixture_reports_source_directory_failure() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("project")).unwrap();
    std::fs::write(directory.path().join("project/src"), "obstruction").unwrap();

    let actual_0 = Fixture::from_directory(Ok(directory), "pass");

    assert!(actual_0.is_err());
}

#[rstest::rstest]
#[case::source("src/case.py")]
#[case::metadata("pyproject.toml")]
fn annotation_fixture_reports_unwritable_file(#[case] path: &str) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project").join(path)).unwrap();

    let actual_0 = Fixture::from_directory(Ok(directory), "pass");

    assert!(actual_0.is_err());
}

#[rstest::rstest]
#[case::exit(2, b"failed", "exit=2, stderr=failed")]
#[case::stderr(0, b"warning", "exit=0, stderr=warning")]
#[case::malformed(0, b"", "malformed plan")]
fn annotation_decoder_reports_invalid_public_plan(
    #[case] code: i32,
    #[case] stderr: &[u8],
    #[case] diagnostic: &str,
) {
    let item = parse_corpus(CORPUS).unwrap().remove(0);

    let actual_0 = decode_public_plan(&item, Ok(code), b"{", stderr);

    assert!(actual_0.unwrap_err().contains(diagnostic));
}

#[tokio::test(start_paused = true)]
async fn annotation_decoder_reports_timeout() {
    let item = parse_corpus(CORPUS).unwrap().remove(0);
    let result =
        tokio::time::timeout(Duration::from_millis(1), std::future::pending::<i32>()).await;

    let actual_0 = decode_public_plan(&item, result, b"", b"");

    assert!(actual_0.unwrap_err().contains("public plan timed out"));
}

fn strict_annotation_case() -> OracleCase {
    parse_corpus(CORPUS)
        .unwrap()
        .into_iter()
        .find(|item| item.id == "global_before_public")
        .unwrap()
}

#[tokio::test]
async fn annotation_normalizer_reports_missing_marker() {
    let mut item = strict_annotation_case();
    let manifest = public_plan(&item).await.unwrap();
    item.marker = "absent marker".to_owned();

    let actual_0 = normalize_manifest(&item, &manifest);

    assert!(actual_0.unwrap_err().contains("marker missing"));
}

#[tokio::test]
async fn annotation_normalizer_retains_overlapping_candidate_count() {
    let item = strict_annotation_case();
    let mut manifest = public_plan(&item).await.unwrap();
    manifest.candidates.push(manifest.candidates[0].clone());

    let observation = normalize_manifest(&item, &manifest).unwrap();

    assert_eq!(observation.count, 2);
    assert!(observation.present);
    assert_eq!(observation.original, None);
}

#[tokio::test]
async fn annotation_normalizer_excludes_overflowing_candidate_span() {
    let item = strict_annotation_case();
    let mut manifest = public_plan(&item).await.unwrap();
    manifest.candidates[0].candidate.span.start = u64::MAX;
    manifest.candidates[0].candidate.span.length = 1;

    let actual_0 = normalize_manifest(&item, &manifest);

    assert!(!actual_0.unwrap().present);
}

#[tokio::test]
async fn annotation_public_plan_preserves_fixture_failure() {
    let item = strict_annotation_case();

    let actual_0 = public_plan_from_fixture(&item, Err("fixture denied".into())).await;

    assert_eq!(actual_0.unwrap_err(), "fixture denied");
}
