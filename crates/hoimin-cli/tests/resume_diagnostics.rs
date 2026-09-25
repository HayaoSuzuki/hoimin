use serde_json::{Value, json};
use std::path::Path;

struct Fixture {
    directory: tempfile::TempDir,
}

struct Run {
    code: i32,
    stdout: String,
    header: Option<Value>,
    report: Option<Value>,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("project")).unwrap();
        std::fs::write(
            directory.path().join("project/calc.py"),
            "value = 1 + 2\nother = 4 + 5\n",
        )
        .unwrap();
        Self { directory }
    }

    async fn run(&self, database: &str, resume: bool, budget: usize, format: &str) -> Run {
        let python = Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
            "../../.venv/Scripts/python.exe"
        } else {
            "../../.venv/bin/python"
        });
        assert!(python.is_file());
        let tmp = tempfile::tempdir().unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["run", "--root"])
            .arg(self.directory.path().join("project"))
            .args([
                "--file",
                "calc.py",
                "--operators",
                "binary_add_sub",
                "--jobs",
                "1",
                "--max-mutants",
            ])
            .arg(budget.to_string())
            .args([
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--format",
                format,
                "--session",
            ])
            .arg(self.directory.path().join(database))
            .env_remove("PYTHONPATH")
            .env("TMPDIR", tmp.path())
            .env("TMP", tmp.path())
            .env("TEMP", tmp.path())
            .kill_on_drop(true);
        if resume {
            command.arg("--resume");
        }
        command.arg("--").arg(python).args([
            "-c",
            "import calc; assert calc.value == 3 and calc.other == 9",
        ]);
        let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let report = if format == "json" {
            Some(serde_json::from_str::<Value>(&stdout).unwrap())
        } else {
            None
        };
        let header = if let Some(value) = &report {
            Some(value["run"].clone())
        } else if format == "jsonl" {
            Some(serde_json::from_str(stdout.lines().next().unwrap()).unwrap())
        } else {
            None
        };
        Run {
            code: output.status.code().unwrap(),
            stdout,
            header,
            report,
        }
    }
}

#[tokio::test]
async fn fresh_resume_reasons_and_reused_outcomes_are_visible_without_changing_execution() {
    let fixture = Fixture::new();
    let first = fixture.run("session.db", true, 1, "json").await;
    assert_eq!(first.code, 4);
    let first_header = first.header.unwrap();
    assert_eq!(
        first_header["resume"],
        json!({"status":"fresh", "reason":"no_prior_run"})
    );
    let second = fixture.run("session.db", true, 1, "json").await;
    assert_eq!(second.code, 4);
    let second_header = second.header.unwrap();
    assert_eq!(second_header["resume"], json!({"status":"resumed"}));
    assert_eq!(first_header["run_id"], second_header["run_id"]);
    let report = second.report.unwrap();
    assert_eq!(report["mutants"][0]["status"], "killed");
    assert!(report["mutants"][0]["termination"].is_null());
    assert_eq!(report["baseline"]["termination"], json!({"Exit":0}));

    std::fs::write(
        fixture.directory.path().join("project/calc.py"),
        "value = 1 + 2\nother = 4 + 5\n# changed\n",
    )
    .unwrap();
    let mismatch = fixture.run("session.db", true, 1, "json").await;
    assert_eq!(mismatch.code, 4);
    let header = mismatch.header.unwrap();
    assert_eq!(
        header["resume"],
        json!({"status":"fresh", "reason":"fingerprint_mismatch"})
    );
    assert_ne!(header["run_id"], first_header["run_id"]);
    assert_eq!(
        mismatch.report.unwrap()["mutants"][0]["termination"],
        json!({"Exit":1})
    );
}

#[tokio::test]
async fn complete_and_decreased_budget_history_have_specific_reasons() {
    let fixture = Fixture::new();
    let complete = fixture.run("complete.db", false, 2, "json").await;
    assert_eq!(complete.code, 0);
    assert!(complete.header.unwrap().get("resume").is_none());
    let restarted = fixture.run("complete.db", true, 2, "json").await;
    assert_eq!(restarted.code, 0);
    assert_eq!(
        restarted.header.unwrap()["resume"],
        json!({"status":"fresh", "reason":"matching_run_complete"})
    );

    std::fs::write(
        fixture.directory.path().join("project/calc.py"),
        "value = 1 + 2\nother = 4 + 5\nthird = 6 + 7\n",
    )
    .unwrap();
    assert_eq!(fixture.run("budget.db", false, 2, "json").await.code, 4);
    let lower = fixture.run("budget.db", true, 1, "json").await;
    assert_eq!(lower.code, 4);
    assert_eq!(
        lower.header.unwrap()["resume"],
        json!({"status":"fresh", "reason":"budget_decreased"})
    );
}

#[tokio::test]
async fn jsonl_and_human_report_resume_at_start() {
    for format in ["jsonl", "human"] {
        let fixture = Fixture::new();
        let fresh = fixture.run("session.db", true, 1, format).await;
        assert_eq!(fresh.code, 4);
        let resumed = fixture.run("session.db", true, 1, format).await;
        assert_eq!(resumed.code, 4);
        if format == "jsonl" {
            assert_eq!(
                fresh.header.unwrap()["resume"],
                json!({"status":"fresh", "reason":"no_prior_run"})
            );
            assert_eq!(
                resumed.header.unwrap()["resume"],
                json!({"status":"resumed"})
            );
            for output in [&fresh.stdout, &resumed.stdout] {
                for line in output.lines() {
                    serde_json::from_str::<Value>(line).unwrap();
                }
            }
        } else {
            assert!(
                fresh
                    .stdout
                    .lines()
                    .nth(1)
                    .unwrap()
                    .contains("starting new run [no_prior_run]"),
                "{}",
                fresh.stdout
            );
            assert!(
                resumed
                    .stdout
                    .lines()
                    .nth(1)
                    .unwrap()
                    .contains("continuing saved run"),
                "{}",
                resumed.stdout
            );
        }
    }
}
