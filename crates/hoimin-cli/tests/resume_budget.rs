use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

struct Fixture {
    temporary: tempfile::TempDir,
    root: PathBuf,
    killed: bool,
}

impl Fixture {
    fn new(killed: bool) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("project");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("calc.py"), "x = 1 + 2\ny = 3 + 4\nz = 5 + 6\n").unwrap();
        Self {
            temporary,
            root,
            killed,
        }
    }

    fn run(&self, limit: usize, resume: bool, database: &str) -> (Value, Value) {
        let python = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            });
        let metrics = self.temporary.path().join("metrics.json");
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["run", "--root"])
            .arg(&self.root)
            .args([
                "--file",
                "calc.py",
                "--operators",
                "binary_add_sub",
                "--jobs",
                "1",
                "--max-mutants",
                &limit.to_string(),
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--session",
            ])
            .arg(self.temporary.path().join(database))
            .arg("--metrics")
            .arg(&metrics);
        if resume {
            command.arg("--resume");
        }
        command
            .arg("--")
            .arg(python)
            .args([
                "-c",
                if self.killed {
                    "import calc; assert (calc.x, calc.y, calc.z) == (3, 7, 11)"
                } else {
                    "import calc"
                },
            ])
            .env_remove("PYTHONPATH");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let output = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::process::Command::from(command)
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .expect("resume CLI deadline")
            .unwrap()
        });
        let expected_exit = if limit < 3 {
            4
        } else {
            i32::from(!self.killed)
        };
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(
            report["run"]["normalized_config"]["limits"]["max_mutants"],
            limit
        );
        let metrics = serde_json::from_slice(&fs::read(metrics).unwrap()).unwrap();
        (report, metrics)
    }
}

#[test]
fn increasing_budget_reuses_stable_prefix_and_keeps_cumulative_accounting() {
    for killed in [true, false] {
        let fixture = Fixture::new(killed);
        let verdict = if killed { "killed" } else { "survived" };
        let (initial, metrics) = fixture.run(1, false, "session.sqlite");
        assert_eq!(metrics["executed"], 1);
        let (same, metrics) = fixture.run(1, true, "session.sqlite");
        assert_eq!(same["run"]["run_id"], initial["run"]["run_id"]);
        assert_eq!(metrics["executed"], 0);
        let (increased, metrics) = fixture.run(2, true, "session.sqlite");
        assert_eq!(increased["run"]["run_id"], initial["run"]["run_id"]);
        assert_eq!(metrics["executed"], 1);
        assert_eq!(increased["summary"]["complete"], false);
        assert_eq!(increased["mutants"][0]["status"], verdict);
        assert!(increased["mutants"][0]["termination"].is_null());
        assert_eq!(increased["mutants"][1]["status"], verdict);
        assert!(!increased["mutants"][1]["termination"].is_null());
        assert_eq!(increased["mutants"][2]["status"], "not_run");
        let (fresh, fresh_metrics) = fixture.run(2, false, "fresh.sqlite");
        assert_eq!(fresh_metrics["executed"], 2);
        for index in 0..3 {
            assert_eq!(
                fresh["mutants"][index]["status"],
                increased["mutants"][index]["status"]
            );
            assert_eq!(
                fresh["mutants"][index]["candidate"],
                increased["mutants"][index]["candidate"]
            );
        }
        let (decreased, metrics) = fixture.run(1, true, "session.sqlite");
        assert_ne!(decreased["run"]["run_id"], initial["run"]["run_id"]);
        assert_eq!(metrics["executed"], 1);
    }
}

#[test]
fn increased_budget_can_complete_then_completed_run_remains_ineligible() {
    for killed in [true, false] {
        let fixture = Fixture::new(killed);
        let (initial, _) = fixture.run(1, false, "session.sqlite");
        let (completed, metrics) = fixture.run(3, true, "session.sqlite");
        assert_eq!(completed["run"]["run_id"], initial["run"]["run_id"]);
        assert_eq!(completed["summary"]["complete"], true);
        assert_eq!(metrics["executed"], 2);
        assert!(completed["mutants"][0]["termination"].is_null());
        let (increased, metrics) = fixture.run(4, true, "session.sqlite");
        assert_ne!(increased["run"]["run_id"], initial["run"]["run_id"]);
        assert_eq!(metrics["executed"], 3);
    }
}
