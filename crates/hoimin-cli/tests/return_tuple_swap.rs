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
#[case::return_lower_upper_1("return lower, upper", "upper, lower")]
#[case::return_lower_upper_2("return (lower, upper,)", "(upper, lower,)")]
#[case::return_lower_first_upper_3(
    "return (lower, # first\n            upper,)",
    "(upper, # first\n            lower,)"
)]
#[case::return_a_b_3_4("return ('a,b', 3)", "(3, 'a,b')")]
#[case::return_x_upper_5("return'x', upper", "(upper, 'x')")]
#[case::return_1_upper_6("return-1, upper", "(upper, -1)")]
#[case::return_none_true_7("return (None, True)", "(True, None)")]
#[tokio::test]
async fn return_tuple_swap_preserves_tuple_source_structure(
    #[case] body: &str,
    #[case] expected: &str,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!("é='前'\ndef f(lower=1, upper=2):\n    {body}\n");
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "return_tuple_swap", "pass").await;
    let cs = doc["candidates"].as_array().unwrap();

    assert_eq!(cs.len(), 1, "{source} {doc}");
    let c = &cs[0];
    assert_eq!(c["replacement"], expected, "{source}");
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.clone();
    mutant.replace_range(start..end, expected);
    compile(&mutant).await;
    assert_eq!(
        doc["candidates"],
        plan(root.path(), "return_tuple_swap", "pass").await["candidates"]
    );
}

#[rstest::rstest]
#[case::def_f_return_1_2_1("def f(): return 1, 2\n", 1)]
#[case::async_def_f_return_1_2_2("async def f(): return 1, 2\n", 0)]
#[case::def_f_yield_0_return_1_2_3("def f():\n    yield 0\n    return 1, 2\n", 0)]
#[case::def_f_yield_from_return_1_2_4("def f():\n    yield from []\n    return 1, 2\n", 0)]
#[case::def_f_def_g_yield_0_return_1_2_5("def f():\n    def g(): yield 0\n    return 1, 2\n", 1)]
#[case::def_f_def_g_x_yield_0_return_3_4_return_1_2_6(
    "def f():\n    def g(x=(yield 0)): return 3, 4\n    return 1, 2\n",
    1
)]
#[case::async_def_f_def_g_return_3_4_return_1_2_7(
    "async def f():\n    def g(): return 3, 4\n    return 1, 2\n",
    1
)]
#[case::def_f_return_x_x_8("def f(): return x, x\n", 0)]
#[case::def_f_u_212a_return_u_212a_k_9("def f(\u{212a}): return \u{212a}, K\n", 0)]
#[case::def_f_return_x_for_x_in_xs_y_10("def f(): return [x for x in xs], y\n", 0)]
#[case::def_f_return_x_x_11("def f(): return 'x', 'x'\n", 0)]
#[case::def_f_return_12("def f(): return (),\n", 0)]
#[case::def_f_return_1_2_3_13("def f(): return 1, 2, 3\n", 0)]
#[case::def_f_return_pair_14("def f(): return pair\n", 0)]
#[case::def_f_return_f_x_15("def f(): return f(), x\n", 0)]
#[case::def_f_return_obj_x_x_16("def f(): return obj.x, x\n", 0)]
#[case::def_f_return_obj_0_x_17("def f(): return obj[0], x\n", 0)]
#[case::def_f_return_x_1_x_18("def f(): return (x:=1), x\n", 0)]
#[case::def_f_return_xs_x_19("def f(): return *xs, x\n", 0)]
#[case::def_f_return_1_2_x_20("def f(): return (1,2), x\n", 0)]
#[case::def_f_return_a_b_x_21("def f(): return 'a' 'b', x\n", 0)]
#[case::def_f_return_f_x_x_22("def f(): return f'x', x\n", 0)]
#[case::def_f_return_b_x_x_23("def f(): return b'x', x\n", 0)]
#[case::def_f_pair_1_2_24("def f(): pair=1,2\n", 0)]
#[tokio::test]
async fn return_tuple_swap_checks_atomic_elements_and_function_scope(
    #[case] source: &str,
    #[case] count: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "return_tuple_swap", "pass").await;

    assert_eq!(
        doc["candidates"].as_array().unwrap().len(),
        count,
        "{source} {doc}"
    );
}

#[rstest::rstest]
#[case::from_subject_import_bounds_assert_len_bounds_2_1(
    "from subject import bounds; assert len(bounds())==2",
    0,
    1
)]
#[case::from_subject_import_bounds_assert_bounds_3_9_2(
    "from subject import bounds; assert bounds()==(3,9)",
    1,
    0
)]
#[tokio::test]
async fn return_tuple_swap_requires_positional_assertions(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def bounds():\n    return 3, 9\n",
    )
    .unwrap();
    let doc = plan(root.path(), "return_tuple_swap", test).await;
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
