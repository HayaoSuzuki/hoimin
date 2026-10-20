#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

async fn marker(path: &Path) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(value) = tokio::fs::read_to_string(path).await
                && !value.is_empty()
            {
                return value;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Git helper did not announce its start")
}

async fn stopped_git_case(block_at: usize, interrupt: bool) {
    use std::os::unix::fs::PermissionsExt;

    // Arrange: real Git handles earlier requests; one helper announces its PID then blocks.
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("project");
    let bin = fixture.path().join("bin");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(root.join("app.py"), "value = True\n").unwrap();
    let output = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python");
    let ready = fixture.path().join("started");
    let trace = fixture.path().join("requests");
    let helper = bin.join("git");
    std::fs::write(&helper, format!("#!{}\nimport os, pathlib, subprocess, sys, time\ntrace = pathlib.Path({})\nwith trace.open('a') as out: out.write('request\\n')\nif len(trace.read_text().splitlines()) == {block_at}:\n    pathlib.Path({}).write_text(str(os.getpid()))\n    time.sleep(30)\nraise SystemExit(subprocess.call([{}, *sys.argv[1:]]))\n", python.display(), serde_json::to_string(trace.to_str().unwrap()).unwrap(), serde_json::to_string(ready.to_str().unwrap()).unwrap(), serde_json::to_string(real_git.to_str().unwrap()).unwrap())).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    // The total deadline includes preparation before Git starts; allow loaded CI
    // enough time to reach the helper whose cancellation this fixture checks.
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["run", "--root"])
        .arg(&root)
        .args([
            "--source",
            ".",
            "--changed",
            "--total-timeout",
            if interrupt { "20s" } else { "5s" },
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--",
        ])
        .arg(&python)
        .args(["-c", "pass"])
        .env("PATH", path)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command.spawn().unwrap();
    let cli_pid = child.id().unwrap();
    let helper_pid = marker(&ready).await.trim().parse::<i32>().unwrap();

    // Act: stop only after the owned helper has started.
    if interrupt {
        // SAFETY: cli_pid identifies the still-owned direct CLI child.
        assert_eq!(
            unsafe { libc::kill(i32::try_from(cli_pid).unwrap(), libc::SIGINT) },
            0
        );
    }
    // Stay below the helper's 30 s sleep so natural completion cannot pass.
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    // Always clean up the self-terminating regression helper, including red runs.
    // SAFETY: helper PID came from the process start handshake in this isolated fixture.
    let helper_alive = unsafe { libc::kill(helper_pid, 0) } == 0;
    if helper_alive {
        unsafe { libc::kill(helper_pid, libc::SIGKILL) };
    }

    // Assert: stopping is incomplete, and all helper work ended before CLI exit.
    assert_eq!(
        output.status.code(),
        Some(if interrupt { 130 } else { 4 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!helper_alive, "target Git child outlived the CLI");
    assert_eq!(
        std::fs::read_to_string(trace).unwrap().lines().count(),
        block_at,
        "Git was dispatched after stop"
    );
}

macro_rules! stopped_git_test {
    ($name:ident, $block:expr, $interrupt:expr) => {
        #[tokio::test]
        async fn $name() {
            stopped_git_case($block, $interrupt).await;
        }
    };
}
stopped_git_test!(total_timeout_reaps_first_git, 1, false);
stopped_git_test!(total_timeout_reaps_later_git, 3, false);
stopped_git_test!(interrupt_reaps_first_git, 1, true);
stopped_git_test!(interrupt_reaps_later_git, 3, true);
