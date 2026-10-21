#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

fn run_args(root: &Path, format: &str, fixed: Option<&str>, code: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["hoimin", "run", "--root"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(root.into());
    args.extend(
        [
            "--file",
            "sample.py",
            "--operators",
            "boolean_literal",
            "--max-mutants",
            "1",
            "--allow-best-effort-memory",
            "--format",
            format,
        ]
        .into_iter()
        .map(OsString::from),
    );
    if let Some(value) = fixed {
        args.extend(["--mutant-timeout".into(), value.into()]);
    }
    let python = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        });
    args.extend([
        "--".into(),
        python.into_os_string(),
        "-c".into(),
        code.into(),
    ]);
    args
}

async fn invoke(args: Vec<OsString>) -> (i32, String) {
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let exit = tokio::time::timeout(
        Duration::from_secs(30),
        hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()),
    )
    .await
    .expect("CLI test deadline");
    assert!(
        !stdout.snapshot().is_empty(),
        "{}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    (exit, String::from_utf8(stdout.snapshot()).unwrap())
}

#[tokio::test]
async fn auto_timeout_is_reported_by_the_public_json_command() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample.py"), "x = True\n").unwrap();
    let (exit, text) = invoke(run_args(root.path(), "json", None, "pass")).await;
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(exit, 1);
    assert_eq!(
        report["summary"]["resolved_mutant_timeout"],
        serde_json::json!({"secs":5,"nanos":0})
    );
}

#[tokio::test]
async fn fixed_timeout_is_reported_by_the_public_jsonl_command() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample.py"), "x = True\n").unwrap();
    let (exit, text) = invoke(run_args(root.path(), "jsonl", Some("1.234567891s"), "pass")).await;
    let report: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(exit, 1);
    assert_eq!(
        report["resolved_mutant_timeout"],
        serde_json::json!({"secs":1,"nanos":234_567_891})
    );
}

#[tokio::test]
async fn fixed_timeout_is_reported_precisely_by_the_public_human_command() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample.py"), "x = True\n").unwrap();
    let (exit, text) = invoke(run_args(root.path(), "human", Some("1.234567891s"), "pass")).await;
    let duration = text
        .lines()
        .find_map(|line| line.strip_prefix("  resolved mutant timeout: "))
        .unwrap();
    assert_eq!(exit, 1);
    assert_eq!(
        humantime::parse_duration(duration).unwrap(),
        Duration::new(1, 234_567_891)
    );
}

async fn assert_failed_baseline_leaves_timeout_unresolved(policy: Option<&str>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample.py"), "x = True\n").unwrap();
    let (exit, text) = invoke(run_args(root.path(), "json", policy, "raise SystemExit(1)")).await;
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(exit, 3);
    assert_eq!(
        report["baseline"]["termination"],
        serde_json::json!({"Exit":1})
    );
    assert_eq!(
        report["summary"].get("resolved_mutant_timeout"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(report["mutants"], serde_json::json!([]));
}

#[tokio::test]
async fn failed_baseline_does_not_report_an_auto_timeout_as_resolved() {
    assert_failed_baseline_leaves_timeout_unresolved(None).await;
}

#[tokio::test]
async fn failed_baseline_does_not_report_a_fixed_timeout_as_resolved() {
    assert_failed_baseline_leaves_timeout_unresolved(Some("1.234567891s")).await;
}

#[tokio::test]
async fn verify_reports_the_saved_fixed_timeout() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample.py"), "x = True\n").unwrap();
    let mut args = run_args(root.path(), "json", Some("1.234567891s"), "pass");
    args[1] = "plan".into();
    for option in ["--max-mutants", "--format"] {
        let index = args.iter().position(|arg| arg == option).unwrap();
        args.drain(index..index + 2);
    }
    let (exit, text) = invoke(args).await;
    assert_eq!(exit, 0);
    let manifest = root.path().join("plan.json");
    std::fs::write(&manifest, text).unwrap();
    let (exit, text) = invoke(vec![
        "hoimin".into(),
        "verify".into(),
        manifest.into_os_string(),
        "--top".into(),
        "1".into(),
    ])
    .await;
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(exit, 1);
    assert_eq!(
        report["summary"]["resolved_mutant_timeout"],
        serde_json::json!({"secs":1,"nanos":234_567_891})
    );
}
