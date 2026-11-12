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
#[case::if_pass_1("if[]: pass\n", 2)]
#[case::if_2("if\"x\": pass\n", 2)]
#[case::if_pass_3("if(): pass\n", 2)]
#[case::if_1_pass_4("if(1,): pass\n", 2)]
#[case::if_x_pass_elif_pass_5("if x: pass\nelif[]: pass\n", 4)]
#[case::if_enabled_action_elif_other_fallback_else_done_6(
    "if enabled:\n    action()\nelif other():\n    fallback()\nelse:\n    done()\n",
    4
)]
#[case::if_a_and_comment_b_run_7(
    "é = '前'\r\nif (\r\n    a and # comment\r\n    b\r\n): run()\r\n",
    2
)]
#[case::if_a_and_b_run_8("if a and \\\n    b: run()\n", 2)]
#[tokio::test]
async fn condition_constants_replace_each_if_elif_test_and_compile(
    #[case] source: &str,
    #[case] expected_count: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "condition_constant", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), expected_count);
    for c in candidates {
        assert!(matches!(
            c["replacement"].as_str().unwrap(),
            "(True)" | "(False)"
        ));
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
    }
    assert_eq!(
        doc["candidates"],
        plan(root.path(), "condition_constant", "pass").await["candidates"]
    );
}

#[rstest::rstest]
#[case::if_true_pass_elif_false_pass_1("if (True): pass\nelif False: pass\n")]
#[case::while_x_pass_assert_x_y_a_if_x_else_b_z_a_for_a_in_xs_i_2(
    "while x: pass\nassert x\ny = a if x else b\nz = [a for a in xs if a]\n"
)]
#[case::if_f_x_1_pass_3("if f(x := 1): pass\n")]
#[case::if_lambda_x_1_pass_4("if (lambda: (x := 1))(): pass\n")]
#[case::async_def_f_if_g_await_h_pass_5("async def f():\n    if g(await h()): pass\n")]
#[case::def_f_if_g_yield_1_pass_6("def f():\n    if g((yield 1)): pass\n")]
#[case::def_f_if_g_yield_from_h_pass_7("def f():\n    if g((yield from h())): pass\n")]
#[tokio::test]
async fn condition_constants_exclude_other_predicates_and_binding_changes(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "condition_constant", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
}

#[rstest::rstest]
#[case::from_subject_import_update_a_update_true_a_assert_a_7_1(
    "from subject import update; a=[]; update(True,a); assert a == [7]",
    1,
    1
)]
#[case::from_subject_import_update_a_update_true_a_update_false_2(
    "from subject import update; a=[]; update(True,a); update(False,a); assert a == [7]",
    2,
    0
)]
#[tokio::test]
async fn condition_constants_saved_plan_needs_both_branch_observations(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def update(enabled, actions):\n    if enabled:\n        actions.append(7)\n",
    )
    .unwrap();
    let doc = plan(root.path(), "condition_constant", test).await;
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

#[rstest::rstest]
#[case::true_1("(True)", "assert calls == [] and outcome == 'yes'\n")]
#[case::false_2("(False)", "assert calls == [] and outcome == 'no'\n")]
#[tokio::test]
async fn condition_constants_skip_predicate_side_effects(
    #[case] forced: &str,
    #[case] assertion: &str,
) {
    let source = "calls = []\ndef predicate():\n    calls.append('condition')\n    return True\nif predicate():\n    outcome = 'yes'\nelse:\n    outcome = 'no'\n";
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "condition_constant", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);

    let c = candidates
        .iter()
        .find(|candidate| candidate["replacement"] == forced)
        .unwrap();
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, forced);
    mutant.push_str(assertion);
    let result = tokio::process::Command::new(python())
        .args(["-c", &mutant])
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
