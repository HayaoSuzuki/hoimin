use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/nested-try-flow.jsonl");

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CandidateExpectation {
    NotObserved,
    Present,
    Absent,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    family: String,
    source: String,
    marker: String,
    entry_category: String,
    expected: serde_json::Value,
    candidate: CandidateExpectation,
}

fn strict_cases() -> Result<Vec<OracleCase>, String> {
    let mut cases = Vec::new();
    for (index, line) in CORPUS.lines().enumerate() {
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        if item.schema != 1
            || item.source.matches(&item.marker).count() != 1
            || !matches!(
                item.entry_category.as_str(),
                "fallthrough" | "break" | "continue" | "terminate"
            )
            || !item.expected.is_object()
        {
            return Err(format!("{} has an invalid public fixture", item.id));
        }
        if item.mode == "strict" {
            let owned = match item.id.as_str() {
                "finally_annotation_meets_normal_and_raise" => {
                    item.family == "finally-annotation"
                        && item.marker == "value: Sequence[int]"
                        && item.candidate == CandidateExpectation::Absent
                }
                "post_finally_uses_only_fallthrough" => {
                    item.family == "post-finally"
                        && item.marker == "value: Sequence[int]"
                        && item.candidate == CandidateExpectation::Present
                }
                _ => false,
            };
            if !owned {
                return Err(format!("{} is not an owned strict fixture", item.id));
            }
            cases.push(item);
        }
    }
    if cases.len() != 2 {
        return Err(format!("expected two strict rows, found {}", cases.len()));
    }
    Ok(cases)
}

fn selected_cases(cases: Vec<OracleCase>) -> Result<Vec<OracleCase>, String> {
    let Some(selected) = std::env::var_os("HOIMIN_NESTED_TRY_CASE") else {
        return Ok(cases);
    };
    let selected = selected
        .into_string()
        .map_err(|_| "HOIMIN_NESTED_TRY_CASE is not UTF-8".to_owned())?;
    let matches = cases
        .into_iter()
        .filter(|item| item.id == selected)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "HOIMIN_NESTED_TRY_CASE={selected} selected {} strict rows",
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
        CandidateExpectation::Absent => PublicObservation {
            count: 0,
            path: None,
            operator: None,
            original: None,
            replacement: None,
            symbol: None,
        },
        CandidateExpectation::Present => PublicObservation {
            count: 1,
            path: Some("target.py".to_owned()),
            operator: Some("type_list_sequence".to_owned()),
            original: Some("Sequence[int]".to_owned()),
            replacement: Some("list[int]".to_owned()),
            symbol: None,
        },
        CandidateExpectation::NotObserved => {
            panic!("non-strict row cannot have a public expectation")
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn strict_public_nested_try_rows_match_complete_candidate_observations() {
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
