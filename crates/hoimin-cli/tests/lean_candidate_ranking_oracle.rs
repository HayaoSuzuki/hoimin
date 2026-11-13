#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[cfg(windows)]
const VENV_PYTHON_PATH: &str = ".venv/Scripts/python.exe";
#[cfg(not(windows))]
const VENV_PYTHON_PATH: &str = ".venv/bin/python";

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/candidate-ranking.jsonl");
const TEST_MIN_FREE_SPACE: &str = "1B";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateInput {
    id: String,
    path: String,
    line: u64,
    column: u64,
    operator: String,
    operator_class: String,
    explicit_line: bool,
    explicit_symbol: bool,
    changed_line: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RankedExpectation {
    id: String,
    rank: u64,
    score: u64,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    limit: u64,
    candidates: Vec<CandidateInput>,
    expected_ranking: Vec<RankedExpectation>,
    expected_strict: Vec<String>,
    expected_diverse: Vec<String>,
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
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1 {
        return Err(format!(
            "{} has unsupported schema {}",
            item.id, item.schema
        ));
    }
    if !matches!(item.mode.as_str(), "strict" | "model-only") {
        return Err(format!("{} has unknown mode {}", item.id, item.mode));
    }
    if !matches!(
        item.scenario.as_str(),
        "selector_scores" | "stable_order" | "diverse_tier" | "truncated_boundary"
    ) || item.id.is_empty()
        || item.candidates.is_empty()
    {
        return Err(format!("{} has an invalid identity or scenario", item.id));
    }
    let mut roles = BTreeSet::new();
    for candidate in &item.candidates {
        if !roles.insert(candidate.id.as_str())
            || !matches!(candidate.path.as_str(), "alpha.py" | "beta.py")
            || candidate.line == 0
            || !known_operator_class(&candidate.operator_class)
            || candidate.operator.is_empty()
        {
            return Err(format!("{} has an invalid candidate", item.id));
        }
        let _ = (
            candidate.column,
            candidate.explicit_line,
            candidate.explicit_symbol,
            candidate.changed_line,
        );
    }
    #[cfg(target_pointer_width = "32")]
    let limit = usize::try_from(item.limit)
        .map_err(|_| format!("{} has a limit too large for this platform", item.id))?;
    #[cfg(target_pointer_width = "64")]
    let limit = usize::try_from(item.limit).expect("u64 fits 64-bit usize");
    let expected_selection_len = limit.min(roles.len());
    if item.expected_ranking.len() != item.candidates.len()
        || item
            .expected_ranking
            .iter()
            .enumerate()
            .any(|(index, ranked)| {
                ranked.rank != (index + 1) as u64
                    || !roles.contains(ranked.id.as_str())
                    || ranked.reasons.is_empty()
                    || ranked.reasons.iter().any(|reason| !known_reason(reason))
            })
        || item
            .expected_strict
            .iter()
            .chain(&item.expected_diverse)
            .any(|role| !roles.contains(role.as_str()))
        || item.expected_strict.len() != expected_selection_len
        || item.expected_diverse.len() != expected_selection_len
    {
        return Err(format!(
            "{} has an inconsistent expected projection",
            item.id
        ));
    }
    Ok(())
}

fn known_operator_class(value: &str) -> bool {
    matches!(
        value,
        "high_value_control"
            | "exception_handling"
            | "behavioral"
            | "arithmetic"
            | "type_annotation"
    )
}

fn known_reason(value: &str) -> bool {
    matches!(
        value,
        "explicit_line"
            | "explicit_symbol"
            | "changed_line"
            | "high_value_control"
            | "exception_handling"
            | "behavioral"
            | "arithmetic"
            | "type_annotation"
    )
}

#[test]
fn lean_candidate_ranking_corpus_is_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");

    assert_eq!(cases.len(), 4);
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 1);
}

#[rstest::rstest]
#[case::strict_1(0)]
#[case::diverse_2(1)]
#[tokio::test]
async fn public_plan_and_verify_match_the_lean_diverse_tier_case(#[case] scenario_index: usize) {
    let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
    let item = cases
        .iter()
        .find(|item| item.id == "diverse_equal_tier" && item.mode == "strict")
        .expect("strict diverse case");
    let fixture = Fixture::new().expect("create isolated project");

    let plan = run_cli(plan_args(&fixture)).await.expect("public plan");

    assert_eq!(plan.code, 0, "stderr={}", plan.stderr);
    let manifest: serde_json::Value = serde_json::from_str(plan.stdout.trim()).expect("plan JSON");
    let candidates = manifest["candidates"].as_array().expect("plan candidates");
    assert_eq!(candidates.len(), item.candidates.len());

    let id_roles = candidates
        .iter()
        .map(|candidate| {
            let id = candidate["id"].as_str().expect("candidate id").to_owned();
            let role = candidate["symbol"]
                .as_str()
                .expect("candidate symbol")
                .to_owned();
            (id, role)
        })
        .collect::<BTreeMap<_, _>>();
    let actual_ranking = candidates
        .iter()
        .map(|candidate| ranked_observation(candidate, &id_roles))
        .collect::<Vec<_>>();
    assert_eq!(actual_ranking, item.expected_ranking);

    let plan_path = fixture.root.join("plan.json");
    std::fs::write(&plan_path, plan.stdout.as_bytes()).expect("write plan manifest");
    let plan_before = std::fs::read(&plan_path).expect("read plan manifest");

    let (policy, expected) = [
        ("strict", &item.expected_strict),
        ("diverse", &item.expected_diverse),
    ]
    .into_iter()
    .nth(scenario_index)
    .unwrap();

    let run = run_cli(verify_args(&plan_path, item.limit, policy))
        .await
        .unwrap_or_else(
            // A failed public CLI invocation fails the correspondence test.
            #[cfg_attr(coverage_nightly, coverage(off))]
            |error| panic!("public verify {policy}: {error}"),
        );
    assert_eq!(run.code, 0, "policy={policy}; stderr={}", run.stderr);

    let actual = selected_roles(&run.stdout, &id_roles).expect("selected role order");

    assert_eq!(&actual, expected, "policy={policy}");
    assert_eq!(std::fs::read(&plan_path).unwrap(), plan_before);
}

fn ranked_observation(
    candidate: &serde_json::Value,
    id_roles: &BTreeMap<String, String>,
) -> RankedExpectation {
    let id = candidate["id"].as_str().expect("candidate id");
    RankedExpectation {
        id: id_roles.get(id).expect("mapped role").clone(),
        rank: candidate["rank"].as_u64().expect("rank"),
        score: candidate["score"].as_u64().expect("score"),
        reasons: candidate["ranking_reasons"]
            .as_array()
            .expect("ranking reasons")
            .iter()
            .map(|reason| {
                reason["code"]
                    .as_str()
                    .expect("ranking reason code")
                    .to_owned()
            })
            .collect(),
    }
}

fn selected_roles(
    stdout: &str,
    id_roles: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .filter(|event| event["kind"] == "mutant_started")
        .map(|event| {
            let id = event["mutant_id"]
                .as_str()
                .ok_or_else(|| "mutant_started event lacks mutant_id".to_owned())?;
            id_roles
                .get(id)
                .cloned()
                .ok_or_else(|| format!("unknown selected candidate {id}"))
        })
        .collect()
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        Self::from_directory(tempfile::tempdir())
    }

    fn from_directory(directory: std::io::Result<tempfile::TempDir>) -> Result<Self, String> {
        let directory = directory.map_err(|error| error.to_string())?;
        let root = directory.path().join("project");
        let source = root.join("src");
        std::fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        std::fs::write(source.join("__init__.py"), "").map_err(|error| error.to_string())?;
        std::fs::write(
            source.join("alpha.py"),
            concat!(
                "def alpha_1(left, right):\n    return left == right\n\n",
                "def alpha_2(left, right):\n    return left == right\n\n",
                "def low(left, right):\n    return left + right\n",
            ),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            source.join("beta.py"),
            "def beta_1(left, right):\n    return left == right\n",
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

fn plan_args(fixture: &Fixture) -> Vec<OsString> {
    vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        fixture.root.as_os_str().to_owned(),
        "--source".into(),
        "src".into(),
        "--file".into(),
        "src/alpha.py".into(),
        "--file".into(),
        "src/beta.py".into(),
        "--operators".into(),
        "compare_eq_ne,binary_add_sub".into(),
        "--jobs".into(),
        "1".into(),
        "--min-free-space".into(),
        TEST_MIN_FREE_SPACE.into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python_executable().into_os_string(),
        "-c".into(),
        "pass".into(),
    ]
}

fn verify_args(path: &Path, limit: u64, policy: &str) -> Vec<OsString> {
    vec![
        "hoimin".into(),
        "verify".into(),
        path.as_os_str().to_owned(),
        "--top".into(),
        limit.to_string().into(),
        "--selection-policy".into(),
        policy.into(),
    ]
}

struct CliOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

async fn run_cli(args: Vec<OsString>) -> Result<CliOutput, String> {
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()),
    )
    .await;
    decode_cli_output(result, stdout.snapshot(), stderr.snapshot())
}

fn decode_cli_output(
    result: Result<i32, tokio::time::error::Elapsed>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> Result<CliOutput, String> {
    let code = result.map_err(|_| "Hoimin public CLI timed out after 10 seconds".to_owned())?;
    Ok(CliOutput {
        code,
        stdout: String::from_utf8(stdout).map_err(|error| error.to_string())?,
        stderr: String::from_utf8(stderr).map_err(|error| error.to_string())?,
    })
}

#[rstest::rstest]
#[case::schema("/schema", serde_json::json!(2), "unsupported schema")]
#[case::mode("/mode", serde_json::json!("unknown"), "unknown mode")]
#[case::scenario("/scenario", serde_json::json!("unknown"), "invalid identity or scenario")]
#[case::identity("/id", serde_json::json!(""), "invalid identity or scenario")]
#[case::empty_candidates("/candidates", serde_json::json!([]), "invalid identity or scenario")]
#[case::duplicate_role("/candidates/1/id", serde_json::json!("selector"), "invalid candidate")]
#[case::path("/candidates/0/path", serde_json::json!("other.py"), "invalid candidate")]
#[case::line("/candidates/0/line", serde_json::json!(0), "invalid candidate")]
#[case::operator_class("/candidates/0/operator_class", serde_json::json!("unknown"), "invalid candidate")]
#[case::operator("/candidates/0/operator", serde_json::json!(""), "invalid candidate")]
#[case::ranking_count("/expected_ranking", serde_json::json!([]), "inconsistent expected projection")]
#[case::rank("/expected_ranking/0/rank", serde_json::json!(2), "inconsistent expected projection")]
#[case::ranking_role("/expected_ranking/0/id", serde_json::json!("absent"), "inconsistent expected projection")]
#[case::reasons("/expected_ranking/0/reasons", serde_json::json!([]), "inconsistent expected projection")]
#[case::reason("/expected_ranking/0/reasons/0", serde_json::json!("unknown"), "inconsistent expected projection")]
#[case::strict_role("/expected_strict/0", serde_json::json!("absent"), "inconsistent expected projection")]
#[case::diverse_role("/expected_diverse/0", serde_json::json!("absent"), "inconsistent expected projection")]
#[case::strict_count("/expected_strict", serde_json::json!([]), "inconsistent expected projection")]
#[case::diverse_count("/expected_diverse", serde_json::json!([]), "inconsistent expected projection")]
fn ranking_schema_rejects_one_invalid_coordinate(
    #[case] pointer: &str,
    #[case] value: serde_json::Value,
    #[case] diagnostic: &str,
) {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    *row.pointer_mut(pointer).unwrap() = value;
    assert!(
        parse_corpus(&row.to_string())
            .unwrap_err()
            .contains(diagnostic)
    );
}

#[rstest::rstest]
#[case::empty(&String::new(), "corpus contains no cases")]
#[case::invalid(&"{".to_owned(), "line 1 is invalid JSON")]
#[case::duplicate(&format!("{CORPUS}\n{}", CORPUS.lines().next().unwrap()), "duplicate case id")]
fn ranking_parser_reports_invalid_corpus(#[case] input: &str, #[case] diagnostic: &str) {
    assert!(parse_corpus(input).unwrap_err().contains(diagnostic));
}

#[test]
fn ranking_parser_ignores_blank_lines() {
    assert_eq!(parse_corpus(&format!("\n{CORPUS}\n")).unwrap().len(), 4);
}

#[rstest::rstest]
#[case::json("{", "EOF")]
#[case::missing_id(r#"{"kind":"mutant_started"}"#, "lacks mutant_id")]
#[case::unknown_id(
    r#"{"kind":"mutant_started","mutant_id":"unknown"}"#,
    "unknown selected candidate"
)]
fn ranking_projection_rejects_invalid_started_events(
    #[case] input: &str,
    #[case] diagnostic: &str,
) {
    assert!(
        selected_roles(input, &BTreeMap::new())
            .unwrap_err()
            .contains(diagnostic)
    );
}

#[test]
fn ranking_fixture_reports_tempdir_failure() {
    assert_eq!(
        Fixture::from_directory(Err(std::io::Error::other("denied")))
            .err()
            .unwrap(),
        "denied"
    );
}

#[test]
fn ranking_fixture_reports_source_directory_failure() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("project")).unwrap();
    std::fs::write(directory.path().join("project/src"), "obstruction").unwrap();
    assert!(Fixture::from_directory(Ok(directory)).is_err());
}

#[rstest::rstest]
#[case::init("__init__.py")]
#[case::alpha("alpha.py")]
#[case::beta("beta.py")]
fn ranking_fixture_reports_unwritable_source(#[case] path: &str) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("project/src").join(path)).unwrap();
    assert!(Fixture::from_directory(Ok(directory)).is_err());
}

#[rstest::rstest]
#[case::stdout(vec![255], vec![])]
#[case::stderr(vec![], vec![255])]
fn ranking_cli_output_rejects_invalid_utf8(#[case] stdout: Vec<u8>, #[case] stderr: Vec<u8>) {
    assert!(
        decode_cli_output(Ok(0), stdout, stderr)
            .err()
            .unwrap()
            .contains("invalid utf-8")
    );
}

#[tokio::test(start_paused = true)]
async fn ranking_cli_output_reports_timeout() {
    let result =
        tokio::time::timeout(Duration::from_millis(1), std::future::pending::<i32>()).await;
    assert!(
        decode_cli_output(result, vec![], vec![])
            .err()
            .unwrap()
            .contains("timed out")
    );
}
