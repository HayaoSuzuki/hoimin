#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use std::ffi::OsString;

#[tokio::test]
async fn filesystem_reserve_stops_before_the_test_command_is_dispatched() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    let marker = project.path().join("command-ran");
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();

    let exit = hoimin_cli::run_with_writers(
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
        stdout.clone(),
        stderr.clone(),
    )
    .await;

    assert_eq!(exit, 2);
    let report: serde_json::Value = serde_json::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(
        report["summary"]["disk"]["stop"]["code"],
        "filesystem.reserve.reached"
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn planned_snapshot_and_worker_bytes_stop_at_the_exact_limit() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
    let marker = project.path().join("command-ran");
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();

    let exit = hoimin_cli::run_with_writers(
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
            OsString::from("--min-free-space"),
            OsString::from("1B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("touch"),
            marker.as_os_str().to_owned(),
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;

    assert_eq!(exit, 2);
    let report: serde_json::Value = serde_json::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(
        report["summary"]["disk"]["stop"]["code"],
        "workspace.size.exceeded"
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn copy_inventory_limit_stops_before_the_test_command_is_dispatched() {
    // Arrange: the marker would be created by the baseline command.
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), b"x = 1 + 2\n").unwrap();
    std::fs::write(project.path().join("padding.bin"), [0; 1024]).unwrap();
    let marker = project.path().join("command-ran");
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();

    // Act.
    let exit = hoimin_cli::run_with_writers(
        [
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--jobs"),
            OsString::from("1"),
            OsString::from("--max-copy-size"),
            OsString::from("1B"),
            OsString::from("--max-workspace-size"),
            OsString::from("64MiB"),
            OsString::from("--min-free-space"),
            OsString::from("1B"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("touch"),
            marker.as_os_str().to_owned(),
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;

    // Assert.
    assert_eq!(exit, 2, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let report: serde_json::Value = serde_json::from_slice(&stdout.snapshot()).unwrap();
    assert!(
        String::from_utf8(stderr.snapshot())
            .unwrap()
            .lines()
            .any(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .is_ok_and(|diagnostic| diagnostic["code"] == "workspace.copy.limit")
            })
    );
    assert!(report["baseline"].is_null());
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(project.path().join("target.py")).unwrap(),
        b"x = 1 + 2\n"
    );
}
