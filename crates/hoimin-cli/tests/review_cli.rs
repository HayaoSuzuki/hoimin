use hoimin_cli::run_with_io;
use serde_json::Value;

async fn run(args: Vec<String>) -> (i32, Vec<u8>, Vec<u8>) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_with_io(args, &mut out, &mut err).await;
    (code, out, err)
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
    let mut errors = Vec::new();

    let output_failure_code = run_with_io(args.clone(), &mut BrokenOutput, &mut errors).await;
    let retry_code = run(args.clone()).await.0;
    args[2] = "show".into();
    args.truncate(10);
    let (_, out, _) = run(args).await;
    let value: Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(output_failure_code, 2);
    assert!(String::from_utf8_lossy(&errors).contains("saved at revision 1"));
    assert_eq!(retry_code, 2);
    assert_eq!(
        value["candidates"][0]["history"].as_array().unwrap().len(),
        1
    );
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
