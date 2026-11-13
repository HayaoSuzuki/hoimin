#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/public_oracle_fixture.rs"]
mod public_oracle_fixture;
use collecting_writer::CollectingWriter;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/compound-pattern-guards.jsonl");

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

const STRICT_IDS: [&str; 3] = [
    "as_failure_public_candidate",
    "mapping_failure_public_candidate",
    "class_failure_public_candidate",
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

fn strict_source(id: &str) -> Option<&'static str> {
    match id {
        "as_failure_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "match value:\n",
            "    case [0] as Sequence:\n",
            "        pass\n",
            "    case _:\n",
            "        as_failure_marker: list[int]\n",
        )),
        "mapping_failure_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "match value:\n",
            "    case {\"tag\": 0, **Sequence}:\n",
            "        pass\n",
            "    case _:\n",
            "        mapping_failure_marker: list[int]\n",
        )),
        "class_failure_public_candidate" => Some(concat!(
            "from typing import Sequence\n",
            "match value:\n",
            "    case Point(0, tail=Sequence):\n",
            "        pass\n",
            "    case _:\n",
            "        class_failure_marker: list[int]\n",
        )),
        _ => None,
    }
}

fn strict_cases_from(input: &str) -> Result<Vec<OracleCase>, String> {
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
        if item.schema != 1
            || !matches!(
                item.mode.as_str(),
                "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
            )
        {
            return Err(format!("{} has invalid identity or mode", item.id));
        }
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        if item.mode != "strict" {
            continue;
        }
        let marker_start = item.source.find(&item.marker);
        let valid = STRICT_IDS.contains(&item.id.as_str())
            && item.observation_kind == "public_candidate"
            && matches!(
                item.family.as_str(),
                "as-binding-point" | "mapping-rest-binding-point" | "class-capture-binding-point"
            )
            && strict_source(&item.id) == Some(item.source.as_str())
            && item.source.match_indices(&item.marker).count() == 1
            && item.marker == "list[int]"
            && item.expected_facts.is_empty()
            && item.candidate_count == 1
            && item.candidate_path.as_deref() == Some("target.py")
            && item.candidate_start == marker_start.and_then(|start| u64::try_from(start).ok())
            && item.candidate_length == Some(9)
            && item.candidate_operator.as_deref() == Some("type_list_sequence")
            && item.candidate_original.as_deref() == Some("list[int]")
            && item.candidate_replacement.as_deref() == Some("Sequence[int]")
            && item.candidate_symbol.is_none();
        if !valid {
            return Err(format!("{} is not an owned strict fixture", item.id));
        }
        cases.push(item);
    }
    let actual = cases
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = STRICT_IDS.into_iter().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err("corpus does not contain the exact strict case set".to_owned());
    }
    Ok(cases)
}

fn strict_cases() -> Result<Vec<OracleCase>, String> {
    strict_cases_from(CORPUS)
}

fn selected_cases(cases: Vec<OracleCase>) -> Result<Vec<OracleCase>, String> {
    selected_cases_from(cases, std::env::var_os("HOIMIN_COMPOUND_PATTERN_CASE"))
}

fn selected_cases_from(
    cases: Vec<OracleCase>,
    selected: Option<OsString>,
) -> Result<Vec<OracleCase>, String> {
    let Some(selected) = selected else {
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
            "HOIMIN_COMPOUND_PATTERN_CASE={selected} selected {} strict rows",
            matching.len()
        ));
    }
    Ok(matching)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublicObservation {
    count: usize,
    path: Option<String>,
    start: Option<u64>,
    length: Option<u64>,
    operator: Option<String>,
    original: Option<String>,
    replacement: Option<String>,
    symbol: Option<String>,
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new(item: &OracleCase) -> Result<Self, String> {
        let (directory, root) =
            public_oracle_fixture::create(tempfile::tempdir(), &item.id, &item.source)?;
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
    #[cfg(windows)]
    {
        repo_root().join(".venv/Scripts/python.exe")
    }
    #[cfg(not(windows))]
    {
        repo_root().join(".venv/bin/python")
    }
}

fn plan_args(fixture: &Fixture) -> Vec<OsString> {
    vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        fixture.root.as_os_str().to_owned(),
        "--file".into(),
        "target.py".into(),
        "--operators".into(),
        "type_list_sequence".into(),
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
    let fixture = Fixture::new(item)?;
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_writers(plan_args(&fixture), stdout.clone(), stderr.clone()),
    )
    .await;
    decode_public_plan(item, outcome, &stdout.snapshot(), &stderr.snapshot())
}

fn decode_public_plan(
    item: &OracleCase,
    outcome: Result<i32, tokio::time::error::Elapsed>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<PlanManifest, String> {
    let code =
        outcome.map_err(|_| format!("infrastructure-error: {} public plan timed out", item.id))?;
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
    // `find` proves this complete marker ends within the source string.
    let marker_end = marker_start + item.marker.len();
    let mut matching = Vec::new();
    for candidate in &manifest.candidates {
        // Every u64 is representable on a 64-bit host; narrower hosts retain fallible validation.
        #[cfg(target_pointer_width = "64")]
        let start = usize::try_from(candidate.span.start).expect("u64 fits 64-bit usize");
        #[cfg(not(target_pointer_width = "64"))]
        let start = usize::try_from(candidate.span.start)
            .map_err(|_| format!("infrastructure-error: {} span start overflow", item.id))?;
        let end = candidate
            .span
            .start
            .checked_add(candidate.span.length)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("infrastructure-error: {} span end overflow", item.id))?;
        if item.source.get(start..end) != Some(candidate.original.as_str()) {
            return Err(format!(
                "infrastructure-error: {} candidate span disagrees with source",
                item.id
            ));
        }
        if start < marker_end && marker_start < end {
            matching.push(candidate);
        }
    }
    match matching.as_slice() {
        [] => Ok(PublicObservation {
            count: 0,
            path: None,
            start: None,
            length: None,
            operator: None,
            original: None,
            replacement: None,
            symbol: None,
        }),
        [candidate] => Ok(PublicObservation {
            count: 1,
            path: Some(candidate.path.to_string()),
            start: Some(candidate.span.start),
            length: Some(candidate.span.length),
            operator: Some(candidate.operator.clone()),
            original: Some(candidate.original.clone()),
            replacement: Some(candidate.replacement.clone()),
            symbol: candidate.symbol.clone(),
        }),
        candidates => Err(format!(
            "same-premise mismatch: {} marker overlaps {} candidates",
            item.id,
            candidates.len()
        )),
    }
}

fn expected_public(item: &OracleCase) -> PublicObservation {
    PublicObservation {
        count: item.candidate_count,
        path: item.candidate_path.clone(),
        start: item.candidate_start,
        length: item.candidate_length,
        operator: item.candidate_operator.clone(),
        original: item.candidate_original.clone(),
        replacement: item.candidate_replacement.clone(),
        symbol: item.candidate_symbol.clone(),
    }
}

#[test]
fn strict_parser_rejects_unknown_fields_and_changed_spans() {
    let mut rows = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut unknown: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    unknown["future"] = serde_json::json!(true);
    rows[0] = unknown.to_string();
    assert!(
        strict_cases_from(&rows.join("\n"))
            .unwrap_err()
            .contains("inexact field set")
    );

    let mut rows = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut changed: serde_json::Value = serde_json::from_str(&rows[7]).unwrap();
    changed["candidate_start"] = serde_json::json!(120);
    rows[7] = changed.to_string();
    assert!(
        strict_cases_from(&rows.join("\n"))
            .unwrap_err()
            .contains("not an owned strict fixture")
    );
}

#[tokio::test]
async fn strict_rows_match_complete_public_candidates() {
    for item in selected_cases(strict_cases().unwrap()).unwrap() {
        let manifest = public_plan(&item).await.unwrap_or_else(
            // The checked-in owned strict rows must satisfy correspondence; this only diagnoses a failing assertion.
            #[cfg_attr(coverage_nightly, coverage(off))]
            |error| panic!("{error}\nsource:\n{}", item.source),
        );
        let actual = normalize_manifest(&item, &manifest).unwrap();
        assert_eq!(
            actual,
            expected_public(&item),
            "same-premise mismatch for {}",
            item.id
        );
    }
}

#[path = "support/public_oracle_validation_tests.rs"]
mod validation_tests;

#[test]
fn parser_rejects_shape_schema_identity_and_duplicate_rows() {
    assert!(
        strict_cases_from("[]")
            .unwrap_err()
            .contains("not an object")
    );
    let rows = CORPUS
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let index = rows.iter().position(|row| row["mode"] == "strict").unwrap();
    for (field, value, expected) in [
        ("schema", serde_json::json!("wrong"), "violates schema"),
        ("schema", serde_json::json!(2), "invalid identity or mode"),
        (
            "mode",
            serde_json::json!("unknown"),
            "invalid identity or mode",
        ),
        (
            "id",
            serde_json::json!("unknown"),
            "not an owned strict fixture",
        ),
    ] {
        let mut changed = rows.clone();
        changed[index][field] = value;
        let input = changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(strict_cases_from(&input).unwrap_err().contains(expected));
    }
    let duplicate = format!("{}\n{}", rows[index], rows[index]);
    assert!(
        strict_cases_from(&duplicate)
            .unwrap_err()
            .contains("duplicate case id")
    );
    assert!(strict_cases_from("\n").is_err());
}
