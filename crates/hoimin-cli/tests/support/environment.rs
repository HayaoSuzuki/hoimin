use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

pub const FLAG: &str = "HOIMIN_AUDIT_ENV_FLAG";

pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub root: PathBuf,
}

pub struct Run {
    pub exit_code: i32,
    pub report: Value,
    pub metrics: Value,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("calc.py"), "value = 1 + 2\nother = 4 + 5\n").unwrap();
        Self { directory, root }
    }

    pub async fn run(
        &self,
        database: &str,
        resume: bool,
        names: &[&str],
        value: Option<&OsStr>,
    ) -> Run {
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
        let metrics = self.directory.path().join("metrics.json");
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
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
                "1",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--session",
            ])
            .arg(self.directory.path().join(database))
            .arg("--metrics")
            .arg(&metrics)
            .env_remove("PYTHONPATH")
            .env_remove(FLAG)
            .kill_on_drop(true);
        if let Some(value) = value {
            command.env(FLAG, value);
        }
        if resume {
            command.arg("--resume");
        }
        for name in names {
            command.args(["--fingerprint-env", name]);
        }
        command.arg("--").arg(python).args(["-c", "import os,calc; assert os.environ.get('HOIMIN_AUDIT_ENV_FLAG','unset') not in ('unset','1') or calc.value==3"]);
        let output = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .expect("infrastructure error: environment fixture CLI timed out")
            .expect("infrastructure error: environment fixture CLI spawn failed");
        let exit_code = output
            .status
            .code()
            .expect("infrastructure error: CLI terminated by signal");
        assert!(
            matches!(exit_code, 0 | 1 | 3 | 4),
            "infrastructure error: exit={exit_code} stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "unexpected diagnostic: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        let metrics = serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
        Run {
            exit_code,
            report,
            metrics,
        }
    }
}
