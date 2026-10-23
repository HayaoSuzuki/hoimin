use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn issue_1085_session_hardlink_outside_project_preserves_source_before_baseline() {
    assert_source_preserved(false, None, false);
}

#[test]
fn issue_1085_session_hardlink_inside_project_preserves_source_before_baseline() {
    assert_source_preserved(true, None, false);
}

fn assert_source_preserved(inside_project: bool, suffix: Option<&str>, current: bool) {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let temporary = fixture.path().join("tmp");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&temporary).unwrap();
    let source = project.join("__init__.py");
    std::fs::write(&source, []).unwrap();
    std::fs::write(project.join("calc.py"), b"x = 1 + 2\n").unwrap();
    let session = if inside_project {
        project.join("session.sqlite3")
    } else {
        fixture.path().join("session.sqlite3")
    };
    if current {
        drop(hoimin_cli::session::SessionHandler::open(&session).unwrap());
    }
    let alias = suffix.map_or_else(
        || session.clone(),
        |suffix| companion_path(&session, suffix),
    );
    std::fs::hard_link(&source, &alias).unwrap();
    let artifacts = hoimin_cli::session::SessionArtifacts::resolve(&session).unwrap();
    let before_artifacts = artifacts
        .files()
        .iter()
        .map(|path| artifact_snapshot(path))
        .collect::<Vec<_>>();
    let before = std::fs::read(&source).unwrap();
    let permissions = std::fs::metadata(&source).unwrap().permissions();
    let marker = fixture.path().join("baseline-started");
    let mut command = session_command(&project, &session, &temporary, &marker, false);
    let output = command.output().unwrap();
    assert!(
        std::fs::read(&source).unwrap() == before,
        "session admission changed source: {output:?}"
    );
    assert_eq!(std::fs::read(&alias).unwrap(), before);
    let after = std::fs::metadata(&source).unwrap().permissions();
    assert_eq!(
        after, permissions,
        "session admission changed source permissions"
    );
    assert!(
        !marker.exists(),
        "baseline started before session rejection"
    );
    let after_artifacts = artifacts
        .files()
        .iter()
        .map(|path| artifact_snapshot(path))
        .collect::<Vec<_>>();
    assert_eq!(
        after_artifacts, before_artifacts,
        "changed database/companion bytes or permissions"
    );
    assert!(
        !hoimin_cli::session::SessionArtifacts::resolve(&session)
            .unwrap()
            .lock_directory()
            .exists()
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostic.contains("hard link"),
        "missing hardlink rejection: {diagnostic}"
    );
    assert_eq!(
        std::fs::read(project.join("calc.py")).unwrap(),
        b"x = 1 + 2\n"
    );
}

fn companion_path(database: &Path, suffix: &str) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn artifact_snapshot(path: &Path) -> Option<(Vec<u8>, std::fs::Permissions)> {
    match std::fs::read(path) {
        Ok(bytes) => Some((bytes, std::fs::metadata(path).unwrap().permissions())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("cannot snapshot {}: {error}", path.display()),
    }
}

fn session_command(
    project: &Path,
    session: &Path,
    temporary: &Path,
    marker: &Path,
    resume: bool,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .env("TMPDIR", temporary)
        .env("TMP", temporary)
        .env("TEMP", temporary)
        .arg("run")
        .arg("--root")
        .arg(project)
        .args(["--source", ".", "--session"])
        .arg(session)
        .args([
            "--operators",
            "binary_add_sub",
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--max-mutants",
            if resume { "2" } else { "1" },
            "--jobs",
            "1",
            "--total-timeout",
            "5s",
        ]);
    if resume {
        command.arg("--resume");
    }
    command.arg("--");
    #[cfg(unix)]
    command
        .args(["/bin/sh", "-c", "printf started > \"$1\"", "sh"])
        .arg(marker);
    #[cfg(windows)]
    command
        .args(["cmd", "/C"])
        .arg(format!("echo started > \"{}\"", marker.display()));
    command
}

#[test]
fn issue_1094_session_companion_hardlinks_preserve_sources_before_baseline() {
    for suffix in ["-journal", "-wal", "-shm"] {
        for current in [false, true] {
            for inside in [false, true] {
                assert_source_preserved(inside, Some(suffix), current);
            }
        }
    }
}

#[test]
fn issue_1094_ordinary_session_creation_and_resume_still_run_baseline() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let temporary = fixture.path().join("tmp");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&temporary).unwrap();
    std::fs::write(project.join("calc.py"), b"x = 1 + 2\ny = 3 + 4\n").unwrap();
    let session = fixture.path().join("session.sqlite3");
    let marker = fixture.path().join("baseline-started");
    let mut original_run_id = None;
    for resume in [false, true] {
        let mut command = session_command(&project, &session, &temporary, &marker, resume);
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if resume { 1 } else { 4 }),
            "{output:?}"
        );
        assert!(marker.exists());
        std::fs::remove_file(&marker).unwrap();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["summary"]["complete"], resume);
        if resume {
            assert_eq!(report["run"]["run_id"].as_str(), original_run_id.as_deref());
            let outcome: hoimin_core::MutationOutcome =
                serde_json::from_value(report["mutants"][0]["outcome"].clone()).unwrap();
            assert!(matches!(
                outcome,
                hoimin_core::MutationOutcome::Reused { .. }
            ));
            assert_eq!(report["run"]["resume"]["status"], "resumed");
        } else {
            original_run_id = Some(report["run"]["run_id"].as_str().unwrap().to_owned());
        }
        assert_eq!(
            std::fs::read(project.join("calc.py")).unwrap(),
            b"x = 1 + 2\ny = 3 + 4\n"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn issue_1094_nonregular_companions_are_rejected_without_blocking_or_creating_database() {
    for suffix in ["-journal", "-wal", "-shm"] {
        for fifo in [false, true] {
            let fixture = tempfile::tempdir().unwrap();
            let project = fixture.path().join("project");
            let temporary = fixture.path().join("tmp");
            std::fs::create_dir(&project).unwrap();
            std::fs::create_dir(&temporary).unwrap();
            std::fs::write(project.join("calc.py"), b"x = 1 + 2\n").unwrap();
            let session = fixture.path().join("session.sqlite3");
            let companion = companion_path(&session, suffix);
            if fifo {
                assert!(
                    Command::new("mkfifo")
                        .arg(&companion)
                        .status()
                        .unwrap()
                        .success()
                );
            } else {
                std::fs::create_dir(&companion).unwrap();
            }
            let marker = fixture.path().join("baseline-started");
            let mut command = tokio::process::Command::from(session_command(
                &project, &session, &temporary, &marker, false,
            ));
            command.kill_on_drop(true);
            let output = tokio::time::timeout(std::time::Duration::from_secs(5), command.output())
                .await
                .expect("nonregular companion blocked session admission")
                .unwrap();
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            assert!(
                !session.exists(),
                "SQLite created the database before refusing a nonregular companion"
            );
            assert!(!marker.exists());
            assert!(companion.exists());
        }
    }
}
