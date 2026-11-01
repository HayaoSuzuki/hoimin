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
#[case::decorate_def_initialize_state_value_3_documentation_sta_1(
    "@decorate\ndef initialize(state, value=3):\n    'documentation'\n    state.ready = True\n    state.count = value\n"
)]
#[case::def_initialize_state_doc_state_ready_true_state_count_0_2(
    "def initialize(state): 'doc'; state.ready = True; state.count = 0 # tail\n"
)]
#[case::def_initialize_state_before_state_ready_true_state_coun_3(
    "é = '前'\r\ndef initialize(state):\r\n    # before\r\n    state.ready = True\r\n    state.count = 0 # tail\r\n"
)]
#[tokio::test]
async fn body_erasure_preserves_headers_docstrings_and_valid_suites(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "function_body_erase", "pass").await;
    let cs = doc["candidates"].as_array().unwrap();

    assert_eq!(cs.len(), 1);
    let c = &cs[0];
    assert_eq!(c["replacement"], "pass");
    assert!(
        c["original"]
            .as_str()
            .unwrap()
            .starts_with("state.ready = True")
    );
    assert!(c["original"].as_str().unwrap().contains("state.count"));
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, "pass");
    compile(&mutant).await;
    assert_eq!(&mutant[..start], &source[..start]);
}

#[rstest::rstest]
#[case::def_f_s_s_x_1_return_1("def f(s):\n    s.x=1\n    return\n", 1)]
#[case::def_f_s_s_x_1_return_none_2("def f(s):\n    s.x=1\n    return None\n", 1)]
#[case::def_f_s_s_x_1_return_1_3("def f(s):\n    s.x=1\n    return 1\n", 0)]
#[case::def_f_yield_1_4("def f():\n    yield 1\n", 0)]
#[case::def_f_yield_from_g_5("def f():\n    yield from g()\n", 0)]
#[case::async_def_f_s_s_x_1_6("async def f(s):\n    s.x=1\n", 0)]
#[case::class_c_def_init_self_self_x_1_7(
    "class C:\n    def __init__(self):\n        self.x=1\n",
    0
)]
#[case::def_f_doc_pass_return_none_8("def f():\n    'doc'\n    pass\n    return None\n", 0)]
#[case::def_f_9("def f(): ...\n", 0)]
#[case::def_f_def_inner_return_1_call_10(
    "def f():\n    def inner():\n        return 1\n    call()\n",
    1
)]
#[case::def_f_def_inner_yield_1_call_11(
    "def f():\n    def inner():\n        yield 1\n    call()\n",
    1
)]
#[case::def_f_def_inner_x_yield_1_return_1_call_12(
    "def f():\n    def inner(x=(yield 1)):\n        return 1\n    call()\n",
    0
)]
#[case::def_f_value_lambda_yield_1_call_13(
    "def f():\n    value = lambda: (yield 1)\n    call()\n",
    1
)]
#[case::def_f_value_lambda_x_yield_1_x_call_14(
    "def f():\n    value = lambda x=(yield 1): x\n    call()\n",
    0
)]
#[tokio::test]
async fn body_erasure_checks_function_scope_and_excludes_noops(
    #[case] source: &str,
    #[case] count: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "function_body_erase", "pass").await;

    assert_eq!(
        doc["candidates"].as_array().unwrap().len(),
        count,
        "{source}"
    );
    assert!(
        doc["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|candidate| candidate["symbol"] == "f")
    );
}

#[rstest::rstest]
#[case::from_subject_import_initialize_s_initialize_s_1(
    "from subject import initialize; s={}; initialize(s)",
    0,
    1
)]
#[case::from_subject_import_initialize_s_initialize_s_assert_s_2(
    "from subject import initialize; s={}; initialize(s); assert s == {'ready': True, 'count': 3}",
    1,
    0
)]
#[tokio::test]
async fn body_erasure_saved_plan_requires_state_observation(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def initialize(s):\n    s['ready'] = True\n    s['count'] = 3\n",
    )
    .unwrap();
    let doc = plan(root.path(), "function_body_erase", test).await;
    let manifest = root.path().join("plan.json");
    std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();

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
}

#[tokio::test]
async fn body_erasure_keeps_the_shared_serialized_size_limit() {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "def f(s):\n    s.x = 1\n    #{}\n    s.y = 2\n",
        "x".repeat(2 * 1024 * 1024)
    );
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(
        vec![
            "hoimin".into(),
            "plan".into(),
            "--root".into(),
            root.path().as_os_str().to_owned(),
            "--file".into(),
            "subject.py".into(),
            "--operators".into(),
            "function_body_erase".into(),
            "--allow-best-effort-memory".into(),
            "--".into(),
            python().into_os_string(),
            "-c".into(),
            "pass".into(),
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;
    assert_eq!(code, 2);
    assert!(String::from_utf8_lossy(&stderr.snapshot()).contains("candidate spool record exceeds"));
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
async fn body_erasure_preserves_encoded_docstrings_and_newlines(
    #[case] newline: &str,
    #[case] encoding: &encoded_source::SourceEncoding,
) {
    let root = tempfile::tempdir().unwrap();
    let encoding_name = encoding.name;
    let text = format!(
        "# coding: {}{newline}def f(s):{newline}    'é'{newline}    s.x=1{newline}    s.y=2{newline}",
        encoding.cookie
    );
    let mut bytes = encoding.source(&text);
    std::fs::write(root.path().join("subject.py"), &bytes).unwrap();

    let document = plan(root.path(), "function_body_erase", "pass").await;
    let cs = document["candidates"].as_array().unwrap();

    assert_eq!(cs.len(), 1, "{encoding_name} {newline:?}");
    let c = &cs[0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    assert_eq!(
        &bytes[start..end],
        format!("s.x=1{newline}    s.y=2").as_bytes()
    );
    bytes.splice(start..end, b"pass".iter().copied());
    let result = tokio::process::Command::new(python()).args(["-c",
                "import json,sys; assert sys.version_info[:2] in ((3, 14), (3, 15)); ns={}; exec(compile(bytes(json.loads(sys.argv[1])), '<encoded>', 'exec'),ns); assert ns['f'].__doc__ == 'é'",
                &serde_json::to_string(&bytes).unwrap()]).output().await.unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[rstest::rstest]
#[case::function_body_erase_1("function_body_erase", 0, 1)]
#[case::collection_min_max_2("collection_min_max", 1, 0)]
#[tokio::test]
async fn body_erasure_does_not_replace_finer_mutations(
    #[case] operator: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def cap(s):\n    s['x'] = min(s['x'], 3)\n",
    )
    .unwrap();

    let doc = plan(
        root.path(),
        operator,
        "from subject import cap; s={'x': 2}; cap(s); assert s['x'] == 2",
    )
    .await;

    assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
    let manifest = root.path().join("plan.json");
    std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
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
}
