use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/nested-match-exits.jsonl");

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
    LoopHead,
    PublicCandidate,
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
    expected_facts: Vec<String>,
    candidate: CandidateExpectation,
    candidate_count: usize,
    candidate_path: Option<String>,
    candidate_operator: Option<String>,
    candidate_original: Option<String>,
    candidate_replacement: Option<String>,
    candidate_symbol: Option<String>,
}

fn strict_source(id: &str) -> Option<&'static str> {
    match id {
        "nested_continue_suppresses_public_candidate" => Some(
            "from typing import Sequence\n\nwhile condition:  # nested_continue_loop\n    match value:\n        case 0:\n            observed: Sequence[int]\n            Sequence = object\n            continue\n        case _:\n            break\n",
        ),
        "post_loop_meets_break_and_natural_exit" => Some(
            "from typing import Sequence\n\nwhile condition:\n    match value:\n        case 0:\n            Sequence = object\n            break\n        case _:\n            continue\nelse:\n    from typing import Sequence\n\npost_loop: Sequence[int]\n",
        ),
        _ => None,
    }
}

fn strict_cases_from(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        if item.schema != 1
            || item.source.matches(&item.marker).count() != 1
            || !item.expected_exits.is_object()
            || !item.expected_facts.is_empty()
        {
            return Err(format!("{} has an invalid public fixture", item.id));
        }
        if !matches!(
            item.mode.as_str(),
            "strict" | "internal-fixture" | "model-only" | "infrastructure-error"
        ) {
            return Err(format!("{} has an unknown mode", item.id));
        }
        if item.mode == "strict" {
            let owned = match item.id.as_str() {
                "nested_continue_suppresses_public_candidate" => {
                    item.observation_kind == ObservationKind::PublicCandidate
                        && item.family == "loop-back-edge"
                        && item.marker == "Sequence[int]"
                        && item.candidate == CandidateExpectation::Absent
                }
                "post_loop_meets_break_and_natural_exit" => {
                    item.observation_kind == ObservationKind::PublicCandidate
                        && item.family == "loop-consumption"
                        && item.marker == "Sequence[int]"
                        && item.candidate == CandidateExpectation::Absent
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
    }
    if cases.len() != 2 {
        return Err(format!("expected two strict rows, found {}", cases.len()));
    }
    Ok(cases)
}

fn strict_cases() -> Result<Vec<OracleCase>, String> {
    strict_cases_from(CORPUS)
}

#[test]
fn strict_parser_rejects_unknown_modes_and_changed_sources() {
    let mut rows = CORPUS.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut unknown: serde_json::Value = serde_json::from_str(&rows[2]).unwrap();
    unknown["mode"] = serde_json::json!("future-mode");
    rows[2] = unknown.to_string();
    assert!(
        strict_cases_from(&rows.join("\n"))
            .unwrap_err()
            .contains("unknown mode")
    );

    let mut changed: serde_json::Value = serde_json::from_str(&rows[4]).unwrap();
    changed["source"] = serde_json::json!(
        changed["source"]
            .as_str()
            .unwrap()
            .replace("Sequence = object", "Sequence = other_object")
    );
    rows[4] = changed.to_string();
    rows[2] = CORPUS.lines().nth(2).unwrap().to_owned();
    assert!(
        strict_cases_from(&rows.join("\n"))
            .unwrap_err()
            .contains("changed source premises")
    );
}

fn selected_cases(cases: Vec<OracleCase>) -> Result<Vec<OracleCase>, String> {
    let Some(selected) = std::env::var_os("HOIMIN_NESTED_MATCH_EXIT_CASE") else {
        return Ok(cases);
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_NESTED_MATCH_EXIT_CASE is not UTF-8".to_owned())?;
    let matches = cases
        .into_iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "HOIMIN_NESTED_MATCH_EXIT_CASE={selected} selected {} strict rows",
            matches.len()
        ));
    }
    Ok(matches)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublicObservation {
    count: usize,
    path: Option<String>,
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
            operator: None,
            original: None,
            replacement: None,
            symbol: None,
        }),
        [candidate] => Ok(PublicObservation {
            count: 1,
            path: Some(candidate.path.to_string()),
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
            operator: item.candidate_operator.clone(),
            original: item.candidate_original.clone(),
            replacement: item.candidate_replacement.clone(),
            symbol: item.candidate_symbol.clone(),
        },
        CandidateExpectation::NotObserved => panic!("non-strict row cannot be public"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn strict_public_nested_match_rows_match_complete_candidate_observations() {
    let cases = selected_cases(strict_cases().expect("valid strict Lean corpus"))
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
