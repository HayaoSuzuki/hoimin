#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use hoimin_cli::run_with_writers;
use serde_json::Value;

async fn run(args: Vec<String>) -> (i32, Vec<u8>, Vec<u8>) {
    let out = CollectingWriter::default();
    let err = CollectingWriter::default();
    let code = run_with_writers(args, out.clone(), err.clone()).await;
    (code, out.snapshot(), err.snapshot())
}

async fn fixture() -> (tempfile::TempDir, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.py"), "value = 1\n").unwrap();
    let (code, out, err) = run(vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        dir.path().to_str().unwrap().into(),
        "--file".into(),
        "sample.py".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--".into(),
        "must-not-execute".into(),
    ])
    .await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let plan = dir.path().join("plan.json");
    std::fs::write(&plan, out).unwrap();
    let args = vec![
        "hoimin".into(),
        "review".into(),
        "list".into(),
        plan.to_str().unwrap().into(),
        "--store".into(),
        dir.path().join("reviews.sqlite3").to_str().unwrap().into(),
        "--format".into(),
        "json".into(),
    ];
    (dir, args)
}

fn record_args(args: &[String]) -> Vec<String> {
    let plan: Value = serde_json::from_slice(&std::fs::read(&args[3]).unwrap()).unwrap();
    let id = plan["candidates"][0]["id"].as_str().unwrap();
    let mut record = args.to_vec();
    record[2] = "record".into();
    record.extend([
        "--candidate".into(),
        id.into(),
        "--assessment".into(),
        "not_useful".into(),
        "--reason".into(),
        "allowed reorder".into(),
        "--expected-revision".into(),
        "0".into(),
    ]);
    record
}

#[tokio::test]
async fn review_list_without_store_has_no_review_or_execution() {
    let (dir, args) = fixture().await;

    let (code, out, err) = run(args).await;
    let list: Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    assert!(!dir.path().join("reviews.sqlite3").exists());
    assert!(list["candidates"][0]["review"].is_null());
    assert_eq!(list["candidates"][0]["execution"]["state"], "not_loaded");
}

#[tokio::test]
async fn review_records_revisions_and_rejects_duplicate_expected_revision() {
    let (_dir, args) = fixture().await;
    let mut record = record_args(&args);
    let before = std::fs::read(&args[3]).unwrap();

    let (record_code, out, err) = run(record.clone()).await;
    let recorded: Value = serde_json::from_slice(&out).unwrap();
    let duplicate_code = run(record.clone()).await.0;
    *record.last_mut().unwrap() = "1".into();
    let next_code = run(record).await.0;
    let (list_code, out, _) = run(args.clone()).await;
    let list: Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(record_code, 0, "{}", String::from_utf8_lossy(&err));
    assert_eq!(recorded["candidates"][0]["review"]["revision"], 1);
    assert_eq!(
        recorded["candidates"][0]["review"]["draft"]["assessment"],
        "not_useful"
    );
    assert_eq!(duplicate_code, 2);
    assert_eq!(next_code, 0);
    assert_eq!(list_code, 0);
    assert_eq!(
        list["candidates"][0]["history"].as_array().unwrap().len(),
        2
    );
    assert_eq!(std::fs::read(&args[3]).unwrap(), before);
}

#[tokio::test]
async fn review_list_excludes_stale_reviews_from_operator_counts() {
    let (dir, args) = fixture().await;
    let (code, _, err) = run(record_args(&args)).await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    std::fs::write(dir.path().join("sample.py"), "value = 9\n").unwrap();

    let (code, out, err) = run(args).await;
    let stale: Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    assert_eq!(stale["candidates"][0]["applicability"], "stale");
    assert!(stale["candidates"][0]["review"].is_null());
    assert!(
        stale["operators"]
            .as_array()
            .unwrap()
            .iter()
            .all(|operator| operator["reviewed"] == 0)
    );
}

#[tokio::test]
async fn review_show_rejects_unknown_candidate() {
    let (_dir, mut args) = fixture().await;
    args[2] = "show".into();
    args.extend(["--candidate".into(), "0".repeat(64)]);

    let (code, _, _) = run(args).await;

    assert_eq!(code, 2);
}

#[tokio::test]
async fn review_output_failure_is_not_duplicate_write() {
    struct BrokenOutput;
    impl std::io::Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("blocked output"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let (_dir, mut args) = fixture().await;
    let plan: Value = serde_json::from_slice(&std::fs::read(&args[3]).unwrap()).unwrap();
    let id = plan["candidates"][0]["id"].as_str().unwrap();
    args[2] = "record".into();
    args.extend([
        "--candidate".into(),
        id.into(),
        "--assessment".into(),
        "actionable".into(),
        "--reason".into(),
        "missing field".into(),
        "--expected-revision".into(),
        "0".into(),
    ]);
    let errors = CollectingWriter::default();

    let output_failure_code = run_with_writers(args.clone(), BrokenOutput, errors.clone()).await;
    let retry_code = run(args.clone()).await.0;
    args[2] = "show".into();
    args.truncate(10);
    let (_, out, _) = run(args).await;
    let value: Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(output_failure_code, 2);
    assert!(String::from_utf8_lossy(&errors.snapshot()).contains("saved at revision 1"));
    assert_eq!(retry_code, 2);
    assert_eq!(
        value["candidates"][0]["history"].as_array().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn review_rejects_old_enum_names_before_project_work() {
    for (field, old) in [
        ("timeout", serde_json::json!("Auto")),
        (
            "timeout",
            serde_json::json!({"Fixed": {"secs": 7, "nanos": 0}}),
        ),
        ("argv", serde_json::json!([{"Unix": [112, 121]}])),
        ("argv", serde_json::json!([{"Windows": [112, 121]}])),
        ("operator", serde_json::json!(["type_mapping"])),
    ] {
        let (dir, args) = fixture().await;
        let mut plan: Value = serde_json::from_slice(&std::fs::read(&args[3]).unwrap()).unwrap();
        match field {
            "timeout" => plan["normalized_config"]["limits"]["mutant_timeout"] = old,
            "argv" => plan["normalized_config"]["test_argv"] = old,
            _ => plan["normalized_config"]["operators"] = old,
        }
        std::fs::write(&args[3], serde_json::to_vec(&plan).unwrap()).unwrap();
        std::fs::remove_file(dir.path().join("sample.py")).unwrap();
        let (code, out, err) = run(args).await;
        assert_eq!(code, 2, "{field}");
        assert!(out.is_empty(), "{field}");
        let error = String::from_utf8(err).unwrap();
        assert!(error.contains("unknown variant"), "{field}: {error}");
        assert!(!dir.path().join("reviews.sqlite3").exists());
    }
}

#[tokio::test]
async fn review_rejects_zero_timeout_before_reading_source_or_creating_store() {
    let (dir, args) = fixture().await;
    let mut plan: Value = serde_json::from_slice(&std::fs::read(&args[3]).unwrap()).unwrap();
    plan["normalized_config"]["limits"]["total_timeout"] =
        serde_json::json!({"secs": 0, "nanos": 0});
    std::fs::write(&args[3], serde_json::to_vec(&plan).unwrap()).unwrap();
    std::fs::remove_file(dir.path().join("sample.py")).unwrap();

    let (code, out, err) = run(args).await;

    assert_eq!(code, 2);
    assert_eq!(out, [] as [u8; 0]);
    let error = String::from_utf8(err).unwrap();
    assert!(
        error.contains("invalid zero or overflowing limit: duration"),
        "{error}"
    );
    assert!(!dir.path().join("reviews.sqlite3").exists());
}

#[tokio::test]
async fn review_record_plan_reference_has_no_run_identity() {
    // Arrange
    let (_dir, args) = fixture().await;

    // Act
    let (code, out, err) = run(record_args(&args)).await;
    let recorded: Value = serde_json::from_slice(&out).unwrap();

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    // Assert
    assert!(
        recorded["candidates"][0]["review"]["draft"]["plan"]
            .get("run_id")
            .is_none()
    );
}

#[cfg(unix)]
fn replace_with_fifo(path: &std::path::Path) {
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated fixture path; mkfifo retains no pointer.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
}

#[cfg(unix)]
struct ReviewChild(Option<std::process::Child>);

#[cfg(unix)]
impl Drop for ReviewChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
fn run_without_fifo_wait(args: &[String], fifo: &std::path::Path) -> (i32, Vec<u8>, Vec<u8>) {
    use std::io::{Read, Seek};
    use std::os::unix::fs::OpenOptionsExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    // File-backed capture cannot block a successful JSON response on pipe capacity.
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut child = ReviewChild(Some(
        Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(&args[1..])
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .unwrap(),
    ));
    let deadline = Instant::now() + Duration::from_secs(2);
    let timed_out = loop {
        if child.0.as_mut().unwrap().try_wait().unwrap().is_some() {
            break false;
        }
        if Instant::now() >= deadline {
            break true;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Release a broken blocking opener, then terminate and reap the process
    // before asserting the deadline. Drop also reaps on an unexpected panic.
    let _rescue = timed_out.then(|| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(fifo)
            .unwrap()
    });
    if timed_out {
        let _ = child.0.as_mut().unwrap().kill();
    }
    let status = child.0.as_mut().unwrap().wait().unwrap();
    child.0 = None;
    let mut out = Vec::new();
    let mut err = Vec::new();
    stdout.rewind().unwrap();
    stderr.rewind().unwrap();
    stdout.read_to_end(&mut out).unwrap();
    stderr.read_to_end(&mut err).unwrap();
    assert!(
        !timed_out,
        "review waited for a FIFO writer: {}; stdout={}; stderr={}",
        fifo.display(),
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err),
    );
    (status.code().unwrap(), out, err)
}

#[cfg(unix)]
#[tokio::test]
async fn review_fifo_plan_is_rejected_without_waiting_for_a_writer() {
    // Arrange
    let (dir, args) = fixture().await;
    let path = std::path::Path::new(&args[3]);
    replace_with_fifo(path);

    // Act
    let (code, out, err) = run_without_fifo_wait(&args, path);

    // Assert
    assert_eq!(code, 2);
    assert_eq!(out, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&err).contains("not a file"));
    assert!(!dir.path().join("reviews.sqlite3").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn review_fifo_report_is_rejected_without_waiting_for_a_writer() {
    // Arrange
    let (dir, mut args) = fixture().await;
    let path = dir.path().join("report.json");
    replace_with_fifo(&path);
    args.extend(["--report".into(), path.to_str().unwrap().into()]);

    // Act
    let (code, out, err) = run_without_fifo_wait(&args, &path);

    // Assert
    assert_eq!(code, 2);
    assert_eq!(out, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&err).contains("not a file"));
    assert!(!dir.path().join("reviews.sqlite3").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn review_fifo_source_without_store_is_unavailable_without_waiting() {
    // Arrange
    let (dir, args) = fixture().await;
    let path = dir.path().join("sample.py");
    replace_with_fifo(&path);

    // Act
    let (code, out, err) = run_without_fifo_wait(&args, &path);

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(list["candidates"][0]["applicability"], "source_unavailable");
    assert!(!dir.path().join("reviews.sqlite3").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn review_fifo_source_with_store_is_unavailable_without_waiting() {
    // Arrange
    let (dir, args) = fixture().await;
    assert_eq!(run(record_args(&args)).await.0, 0);
    let path = dir.path().join("sample.py");
    replace_with_fifo(&path);

    // Act
    let (code, out, err) = run_without_fifo_wait(&args, &path);

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(list["candidates"][0]["applicability"], "source_unavailable");
    assert!(list["candidates"][0]["review"].is_null());
    assert_eq!(
        list["candidates"][0]["history"].as_array().unwrap().len(),
        1
    );
}

#[cfg(unix)]
#[tokio::test]
async fn review_fifo_fingerprint_with_store_does_not_wait_for_a_writer() {
    // Arrange
    let (dir, args) = fixture().await;
    assert_eq!(run(record_args(&args)).await.0, 0);
    let path = dir.path().join("config.toml");
    replace_with_fifo(&path);
    let mut plan: Value = serde_json::from_slice(&std::fs::read(&args[3]).unwrap()).unwrap();
    plan["normalized_config"]["fingerprint_inputs"] = serde_json::json!([{
        "path": "config.toml", "hash": "a".repeat(64)
    }]);
    std::fs::write(&args[3], serde_json::to_vec(&plan).unwrap()).unwrap();

    // Act
    let (code, out, err) = run_without_fifo_wait(&args, &path);

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(list["candidates"][0]["applicability"], "current");
    assert_eq!(list["candidates"][0]["review"]["revision"], 1);
}

#[cfg(unix)]
#[tokio::test]
async fn review_accepts_a_symlink_to_a_regular_plan() {
    // Arrange
    let (dir, mut args) = fixture().await;
    let alias = dir.path().join("plan-link.json");
    std::os::unix::fs::symlink(&args[3], &alias).unwrap();
    args[3] = alias.to_str().unwrap().into();

    // Act
    let (code, out, err) = run(args).await;

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(list["candidates"][0]["applicability"], "current");
}

#[cfg(unix)]
#[tokio::test]
async fn review_regular_source_symlink_with_store_remains_current() {
    // Arrange
    let (dir, args) = fixture().await;
    assert_eq!(run(record_args(&args)).await.0, 0);
    let source = dir.path().join("sample.py");
    let target = dir.path().join("source-target.py");
    std::fs::rename(&source, &target).unwrap();
    std::os::unix::fs::symlink(&target, &source).unwrap();

    // Act
    let (code, out, err) = run(args).await;

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(list["candidates"][0]["applicability"], "current");
    assert_eq!(list["candidates"][0]["review"]["revision"], 1);
}

#[cfg(unix)]
#[tokio::test]
async fn review_store_symlink_is_rejected() {
    // Arrange
    let (dir, args) = fixture().await;
    assert_eq!(run(record_args(&args)).await.0, 0);
    let store = dir.path().join("reviews.sqlite3");
    let target = dir.path().join("store-target.sqlite3");
    std::fs::rename(&store, &target).unwrap();
    std::os::unix::fs::symlink(&target, &store).unwrap();

    // Act
    let (code, out, err) = run(args).await;

    // Assert
    assert_eq!(code, 2);
    assert_eq!(out, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&err).contains("store must be a regular file, not a symlink"));
}

#[tokio::test]
async fn review_missing_source_with_store_is_unavailable() {
    // Arrange
    let (dir, args) = fixture().await;
    assert_eq!(run(record_args(&args)).await.0, 0);
    std::fs::remove_file(dir.path().join("sample.py")).unwrap();

    // Act
    let (code, out, err) = run(args).await;

    // Assert
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(list["candidates"][0]["applicability"], "source_unavailable");
    assert!(list["candidates"][0]["review"].is_null());
    assert_eq!(
        list["candidates"][0]["history"].as_array().unwrap().len(),
        1
    );
}
