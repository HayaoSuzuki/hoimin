use std::ffi::OsString;
use std::process::Output;

use tempfile::TempDir;

struct Project {
    root: TempDir,
    evidence: TempDir,
}

impl Project {
    fn new() -> Self {
        let project = Self {
            root: tempfile::tempdir().unwrap(),
            evidence: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir(project.root.path().join("src")).unwrap();
        std::fs::create_dir(project.root.path().join("empty")).unwrap();
        std::fs::write(
            project.root.path().join("src/calc.py"),
            "def add(a, b):\n    return a + b\n",
        )
        .unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "fixture",
            ],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(project.root.path())
                    .status()
                    .unwrap()
                    .success()
            );
        }
        project
    }

    async fn invoke(&self, args: Vec<OsString>) -> Output {
        let temporary = tempfile::tempdir().unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(args)
            .env("TMPDIR", temporary.path())
            .env("TMP", temporary.path())
            .env("TEMP", temporary.path())
            .kill_on_drop(true);
        tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
            .await
            .unwrap()
            .unwrap()
    }

    async fn select(&self, command: &str, selectors: &[&str]) -> Output {
        let marker = self.evidence.path().join("baseline");
        let python = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
            "../../.venv/Scripts/python.exe"
        } else {
            "../../.venv/bin/python"
        });
        assert!(
            python.is_file(),
            "missing controlled Python: {}",
            python.display()
        );
        let script = format!(
            "from pathlib import Path; Path({:?}).touch()",
            marker.to_str().unwrap()
        );
        let mut args = vec![
            command.into(),
            "--root".into(),
            self.root.path().as_os_str().to_owned(),
        ];
        args.extend(selectors.iter().map(OsString::from));
        args.extend(
            [
                "--operators",
                "binary_add_sub",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--",
            ]
            .map(OsString::from),
        );
        args.push(python.into_os_string());
        args.push("-c".into());
        args.push(script.into());
        self.invoke(args).await
    }
}

fn assert_missing(output: &Output, path: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostic.contains("source")
            && diagnostic.contains("does not exist")
            && diagnostic.contains(path),
        "{diagnostic}"
    );
}

#[tokio::test]
async fn missing_sources_are_rejected_before_baseline_even_when_mixed_or_changed_clean() {
    for command in ["run", "plan"] {
        for selectors in [
            vec!["--source", "srrc"],
            vec!["--source", "src", "--source", "srrc"],
            vec!["--source", "srrc", "--changed"],
        ] {
            let project = Project::new();
            let output = project.select(command, &selectors).await;
            assert_missing(&output, "srrc");
            assert!(!project.evidence.path().join("baseline").exists());
        }
    }
}

#[tokio::test]
async fn legitimate_empty_sources_still_succeed() {
    for command in ["run", "plan"] {
        for selectors in [
            vec!["--source", "empty"],
            vec!["--source", "src", "--changed"],
            vec!["--source", "src", "--exclude", "src"],
        ] {
            let project = Project::new();
            let output = project.select(command, &selectors).await;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            if command == "plan" {
                assert!(
                    value["candidates"].as_array().unwrap().is_empty(),
                    "unexpected candidates: {}",
                    value["candidates"]
                );
            } else {
                assert!(
                    value["mutants"].as_array().unwrap().is_empty(),
                    "unexpected mutants: {}",
                    value["mutants"]
                );
                assert!(project.evidence.path().join("baseline").exists());
            }
        }
    }
}

#[tokio::test]
async fn saved_plan_rechecks_all_sources_before_baseline() {
    let project = Project::new();
    let output = project
        .select("plan", &["--source", "src", "--source", "empty"])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = project.evidence.path().join("plan.json");
    std::fs::write(&manifest, output.stdout).unwrap();
    std::fs::remove_dir(project.root.path().join("empty")).unwrap();
    let verified = project
        .invoke(vec![
            "verify".into(),
            manifest.into_os_string(),
            "--top".into(),
            "1".into(),
        ])
        .await;
    assert_missing(&verified, "empty");
    assert!(!project.evidence.path().join("baseline").exists());
}
