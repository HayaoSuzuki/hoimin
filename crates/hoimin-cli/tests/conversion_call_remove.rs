#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

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
#[case::int_1("int")]
#[case::float_2("float")]
#[case::complex_3("complex")]
#[case::bool_4("bool")]
#[case::str_5("str")]
#[case::bytes_6("bytes")]
#[case::list_7("list")]
#[case::tuple_8("tuple")]
#[case::dict_9("dict")]
#[case::set_10("set")]
#[case::frozenset_11("frozenset")]
#[tokio::test]
async fn conversions_cover_builtins_source_and_precedence_1(#[case] name: &str) {
    let source =
        format!("é = 1\r\ndef f(raw):\r\n    return {name}(\r\n        raw # comment\r\n    )\r\n");
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "conversion_call_remove", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), 1, "{name}");
    assert_eq!(candidates[0]["replacement"], "(raw)");
    let c = &candidates[0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.clone();
    mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
    compile(&mutant).await;
    assert_eq!(
        doc["candidates"],
        plan(root.path(), "conversion_call_remove", "pass").await["candidates"]
    );
}

#[rstest::rstest]
#[case::value_int_1_if_true_else_2_1("value = int(1 if True else 2)\n")]
#[case::value_list_1_2_2("value = list((1,2))\n")]
#[case::value_str_lambda_1_3("value = str(lambda: 1)\n")]
#[tokio::test]
async fn conversions_cover_builtins_source_and_precedence_2(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "conversion_call_remove", "pass").await;
    let c = &doc["candidates"][0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
    compile(&mutant).await;
}

#[tokio::test]
async fn conversion_removal_keeps_arithmetic_precedence_at_runtime() {
    let source = "value = 2 * int(1 + 2)\n";
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "conversion_call_remove", "pass").await;
    let candidate = &doc["candidates"][0];
    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, candidate["replacement"].as_str().unwrap());
    compile(&mutant).await;
    mutant.push_str("assert value == 6\n");

    assert!(
        tokio::process::Command::new(python())
            .args(["-c", &mutant])
            .status()
            .await
            .unwrap()
            .success()
    );
}

#[rstest::rstest]
#[case::int_custom_x_int_raw_1("int = custom\nx = int(raw)\n")]
#[case::complex_custom_x_complex_raw_2("complex = custom\nx = complex(raw)\n")]
#[case::def_f_complex_return_complex_raw_3("def f(complex): return complex(raw)\n")]
#[case::def_rebind_global_complex_complex_custom_def_f_return_c_4(
    "def rebind():\n    global complex\n    complex = custom\ndef f(): return complex(raw)\n"
)]
#[case::def_f_int_return_int_raw_5("def f(int): return int(raw)\n")]
#[case::def_f_x_int_raw_int_custom_6("def f():\n    x = int(raw)\n    int = custom\n")]
#[case::def_rebind_global_int_int_custom_def_f_return_int_raw_7(
    "def rebind():\n    global int\n    int = custom\ndef f(): return int(raw)\n"
)]
#[case::def_outer_int_custom_def_inner_nonlocal_int_return_int_8(
    "def outer():\n    int = custom\n    def inner():\n        nonlocal int\n        return int(raw)\n"
)]
#[case::from_somewhere_import_x_int_raw_9("from somewhere import *\nx = int(raw)\n")]
#[case::exec_code_x_int_raw_10("exec(code)\nx = int(raw)\n")]
#[case::x_builtins_int_raw_x_int_x_int_raw_10_x_int_raw_base_10_11(
    "x = builtins.int(raw)\nx = int()\nx = int(raw, 10)\nx = int(raw, base=10)\nx = int(*args)\nx = int(**kw)\n"
)]
#[case::x_list_x_for_x_in_xs_x_int_x_for_x_in_y_for_y_in_ys_x_i_12(
    "x = list(x for x in xs)\nx = int([x for x in (y for y in ys)])\nx = int(a:=1)\n"
)]
#[case::async_def_f_return_int_await_g_13("async def f(): return int(await g())\n")]
#[case::def_f_return_int_yield_1_14("def f(): return int((yield 1))\n")]
#[case::from_typing_import_typealias_a_typealias_int_raw_x_int_15(
    "from typing import TypeAlias\nA: TypeAlias = int(raw)\nx: int(raw)\ntype B = int(raw)\na[int(raw)] = 1\n"
)]
#[tokio::test]
async fn conversions_exclude_shadowing_unsafe_and_nonruntime_roles(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    assert!(
        plan(root.path(), "conversion_call_remove", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_subject_import_count_assert_count_7_7_1(
    "from subject import count; assert count(7)==7",
    0,
    1
)]
#[case::from_subject_import_count_value_count_7_assert_type_val_2(
    "from subject import count; value=count('7'); assert type(value) is int and value==7",
    1,
    0
)]
#[tokio::test]
async fn conversions_saved_plan_needs_string_input_and_type_check(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def count(raw): return int(raw)\n",
    )
    .unwrap();
    let doc = plan(root.path(), "conversion_call_remove", test).await;
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

#[rstest::rstest]
#[case::calls_class_box_def_int_self_calls_append_convert_retur_1(
    "calls=[]\nclass Box:\n    def __int__(self):\n        calls.append('convert')\n        return 7\ndef get():\n    calls.append('argument')\n    return Box()\nvalue = int(get())\n",
    "assert calls == ['argument'] and type(value) is Box\n"
)]
#[case::original_value_dict_original_value_x_1_2(
    "original={}\nvalue=dict(original)\nvalue['x']=1\n",
    "assert original == {'x':1} and value is original\n"
)]
#[tokio::test]
async fn conversions_keep_one_argument_evaluation_and_distinct_legacy_edits(
    #[case] source: &str,
    #[case] assertion: &str,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "conversion_call_remove", "pass").await;
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
    let c = &doc["candidates"][0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
    mutant.push_str(assertion);
    let output = tokio::process::Command::new(python())
        .args(["-c", &mutant])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn conversions_preserve_distinct_legacy_operator_edits() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "x = list(raw)\n").unwrap();
    let doc = plan(
        root.path(),
        "conversion_call_remove,collection_list_tuple",
        "pass",
    )
    .await;
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 2);
    assert_ne!(
        doc["candidates"][0]["operator"],
        doc["candidates"][1]["operator"]
    );
}
