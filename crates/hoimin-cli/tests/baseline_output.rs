use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        })
}

async fn run(root: &Path, format: &str, script: &str, extra: &[&str]) -> std::process::Output {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--root"])
            .arg(root)
            .args([
                "--file",
                "case.py",
                "--format",
                format,
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--total-timeout",
                "5s",
            ])
            .args(extra)
            .arg("--")
            .arg(python())
            .args(["-B", "-c", script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CLI deadline")
    .unwrap()
}

fn diagnostics(output: &std::process::Output, format: &str) -> String {
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    if format == "human" {
        return stderr;
    }
    stderr
        .lines()
        .map(|line| {
            let event: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(event["kind"], "diagnostic");
            assert!(event["run_id"].as_str().is_some_and(|id| !id.is_empty()));
            event["message"].as_str().unwrap().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn failed_baseline_logs_reach_all_cli_formats_before_clean_cleanup() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("case.py"), "value = 1 + 2\n").unwrap();
    let script = "import sys; print('AUDIT_BASELINE_STDOUT', flush=True); print('AUDIT_BASELINE_STDERR', file=sys.stderr, flush=True); raise SystemExit(7)";
    for format in ["human", "json", "jsonl"] {
        let output = run(root.path(), format, script, &[]).await;
        assert_eq!(
            output.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let logs = diagnostics(&output, format);
        assert!(logs.contains("AUDIT_BASELINE_STDOUT"), "{logs}");
        assert!(logs.contains("AUDIT_BASELINE_STDERR"), "{logs}");
        assert!(logs.contains("truncated=false"), "{logs}");
        if format == "json" {
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["baseline"]["termination"]["Exit"], 7);
            assert!(
                report["summary"]["disk"]["cleanup"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|record| record["root_id"] == "execution" && record["status"] == "clean")
            );
        } else if format == "jsonl" {
            let events: Vec<serde_json::Value> = output
                .stdout
                .split(|b| *b == b'\n')
                .filter(|s| !s.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect();
            assert_eq!(events.last().unwrap()["kind"], "run_finished");
        }
    }
}

#[tokio::test]
async fn baseline_output_retains_tail_and_decodes_split_unicode_and_invalid_bytes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("case.py"), "value = 1 + 2\n").unwrap();
    let output = run(
        root.path(),
        "json",
        "import os; os.write(1, b'PREFIX12345678'); raise SystemExit(1)",
        &["--max-output", "8B"],
    )
    .await;
    assert_eq!(output.status.code(), Some(3));
    let logs = diagnostics(&output, "json");
    assert!(
        logs.contains("retained=8 observed=14 truncated=true"),
        "{logs}"
    );
    assert!(logs.contains("12345678"));
    assert!(!logs.contains("PREFIX"));
    let output = run(root.path(), "jsonl", "import os; os.write(1, b'x'*16383 + '🦀'.encode() + b'TAIL\\xff\\x1b\\xf0\\x9f'); raise SystemExit(1)", &["--max-output", "64KiB"]).await;
    assert_eq!(output.status.code(), Some(3));
    let logs = diagnostics(&output, "jsonl");
    assert_eq!(logs.matches('\u{1f980}').count(), 1);
    assert_eq!(
        logs.matches('\u{fffd}').count(),
        2,
        "invalid byte and incomplete EOF scalar should be replaced"
    );
    assert!(logs.contains("TAIL"));
    assert!(
        !logs.contains('\u{1b}'),
        "terminal controls must be escaped"
    );
    assert!(logs.contains("\\u{1b}"));
}

#[tokio::test]
async fn timed_out_baseline_exports_logs_and_success_is_quiet() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("case.py"), "value = 1 + 2\n").unwrap();
    let output = run(
        root.path(),
        "json",
        "import time; print('BASELINE_TIMEOUT_LOG', flush=True); time.sleep(20)",
        &["--baseline-timeout", "1s"],
    )
    .await;
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(diagnostics(&output, "json").contains("BASELINE_TIMEOUT_LOG"));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"], "Timeout");
    let output = run(root.path(), "json", "print('SUCCESS_LOG')", &[]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(!diagnostics(&output, "json").contains("baseline.output"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SUCCESS_LOG"));
}

#[tokio::test]
async fn verify_failed_baseline_exports_logs_before_cleanup() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("case.py"), "value = 1 + 2\n").unwrap();
    let plan = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["plan", "--root"])
            .arg(root.path())
            .args([
                "--file",
                "case.py",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--total-timeout",
                "5s",
                "--",
            ])
            .arg(python())
            .args([
                "-B",
                "-c",
                "print('VERIFY_BASELINE_LOG'); raise SystemExit(7)",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("plan deadline")
    .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let path = root.path().join("plan.json");
    std::fs::write(&path, plan.stdout).unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .arg("verify")
            .arg(path)
            .args(["--top", "1"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("verify deadline")
    .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(diagnostics(&output, "json").contains("VERIFY_BASELINE_LOG"));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 7);
}
