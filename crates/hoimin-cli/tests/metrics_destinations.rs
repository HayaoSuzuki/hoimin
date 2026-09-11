use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

const SOURCE: &[u8] = b"def add(a, b):\n    return a + b\n";

fn python() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    })
}

async fn run(root: &Path, metrics: &Path, options: &[&str]) -> Output {
    run_optional(root, Some(metrics), options).await
}

async fn run_optional(root: &Path, metrics: Option<&Path>, options: &[&str]) -> Output {
    let marker = root.parent().unwrap().join("baseline-marker");
    let script = format!(
        "from pathlib import Path; Path({:?}).touch(); from calc import add; assert add(2,3)==5",
        marker.to_str().unwrap()
    );
    run_script(root, metrics, options, &script).await
}

async fn run_script(root: &Path, metrics: Option<&Path>, options: &[&str], script: &str) -> Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command.current_dir(root).args([
        "run",
        "--root",
        ".",
        "--file",
        "calc.py",
        "--operators",
        "binary_add_sub",
        "--format",
        "json",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--total-timeout",
        "20s",
    ]);
    if let Some(metrics) = metrics {
        command.arg("--metrics").arg(metrics);
    }
    command
        .args(options)
        .arg("--")
        .arg(python())
        .args(["-c", script])
        .kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("CLI timed out")
        .unwrap()
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("calc.py"), SOURCE).unwrap();
    (temp, root)
}

#[tokio::test]
async fn metrics_collision_preserves_selected_source_before_baseline() {
    let (temp, root) = fixture();
    let output = run(&root, &root.join("calc.py"), &[]).await;
    let bytes = std::fs::read(root.join("calc.py")).unwrap();
    assert_eq!(
        bytes,
        SOURCE,
        "exit={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success());
    assert!(!temp.path().join("baseline-marker").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.destination.collision"));
}

fn assert_collision(output: &Output, temp: &Path, protected: &Path, expected: &[u8]) {
    assert_eq!(
        std::fs::read(protected).unwrap(),
        expected,
        "protected {} changed",
        protected.display()
    );
    assert_eq!(
        blake3::hash(&std::fs::read(protected).unwrap()),
        blake3::hash(expected)
    );
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!temp.join("baseline-marker").exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("metrics.destination.collision"), "{stderr}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["baseline"].is_null(), "{report}");
    assert_eq!(report["summary"]["complete"], false);
    assert!(
        report["summary"]["disk"]["cleanup"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["root_id"] == "execution" && entry["status"] == "clean")
    );
}

#[tokio::test]
async fn metrics_collision_rejects_relative_dot_parent_and_native_case_aliases() {
    for spelling in ["calc.py", "./calc.py", "sub/../calc.py", "CALC.PY"] {
        let (temp, root) = fixture();
        std::fs::create_dir(root.join("sub")).unwrap();
        if spelling == "CALC.PY" && !root.join(spelling).exists() {
            continue;
        }
        let output = run(&root, Path::new(spelling), &[]).await;
        assert_collision(&output, temp.path(), &root.join("calc.py"), SOURCE);
    }
}

#[tokio::test]
async fn metrics_collision_preserves_explicit_fingerprint_input() {
    let (temp, root) = fixture();
    let input = root.join("settings.toml");
    let bytes = b"setting = 42\n";
    std::fs::write(&input, bytes).unwrap();
    let output = run(&root, &input, &["--fingerprint-include", "settings.toml"]).await;
    assert_collision(&output, temp.path(), &input, bytes);
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
}

#[tokio::test]
async fn metrics_collision_rejects_new_session_database_before_creating_it() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    let output = run(&root, &database, &["--session", database.to_str().unwrap()]).await;
    assert!(!output.status.success());
    assert!(!temp.path().join("baseline-marker").exists());
    assert!(
        !database.exists(),
        "collision must precede session creation"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.destination.collision"));
}

#[tokio::test]
async fn metrics_collision_preserves_existing_session_database_without_recovery() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    drop(hoimin_cli::session::SessionHandler::open(&database).unwrap());
    let bytes = std::fs::read(&database).unwrap();
    let output = run(&root, &database, &["--session", database.to_str().unwrap()]).await;
    assert_collision(&output, temp.path(), &database, &bytes);
    assert!(bytes.starts_with(b"SQLite format 3\0"));
    let connection = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[tokio::test]
async fn metrics_collision_protects_exact_session_companions_and_lock_tree() {
    for suffix in [
        "session.db-wal",
        "session.db-shm",
        "session.db-journal",
        ".session.db.hoimin-locks/owned.lock",
    ] {
        let (temp, root) = fixture();
        let database = temp.path().join("session.db");
        let protected = temp.path().join(suffix);
        std::fs::create_dir_all(protected.parent().unwrap()).unwrap();
        let bytes = b"owned artifact\n";
        std::fs::write(&protected, bytes).unwrap();
        let output = run(
            &root,
            &protected,
            &["--session", database.to_str().unwrap()],
        )
        .await;
        assert_collision(&output, temp.path(), &protected, bytes);
        assert!(!database.exists());
    }
}

#[tokio::test]
async fn metrics_allows_in_root_creation_and_existing_file_updates() {
    let (temp, root) = fixture();
    let metrics = root.join("metrics.json");
    let ordinary = run_optional(&root, None, &[]).await;
    assert!(ordinary.status.success());
    let ordinary: serde_json::Value = serde_json::from_slice(&ordinary.stdout).unwrap();
    for previous in [None, Some(b"old metrics\n".as_slice())] {
        if let Some(bytes) = previous {
            std::fs::write(&metrics, bytes).unwrap();
        }
        let output = run(&root, &metrics, &[]).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            report["mutants"][0]["candidate"],
            ordinary["mutants"][0]["candidate"]
        );
        assert_eq!(
            report["mutants"][0]["status"],
            ordinary["mutants"][0]["status"]
        );
        assert_eq!(report["summary"]["counts"], ordinary["summary"]["counts"]);
        let metrics: hoimin_core::RunMetrics =
            serde_json::from_slice(&std::fs::read(&metrics).unwrap()).unwrap();
        assert_eq!(metrics.executed, 1);
        assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
        assert!(temp.path().join("baseline-marker").exists());
    }
}

#[tokio::test]
async fn metrics_replaces_distinct_hardlink_without_changing_source() {
    let (_temp, root) = fixture();
    let metrics = root.join("metrics.json");
    std::fs::hard_link(root.join("calc.py"), &metrics).unwrap();
    let output = run(&root, &metrics, &[]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    serde_json::from_slice::<hoimin_core::RunMetrics>(&std::fs::read(metrics).unwrap()).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_distinguishes_parent_symlink_collision_from_safe_leaf_replacement() {
    let (temp, root) = fixture();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let output = run(&root, &alias.join("calc.py"), &[]).await;
    assert_collision(&output, temp.path(), &root.join("calc.py"), SOURCE);
    let metrics = temp.path().join("metrics.json");
    std::os::unix::fs::symlink(root.join("calc.py"), &metrics).unwrap();
    let output = run(&root, &metrics, &[]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    assert!(
        !std::fs::symlink_metadata(&metrics)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    serde_json::from_slice::<hoimin_core::RunMetrics>(&std::fs::read(metrics).unwrap()).unwrap();
}

#[tokio::test]
async fn metrics_collision_rejects_prospective_session_native_case_alias() {
    let (temp, root) = fixture();
    if !root.join("CALC.PY").exists() {
        return;
    }
    let database = temp.path().join("session.db");
    let output = run(
        &root,
        &temp.path().join("SESSION.DB"),
        &["--session", database.to_str().unwrap()],
    )
    .await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.destination.collision"));
    assert!(!temp.path().join("baseline-marker").exists());
    assert!(!database.exists());
}

#[tokio::test]
async fn metrics_withholds_uncertain_prospective_identity_without_changing_run_result() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    let output_path = temp.path().join("測定.json");
    let output = run(
        &root,
        &output_path,
        &["--session", database.to_str().unwrap()],
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(temp.path().join("baseline-marker").exists());
    assert!(!output_path.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.write"));
    assert!(
        std::fs::read(database)
            .unwrap()
            .starts_with(b"SQLite format 3\0")
    );
}

#[tokio::test]
async fn metrics_missing_parent_retains_late_warning() {
    let (temp, root) = fixture();
    let metrics = temp.path().join("missing/metrics.json");
    let output = run(&root, &metrics, &[]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(temp.path().join("baseline-marker").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.write"));
    assert!(!metrics.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_collision_preserves_configured_and_actual_session_symlink_entries() {
    for actual in [false, true] {
        let (temp, root) = fixture();
        let database = temp.path().join("session.db");
        drop(hoimin_cli::session::SessionHandler::open(&database).unwrap());
        let bytes = std::fs::read(&database).unwrap();
        let configured = temp.path().join("session-link.db");
        std::os::unix::fs::symlink(&database, &configured).unwrap();
        let destination = if actual { &database } else { &configured };
        let output = run(
            &root,
            destination,
            &["--session", configured.to_str().unwrap()],
        )
        .await;
        assert_collision(&output, temp.path(), &database, &bytes);
        assert!(
            std::fs::symlink_metadata(configured)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[tokio::test]
async fn metrics_allows_similarly_named_inactive_session_artifact() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    let metrics = temp.path().join("session.db-wal.backup");
    let output = run(&root, &metrics, &["--session", database.to_str().unwrap()]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<hoimin_core::RunMetrics>(&std::fs::read(metrics).unwrap()).unwrap();
    assert!(
        std::fs::read(database)
            .unwrap()
            .starts_with(b"SQLite format 3\0")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_collision_protects_session_lock_directory_symlink_referent() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    let locks = temp.path().join("actual-locks");
    std::fs::create_dir(&locks).unwrap();
    std::os::unix::fs::symlink(&locks, temp.path().join(".session.db.hoimin-locks")).unwrap();
    let protected = locks.join("owned.lock");
    let bytes = b"owned lock\n";
    std::fs::write(&protected, bytes).unwrap();
    let output = run(
        &root,
        &protected,
        &["--session", database.to_str().unwrap()],
    )
    .await;
    assert_collision(&output, temp.path(), &protected, bytes);
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_allows_exact_entry_among_three_hardlinked_symlinks() {
    let (_temp, root) = fixture();
    let one = root.join("one.json");
    std::os::unix::fs::symlink(root.join("calc.py"), &one).unwrap();
    let two = root.join("two.json");
    let three = root.join("three.json");
    std::fs::hard_link(&one, &two).unwrap();
    std::fs::hard_link(&one, &three).unwrap();
    // The last enumerated link specifically exercises exact-name precedence.
    let metrics = std::fs::read_dir(&root)
        .unwrap()
        .map(Result::unwrap)
        .filter(|item| {
            ["one.json", "two.json", "three.json"]
                .iter()
                .any(|name| item.file_name() == *name)
        })
        .last()
        .unwrap()
        .path();
    let output = run(&root, &metrics, &[]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    serde_json::from_slice::<hoimin_core::RunMetrics>(&std::fs::read(&metrics).unwrap()).unwrap();
    for link in [one, two, three] {
        if link != metrics {
            assert!(
                std::fs::symlink_metadata(link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_non_directory_path_component_retains_late_warning() {
    let (temp, root) = fixture();
    let metrics = root.join("calc.py/");
    let output = run(&root, &metrics, &[]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(temp.path().join("baseline-marker").exists());
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.write"));
}

#[tokio::test]
async fn metrics_retains_incomplete_warning_after_later_preflight_budget_failure() {
    let (temp, root) = fixture();
    let metrics = temp.path().join("metrics.json");
    let output = run(&root, &metrics, &["--max-copy-size", "1B"]).await;
    assert!(!output.status.success());
    assert!(!temp.path().join("baseline-marker").exists());
    assert!(!metrics.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.incomplete"));
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
}

#[cfg(windows)]
#[tokio::test]
async fn metrics_withholds_unsupported_windows_entry_spellings() {
    for name in ["calc.py.", "calc.py ", "calc.py:stream", "SESSIO~1.DB"] {
        let (temp, root) = fixture();
        let metrics = temp.path().join(name);
        let database = temp.path().join("session-database.db");
        let output = run(&root, &metrics, &["--session", database.to_str().unwrap()]).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(temp.path().join("baseline-marker").exists());
        assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.write"));
        assert!(!metrics.exists());
        assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_writes_the_authorized_parent_when_configured_alias_changes() {
    let (temp, root) = fixture();
    let safe = temp.path().join("safe");
    std::fs::create_dir(&safe).unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&safe, &alias).unwrap();
    let script = format!(
        "from pathlib import Path; p=Path({:?}); p.unlink(); p.symlink_to({:?}, target_is_directory=True); from calc import add; assert add(2,3)==5",
        alias.to_str().unwrap(),
        root.to_str().unwrap()
    );
    let output = run_script(&root, Some(&alias.join("calc.py")), &[], &script).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    serde_json::from_slice::<hoimin_core::RunMetrics>(
        &std::fs::read(safe.join("calc.py")).unwrap(),
    )
    .unwrap();
}

#[cfg(windows)]
#[tokio::test]
async fn metrics_collision_protects_windows_configured_session_symlink_sidecars() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    drop(hoimin_cli::session::SessionHandler::open(&database).unwrap());
    let configured = temp.path().join("session-link.db");
    if let Err(error) = std::os::windows::fs::symlink_file(&database, &configured) {
        assert!(
            std::env::var_os("HOIMIN_REQUIRE_WINDOWS_SYMLINKS").is_none(),
            "required Windows session symlink setup failed: {error}"
        );
        eprintln!("SKIP Windows session symlink sidecars: symlink setup unavailable: {error}");
        return;
    }
    let database_before = std::fs::read(&database).unwrap();
    for basename in ["session-link.db", "session.db"] {
        for suffix in ["-wal", "-shm", "-journal"] {
            let protected = temp.path().join(format!("{basename}{suffix}"));
            let bytes = b"protected Windows session artifact\n";
            std::fs::write(&protected, bytes).unwrap();
            let output = run(
                &root,
                &protected,
                &["--session", configured.to_str().unwrap()],
            )
            .await;
            assert_collision(&output, temp.path(), &protected, bytes);
            assert_eq!(std::fs::read(&database).unwrap(), database_before);
            assert!(
                std::fs::symlink_metadata(&configured)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            std::fs::remove_file(protected).unwrap();
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_withholds_missing_parent_created_as_alias_during_baseline() {
    let (temp, root) = fixture();
    let alias = temp.path().join("created-parent");
    let script = format!(
        "from pathlib import Path; p=Path({:?}); p.exists() or p.symlink_to({:?}, target_is_directory=True); from calc import add; assert add(2,3)==5",
        alias.to_str().unwrap(),
        root.to_str().unwrap()
    );
    let output = run_script(&root, Some(&alias.join("calc.py")), &[], &script).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(alias.is_dir(), "baseline created the alias");
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.write"));
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_directory_symlink_with_separator_retains_late_warning() {
    assert_directory_symlink_syntax("").await;
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_directory_symlink_with_terminal_dot_retains_late_warning() {
    assert_directory_symlink_syntax(".").await;
}

#[cfg(unix)]
#[tokio::test]
async fn metrics_directory_symlink_with_terminal_dotdot_retains_late_warning() {
    assert_directory_symlink_syntax("..").await;
}

#[cfg(unix)]
async fn assert_directory_symlink_syntax(suffix: &str) {
    let (temp, root) = fixture();
    let alias = temp.path().join("directory-link");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let metrics = PathBuf::from(format!("{}/{suffix}", alias.display()));
    let output = run(&root, &metrics, &[]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(temp.path().join("baseline-marker").exists());
    assert!(
        std::fs::symlink_metadata(alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.write"));
}

#[tokio::test]
async fn metrics_collision_rejects_future_ownership_tree_before_missing_parent_warning() {
    let (temp, root) = fixture();
    let database = temp.path().join("session.db");
    let metrics = temp.path().join(".session.db.hoimin-locks/future.lock");
    let output = run(&root, &metrics, &["--session", database.to_str().unwrap()]).await;
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("metrics.destination.collision"));
    assert!(!temp.path().join("baseline-marker").exists());
    assert!(!database.exists());
    assert!(!metrics.exists());
}

#[tokio::test]
async fn metrics_and_in_root_session_complete_without_copying_session_artifacts() {
    let (temp, root) = fixture();
    let session = root.join("session.db");
    let metrics = temp.path().join("metrics.json");
    let script = "from pathlib import Path; assert not Path('session.db').exists(); assert not Path('.session.db.hoimin-locks').exists(); from calc import add; assert add(2,3)==5";
    let output = run_script(
        &root,
        Some(&metrics),
        &["--session", session.to_str().unwrap(), "--jobs", "2"],
        script,
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["summary"]["counts"]["killed"], 1);
    assert!(session.is_file());
    let metrics: hoimin_core::RunMetrics =
        serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
    assert_eq!(metrics.executed, 1);
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), SOURCE);
}
