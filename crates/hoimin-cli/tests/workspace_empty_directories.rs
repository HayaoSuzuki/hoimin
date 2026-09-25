use camino::Utf8Path;
use hoimin_cli::workspace::{CopyOptions, WorkspacePlan};
use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};
use std::fs;

fn worker(plan: &WorkspacePlan) -> hoimin_cli::workspace::WorkerWorkspace {
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: plan.aggregate_bytes(),
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
    plan.create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
        .unwrap()
}

#[test]
fn empty_directories_survive_creation_reset_and_original_integrity_checks() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::create_dir_all(root.join("fixtures/empty/nested")).unwrap();
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
    assert_eq!(plan.aggregate_bytes(), 0);
    let mut copied = worker(&plan);
    let empty = copied.root().join("fixtures/empty/nested");
    assert!(
        empty.is_dir(),
        "selected empty fixture directory must be copied"
    );
    fs::remove_dir_all(copied.root().join("fixtures")).unwrap();
    fs::write(copied.root().join("fixtures"), "wrong type").unwrap();
    fs::create_dir_all(copied.root().join("extra/nested")).unwrap();
    copied.reset().unwrap();
    assert!(empty.is_dir());
    assert!(!copied.root().join("extra").exists());
    fs::write(empty.join("generated.txt"), "mutant output").unwrap();
    copied.reset().unwrap();
    assert!(empty.is_dir());
    assert!(!empty.join("generated.txt").exists());
    plan.verify_originals().unwrap();
    fs::remove_dir(root.join("fixtures/empty/nested")).unwrap();
    assert!(plan.verify_originals().is_err());
}

#[test]
fn explicit_includes_preserve_only_selected_ignored_empty_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
    for directory in [
        "normal/empty",
        "ignored/wanted/empty",
        "ignored/unwanted/empty",
        "excluded/empty",
    ] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    let options = CopyOptions {
        includes: vec!["ignored/wanted/**".into()],
        excludes: vec!["excluded/**".into()],
        ..CopyOptions::default()
    };
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, options).unwrap();
    let mut copied = worker(&plan);
    assert!(copied.root().join("normal/empty").is_dir());
    assert!(copied.root().join("ignored/wanted/empty").is_dir());
    assert!(!copied.root().join("ignored/unwanted").exists());
    assert!(!copied.root().join("excluded/empty").exists());
    // A children-only glob does not exclude its parent directory itself.
    assert!(copied.root().join("excluded").is_dir());
    fs::remove_dir_all(copied.root().join("ignored")).unwrap();
    copied.reset().unwrap();
    assert!(copied.root().join("ignored/wanted/empty").is_dir());
    assert!(!copied.root().join("ignored/unwanted").exists());
}

#[test]
fn lean_directory_cases_match_public_create_and_repeated_reset() {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(clippy::struct_excessive_bools)] // Exact Lean corpus contract.
    struct Case {
        schema: u8,
        mode: String,
        id: String,
        selected: bool,
        excluded: bool,
        initial: String,
        extra: bool,
        directory: bool,
        extra_after: bool,
    }
    let mut ids = std::collections::BTreeSet::new();
    for line in include_str!("../../../formal/HoiminOracle/corpus/empty-directory.jsonl").lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "strict");
        assert_eq!(
            case.id,
            format!(
                "{}-{}-{}-{}",
                case.selected, case.excluded, case.initial, case.extra
            )
        );
        assert!(ids.insert(case.id));
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        if case.selected {
            fs::create_dir_all(root.join("fixture/empty")).unwrap();
        }
        let options = CopyOptions {
            excludes: if case.excluded {
                vec!["fixture/**".into()]
            } else {
                vec![]
            },
            ..CopyOptions::default()
        };
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, options).unwrap();
        assert_eq!(plan.aggregate_bytes(), 0);
        let mut copied = worker(&plan);
        let path = copied.root().join("fixture/empty");
        assert_eq!(path.is_dir(), case.directory);
        if path.exists() {
            fs::remove_dir(&path).unwrap();
        }
        match case.initial.as_str() {
            "absent" => {}
            "file" => {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, "wrong type").unwrap();
            }
            "directory" => fs::create_dir_all(&path).unwrap(),
            _ => panic!("unknown entry state"),
        }
        if case.extra {
            fs::create_dir_all(copied.root().join("extra")).unwrap();
        }
        for _ in 0..2 {
            copied.reset().unwrap();
            assert_eq!(path.is_dir(), case.directory);
            assert_eq!(path.exists(), case.directory);
            assert_eq!(copied.root().join("extra").exists(), case.extra_after);
        }
    }
    assert_eq!(ids.len(), 24);
}

#[cfg(unix)]
#[test]
fn reset_replaces_directory_symlink_without_modifying_its_referent() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::create_dir_all(root.join("fixture/empty")).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("sentinel"), "preserved").unwrap();
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
    let mut copied = worker(&plan);
    let path = copied.root().join("fixture/empty");
    fs::remove_dir(&path).unwrap();
    std::os::unix::fs::symlink(outside.path(), &path).unwrap();
    copied.reset().unwrap();
    assert!(path.is_dir());
    assert!(
        !fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(outside.path().join("sentinel")).unwrap(),
        b"preserved"
    );
}

#[tokio::test]
async fn public_cli_baseline_and_mutants_see_empty_fixture_directory() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("fixture/empty")).unwrap();
    fs::write(temp.path().join("calc.py"), "a = 1 + 2\nb = 3 + 4\n").unwrap();
    let python = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        });
    let output = tokio::time::timeout(std::time::Duration::from_secs(30),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--root"]).arg(temp.path())
            .args(["--file", "calc.py", "--operators", "binary_add_sub", "--jobs", "1",
                "--include", "fixture/**", "--allow-best-effort-memory", "--min-free-space", "1B", "--"])
            .arg(python).args(["-c", "from pathlib import Path; import calc; assert Path('fixture/empty').is_dir(); Path('fixture/empty').rmdir(); assert (calc.a,calc.b)==(3,7)"])
            .env_remove("PYTHONPATH").kill_on_drop(true).output())
        .await.expect("directory CLI deadline").unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], 2);
    assert!(temp.path().join("fixture/empty").is_dir());
}

#[test]
fn directory_includes_cannot_restore_default_or_literal_exclusions() {
    use hoimin_cli::workspace::LiteralExclusion;
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    for name in ["normal/empty", "blocked/empty", ".git/empty", "locks/empty"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    let options = CopyOptions {
        includes: vec!["**".into()],
        excludes: vec!["blocked".into()],
        literal_exclusions: vec![LiteralExclusion::Tree("locks".into())],
    };
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, options).unwrap();
    let copied = worker(&plan);
    assert!(copied.root().join("normal/empty").is_dir());
    for name in ["blocked", ".git", "locks"] {
        assert!(!copied.root().join(name).exists(), "{name}");
    }
}
