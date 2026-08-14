use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/except-star-flow.jsonl");

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
    ModelWitness,
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
    expected_exits: serde_json::Value,
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

fn strict_source(id: &str) -> Option<&'static str> {
    match id {
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

fn strict_cases_from(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        if item.schema != 1
            || !item.expected_exits.is_object()
            || !matches!(
                item.mode.as_str(),
                "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
            )
        {
            return Err(format!("{} has an invalid public fixture", item.id));
        }
        if item.mode != "strict" {
            continue;
        }
        if item.source.matches(&item.marker).count() != 1 {
            return Err(format!("{} has a non-unique marker", item.id));
        }
        let owned = match item.id.as_str() {
            "starred_public_candidate_present" => {
                item.observation_kind == ObservationKind::PublicCandidate
                    && item.family == "collapsed-summary"
                    && item.marker == "Sequence[int]"
                    && item.candidate == CandidateExpectation::Present
                    && item.candidate_count == 1
                    && item.candidate_path.as_deref() == Some("target.py")
                    && item.candidate_start == Some(166)
                    && item.candidate_length == Some(13)
                    && item.candidate_operator.as_deref() == Some("type_list_sequence")
                    && item.candidate_original.as_deref() == Some("Sequence[int]")
                    && item.candidate_replacement.as_deref() == Some("list[int]")
                    && item.candidate_symbol.is_none()
            }
            "starred_public_candidate_absent" => {
                item.observation_kind == ObservationKind::PublicCandidate
                    && item.family == "collapsed-summary"
                    && item.marker == "Sequence[int]"
                    && item.candidate == CandidateExpectation::Absent
                    && item.candidate_count == 0
                    && item.candidate_path.is_none()
                    && item.candidate_start.is_none()
                    && item.candidate_length.is_none()
                    && item.candidate_operator.is_none()
                    && item.candidate_original.is_none()
                    && item.candidate_replacement.is_none()
                    && item.candidate_symbol.is_none()
            }
            _ => false,
        };
        if !owned {
            return Err(format!("{} is not an owned strict fixture", item.id));
        }
        if strict_source(&item.id) != Some(item.source.as_str()) {
            return Err(format!("{} has changed source premises", item.id));
        }
        cases.push(item);
    }
    if cases.len() != 2 {
        return Err(format!("expected two strict rows, found {}", cases.len()));
    }
    Ok(cases)
}

#[test]
fn strict_parser_rejects_modes_sources_and_spans_that_change_the_premise() {
    let original = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();

    let mut unknown_rows = original.clone();
    let mut unknown: serde_json::Value = serde_json::from_str(&unknown_rows[0]).unwrap();
    unknown["mode"] = serde_json::json!("future-mode");
    unknown_rows[0] = unknown.to_string();
    assert!(
        strict_cases_from(&unknown_rows.join("\n"))
            .unwrap_err()
            .contains("invalid public fixture")
    );

    let mut changed_rows = original.clone();
    let mut changed: serde_json::Value = serde_json::from_str(&changed_rows[6]).unwrap();
    changed["source"] = serde_json::json!(
        changed["source"]
            .as_str()
            .unwrap()
            .replace("work()", "other_work()")
    );
    changed_rows[6] = changed.to_string();
    assert!(
        strict_cases_from(&changed_rows.join("\n"))
            .unwrap_err()
            .contains("changed source premises")
    );

    let mut span_rows = original;
    let mut bad_span: serde_json::Value = serde_json::from_str(&span_rows[6]).unwrap();
    bad_span["candidate_start"] = serde_json::json!(167);
    span_rows[6] = bad_span.to_string();
    assert!(
        strict_cases_from(&span_rows.join("\n"))
            .unwrap_err()
            .contains("not an owned strict fixture")
    );
}

fn selected_cases(cases: Vec<OracleCase>) -> Result<Vec<OracleCase>, String> {
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
            "HOIMIN_EXCEPT_STAR_CASE={selected} selected {} strict rows",
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
        let directory = tempfile::tempdir().map_err(|error| {
            format!("infrastructure-error: {} fixture tempdir: {error}", item.id)
        })?;
        let root = directory.path().join("project");
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("infrastructure-error: {} fixture root: {error}", item.id))?;
        std::fs::write(root.join("target.py"), &item.source).map_err(|error| {
            format!("infrastructure-error: {} fixture source: {error}", item.id)
        })?;
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
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_io(plan_args(&fixture), &mut stdout, &mut stderr),
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

fn normalize_manifest(
    item: &OracleCase,
    manifest: &PlanManifest,
) -> Result<PublicObservation, String> {
    let marker_start = item
        .source
        .find(&item.marker)
        .ok_or_else(|| format!("infrastructure-error: {} marker missing", item.id))?;
    let marker_end = marker_start
        .checked_add(item.marker.len())
        .ok_or_else(|| format!("infrastructure-error: {} marker range overflow", item.id))?;
    let mut matching = Vec::new();
    for candidate in &manifest.candidates {
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
                "infrastructure-error: {} candidate span disagrees with original",
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
    match item.candidate {
        CandidateExpectation::Absent | CandidateExpectation::Present => PublicObservation {
            count: item.candidate_count,
            path: item.candidate_path.clone(),
            start: item.candidate_start,
            length: item.candidate_length,
            operator: item.candidate_operator.clone(),
            original: item.candidate_original.clone(),
            replacement: item.candidate_replacement.clone(),
            symbol: item.candidate_symbol.clone(),
        },
        CandidateExpectation::NotObserved => panic!("non-strict row cannot be public"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn strict_starred_rows_match_complete_public_candidate_observations() {
    let cases = selected_cases(strict_cases_from(CORPUS).expect("valid strict except-star corpus"))
        .expect("single-case filter must select a strict identity");
    for item in cases {
        let manifest = public_plan(&item)
            .await
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        let actual = normalize_manifest(&item, &manifest)
            .unwrap_or_else(|error| panic!("{error}\nsource:\n{}", item.source));
        assert_eq!(
            actual,
            expected_public(&item),
            "same-premise mismatch for {}\nsource:\n{}",
            item.id,
            item.source
        );
    }
}
