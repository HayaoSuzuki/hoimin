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
        assert_eq!(output.stderr, Vec::<u8>::new());
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
    for (environment, exit_code, marker_exists, expected_errors) in [
        (secret, 1, true, [].as_slice()),
        (
            "different",
            2,
            false,
            ["plan.fingerprint_env.changed"].as_slice(),
        ),
    ] {
        let fixture = Fixture::new();
        let manifest = fixture.plan(true, OsStr::new(secret)).await;
        assert_eq!(manifest["schema_version"], 8);
        assert_eq!(
            manifest["normalized_config"]["environment"]["names"],
            json!([FLAG])
        );
        assert!(!manifest.to_string().contains(secret));
        let output = fixture.verify(&manifest, OsStr::new(environment)).await;
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
        assert_eq!(
            output.status.code(),
            Some(exit_code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fixture.marker.exists(), marker_exists);
        assert_eq!(output.stdout.is_empty(), !expected_errors.is_empty());
        for expected_error in expected_errors {
            assert!(String::from_utf8_lossy(&output.stderr).contains(expected_error));
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
        changed["normalized_config"]["environment"]["digest"] = hash;
        cases.push(changed);
    }
    for names in [
        json!([]),
        json!([FLAG, FLAG]),
        json!(["Z", "A"]),
        json!(["NAME=VALUE"]),
    ] {
        let mut changed = manifest.clone();
        changed["normalized_config"]["environment"]["names"] = names;
        cases.push(changed);
    }
    for version in [4, 5] {
        let mut old = manifest.clone();
        old["schema_version"] = json!(version);
        cases.push(old);
    }
    for changed in cases {
        let output = fixture.verify(&changed, OsStr::new("value")).await;
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!fixture.marker.exists());
    }
}

#[tokio::test]
async fn untracked_plan_persists_explicit_state_and_ignores_environment_changes() {
    let fixture = Fixture::new();
    let manifest = fixture.plan(false, OsStr::new("before")).await;
    assert_eq!(
        manifest["normalized_config"]["environment"],
        json!({"state":"untracked"})
    );
    let output = fixture.verify(&manifest, OsStr::new("after")).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(fixture.marker.exists());
}

#[tokio::test]
async fn plan_persists_one_explicit_environment_snapshot() {
    let fixture = Fixture::new();
    let manifest = fixture
        .plan(true, OsStr::new("private-snapshot-value"))
        .await;
    let config = &manifest["normalized_config"];
    assert_eq!(config["environment"]["state"], "captured");
    assert_eq!(config["environment"]["names"], json!([FLAG]));
    assert_eq!(config["environment"]["digest"].as_str().unwrap().len(), 64);
    assert!(config.get("fingerprint_env").is_none());
    assert!(config.get("fingerprint_env_hash").is_none());
    assert!(!manifest.to_string().contains("private-snapshot-value"));
}
