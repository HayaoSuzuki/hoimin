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

#[tokio::test]
async fn review_cli_contract_and_status_counts() {
    let (dir, args) = fixture().await;
    let (code, out, err) = run(args.clone()).await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    assert!(!dir.path().join("reviews.sqlite3").exists());
    let list: Value = serde_json::from_slice(&out).unwrap();
    let id = list["candidates"][0]["candidate"]["id"].as_str().unwrap();
    assert!(list["candidates"][0]["review"].is_null());
    assert_eq!(list["candidates"][0]["execution"]["state"], "not_loaded");
    let mut record = args.clone();
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
    let before = std::fs::read(&args[3]).unwrap();
    let (code, out, err) = run(record.clone()).await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let recorded: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(recorded["candidates"][0]["review"]["revision"], 1);
    assert_eq!(
        recorded["candidates"][0]["review"]["draft"]["assessment"],
        "not_useful"
    );
    assert_eq!(run(record.clone()).await.0, 2);
    *record.last_mut().unwrap() = "1".into();
    assert_eq!(run(record).await.0, 0);
    let (code, out, _) = run(args.clone()).await;
    assert_eq!(code, 0);
    let list: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        list["candidates"][0]["history"].as_array().unwrap().len(),
        2
    );
    assert_eq!(std::fs::read(&args[3]).unwrap(), before);
    std::fs::write(dir.path().join("sample.py"), "value = 9\n").unwrap();
    let (_, out, _) = run(args.clone()).await;
    let stale: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(stale["candidates"][0]["applicability"], "stale");
    assert!(stale["candidates"][0]["review"].is_null());
    assert!(
        stale["operators"]
            .as_array()
            .unwrap()
            .iter()
            .all(|operator| operator["reviewed"] == 0)
    );
    let mut unknown = args;
    unknown[2] = "show".into();
    unknown.extend(["--candidate".into(), "0".repeat(64)]);
    assert_eq!(run(unknown).await.0, 2);
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
    assert_eq!(
        run_with_io(args.clone(), &mut BrokenOutput, &mut errors).await,
        2
    );
    assert!(String::from_utf8_lossy(&errors).contains("saved at revision 1"));
    assert_eq!(run(args.clone()).await.0, 2);
    args[2] = "show".into();
    args.truncate(10);
    let (_, out, _) = run(args).await;
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        value["candidates"][0]["history"].as_array().unwrap().len(),
        1
    );
}
