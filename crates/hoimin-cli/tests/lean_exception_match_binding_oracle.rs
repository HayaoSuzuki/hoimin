use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str = include_str!(
    "../../../formal/HoiminOracle/corpus/exception-match-binding-correspondence.jsonl"
);

const CORPUS_FIELDS: [&str; 16] = [
    "expected_exit_category",
    "expected_facts",
    "expected_present",
    "expected_resolution",
    "family",
    "id",
    "marker",
    "mode",
    "name",
    "observation_kind",
    "operator",
    "original",
    "replacement",
    "schema",
    "source",
    "symbol",
];

const EXPECTED_CASE_KEYS: &[(&str, &str, &str, &str)] = &[
    (
        "handler_type_before_target",
        "internal-fixture",
        "handler",
        "annotation",
    ),
    (
        "handler_body_after_target",
        "internal-fixture",
        "handler",
        "annotation",
    ),
    (
        "handler_cleanup_fallthrough",
        "internal-fixture",
        "handler",
        "exits",
    ),
    (
        "handler_cleanup_return",
        "internal-fixture",
        "handler",
        "exits",
    ),
    (
        "handler_cleanup_raise",
        "internal-fixture",
        "handler",
        "exits",
    ),
    (
        "handler_cleanup_break",
        "internal-fixture",
        "handler",
        "exits",
    ),
    (
        "handler_cleanup_continue",
        "internal-fixture",
        "handler",
        "exits",
    ),
    (
        "handler_nonselected_join_observable",
        "internal-fixture",
        "handler",
        "annotation",
    ),
    (
        "handler_preserves_unrelated_import",
        "internal-fixture",
        "handler",
        "annotation",
    ),
    (
        "match_capture_body",
        "internal-fixture",
        "match-case",
        "annotation",
    ),
    (
        "match_partial_failure_observable",
        "internal-fixture",
        "match-case",
        "annotation",
    ),
    (
        "match_false_guard_next_case",
        "internal-fixture",
        "match-case",
        "annotation",
    ),
    (
        "match_refutable_unmatched_join",
        "internal-fixture",
        "match-case",
        "annotation",
    ),
    (
        "match_irrefutable_exhaustion",
        "internal-fixture",
        "match-case",
        "annotation",
    ),
    (
        "match_preserves_unrelated_import",
        "internal-fixture",
        "match-case",
        "annotation",
    ),
    (
        "handler_nonselected_join",
        "model-only",
        "handler",
        "resolution",
    ),
    (
        "match_partial_failure_next_case",
        "model-only",
        "match-case",
        "resolution",
    ),
    (
        "handler_body_after_target_public",
        "strict",
        "handler",
        "public-candidate",
    ),
    (
        "handler_cleanup_fallthrough_public",
        "strict",
        "handler",
        "public-candidate",
    ),
    (
        "handler_nonselected_join_public",
        "strict",
        "handler",
        "public-candidate",
    ),
    (
        "match_capture_body_public",
        "strict",
        "match-case",
        "public-candidate",
    ),
    (
        "match_partial_failure_next_case_public",
        "strict",
        "match-case",
        "public-candidate",
    ),
    (
        "match_false_guard_next_case_public",
        "strict",
        "match-case",
        "public-candidate",
    ),
    (
        "match_refutable_unmatched_join_public",
        "strict",
        "match-case",
        "public-candidate",
    ),
    (
        "match_irrefutable_exhaustion_public",
        "strict",
        "match-case",
        "public-candidate",
    ),
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    family: String,
    observation_kind: String,
    source: String,
    marker: String,
    name: String,
    expected_facts: Vec<String>,
    expected_resolution: Option<String>,
    expected_exit_category: Option<String>,
    expected_present: bool,
    operator: Option<String>,
    original: Option<String>,
    replacement: Option<String>,
    symbol: Option<String>,
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
        let actual_fields = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if actual_fields != expected_fields {
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
    let actual_keys = cases
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.mode.as_str(),
                item.family.as_str(),
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

fn validate_case(item: &OracleCase) -> Result<(), String> {
    let facts_are_normalized = item.expected_facts.windows(2).all(|pair| pair[0] < pair[1])
        && item.expected_facts.iter().all(|fact| {
            matches!(
                fact.as_str(),
                "direct:Sequence=typing.Sequence" | "direct:Mapping=typing.Mapping"
            )
        });
    if item.schema != 1
        || item.id.is_empty()
        || item.source.is_empty()
        || item.marker.is_empty()
        || item.name.is_empty()
        || item.source.match_indices(&item.marker).count() != 1
        || !matches!(
            item.mode.as_str(),
            "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
        )
        || !matches!(item.family.as_str(), "handler" | "match-case")
        || !matches!(
            item.observation_kind.as_str(),
            "annotation" | "exits" | "resolution" | "public-candidate"
        )
        || !facts_are_normalized
    {
        return Err(format!("{} violates the closed corpus schema", item.id));
    }

    let fields_match = match item.observation_kind.as_str() {
        "annotation" => {
            item.mode == "internal-fixture"
                && item.expected_resolution.is_none()
                && item.expected_exit_category.is_none()
                && item.operator.is_none()
                && item.original.is_none()
                && item.replacement.is_none()
                && item.symbol.is_none()
                && item.expected_present != item.expected_facts.is_empty()
        }
        "exits" => {
            item.mode == "internal-fixture"
                && item.expected_facts.is_empty()
                && matches!(
                    item.expected_exit_category.as_deref(),
                    Some("fallthrough" | "terminate" | "break" | "continue")
                )
                && item.expected_resolution.is_none()
                && !item.expected_present
                && item.operator.is_none()
                && item.original.is_none()
                && item.replacement.is_none()
                && item.symbol.is_none()
        }
        "resolution" => {
            item.mode == "model-only"
                && item.expected_facts.is_empty()
                && matches!(
                    item.expected_resolution.as_deref(),
                    Some("unknown" | "shadowed")
                )
                && item.expected_exit_category.is_none()
                && !item.expected_present
                && item.operator.is_none()
                && item.original.is_none()
                && item.replacement.is_none()
                && item.symbol.is_none()
        }
        "public-candidate" => {
            item.mode == "strict"
                && item.name == "Sequence"
                && item.expected_facts.is_empty()
                && item.expected_resolution.is_none()
                && item.expected_exit_category.is_none()
                && item.operator.as_deref() == Some("type_list_sequence")
                && item.original.as_deref() == Some("list[str]")
                && item.replacement.as_deref() == Some("Sequence[str]")
                && item.symbol.is_none()
        }
        _ => false,
    };
    if !fields_match {
        return Err(format!(
            "{} has fields incompatible with its mode and observation kind",
            item.id
        ));
    }
    Ok(())
}

fn decode_plan_result(
    case_id: &str,
    code: i32,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<PlanManifest, String> {
    if code != 0 || !stderr.is_empty() {
        return Err(format!(
            "infrastructure-error: {case_id} public plan exit={code}, stderr={}",
            String::from_utf8_lossy(stderr)
        ));
    }
    serde_json::from_slice(stdout)
        .map_err(|error| format!("infrastructure-error: {case_id} malformed plan: {error}"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublicObservation {
    count: usize,
    present: bool,
    operator: Option<String>,
    original: Option<String>,
    replacement: Option<String>,
    symbol: Option<String>,
}

fn expected_public_observation(item: &OracleCase) -> PublicObservation {
    PublicObservation {
        count: usize::from(item.expected_present),
        present: item.expected_present,
        operator: item
            .expected_present
            .then(|| item.operator.clone())
            .flatten(),
        original: item
            .expected_present
            .then(|| item.original.clone())
            .flatten(),
        replacement: item
            .expected_present
            .then(|| item.replacement.clone())
            .flatten(),
        symbol: item.symbol.clone(),
    }
}

fn normalize_manifest(
    item: &OracleCase,
    manifest: &PlanManifest,
) -> Result<PublicObservation, String> {
    let marker_offsets = item
        .source
        .match_indices(&item.marker)
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
        .checked_add(
            u64::try_from(item.marker.len())
                .map_err(|_| format!("infrastructure-error: {} marker length overflow", item.id))?,
        )
        .ok_or_else(|| format!("infrastructure-error: {} marker range overflow", item.id))?;

    let mut matching = Vec::new();
    for candidate in &manifest.candidates {
        let candidate_end = candidate
            .span
            .start
            .checked_add(candidate.span.length)
            .ok_or_else(|| format!("infrastructure-error: {} candidate range overflow", item.id))?;
        let exact_source = usize::try_from(candidate.span.start)
            .ok()
            .zip(usize::try_from(candidate_end).ok())
            .and_then(|(start, end)| item.source.get(start..end));
        if exact_source != Some(candidate.original.as_str()) {
            return Err(format!(
                "infrastructure-error: {} candidate span does not match its original",
                item.id
            ));
        }
        if candidate.span.start < marker_end && marker_start < candidate_end {
            matching.push(candidate);
        }
    }

    match matching.as_slice() {
        [] => Ok(PublicObservation {
            count: 0,
            present: false,
            operator: None,
            original: None,
            replacement: None,
            symbol: None,
        }),
        [candidate] => Ok(PublicObservation {
            count: 1,
            present: true,
            operator: Some(candidate.operator.clone()),
            original: Some(candidate.original.clone()),
            replacement: Some(candidate.replacement.clone()),
            symbol: candidate.symbol.clone(),
        }),
        _ => Ok(PublicObservation {
            count: matching.len(),
            present: true,
            operator: None,
            original: None,
            replacement: None,
            symbol: None,
        }),
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
            "[project]\nname = \"exception-match-binding-case\"\nversion = \"0.0.0\"\n",
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
    let operator = item
        .operator
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
    decode_plan_result(&item.id, code, &stdout, &stderr)
}

#[test]
fn corpus_accepts_the_exact_lean_schema_and_dispatch() {
    let cases = parse_corpus(CORPUS).expect("Lean exception/match corpus must be valid");
    assert_eq!(cases.len(), 25);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "internal-fixture")
            .count(),
        15
    );
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "model-only")
            .count(),
        2
    );
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 8);
    assert_eq!(
        cases
            .iter()
            .filter(|item| item.mode == "strict")
            .map(|item| item.observation_kind.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["public-candidate"])
    );
}

#[test]
fn corpus_rejects_unknown_fields_duplicate_ids_and_ambiguous_markers() {
    let first = CORPUS.lines().next().expect("first corpus row");

    let mut unknown: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(parse_corpus(&format!("{unknown}\n")).is_err());

    assert!(parse_corpus(&format!("{first}\n{first}\n")).is_err());

    let mut duplicate_marker: serde_json::Value = serde_json::from_str(first).unwrap();
    let source = duplicate_marker["source"].as_str().unwrap();
    let marker = duplicate_marker["marker"].as_str().unwrap();
    duplicate_marker["source"] = serde_json::json!(format!("{source}\n# {marker}"));
    assert!(parse_corpus(&format!("{duplicate_marker}\n")).is_err());

    let mut substituted = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut replacement: serde_json::Value = serde_json::from_str(&substituted[0]).unwrap();
    replacement["id"] = serde_json::json!("valid_but_unowned_case");
    substituted[0] = replacement.to_string();
    assert!(parse_corpus(&format!("{}\n", substituted.join("\n"))).is_err());
}

#[test]
fn corpus_rejects_unknown_enums_and_crossed_field_groups() {
    let first = CORPUS.lines().next().expect("first corpus row");
    for (field, value) in [
        ("mode", "speculative"),
        ("family", "finally"),
        ("observation_kind", "report"),
    ] {
        let mut invalid: serde_json::Value = serde_json::from_str(first).unwrap();
        invalid[field] = serde_json::json!(value);
        assert!(parse_corpus(&format!("{invalid}\n")).is_err());
    }

    let strict = CORPUS
        .lines()
        .find(|line| line.contains("handler_body_after_target_public"))
        .expect("strict public row");
    let mut crossed: serde_json::Value = serde_json::from_str(strict).unwrap();
    crossed["expected_resolution"] = serde_json::json!("shadowed");
    assert!(parse_corpus(&format!("{crossed}\n")).is_err());
}

#[test]
fn command_and_manifest_failures_are_infrastructure_errors() {
    let nonzero = decode_plan_result("nonzero", 7, b"{}", b"analyzer failed").unwrap_err();
    assert!(nonzero.starts_with("infrastructure-error: nonzero public plan exit=7"));

    let malformed = decode_plan_result("malformed", 0, b"not JSON", b"").unwrap_err();
    assert!(malformed.starts_with("infrastructure-error: malformed malformed plan:"));
}

#[tokio::test(flavor = "current_thread")]
async fn strict_public_exception_match_observations_match_lean() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    for item in cases.iter().filter(|item| item.mode == "strict") {
        let manifest = public_plan(item)
            .await
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        let actual = normalize_manifest(item, &manifest)
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
async fn overlapping_multiple_candidates_are_a_semantic_mismatch() {
    let mut item = parse_corpus(CORPUS)
        .expect("valid Lean corpus")
        .into_iter()
        .find(|item| item.mode == "strict" && item.expected_present)
        .expect("positive strict row");
    let mut manifest = public_plan(&item).await.expect("public plan");
    let candidate = manifest
        .candidates
        .iter()
        .find(|candidate| candidate.original == "list[str]")
        .expect("marker candidate")
        .clone();
    manifest.candidates.push(candidate);

    let actual = normalize_manifest(&item, &manifest)
        .expect("multiple candidates are an observation, not infrastructure");
    assert_eq!(actual.count, 2);
    assert_ne!(actual, expected_public_observation(&item));

    item.source.push_str("\n# ");
    item.source.push_str(&item.marker);
    let marker_error = normalize_manifest(&item, &manifest).unwrap_err();
    assert!(marker_error.starts_with("infrastructure-error:"));
}
