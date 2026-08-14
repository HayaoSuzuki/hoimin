use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hoimin_cli::analyzer::{AnalyzerHandler, CandidateStore};
use hoimin_core::{
    AnalyzeFile, CandidateCursor, EffectId, MutationOperatorSelection, MutationProfile, TargetSlice,
};
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/bounded-candidate-discovery.jsonl");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateInput {
    identity: u64,
    order_key: u64,
    producer: String,
    eligible: bool,
    emission_index: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    limit: u64,
    token: Vec<CandidateInput>,
    ast: Vec<CandidateInput>,
    annotation: Vec<CandidateInput>,
    targets: Vec<Vec<CandidateInput>>,
    expected_identities: Vec<u64>,
    expected_truncated: bool,
    expected_sequences: Vec<u64>,
    expected_targets_read: u64,
    expected_spool_finished: bool,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&item)?;
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        cases.push(item);
    }
    let expected = BTreeSet::from([
        "complete_two_targets",
        "eligibility_before_capacity",
        "out_of_order_duplicate",
        "public_truncated_plan",
        "three_producer_merge",
        "truncated_non_final_target",
        "usize_max",
        "zero_limit",
    ]);
    if ids.iter().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err("corpus does not contain the exact closed case set".to_owned());
    }
    Ok(cases)
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    let identity = item.id.as_str();
    let expected_shape = match identity {
        "out_of_order_duplicate" | "eligibility_before_capacity" | "zero_limit" => {
            ("internal-fixture", "candidate_prefix")
        }
        "three_producer_merge" => ("internal-fixture", "producer_merge"),
        "complete_two_targets" | "truncated_non_final_target" => {
            ("internal-fixture", "ordered_targets")
        }
        "usize_max" => ("model-only", "candidate_prefix"),
        "public_truncated_plan" => ("strict", "public_plan"),
        _ => return Err(format!("unknown case id {}", item.id)),
    };
    if item.schema != 1
        || item.mode != expected_shape.0
        || item.scenario != expected_shape.1
        || item.expected_identities.len() > usize::try_from(item.limit).unwrap_or(usize::MAX)
        || item.expected_sequences
            != (1..=u64::try_from(item.expected_identities.len())
                .map_err(|_| format!("{} expectation length overflows u64", item.id))?)
                .collect::<Vec<_>>()
    {
        return Err(format!("{} violates its closed expectation shape", item.id));
    }
    for candidate in item
        .token
        .iter()
        .chain(&item.ast)
        .chain(&item.annotation)
        .chain(item.targets.iter().flatten())
    {
        if !matches!(candidate.producer.as_str(), "token" | "ast" | "annotation") {
            return Err(format!("{} has an unknown producer", item.id));
        }
        let _ = (
            candidate.identity,
            candidate.order_key,
            candidate.eligible,
            candidate.emission_index,
        );
    }
    if item.scenario == "ordered_targets" {
        if item.targets.is_empty()
            || !item.expected_spool_finished
            || item.expected_targets_read == 0
        {
            return Err(format!("{} has an invalid target projection", item.id));
        }
    } else if !item.targets.is_empty() || item.expected_targets_read != 0 {
        return Err(format!("{} crosses target and producer premises", item.id));
    }
    Ok(())
}

#[test]
fn bounded_discovery_corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean bounded-discovery corpus must be valid");
    assert_eq!(cases.len(), 8);
}

#[test]
fn bounded_discovery_corpus_rejects_unknown_fields_and_crossed_modes() {
    let first = CORPUS.lines().next().expect("first corpus row");
    let mut unknown: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown["future"] = serde_json::json!(true);
    assert!(parse_corpus(&format!("{unknown}\n")).is_err());

    let crossed = CORPUS.replace(
        "\"id\":\"public_truncated_plan\",\"limit\":1,\"mode\":\"strict\"",
        "\"id\":\"public_truncated_plan\",\"limit\":1,\"mode\":\"model-only\"",
    );
    assert!(parse_corpus(&crossed).is_err());
    assert!(parse_corpus(&format!("{first}\n{first}\n")).is_err());
}

#[tokio::test]
async fn strict_public_plan_matches_the_lean_truncation_projection() {
    let item = parse_corpus(CORPUS)
        .expect("valid Lean corpus")
        .into_iter()
        .find(|item| item.id == "public_truncated_plan")
        .expect("strict public case");
    let fixture = Fixture::new().expect("isolated public fixture");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_io(plan_args(&fixture, item.limit), &mut stdout, &mut stderr),
    )
    .await
    .expect("public plan timed out");

    assert_eq!(code, 4, "stderr={}", String::from_utf8_lossy(&stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&stdout).expect("plan JSON");
    let candidates = manifest["candidates"].as_array().expect("plan candidates");
    assert_eq!(candidates.len(), item.expected_identities.len());
    assert_eq!(manifest["truncated"], item.expected_truncated);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate["sequence"].as_u64().expect("candidate sequence"))
            .collect::<Vec<_>>(),
        item.expected_sequences
    );
    assert!(
        manifest["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "candidate_limit")
    );
}

#[tokio::test]
async fn ordered_target_spools_match_the_lean_terminal_cases() {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    for case_id in ["complete_two_targets", "truncated_non_final_target"] {
        let item = cases
            .iter()
            .find(|item| item.id == case_id)
            .expect("owned target case");
        let fixture = TargetFixture::new(case_id).expect("target fixture");
        let root = camino::Utf8PathBuf::from_path_buf(fixture.root.clone()).unwrap();
        let mut handler = AnalyzerHandler::new(root).unwrap();
        let mut spool = None;
        let mut requests = 0_u64;
        for (index, path) in fixture.paths.iter().enumerate() {
            if spool.is_some() {
                break;
            }
            requests += 1;
            let finished = handler
                .handle(
                    AnalyzeFile {
                        id: EffectId(index as u64 + 1),
                        target: TargetSlice {
                            path: path.as_str().into(),
                            lines: Vec::new(),
                            symbols: Vec::new(),
                        },
                        final_target: index + 1 == fixture.paths.len(),
                        max_candidates: item.limit,
                    },
                    &MutationOperatorSelection::default(),
                    MutationProfile::Full,
                )
                .await
                .expect("bounded target analysis");
            assert_eq!(
                finished.truncated,
                item.expected_truncated && index + 1 == requests as usize
            );
            spool = finished.spool;
        }
        let spool = spool.expect("Lean terminal case finishes the spool");
        let mut cursor = CandidateCursor::START;
        let mut sequences = Vec::new();
        while let Some((candidate, next)) = CandidateStore::replay_one(&spool, cursor).unwrap() {
            sequences.push(candidate.sequence);
            cursor = next;
        }
        assert_eq!(requests, item.expected_targets_read);
        assert_eq!(sequences, item.expected_sequences);
        assert_eq!(spool.records as usize, item.expected_identities.len());
        assert!(item.expected_spool_finished);
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

struct TargetFixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    paths: Vec<String>,
}

impl TargetFixture {
    fn new(case_id: &str) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let root = directory.path().join("project");
        let source = root.join("src");
        std::fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        let first = if case_id == "truncated_non_final_target" {
            "first = left == right\nsecond = top == bottom\n"
        } else {
            "first = left == right\n"
        };
        std::fs::write(source.join("first.py"), first).map_err(|error| error.to_string())?;
        std::fs::write(source.join("second.py"), "second = top == bottom\n")
            .map_err(|error| error.to_string())?;
        Ok(Self {
            _directory: directory,
            root,
            paths: vec!["src/first.py".to_owned(), "src/second.py".to_owned()],
        })
    }
}

impl Fixture {
    fn new() -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let root = directory.path().join("project");
        let source = root.join("src");
        std::fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        std::fs::write(
            source.join("calc.py"),
            "first = left == right\nsecond = top == bottom\n",
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

fn plan_args(fixture: &Fixture, limit: u64) -> Vec<OsString> {
    vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        fixture.root.as_os_str().to_owned(),
        "--source".into(),
        "src".into(),
        "--file".into(),
        "src/calc.py".into(),
        "--max-candidates".into(),
        limit.to_string().into(),
        "--jobs".into(),
        "1".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python_executable().into_os_string(),
        "-c".into(),
        "pass".into(),
    ]
}
