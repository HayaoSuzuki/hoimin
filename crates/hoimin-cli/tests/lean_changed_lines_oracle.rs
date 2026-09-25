//! Issue 612 correspondence uses real Git, `CPython`, and public plan/run/verify.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::process::Command;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/changed-lines.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u32,
    id: String,
    mode: String,
    eol1: String,
    eol2: String,
    git_state: String,
    source: String,
    original_source: String,
    candidate_line: u32,
    candidate_offset: u64,
    changed_first_git_line: u32,
    changed_last_git_line: u32,
    eligible: bool,
    broken_eligible: bool,
}

fn cases(input: &str) -> Vec<Case> {
    let cases: Vec<Case> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut ids = BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert_eq!(
            case.id,
            format!("{}-{}-{}", case.eol1, case.eol2, case.git_state)
        );
        assert!(ids.insert(case.id.clone()), "duplicate {}", case.id);
        assert!(case.changed_first_git_line > 0);
        assert!(case.changed_first_git_line <= case.changed_last_git_line);
    }
    let mut expected = BTreeSet::new();
    for a in ["LF", "CRLF", "CR"] {
        for b in ["LF", "CRLF", "CR"] {
            for state in [
                "untracked",
                "unborn-indexed",
                "staged-new",
                "tracked-modified",
            ] {
                expected.insert(format!("{a}-{b}-{state}"));
            }
        }
    }
    assert_eq!(ids, expected);
    assert_eq!(cases.len(), 36);
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.eligible != case.broken_eligible)
            .count(),
        20
    );
    cases
}

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

async fn output(command: &mut Command) -> Output {
    tokio::time::timeout(Duration::from_secs(20), command.kill_on_drop(true).output())
        .await
        .expect("infrastructure error: subprocess deadline")
        .expect("infrastructure error: subprocess spawn")
}

async fn git(root: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "user.name=Hoimin audit",
            "-c",
            "user.email=audit@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .current_dir(root);
    let result = output(&mut command).await;
    assert!(
        result.status.success(),
        "infrastructure error: git {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

async fn fixture(case: &Case) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::create_dir(root.join("src")).unwrap();
    git(root, &["init", "-q"]).await;
    if matches!(case.git_state.as_str(), "staged-new" | "tracked-modified") {
        git(root, &["commit", "--allow-empty", "-qm", "initial"]).await;
    }
    if case.git_state == "tracked-modified" {
        std::fs::write(root.join("src/subject.py"), &case.original_source).unwrap();
        git(root, &["add", "src/subject.py"]).await;
        git(root, &["commit", "-qm", "before"]).await;
    }
    std::fs::write(root.join("src/subject.py"), &case.source).unwrap();
    if matches!(case.git_state.as_str(), "unborn-indexed" | "staged-new") {
        git(root, &["add", "src/subject.py"]).await;
    }
    directory
}

fn args(root: &Path, mode: &str, changed: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "hoimin".into(),
        mode.into(),
        "--root".into(),
        root.into(),
        "--source".into(),
        "src".into(),
        "--operators".into(),
        "binary_add_sub".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--total-timeout".into(),
        "20s".into(),
    ];
    if changed {
        args.push("--changed".into());
    }
    args.extend([
        "--".into(),
        python().into(),
        "-c".into(),
        "from subject import f; assert f() == 3".into(),
    ]);
    args
}

async fn invoke(args: Vec<OsString>) -> Value {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(40),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .expect("infrastructure error: public CLI deadline");
    assert_eq!(
        code,
        0,
        "infrastructure error: {}",
        String::from_utf8_lossy(&stderr)
    );
    serde_json::from_slice(&stdout).expect("infrastructure error: invalid CLI JSON")
}

async fn assert_external_coordinates(root: &Path, case: &Case) {
    let result = output(Command::new(python()).args(["-c",
        "import ast,json,pathlib,sys; print(json.dumps([n.lineno for n in ast.walk(ast.parse(pathlib.Path(sys.argv[1]).read_bytes())) if isinstance(n,ast.BinOp)]))"])
        .arg(root.join("src/subject.py"))).await;
    assert!(
        result.status.success(),
        "infrastructure error: CPython {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Vec<u32>>(&result.stdout).unwrap(),
        [case.candidate_line]
    );
    if matches!(case.git_state.as_str(), "staged-new" | "tracked-modified") {
        let patch = git(
            root,
            &[
                "diff",
                "--unified=0",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--inter-hunk-context=0",
                "--diff-algorithm=myers",
                "--no-indent-heuristic",
                "HEAD",
            ],
        )
        .await;
        let headers = patch
            .lines()
            .filter(|line| line.starts_with("@@ "))
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 1, "infrastructure error: unexpected hunks");
        let new = headers[0]
            .split_whitespace()
            .nth(2)
            .unwrap()
            .strip_prefix('+')
            .unwrap();
        let (first, count) = new.split_once(',').unwrap_or((new, "1"));
        assert_eq!(first.parse::<u32>().unwrap(), case.changed_first_git_line);
        assert_eq!(
            count.parse::<u32>().unwrap(),
            case.changed_last_git_line - case.changed_first_git_line + 1
        );
    }
}

fn identities(candidates: &[Value]) -> Vec<Value> {
    candidates
        .iter()
        .map(|candidate| {
            let fields = [
                "id",
                "path",
                "span",
                "original",
                "replacement",
                "operator",
                "line",
                "column",
                "symbol",
                "file_hash",
            ];
            Value::Object(
                fields
                    .into_iter()
                    .map(|key| (key.to_owned(), candidate[key].clone()))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn changed_lines_corpus_is_closed_and_rejects_duplicates_and_schema_drift() {
    cases(CORPUS);
    let mut lines = CORPUS.lines().collect::<Vec<_>>();
    lines[1] = lines[0];
    assert!(std::panic::catch_unwind(|| cases(&lines.join("\n"))).is_err());
    assert!(
        std::panic::catch_unwind(|| cases(&CORPUS.replacen("\"schema\":1", "\"schema\":2", 1)))
            .is_err()
    );
}

#[tokio::test]
async fn changed_lines_public_plan_matches_all_lean_cases() {
    let mut mismatches = Vec::new();
    for case in cases(CORPUS) {
        let directory = fixture(&case).await;
        let root = directory.path();
        assert_external_coordinates(root, &case).await;
        let plain = invoke(args(root, "plan", false)).await;
        assert_eq!(plain["diagnostics"], json!([]));
        assert_eq!(plain["truncated"], false);
        let candidates = plain["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{}", case.id);
        assert_eq!(candidates[0]["line"], case.candidate_line);
        assert_eq!(
            candidates[0]["span"],
            json!({"start":case.candidate_offset,"length":1})
        );
        assert_eq!(candidates[0]["original"], "+");
        assert_eq!(candidates[0]["replacement"], "-");
        assert_eq!(
            candidates[0]["file_hash"],
            blake3::hash(case.source.as_bytes()).to_hex().as_str()
        );
        let expected = if case.eligible {
            identities(candidates)
        } else {
            Vec::new()
        };
        let changed = invoke(args(root, "plan", true)).await;
        assert_eq!(changed["diagnostics"], json!([]));
        assert_eq!(changed["truncated"], false);
        let actual = identities(changed["candidates"].as_array().unwrap());
        if actual != expected {
            mismatches.push((case.id, expected, actual));
        }
    }
    assert!(
        mismatches.is_empty(),
        "semantic mismatches: {mismatches:#?}"
    );
}

#[tokio::test]
async fn changed_lines_public_run_and_saved_plan_verify_execute_the_mutant() {
    let selected = [
        "LF-LF-untracked",
        "CRLF-CRLF-tracked-modified",
        "CR-CR-untracked",
        "CR-CR-tracked-modified",
        "LF-CR-staged-new",
        "CRLF-CR-unborn-indexed",
    ];
    for case in cases(CORPUS)
        .into_iter()
        .filter(|case| selected.contains(&case.id.as_str()))
    {
        let directory = fixture(&case).await;
        let root = directory.path();
        let plan = invoke(args(root, "plan", true)).await;
        let report = invoke(args(root, "run", true)).await;
        assert_eq!(
            report["baseline"]["termination"],
            json!({"Exit":0}),
            "{}",
            case.id
        );
        assert_eq!(report["summary"]["counts"]["killed"], 1, "{}", case.id);
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
        let output = tempfile::tempdir().unwrap();
        let saved = output.path().join("plan.json");
        std::fs::write(&saved, serde_json::to_vec(&plan).unwrap()).unwrap();
        let verified = invoke(vec![
            "hoimin".into(),
            "verify".into(),
            saved.into_os_string(),
            "--top".into(),
            "1".into(),
        ])
        .await;
        assert_eq!(verified["summary"]["counts"]["killed"], 1, "{}", case.id);
        assert_eq!(
            verified["mutants"][0]["candidate"]["id"],
            plan["candidates"][0]["id"]
        );
        assert_eq!(
            std::fs::read(root.join("src/subject.py")).unwrap(),
            case.source.as_bytes()
        );
    }
}

#[tokio::test]
async fn changed_codec_rows_preserve_public_identity_and_explicit_selectors() {
    for (header, comment) in [
        (
            b"\xef\xbb\xbf# UTF-8\r\n".as_slice(),
            b"# caf\xc3\xa9\r".as_slice(),
        ),
        (b"# coding: latin-1\r", b"# caf\xe9\r\n"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir(root.join("src")).unwrap();
        git(root, &["init", "-q"]).await;
        let mut source = header.to_vec();
        source.extend_from_slice(comment);
        source.extend_from_slice(b"def f():\r    return 1 - 2\ndef g():\r    return 4 + 5\n");
        std::fs::write(root.join("src/subject.py"), &source).unwrap();
        git(root, &["add", "src/subject.py"]).await;
        git(root, &["commit", "-qm", "before"]).await;
        let target = source
            .windows(b"1 - 2".len())
            .position(|bytes| bytes == b"1 - 2")
            .unwrap()
            + 2;
        source[target] = b'+';
        std::fs::write(root.join("src/subject.py"), &source).unwrap();
        let baseline_manifest = invoke(args(root, "plan", false)).await;
        let expected = baseline_manifest["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|candidate| candidate["symbol"] == "f")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0]["line"], 4);
        assert_eq!(expected[0]["span"], json!({"start":target,"length":1}));
        assert_eq!(
            expected[0]["file_hash"],
            blake3::hash(&source).to_hex().as_str()
        );
        for (line, symbol, selected) in [
            (4, "subject:f", true),
            (3, "subject:f", false),
            (6, "subject:g", false),
        ] {
            let mut selected_args = args(root, "plan", true);
            let separator = selected_args.iter().position(|arg| arg == "--").unwrap();
            selected_args.splice(
                separator..separator,
                [
                    "--line".into(),
                    format!("src/subject.py:{line}").into(),
                    "--symbol".into(),
                    symbol.into(),
                ],
            );
            let plan = invoke(selected_args).await;
            assert_eq!(
                identities(plan["candidates"].as_array().unwrap()),
                if selected {
                    identities(&expected)
                } else {
                    Vec::new()
                }
            );
        }
        let report = invoke(args(root, "run", true)).await;
        assert_eq!(report["summary"]["counts"]["killed"], 1);
        assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
        assert_eq!(std::fs::read(root.join("src/subject.py")).unwrap(), source);
    }
}
