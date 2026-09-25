use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use serde_json::{Value, json};

const FLAG: &str = "HOIMIN_AUDIT_PLAN_ENV";

struct Fixture {
    directory: tempfile::TempDir,
    root: PathBuf,
    marker: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("calc.py"), "value = 1 + 2\n").unwrap();
        let marker = directory.path().join("baseline-marker");
        Self {
            directory,
            root,
            marker,
        }
    }

    async fn plan(&self, selected: bool, value: &OsStr) -> Value {
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
        let code = format!(
            "from pathlib import Path; Path({}).write_text('ran'); import calc",
            serde_json::to_string(&self.marker).unwrap()
        );
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["plan", "--root"])
            .arg(&self.root)
            .args([
                "--file",
                "calc.py",
                "--operators",
                "binary_add_sub",
                "--max-mutants",
                "1",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
            ])
            .env(FLAG, value)
            .env_remove("PYTHONPATH");
        if selected {
            command.args(["--fingerprint-env", FLAG]);
        }
        command.arg("--").arg(python).args(["-c", &code]);
        let output = bounded(command).await;
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert!(!self.marker.exists());
        serde_json::from_slice(&output.stdout).unwrap()
    }

    async fn verify(&self, manifest: &Value, value: &OsStr) -> Output {
        let path = self.directory.path().join("plan.json");
        std::fs::write(&path, serde_json::to_vec(manifest).unwrap()).unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .arg("verify")
            .arg(path)
            .args(["--top", "1"])
            .env(FLAG, value)
            .env_remove("PYTHONPATH");
        bounded(command).await
    }
}

async fn bounded(mut command: tokio::process::Command) -> Output {
    command.kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("plan/verify fixture deadline")
        .unwrap()
}

#[tokio::test]
async fn plan_tracks_environment_without_plaintext_and_verify_checks_before_baseline() {
    let secret = "private-plan-environment-marker-628-unique";
    for changed in [false, true] {
        let fixture = Fixture::new();
        let manifest = fixture.plan(true, OsStr::new(secret)).await;
        assert_eq!(manifest["schema_version"], 5);
        assert_eq!(
            manifest["normalized_config"]["fingerprint_env"],
            json!([FLAG])
        );
        assert!(!manifest.to_string().contains(secret));
        let output = fixture
            .verify(
                &manifest,
                OsStr::new(if changed { "different" } else { secret }),
            )
            .await;
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
        if changed {
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("plan.fingerprint_env.changed")
            );
            assert!(!fixture.marker.exists());
        } else {
            assert_eq!(
                output.status.code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(fixture.marker.exists());
        }
    }
}

#[tokio::test]
async fn verify_rejects_inconsistent_environment_metadata_and_old_plan_schema() {
    let fixture = Fixture::new();
    let manifest = fixture.plan(true, OsStr::new("value")).await;
    let mut cases = Vec::new();
    for hash in [
        Value::Null,
        json!(""),
        json!("g".repeat(64)),
        json!("A".repeat(64)),
        json!(42),
    ] {
        let mut changed = manifest.clone();
        changed["normalized_config"]["fingerprint_env_hash"] = hash;
        cases.push(changed);
    }
    for names in [
        json!([]),
        json!([FLAG, FLAG]),
        json!(["Z", "A"]),
        json!(["NAME=VALUE"]),
    ] {
        let mut changed = manifest.clone();
        changed["normalized_config"]["fingerprint_env"] = names;
        cases.push(changed);
    }
    let mut old = manifest.clone();
    old["schema_version"] = json!(4);
    cases.push(old);
    for changed in cases {
        let output = fixture.verify(&changed, OsStr::new("value")).await;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!fixture.marker.exists());
    }
}

#[tokio::test]
async fn untracked_plan_omits_optional_fields_and_ignores_environment_changes() {
    let fixture = Fixture::new();
    let manifest = fixture.plan(false, OsStr::new("before")).await;
    assert!(
        manifest["normalized_config"]
            .get("fingerprint_env")
            .is_none()
    );
    assert!(
        manifest["normalized_config"]
            .get("fingerprint_env_hash")
            .is_none()
    );
    let output = fixture.verify(&manifest, OsStr::new("after")).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(fixture.marker.exists());
}
