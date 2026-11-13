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
#[case::render_escape_false_1("render(escape=False)", 1)]
#[case::render_text_v_mode_raw_escape_false_2("render(text='v', mode='raw', escape=False)", 3)]
#[case::render_v_mode_raw_escape_false_ending_3(
    "render('v', mode='raw', escape=False, ending='.')",
    3
)]
#[case::render_text_v_comment_escape_false_ending_4(
    "render(\n text='v', # comment\n escape=(False),\n ending='.',\n)",
    3
)]
#[case::render_escape_false_5("(render)(escape=False)", 1)]
#[tokio::test]
async fn optional_keywords_cover_positions_and_compile(#[case] call: &str, #[case] count: usize) {
    let definition = "def render(text='x', mode='plain', *, escape=True, ending='!'):\n    return text, mode, escape, ending\n";

    let source = format!("é = 1\r\n{definition}value = {call}\n");
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "optional_keyword_delete", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), count, "{source}");
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
        mutant.push_str("assert type(value) is tuple and len(value)==4\n");
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
        plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
    );
}

#[rstest::rstest]
#[case::f_a_2_1("f(a=2)")]
#[case::f_1_x_2_a_3_2("f(1,x=2,a=3)")]
#[case::f_1_a_2_unknown_3_3("f(1,a=2,unknown=3)")]
#[case::f_1_2_a_3_4("f(1,2,a=3)")]
#[case::f_x_1_a_2_5("f(x=1,a=2)")]
#[case::f_args_a_2_6("f(*args,a=2)")]
#[case::f_1_kw_7("f(1,**kw)")]
#[tokio::test]
async fn optional_keywords_reject_incomplete_signature_binding(#[case] call: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        format!("def f(x, /, *, a=1): return x+a\n{call}\n"),
    )
    .unwrap();
    assert!(
        plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{call}"
    );
}

#[tokio::test]
async fn optional_keywords_accept_complete_signature_binding() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "def f(x, /, y=2, *, required, a=1): return x+y+a\nf(1,required=0,a=3)\nf(1,y=3,required=0,a=4)\n").unwrap();
    assert_eq!(
        plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[rstest::rstest]
#[case::def_f_args_a_1_pass_f_a_2_1("def f(*args,a=1): pass\nf(a=2)\n")]
#[case::def_f_a_1_kw_pass_f_a_2_2("def f(a=1,**kw): pass\nf(a=2)\n")]
#[case::async_def_f_a_1_pass_f_a_2_3("async def f(a=1): pass\nf(a=2)\n")]
#[case::decorator_def_f_a_1_pass_f_a_2_4("@decorator\ndef f(a=1): pass\nf(a=2)\n")]
#[case::if_flag_def_f_a_1_pass_f_a_2_5("if flag:\n    def f(a=1): pass\nf(a=2)\n")]
#[case::def_f_a_1_pass_def_f_a_1_pass_f_a_2_6("def f(a=1): pass\ndef f(a=1): pass\nf(a=2)\n")]
#[case::def_f_a_1_pass_f_other_f_a_2_7("def f(a=1): pass\nf = other\nf(a=2)\n")]
#[case::from_other_import_f_f_a_2_8("from other import f\nf(a=2)\n")]
#[case::def_f_a_1_pass_alias_f_alias_a_2_f_a_2_9("def f(a=1): pass\nalias=f\nalias(a=2)\nf(a=2)\n")]
#[case::def_f_a_1_pass_def_g_f_return_f_a_2_10("def f(a=1): pass\ndef g(f): return f(a=2)\n")]
#[case::def_f_a_1_pass_def_g_f_a_2_f_other_11(
    "def f(a=1): pass\ndef g():\n    f(a=2)\n    f=other\n"
)]
#[case::def_f_a_1_pass_def_g_global_f_f_other_f_a_2_12(
    "def f(a=1): pass\ndef g():\n    global f\n    f=other\nf(a=2)\n"
)]
#[case::def_f_a_1_pass_f_defaults_3_f_a_2_13("def f(a=1): pass\nf.__defaults__=(3,)\nf(a=2)\n")]
#[case::def_f_a_1_pass_sink_f_f_a_2_14("def f(a=1): pass\nsink(f)\nf(a=2)\n")]
#[case::def_f_a_1_pass_exec_code_f_a_2_15("def f(a=1): pass\nexec(code)\nf(a=2)\n")]
#[case::f_a_2_def_f_a_1_pass_16("f(a=2)\ndef f(a=1): pass\n")]
#[case::def_f_a_1_pass_f_a_x_2_17("def f(a=1): pass\nf(a=(x:=2))\n")]
#[case::def_f_a_1_pass_async_def_g_return_f_a_await_h_18(
    "def f(a=1): pass\nasync def g(): return f(a=await h())\n"
)]
#[case::def_f_a_1_pass_def_g_return_f_a_yield_2_19(
    "def f(a=1): pass\ndef g(): return f(a=(yield 2))\n"
)]
#[case::from_typing_import_typealias_def_f_a_1_pass_a_typealias_20(
    "from typing import TypeAlias\ndef f(a=1): pass\nA: TypeAlias = f(a=2)\nx: f(a=2)\ntype B = f(a=2)\ntarget[f(a=2)] = 1\n"
)]
#[tokio::test]
async fn optional_keywords_exclude_ambiguous_and_unsafe_calls(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    assert!(
        plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{source}"
    );
}

#[rstest::rstest]
#[case::from_subject_import_page_assert_isinstance_page_str_1(
    "from subject import page; assert isinstance(page(),str)",
    0,
    1
)]
#[case::from_subject_import_page_assert_page_x_2(
    "from subject import page; assert page() == '<x>'",
    1,
    0
)]
#[tokio::test]
async fn optional_keywords_saved_plan_observes_explicit_option(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "def render(text, *, escape=True):\n    return text.replace('<','&lt;') if escape else text\ndef page(): return render('<x>',escape=False)\n").unwrap();
    let doc = plan(root.path(), "optional_keyword_delete", test).await;
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
#[case::events_def_default_events_append_default_return_true_de_1(
    "events=[]\ndef default():\n    events.append('default')\n    return True\ndef explicit():\n    events.append('argument')\n    return False\ndef f(escape=default()): return escape\nvalue=f(escape=explicit())\n",
    1,
    "assert events==['default'] and value is True\n"
)]
#[case::events_def_mark_n_events_append_n_return_n_def_f_x_a_0_2(
    "events=[]\ndef mark(n):\n    events.append(n)\n    return n\ndef f(x,a=0,b=0): return x,a,b\nvalue=f(mark(1),a=mark(2),b=mark(3))\n",
    2,
    "assert (events,value) in [([1,2],(1,2,0)),([1,3],(1,0,3))]\n"
)]
#[tokio::test]
async fn optional_keywords_use_existing_default_and_keep_evaluation_order(
    #[case] source: &str,
    #[case] count: usize,
    #[case] assertion: &str,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "optional_keyword_delete", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), count);
    for c in candidates {
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
}

#[rstest::rstest]
#[case::def_f_flag_true_return_flag_def_g_return_f_flag_false_1(
    "def f(*, flag=True): return flag\ndef g(): return(f)(flag=False)\n",
    "assert g() is True\n"
)]
#[case::def_f_flag_true_return_flag_if_f_flag_false_value_1_els_2(
    "def f(*, flag=True): return flag\nif(f)(flag=False): value=1\nelse: value=0\n",
    "assert value==1\n"
)]
#[case::def_f_flag_true_return_flag_while_f_flag_false_value_1_3(
    "def f(*, flag=True): return flag\nwhile(f)(flag=False): value=1; break\nelse: value=0\n",
    "assert value==1\n"
)]
#[case::def_true_return_value_false_4(
    "def 𝒊𝒇(*, 𝒇𝒐𝒓=True): return 𝒇𝒐𝒓\nvalue=𝒊𝒇(𝒇𝒐𝒓=False)\n",
    "assert value is True\n"
)]
#[tokio::test]
async fn optional_keywords_preserve_keyword_adjacency_and_unicode_spelling(
    #[case] source: &str,
    #[case] assertion: &str,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "optional_keyword_delete", "pass").await;

    assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
    let c = &doc["candidates"][0];
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
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
