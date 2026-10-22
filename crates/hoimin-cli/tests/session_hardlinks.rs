use std::process::Command;

#[test]
fn issue_1085_session_hardlink_outside_project_preserves_source_before_baseline() {
    assert_source_preserved(false);
}

#[test]
fn issue_1085_session_hardlink_inside_project_preserves_source_before_baseline() {
    assert_source_preserved(true);
}

fn assert_source_preserved(inside_project: bool) {
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
    std::fs::hard_link(&source, &session).unwrap();
    let before = std::fs::read(&source).unwrap();
    let permissions = std::fs::metadata(&source).unwrap().permissions();
    let marker = fixture.path().join("baseline-started");
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .env("TMPDIR", &temporary)
        .env("TMP", &temporary)
        .env("TEMP", &temporary)
        .arg("run")
        .arg("--root")
        .arg(&project)
        .args(["--source", ".", "--session"])
        .arg(&session)
        .args([
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--max-mutants",
            "1",
            "--jobs",
            "1",
            "--total-timeout",
            "5s",
            "--",
        ]);
    #[cfg(unix)]
    command
        .args(["/bin/sh", "-c", "printf started > \"$1\"", "sh"])
        .arg(&marker);
    #[cfg(windows)]
    command
        .args(["cmd", "/C"])
        .arg(format!("echo started > \"{}\"", marker.display()));
    let output = command.output().unwrap();
    assert!(
        std::fs::read(&source).unwrap() == before,
        "session admission changed source: {output:?}"
    );
    assert_eq!(std::fs::read(&session).unwrap(), before);
    let after = std::fs::metadata(&source).unwrap().permissions();
    assert_eq!(
        after, permissions,
        "session admission changed source permissions"
    );
    assert!(
        !marker.exists(),
        "baseline started before session rejection"
    );
    for artifact in hoimin_cli::session::SessionArtifacts::resolve(&session)
        .unwrap()
        .files()
        .iter()
        .skip(1)
    {
        assert!(
            !artifact.exists(),
            "created companion {}",
            artifact.display()
        );
    }
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
