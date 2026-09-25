//! Lean generates expected order; this adapter only maps public manifest IDs
//! to file/line identities and observes public plan/verify dry-run behavior.
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/paging.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u32,
    id: String,
    mode: String,
    policy: String,
    offset: u64,
    count: u64,
    accepted: bool,
    exit: i32,
    expected_ids: Vec<String>,
    full_order: Vec<String>,
}

fn parse(input: &str) -> Result<Vec<Case>, String> {
    let cases = input
        .lines()
        .map(serde_json::from_str::<Case>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut identities = BTreeSet::new();
    let mut coordinates = BTreeSet::new();
    let roles = BTreeSet::from(["a1", "a2", "a3", "b1", "b2", "b3"]);
    for case in &cases {
        if case.schema != 1
            || case.mode != "strict"
            || !matches!(case.policy.as_str(), "strict" | "diverse")
            || case.id.is_empty()
            || !identities.insert(case.id.as_str())
            || !coordinates.insert((case.policy.as_str(), case.offset, case.count))
            || case.full_order.len() != 6
            || case
                .full_order
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != roles
            || case
                .expected_ids
                .iter()
                .any(|id| !roles.contains(id.as_str()))
            || case.expected_ids.iter().collect::<BTreeSet<_>>().len() != case.expected_ids.len()
            || case.exit != if case.accepted { 0 } else { 2 }
            || (!case.accepted && !case.expected_ids.is_empty())
        {
            return Err(format!("invalid paging case {}", case.id));
        }
    }
    let mut required = BTreeSet::new();
    for policy in ["strict", "diverse"] {
        for offset in 0..7 {
            for count in [1, 2, 4, 8] {
                required.insert((policy, offset, count));
            }
        }
    }
    required.extend([
        ("strict", 0, 0),
        ("strict", u64::MAX, 1),
        ("strict", 1, u64::MAX),
    ]);
    if coordinates != required {
        return Err("paging corpus matrix is incomplete".into());
    }
    Ok(cases)
}

#[test]
fn corpus_requires_the_complete_strict_matrix_and_rejects_invalid_rows() {
    assert_eq!(parse(CORPUS).unwrap().len(), 59);
    assert!(parse("").is_err());
    assert!(parse(&CORPUS.lines().skip(1).collect::<Vec<_>>().join("\n")).is_err());
    assert!(parse(&format!("{CORPUS}{}\n", CORPUS.lines().next().unwrap())).is_err());
    for (field, value) in [
        ("schema", serde_json::json!(2)),
        ("mode", serde_json::json!("model-only")),
        ("policy", serde_json::json!("unknown")),
        ("full_order", serde_json::json!(["a1"])),
        ("expected_ids", serde_json::json!(["unknown"])),
        ("exit", serde_json::json!(7)),
    ] {
        let mut rows = CORPUS
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        rows[0][field] = value;
        let invalid = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse(&invalid).is_err(), "accepted invalid {field}");
    }
}

async fn cli(args: Vec<String>) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(20),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .expect("infrastructure error: public CLI timed out");
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[tokio::test]
async fn public_plan_and_verify_match_lean_paging() {
    let cases = parse(CORPUS).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.py"), "a = True\nb = False\nc = 1 + 2\n").unwrap();
    std::fs::write(root.join("b.py"), "a = True\nb = 2 + 3\nc = 1 + 2\n").unwrap();
    let manifest_text = create_plan(&root).await;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    let candidates = manifest["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 6);
    assert_eq!(manifest["truncated"], false);
    assert_eq!(manifest["diagnostics"], serde_json::json!([]));
    let roles = candidates
        .iter()
        .map(|candidate| {
            let path = std::path::Path::new(candidate["path"].as_str().unwrap());
            let role = format!(
                "{}{}",
                path.file_stem().unwrap().to_str().unwrap(),
                candidate["line"].as_u64().unwrap()
            );
            (candidate["id"].as_str().unwrap().to_owned(), role)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(roles.values().collect::<BTreeSet<_>>().len(), 6);
    let ranks = candidates
        .iter()
        .map(|candidate| (candidate["id"].as_str().unwrap(), &candidate["rank"]))
        .collect::<BTreeMap<_, _>>();
    let plan = directory.path().join("plan.json");
    std::fs::write(&plan, &manifest_text).unwrap();
    let mut executed = 0;
    for case in cases {
        // The two explicit u64::MAX controls describe a 64-bit usize CLI.
        // Ordinary cases still run on narrower hosts; do not truncate inputs.
        if usize::try_from(case.offset).is_err() || usize::try_from(case.count).is_err() {
            continue;
        }
        if case.policy == "strict" {
            assert_eq!(
                candidates
                    .iter()
                    .map(|c| roles[c["id"].as_str().unwrap()].clone())
                    .collect::<Vec<_>>(),
                case.full_order
            );
        }
        let (code, stdout, stderr) = cli(vec![
            "hoimin".into(),
            "verify".into(),
            plan.display().to_string(),
            "--offset".into(),
            case.offset.to_string(),
            "--top".into(),
            case.count.to_string(),
            "--selection-policy".into(),
            case.policy.clone(),
            "--dry-run".into(),
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_case(&case, code, &stdout, &stderr, &roles, &ranks);
        assert_eq!(std::fs::read_to_string(&plan).unwrap(), manifest_text);
        executed += 1;
    }
    assert_eq!(executed, if usize::BITS >= 64 { 59 } else { 57 });
}

async fn create_plan(root: &std::path::Path) -> String {
    let (code, manifest_text, stderr) = cli(vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        root.display().to_string(),
        "--source".into(),
        ".".into(),
        "--operators".into(),
        "boolean_literal,binary_add_sub".into(),
        "--max-mutants".into(),
        "6".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        "python3".into(),
        "-c".into(),
        "pass".into(),
    ])
    .await;
    assert_eq!(code, 0, "infrastructure error: plan failed: {stderr}");
    manifest_text
}

fn assert_case(
    case: &Case,
    code: i32,
    stdout: &str,
    stderr: &str,
    roles: &BTreeMap<String, String>,
    ranks: &BTreeMap<&str, &serde_json::Value>,
) {
    assert!(
        matches!(code, 0 | 2),
        "infrastructure error: {} exit={code} stderr={stderr}",
        case.id
    );
    assert_eq!(code, case.exit, "{}: {stderr}", case.id);
    if case.accepted {
        let preview: serde_json::Value = serde_json::from_str(stdout).unwrap();
        let selected = preview["candidates"].as_array().unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|c| roles[c["id"].as_str().unwrap()].clone())
                .collect::<Vec<_>>(),
            case.expected_ids,
            "{}",
            case.id
        );
        assert_eq!(preview["offset"], case.offset);
        assert_eq!(preview["retained_candidates"], 6);
        assert_eq!(preview["verification_selection"]["requested"], case.count);
        assert_eq!(
            preview["verification_selection"]["selected"],
            selected.len()
        );
        for (index, candidate) in selected.iter().enumerate() {
            assert_eq!(candidate["selection_order"], index + 1);
            assert_eq!(&candidate["rank"], ranks[candidate["id"].as_str().unwrap()]);
        }
    } else {
        assert!(stdout.is_empty(), "{} emitted unexpected stdout", case.id);
        if case.count == 0 {
            assert!(
                stderr.contains("invalid value '0'"),
                "{}: {stderr}",
                case.id
            );
        } else {
            assert!(
                stderr.contains(&format!(
                    "--offset {} is outside the 6 retained candidates",
                    case.offset
                )),
                "{}: {stderr}",
                case.id
            );
        }
    }
}
