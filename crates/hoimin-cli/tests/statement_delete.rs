#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

#[path = "support/encoded_source.rs"]
mod encoded_source;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
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
        },
        PathBuf::from,
    )
}

async fn compile(source: &str) {
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args([
                "-c",
                "import sys; assert sys.version_info[:2] in ((3, 14), (3, 15)), sys.version; compile(sys.argv[1], '<neighbor>', 'exec')",
                source,
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython compiler deadline")
    .expect("CPython 3.14 or 3.15; set HOIMIN_OPERATOR_TEST_PYTHON to its executable");
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value) {
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert!(
        code == 0,
        "exit={code}, stderr={}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    (
        code,
        jsonl_report::from_slice(&stdout.snapshot()).expect("one JSON document"),
    )
}

async fn plan(root: &Path, operators: &str, test: &str) -> serde_json::Value {
    let args = [
        OsString::from("hoimin"),
        "plan".into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        operators.into(),
        "--min-free-space".into(),
        "1B".into(),
        "--baseline-timeout".into(),
        "10s".into(),
        "--mutant-timeout".into(),
        "10s".into(),
        "--total-timeout".into(),
        "30s".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        test.into(),
    ];
    let (code, document) = cli(args.into()).await;
    assert_eq!(code, 0);
    document
}

#[rstest::rstest]
#[case::def_save_store_record_store_persist_record_1(
    "def save(store, record):\n    store.persist(record)\n"
)]
#[case::if_flag_call_other_1_tail_2("if flag: call(); other = 1 # tail\n")]
#[case::input_1_call_1_inside_tail_3("é = 1\ncall(\n    1, # inside\n) # tail\n")]
#[case::class_c_call_4("class C:\n    call()\n")]
#[case::call_parentheses_5("(call()) # parentheses\n")]
#[tokio::test]
async fn statement_deletion_preserves_suites_and_rejects_binding_changes_1(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let document = plan(root.path(), "statement_delete", "pass").await;
    let candidates = document["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), 1, "{source}");
    let candidate = &candidates[0];
    assert_eq!(candidate["replacement"], "pass");
    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, "pass");
    compile(&mutant).await;
}

#[rstest::rstest]
#[case::value_call_1("value = call()\n")]
#[case::def_f_return_call_2("def f():\n    return call()\n")]
#[case::async_def_f_call_await_g_3("async def f():\n    call(await g())\n")]
#[case::def_f_call_yield_1_4("def f():\n    call((yield 1))\n")]
#[case::call_lambda_yield_1_5("call(lambda: (yield 1))\n")]
#[case::def_f_call_yield_from_g_6("def f():\n    call((yield from g()))\n")]
#[case::call_x_1_7("call((x := 1))\n")]
#[case::x_call_8("x: call()\n")]
#[case::type_t_call_9("type T = call()\n")]
#[tokio::test]
async fn statement_deletion_preserves_suites_and_rejects_binding_changes_2(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let document = plan(root.path(), "statement_delete", "pass").await;

    assert_eq!(
        document["candidates"].as_array().unwrap().len(),
        0,
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_subject_import_save_save_1("from subject import save; save([])", 0, 1)]
#[case::from_subject_import_save_store_save_store_assert_store_2(
    "from subject import save; store = []; save(store); assert store == [7]",
    1,
    0
)]
#[tokio::test]
async fn statement_deletion_saved_plan_exposes_unobserved_persistence(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    let source = "def save(store):\n    store.append(7)\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let document = plan(root.path(), "statement_delete", test).await;

    assert_eq!(document["candidates"].as_array().unwrap().len(), 1);
    let repeated = plan(root.path(), "statement_delete", test).await;
    assert_eq!(document["candidates"], repeated["candidates"]);
    let manifest = root.path().join("plan.json");
    std::fs::write(&manifest, serde_json::to_vec(&document).unwrap()).unwrap();
    let (_, report) = cli(vec![
        "hoimin".into(),
        "verify".into(),
        manifest.into_os_string(),
        "--top".into(),
        "1".into(),
    ])
    .await;
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], killed);
    assert_eq!(report["summary"]["counts"]["survived"], survived);
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}

#[rstest::rstest]
#[case::lf_utf8("\n", &encoded_source::ENCODINGS[0])]
#[case::lf_bom("\n", &encoded_source::ENCODINGS[1])]
#[case::lf_latin1("\n", &encoded_source::ENCODINGS[2])]
#[case::crlf_utf8("\r\n", &encoded_source::ENCODINGS[0])]
#[case::crlf_bom("\r\n", &encoded_source::ENCODINGS[1])]
#[case::crlf_latin1("\r\n", &encoded_source::ENCODINGS[2])]
#[case::cr_utf8("\r", &encoded_source::ENCODINGS[0])]
#[case::cr_bom("\r", &encoded_source::ENCODINGS[1])]
#[case::cr_latin1("\r", &encoded_source::ENCODINGS[2])]
#[tokio::test]
async fn statement_deletion_preserves_encoded_bytes_and_newlines(
    #[case] newline: &str,
    #[case] encoding: &encoded_source::SourceEncoding,
) {
    let root = tempfile::tempdir().unwrap();
    let encoding_name = encoding.name;
    let text = format!(
        "# coding: {}{newline}é = 1{newline}call(); other = 2 # tail{newline}",
        encoding.cookie
    );
    let mut bytes = encoding.source(&text);
    std::fs::write(root.path().join("subject.py"), &bytes).unwrap();

    let document = plan(root.path(), "statement_delete", "pass").await;
    let candidates = document["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), 1, "{encoding_name} {newline:?}");
    let c = &candidates[0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    assert_eq!(&bytes[start..end], b"call()");
    bytes.splice(start..end, b"pass".iter().copied());
    let result = tokio::process::Command::new(python()).args(["-c",
                "import json,sys; assert sys.version_info[:2] in ((3, 14), (3, 15)); compile(bytes(json.loads(sys.argv[1])), '<encoded>', 'exec')",
                &serde_json::to_string(&bytes).unwrap()]).output().await.unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn statement_deletion_is_default_and_excludable() {
    use hoimin_core::{MutationOperator, MutationOperatorSelection};
    let operator = MutationOperator::from_name("statement_delete").unwrap();
    let mut selection = MutationOperatorSelection::default();

    let initially_selected = selection.contains(operator);
    selection.include(operator);
    let included = selection.contains(operator);
    selection.exclude(operator);
    let excluded = selection.contains(operator);

    assert!(initially_selected);
    assert!(included);
    assert!(!excluded);
}
