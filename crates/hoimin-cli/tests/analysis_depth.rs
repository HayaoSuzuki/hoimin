//! Public subprocess regressions: a native stack overflow must not kill the test runner.
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

fn bounded_output(command: &mut Command) -> Output {
    // Files avoid pipe backpressure while the parent enforces the deadline.
    let stdout = tempfile::NamedTempFile::new().unwrap();
    let stderr = tempfile::NamedTempFile::new().unwrap();
    let mut child = command
        .stdout(Stdio::from(stdout.reopen().unwrap()))
        .stderr(Stdio::from(stderr.reopen().unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "analysis subprocess timed out: {}",
                std::fs::read_to_string(stderr.path()).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(stdout.path()).unwrap(),
        stderr: std::fs::read(stderr.path()).unwrap(),
    }
}

#[test]
fn plan_and_run_reject_deep_ast_with_path_and_cause() {
    for operation in ["plan", "run"] {
        for terms in [2_000, 20_000, 25_000] {
            let root = tempfile::tempdir().unwrap();
            let source = format!("value = {}\n", vec!["1"; terms].join("+"));
            std::fs::write(root.path().join("calc.py"), &source).unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
            command
                .args([operation, "--root"])
                .arg(root.path())
                .args([
                    "--file",
                    "calc.py",
                    "--operators",
                    "binary_add_sub",
                    "--max-candidates",
                    "1",
                    "--min-free-space",
                    "1B",
                    "--allow-best-effort-memory",
                ])
                .env_remove("RUST_MIN_STACK");
            if operation == "run" {
                command.args(["--format", "json"]);
            }
            command.arg("--").arg(python()).args(["-c", "pass"]);
            let output = bounded_output(&mut command);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.code().is_some_and(|code| code != 0),
                "{operation}/{terms}: {output:?}"
            );
            assert!(
                stderr.contains("calc.py")
                    && stderr.contains("analysis depth")
                    && stderr.contains("128"),
                "{operation}/{terms}: {stderr}"
            );
            assert_eq!(
                std::fs::read_to_string(root.path().join("calc.py")).unwrap(),
                source
            );
            if operation == "plan" {
                assert!(output.stdout.is_empty(), "failed analysis emitted a plan");
            } else {
                let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(report["summary"]["complete"], false);
                assert!(
                    !report["baseline"].is_null(),
                    "run still executes its baseline first"
                );
            }
        }
    }
}

#[test]
fn invalid_deep_partial_trees_keep_invalid_syntax_behavior() {
    let deep = vec!["1"; 25_000].join("+");
    for source in [
        format!("value = {deep}+\n"),
        format!("value = {deep}\nbroken =\n"),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("calc.py"), source).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["plan", "--root"])
            .arg(root.path())
            .args([
                "--file",
                "calc.py",
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
                "--",
            ])
            .arg(python())
            .args(["-c", "pass"])
            .env_remove("RUST_MIN_STACK");
        let output = bounded_output(&mut command);
        assert!(output.status.code().is_some(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("calc.py") && stderr.contains("syntax"),
            "{stderr}"
        );
        assert!(
            !stderr.contains("analysis depth"),
            "invalid syntax semantics changed: {stderr}"
        );
    }
}
