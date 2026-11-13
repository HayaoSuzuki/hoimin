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
#[case::input_1_2_3_1("[1,2,3]", 3, "assert value in ([2,3],[1,3],[1,2])")]
#[case::input_1_2_2("(1,2)", 2, "assert type(value) is tuple and value in ((1,), (2,))")]
#[case::a_1_b_2_c_3_3(
    "{'a':1,'b':2,'c':3}",
    3,
    "assert type(value) is dict and len(value)==2"
)]
#[case::input_1_4("[1]", 1, "assert value == []")]
#[case::input_1_5("(1,)", 1, "assert value == ()")]
#[case::a_1_6("{'a':1}", 1, "assert value == {}")]
#[case::input_1_1_7("[1,1]", 1, "assert value == [1]")]
#[case::input_1_2_3_8("[[1,2],3]", 4, "assert type(value) is list")]
#[case::a_b_comma_lambda_c_d_9(
    "[\n 'a,b', # comma\n (lambda: 'c,d')(),\n]",
    2,
    "assert value in (['a,b'], ['c,d'])"
)]
#[tokio::test]
async fn containers_delete_one_element_and_compile(
    #[case] literal: &str,
    #[case] count: usize,
    #[case] assertion: &str,
) {
    let source = format!("é = 1\r\nvalue = {literal}\n");
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "container_element_delete", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), count, "{source}");
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
        mutant.push_str(assertion);

        let output = tokio::process::Command::new(python())
            .args(["-c", &mutant])
            .output()
            .await
            .unwrap();

        assert!(
            output.status.success(),
            "{mutant}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        doc["candidates"],
        plan(root.path(), "container_element_delete", "pass").await["candidates"]
    );
}

#[rstest::rstest]
#[case::a_b_c_d_1_2_e_x_for_x_in_xs_f_1_2_1(0)]
#[case::a_items_1_b_mapping_a_1_2(1)]
#[case::a_x_1_2_3(2)]
#[case::async_def_f_return_await_g_1_4(3)]
#[case::def_f_return_yield_1_2_5(4)]
#[case::a_b_items_a_1_2_3_del_a_1_2_6(5)]
#[case::from_typing_import_typealias_literal_a_typealias_literal_1_2_x_li_7(6)]
#[case::match_x_case_1_2_pass_8(7)]
#[tokio::test]
async fn containers_exclude_unsupported_and_nonruntime_roles_case(#[case] scenario_index: usize) {
    let source = [
        "a=[]; b=(); c={}; d={1,2}; e=[x for x in xs]; f=1,2\n",
        "a=[*items,1]; b={**mapping,'a':1}\n",
        "a=[(x:=1),2]\n",
        "async def f():\n    return [await g(),1]\n",
        "def f():\n    return [(yield 1),2]\n",
        "[a,b]=items\na[[1,2]]=3\ndel a[(1,2)]\n",
        "from typing import TypeAlias, Literal\nA: TypeAlias = Literal[[1,2]]\nx: Literal[(1,2)]\ntype B = Literal[[3,4]]\n",
        "match x:\n    case [1,2]: pass\n",
    ].into_iter().nth(scenario_index).unwrap();

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    assert!(
        plan(root.path(), "container_element_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{source}"
    );
}

#[tokio::test]
async fn containers_exclude_unsupported_and_nonruntime_roles() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "x: list = [1,2]\ny = matrix[[3,4]]\n",
    )
    .unwrap();
    assert_eq!(
        plan(root.path(), "container_element_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[rstest::rstest]
#[case::from_subject_import_payload_assert_payload_7_user_id_7_1(
    "from subject import payload; assert payload(7)['user_id']==7",
    1,
    1
)]
#[case::from_subject_import_payload_assert_payload_7_user_id_7_2(
    "from subject import payload; assert payload(7)=={'user_id':7,'enabled':True}",
    2,
    0
)]
#[tokio::test]
async fn containers_saved_plan_requires_all_payload_fields(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "def payload(user_id):\n    return {'user_id':user_id,'enabled':True}\n",
    )
    .unwrap();
    let doc = plan(root.path(), "container_element_delete", test).await;
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

#[tokio::test]
async fn containers_remove_element_evaluation_once() {
    let source =
        "calls=[]\ndef mark(n):\n    calls.append(n)\n    return n\nvalue = [mark(1),mark(2)]\n";
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "container_element_delete", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        mutant.push_str("assert calls == value and len(calls)==1\n");
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
}
