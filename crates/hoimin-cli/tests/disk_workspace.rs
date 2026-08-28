use std::ffi::OsString;

#[tokio::test]
async fn filesystem_reserve_stops_before_the_test_command_is_dispatched() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    let marker = project.path().join("command-ran");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(
        [
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--min-free-space"),
            OsString::from("18446744073709551615B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("touch"),
            marker.as_os_str().to_owned(),
        ],
        &mut stdout,
        &mut stderr,
    )
    .await;

    assert_eq!(exit, 2);
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("filesystem.reserve.reached")
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn planned_snapshot_and_worker_bytes_stop_at_the_exact_limit() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    let marker = project.path().join("command-ran");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(
        [
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--jobs"),
            OsString::from("1"),
            OsString::from("--max-workspace-size"),
            OsString::from("10B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("touch"),
            marker.as_os_str().to_owned(),
        ],
        &mut stdout,
        &mut stderr,
    )
    .await;

    assert_eq!(exit, 2);
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("workspace.size.exceeded")
    );
    assert!(!marker.exists());
}
