#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
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
#[case::def_f_return_3_1("def f():\n    return 3\n", vec!["(2)", "(4)"])]
#[case::x_3_2("x = -3\n", vec!["(-4)", "(-2)"])]
#[case::x_3_comment_3("é = '名前'\r\nx = -(\r\n3 # comment\r\n)\r\n", vec!["(-4)", "(-2)"])]
#[case::x_3_real_4("x = 3 .real\n", vec!["(2)", "(4)"])]
#[case::x_range_3_5("x = range(3)\n", vec!["(2)", "(4)"])]
#[case::x_0_y_6("x = 0 ** y\n", vec!["(-1)", "(1)"])]
#[case::x_0_real_7("x = (0).real\n", vec!["(-1)", "(1)"])]
#[case::x_18446744073709551615_8("x = 18446744073709551615\n", vec!["(18446744073709551614)"])]
#[case::x_18446744073709551615_9("x = -18446744073709551615\n", vec!["(-18446744073709551614)"])]
#[tokio::test]
async fn integer_neighbors_preserve_signed_values_precedence_and_bounds(
    #[case] source: &str,
    #[case] mut expected: Vec<&str>,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "integer_literal_neighbor", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    let mut actual: Vec<_> = candidates
        .iter()
        .map(|c| c["replacement"].as_str().unwrap())
        .collect();
    actual.sort_unstable();
    expected.sort_unstable();

    assert_eq!(actual, expected, "{source}");
    assert_eq!(
        doc["candidates"],
        plan(root.path(), "integer_literal_neighbor", "pass").await["candidates"]
    );
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
    }
}

#[rstest::rstest]
#[case::x_true_y_123_3_1("x = True\ny = '123' # 3\n")]
#[case::x_literal_3_2("x: Literal[3]\n")]
#[case::type_t_literal_3_3("type T = Literal[3]\n")]
#[case::match_x_case_3_pass_4("match x:\n    case 3: pass\n")]
#[case::x_a_3_4_5("x = a[3 + 4]\n")]
#[case::x_a_1_5_1_6("x = a[1:5:1]\n")]
#[case::a_3_4_0_7("a[3] = 4.0\n")]
#[case::input_3_x_4_0_8("(3).x = 4.0\n")]
#[case::del_3_x_9("del (3).x\n")]
#[case::x_a_f_3_g_4_10("x = a[f(3)][g(4):]\n")]
#[case::x_0x10_y_1_000_z_3j_11("x = 0x10\ny = 1_000\nz = 3j\n")]
#[case::x_18446744073709551616_12("x = 18446744073709551616\n")]
#[case::x_18446744073709551616_13("x = -18446744073709551616\n")]
#[tokio::test]
async fn integer_neighbors_exclude_nonruntime_and_structural_slices(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "integer_literal_neighbor", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
}

#[rstest::rstest]
#[case::from_subject_import_count_count_1("from subject import count; count()", 0, 2)]
#[case::from_subject_import_count_assert_count_3_2(
    "from subject import count; assert count() == 3",
    2,
    0
)]
#[tokio::test]
async fn integer_neighbors_saved_plan_distinguishes_observed_constant(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def count():\n    return 3\n",
    )
    .unwrap();
    let doc = plan(root.path(), "integer_literal_neighbor", test).await;
    let manifest = root.path().join("plan.json");
    std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();

    let (_, report) = cli(vec![
        "hoimin".into(),
        "verify".into(),
        manifest.into_os_string(),
        "--top".into(),
        "2".into(),
    ])
    .await;

    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], killed);
    assert_eq!(report["summary"]["counts"]["survived"], survived);
}
