use std::path::PathBuf;
use std::process::{Command, Output};

fn plan(log: Option<&str>) -> Output {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("target.py"), "x = 1 + 2\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .env_remove("RUST_LOG")
        .args(["plan", "--root"])
        .arg(root.path())
        .args([
            "--file",
            "target.py",
            "--allow-best-effort-memory",
            "--",
            "python",
            "-c",
            "pass",
        ]);
    if let Some(log) = log {
        command.env("RUST_LOG", log);
    }
    command.output().unwrap()
}

#[test]
fn redirected_plan_stays_machine_readable_without_logging() {
    let output = plan(None);
    assert!(output.status.success(), "{output:?}");
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        !manifest["candidates"].as_array().unwrap().is_empty(),
        "{manifest}"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn debug_logs_are_opt_in_json_records_on_stderr() {
    let output = plan(Some("hoimin_cli=debug"));
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let logs: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).expect("redirected logs must be JSON Lines"))
        .collect();
    assert!(logs.iter().any(|log| log["level"] == "DEBUG"), "{stderr}");
    assert!(
        logs.iter()
            .any(|log| log["fields"]["stage"] == "analyzing sources"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("python"),
        "test argv must not be logged: {stderr}"
    );
}

#[test]
fn disabled_or_invalid_filter_does_not_change_command_result() {
    for filter in ["off", "hoimin_cli=warn", "hoimin_cli=invalid-level"] {
        let output = plan(Some(filter));
        assert!(output.status.success(), "{filter}: {output:?}");
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
        assert!(output.stderr.is_empty(), "{filter}: {output:?}");
    }
}

#[tokio::test]
async fn reused_result_updates_progress_before_the_next_process_starts() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("target.py"), "value = 1 + 2\nother = 4 + 5\n").unwrap();
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    });
    for budget in ["1", "2"] {
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .env("RUST_LOG", "hoimin_cli=debug")
            .args(["run", "--root"])
            .arg(&project)
            .args([
                "--file",
                "target.py",
                "--operators",
                "binary_add_sub",
                "--jobs",
                "1",
                "--format",
                "json",
                "--max-mutants",
                budget,
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
                "--session",
            ])
            .arg(root.path().join("session.db"))
            .args(["--resume", "--"])
            .arg(&python)
            .args([
                "-c",
                "import target; assert target.value == 3 and target.other == 9",
            ])
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            output.status.code(),
            Some(if budget == "1" { 4 } else { 0 }),
            "{stderr}"
        );
        if budget == "1" {
            continue;
        }
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["run"]["resume"]["status"], "resumed");
        let logs: Vec<serde_json::Value> = stderr
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let progress = logs
            .iter()
            .position(|log| {
                log["fields"]["stage"] == "testing mutants" && log["fields"]["completed"] == 1
            })
            .unwrap_or_else(|| panic!("missing reused progress: {stderr}"));
        let next_process = logs
            .iter()
            .position(|log| {
                log["fields"]["message"] == "starting test process"
                    && log["span"]["mutant_id"].is_string()
            })
            .unwrap();
        assert!(
            progress < next_process,
            "reused result must be counted before the next test: {stderr}"
        );
    }
}

#[tokio::test]
async fn spawned_process_logs_keep_run_and_mutant_context_without_changing_reports() {
    use std::process::Stdio;
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("target.py"), "x = 1 + 2\n").unwrap();
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    });
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .env("RUST_LOG", "hoimin_cli=debug")
        .args(["run", "--root"])
        .arg(root.path())
        .args([
            "--file",
            "target.py",
            "--format",
            "jsonl",
            "--operators",
            "binary_add_sub",
            "--max-mutants",
            "1",
            "--min-free-space",
            "1B",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(python)
        .args(["-c", "pass # PRIVATE_TEST_ARGV"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let events: Vec<hoimin_core::OutputEvent> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let finished = events
        .iter()
        .find_map(|event| match event {
            hoimin_core::OutputEvent::MutantFinished(value) => Some(value),
            _ => None,
        })
        .unwrap();
    let logs: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let resolved = logs
        .iter()
        .position(|log| log["fields"]["message"] == "resolved explicit targets")
        .unwrap();
    let preparing = logs
        .iter()
        .position(|log| log["fields"]["stage"] == "preparing workers")
        .unwrap();
    assert!(
        logs[resolved..preparing]
            .iter()
            .any(|log| log["fields"]["stage"] == "checking workspace"),
        "target completion must restore the preflight stage: {stderr}"
    );
    let process = logs
        .iter()
        .find(|log| {
            log["fields"]["message"] == "test process finished"
                && log["span"]["mutant_id"] == finished.candidate.id
        })
        .unwrap_or_else(|| panic!("missing correlated process log: {stderr}"));
    assert!(
        process["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| { span["name"] == "mutation_run" && span["run_id"] == finished.run_id }),
        "{process}"
    );
    assert!(!stderr.contains("PRIVATE_TEST_ARGV"), "{stderr}");
    assert!(!stderr.contains("hoimin:"), "redirected progress: {stderr}");
}

#[cfg(unix)]
fn terminal() -> (std::fs::File, std::fs::File) {
    use std::os::fd::FromRawFd;
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: openpty initializes both descriptors on success. Each is owned once.
    assert_eq!(
        unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    }
}

#[cfg(unix)]
#[tokio::test]
async fn debug_logging_to_stalled_stderr_keeps_the_execution_deadline() {
    use std::io::{self, Write};
    use std::process::Stdio;
    use std::time::Duration;

    let (consumer, mut producer) = std::os::unix::net::UnixStream::pair().unwrap();
    producer.set_nonblocking(true).unwrap();
    loop {
        match producer.write(&[b'x'; 4096]) {
            Ok(0) => panic!("stderr transport closed"),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("filling stderr: {error}"),
        }
    }
    producer.set_nonblocking(false).unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("target.py"), "x = 1 + 2\n").unwrap();
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python");
    let stderr: std::os::fd::OwnedFd = producer.into();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .env("RUST_LOG", "hoimin_cli=debug")
        .args(["run", "--root"])
        .arg(root.path())
        .args([
            "--file",
            "target.py",
            "--format",
            "json",
            "--total-timeout",
            "1s",
            "--min-free-space",
            "1B",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(python)
        .args(["-c", "import time; time.sleep(60)"])
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(6), child.wait()).await;
    if status.is_err() {
        child.kill().await.unwrap();
        child.wait().await.unwrap();
    }
    drop(consumer);
    assert!(
        !status
            .expect("debug sink held the command past its deadline and grace")
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn terminal_shows_heartbeat_and_completed_count_with_logging_off() {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("target.py"), "x = 1 + 2\n").unwrap();
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python");
    let (mut reader, writer) = terminal();
    let capture = std::thread::spawn(move || {
        let mut result = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => result.extend_from_slice(&buffer[..n]),
                Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                Err(error) => panic!("terminal read: {error}"),
            }
        }
        String::from_utf8(result).unwrap()
    });
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .env("RUST_LOG", "off")
        .args(["run", "--root"])
        .arg(root.path())
        .args([
            "--file",
            "target.py",
            "--format",
            "json",
            "--operators",
            "binary_add_sub",
            "--max-mutants",
            "1",
            "--min-free-space",
            "1B",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(python)
        .args(["-c", "import time; time.sleep(1.3)"])
        .stdout(Stdio::piped())
        .stderr(Stdio::from(writer))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let stderr = capture.join().unwrap();
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
    assert!(
        stderr.contains("baseline") && stderr.contains("elapsed"),
        "{stderr}"
    );
    assert!(stderr.contains("1 completed"), "{stderr}");
    assert!(!stderr.contains("DEBUG"), "{stderr}");
}
