use std::{collections::BTreeMap, collections::BTreeSet, fs, path::Path};

use serde::Deserialize;
use serde_json::Value;

use super::{
    PlanManifest, Project, RankedPlanCandidate, preview_candidate_value, preview_cli,
    preview_started_ids, write_json, write_plan_manifest_with_marker,
};

#[derive(Deserialize)]
struct Case {
    schema: u32,
    id: String,
    shape: Shape,
    budget: usize,
    request: Request,
    ranked: Vec<Ranked>,
    expected: Expected,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Shape {
    Complete,
    Truncated,
    Empty,
    EmptyTruncated,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Request {
    Explicit {
        ids: Vec<String>,
    },
    Top {
        policy: String,
        offset: usize,
        count: usize,
    },
    Sample {
        count: usize,
        seed: u64,
    },
}

#[derive(Deserialize)]
struct Ranked {
    id: String,
    path: String,
    line: u32,
    column: u32,
    operator: String,
}

#[derive(Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
enum Expected {
    Rejected {
        message: String,
    },
    Accepted {
        preview: Vec<String>,
        started: Vec<String>,
        offset: Option<usize>,
        provenance: Value,
        run_exit: i32,
    },
}

async fn fixture(case: &Case, marker: &Path) -> (Project, std::path::PathBuf, PlanManifest) {
    let sources = match case.shape {
        Shape::Complete | Shape::Truncated => [
            ("a.py", "x = 1 + 2\na = 1 == 2; b = 3 == 4\nc = 5 == 6\n"),
            ("b.py", "d = 7 == 8; e = 9 == 10\nf = 11 + 12\n"),
        ],
        Shape::Empty | Shape::EmptyTruncated => [("a.py", "pass\n"), ("b.py", "pass\n")],
    };
    let project = Project::new_with_sources(&sources);
    let limit = if matches!(case.shape, Shape::Truncated) {
        "6"
    } else {
        "8"
    };
    let options = [
        "--jobs",
        "1",
        "--operators",
        "compare_eq_ne,binary_add_sub",
        "--max-candidates",
        limit,
        "--max-mutants",
        &case.budget.to_string(),
    ];
    let (path, mut manifest) = write_plan_manifest_with_marker(&project, &options, marker).await;
    if matches!(case.shape, Shape::EmptyTruncated) {
        manifest.truncated = true;
        write_json(&path, &serde_json::to_value(&manifest).unwrap());
    }
    assert_eq!(
        manifest.truncated,
        matches!(case.shape, Shape::Truncated | Shape::EmptyTruncated),
        "{}",
        case.id
    );
    (project, path, manifest)
}

type Candidates<'a> = BTreeMap<&'a str, &'a RankedPlanCandidate>;

fn known_candidates<'a>(case: &'a Case, manifest: &'a PlanManifest) -> Candidates<'a> {
    assert_eq!(manifest.candidates.len(), case.ranked.len(), "{}", case.id);
    case.ranked
        .iter()
        .zip(&manifest.candidates)
        .enumerate()
        .map(|(index, (expected, actual))| {
            assert_eq!(actual.rank, index + 1, "{}", case.id);
            assert_eq!(actual.path.as_str(), expected.path, "{}", case.id);
            assert_eq!(actual.line, expected.line, "{}", case.id);
            assert_eq!(
                actual.column, expected.column,
                "{}: {}",
                case.id, expected.id
            );
            assert_eq!(actual.operator, expected.operator, "{}", case.id);
            (expected.id.as_str(), actual)
        })
        .collect()
}

fn args(request: &Request, known: &Candidates<'_>, missing: &str) -> Vec<String> {
    match request {
        Request::Explicit { ids } => ids
            .iter()
            .flat_map(|id| {
                let resolved = if id == "missing" {
                    missing
                } else {
                    known.get(id.as_str()).map_or(id, |row| &row.id)
                };
                ["--candidate".into(), resolved.into()]
            })
            .collect(),
        Request::Top {
            policy,
            offset,
            count,
        } => vec![
            "--top".into(),
            count.to_string(),
            "--offset".into(),
            offset.to_string(),
            "--selection-policy".into(),
            policy.clone(),
        ],
        Request::Sample { count, seed } => {
            vec![
                "--sample".into(),
                count.to_string(),
                "--seed".into(),
                seed.to_string(),
            ]
        }
    }
}

fn assert_rejected(output: &std::process::Output, message: &str, case: &str) {
    assert_eq!(output.status.code(), Some(2), "{case}");
    assert_eq!(output.stdout, b"", "{case}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("plan.candidate.invalid: {message}")),
        "{case}: {stderr}"
    );
}

fn bind_sample_ids(mut selection: Value, known: &Candidates<'_>) -> Value {
    if let Some(sampling) = selection.get_mut("sampling") {
        for id in sampling["selected_ids"].as_array_mut().unwrap() {
            *id = Value::String(known[id.as_str().unwrap()].id.clone());
        }
    }
    selection
}

fn assert_preview(
    output: &std::process::Output,
    case: &Case,
    order: &[String],
    offset: Option<usize>,
    provenance: &Value,
    known: &Candidates<'_>,
) {
    assert!(
        output.status.success(),
        "{}: {}",
        case.id,
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["kind"], "verify_preview", "{}", case.id);
    assert_eq!(
        preview["retained_candidates"],
        case.ranked.len(),
        "{}",
        case.id
    );
    assert_eq!(preview["offset"], serde_json::json!(offset), "{}", case.id);
    assert_eq!(
        &preview["verification_selection"], provenance,
        "{}",
        case.id
    );
    let expected: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(index, id)| preview_candidate_value(known[id.as_str()], index + 1))
        .collect();
    assert_eq!(
        preview["candidates"],
        serde_json::json!(expected),
        "{}",
        case.id
    );
}

fn assert_execution(
    output: &std::process::Output,
    case: &str,
    exit: i32,
    order: &[String],
    provenance: &Value,
    known: &Candidates<'_>,
) {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{case}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<_> = order
        .iter()
        .map(|id| known[id.as_str()].id.as_str())
        .collect();
    assert_eq!(preview_started_ids(&output.stdout), expected, "{case}");
    let events: Vec<Value> = output
        .stdout
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    for kind in ["run_started", "run_finished"] {
        let event = events
            .iter()
            .find(|event| event["kind"] == kind)
            .expect(case);
        assert_eq!(
            &event["verification_selection"], provenance,
            "{case}: {kind}"
        );
    }
}

#[tokio::test]
async fn verification_preview_and_execution_agree_with_lean() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/verify-selection-flow.jsonl"
    ));
    let mut ids = BTreeSet::new();
    let mut accepted = 0;
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        let coordinator = tempfile::tempdir().unwrap();
        let marker = coordinator.path().join("test-command-ran");
        let (_project, path, manifest) = fixture(&case, &marker).await;
        let known = known_candidates(&case, &manifest);
        let original = fs::read(&path).unwrap();
        let missing = format!("m1_{}", "f".repeat(64));
        let arguments = args(&case.request, &known, &missing);
        let arguments: Vec<_> = arguments.iter().map(String::as_str).collect();
        let temporary = tempfile::tempdir().unwrap();
        let mut dry_args = arguments.clone();
        dry_args.push("--dry-run");
        let dry = preview_cli(&path, &dry_args, temporary.path()).await;
        assert!(
            !marker.exists(),
            "{}: preview ran the test command",
            case.id
        );
        assert_eq!(fs::read(&path).unwrap(), original, "{}", case.id);

        match &case.expected {
            Expected::Rejected { message } => {
                let message =
                    message.replace("in the plan: missing", &format!("in the plan: {missing}"));
                assert_rejected(&dry, &message, &case.id);
                let run = preview_cli(&path, &arguments, temporary.path()).await;
                assert_rejected(&run, &message, &case.id);
                assert!(
                    !marker.exists(),
                    "{}: rejected verify ran the test command",
                    case.id
                );
            }
            Expected::Accepted {
                preview,
                started,
                offset,
                provenance,
                run_exit,
            } => {
                let provenance = bind_sample_ids(provenance.clone(), &known);
                assert_preview(&dry, &case, preview, *offset, &provenance, &known);
                let run = preview_cli(&path, &arguments, temporary.path()).await;
                assert_execution(&run, &case.id, *run_exit, started, &provenance, &known);
                assert!(
                    marker.exists(),
                    "{}: verify must run the test command",
                    case.id
                );
                accepted += 1;
            }
        }
        assert_eq!(fs::read(&path).unwrap(), original, "{}", case.id);
    }
    assert_eq!(ids.len(), 32);
    assert_eq!(accepted, 21);
}
