use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/candidate-ranking.jsonl");

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
    let expected_selection_len = usize::try_from(item.limit)
        .map_err(|_| format!("{} has a limit too large for this platform", item.id))?
        .min(roles.len());
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

#[tokio::test]
async fn public_plan_and_verify_match_the_lean_diverse_tier_case() {
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
    for (policy, expected) in [
        ("strict", &item.expected_strict),
        ("diverse", &item.expected_diverse),
    ] {
        let run = run_cli(verify_args(&plan_path, item.limit, policy))
            .await
            .unwrap_or_else(|error| panic!("public verify {policy}: {error}"));
        assert_eq!(run.code, 1, "policy={policy}; stderr={}", run.stderr);
        let actual = selected_roles(&run.stdout, &id_roles).expect("selected role order");
        assert_eq!(&actual, expected, "policy={policy}");
        assert_eq!(std::fs::read(&plan_path).unwrap(), plan_before);
    }
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
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
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
        "--format".into(),
        "jsonl".into(),
    ]
}

struct CliOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

async fn run_cli(args: Vec<OsString>) -> Result<CliOutput, String> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .map_err(|_| "Hoimin public CLI timed out after 10 seconds".to_owned())?;
    Ok(CliOutput {
        code,
        stdout: String::from_utf8(stdout).map_err(|error| error.to_string())?,
        stderr: String::from_utf8(stderr).map_err(|error| error.to_string())?,
    })
}
