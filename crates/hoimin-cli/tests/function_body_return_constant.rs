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
#[case::bool_1("bool", ["False", "True"])]
#[case::int_2("int", ["0", "1"])]
#[case::str_3("str", ["\"\"", "\"A\""])]
#[tokio::test]
async fn return_constant_emits_complementary_builtin_literals(
    #[case] annotation: &str,
    #[case] values: [&str; 2],
) {
    let source = format!(
        "def compute(value) -> {annotation}:\n    'documentation'\n    result = value\n    return result\n"
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "function_body_return_constant", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();

    assert_eq!(candidates.len(), 2);
    let actual: std::collections::BTreeSet<_> = candidates
        .iter()
        .map(|c| c["replacement"].as_str().unwrap())
        .collect();
    let expected: std::collections::BTreeSet<_> =
        values.iter().map(|v| format!("return {v}")).collect();
    assert_eq!(actual, expected.iter().map(String::as_str).collect());
    for c in candidates {
        assert_eq!(c["original"], "result = value\n    return result");
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
        assert_eq!(&mutant[..start], &source[..start]);
    }
}

fn mutated(source: &str, candidate: &serde_json::Value) -> String {
    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    assert_eq!(&source[start..end], candidate["original"].as_str().unwrap());
    let mut output = source.to_owned();
    output.replace_range(start..end, candidate["replacement"].as_str().unwrap());
    output
}

async fn execute(source: &str) {
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args(["-B", "-c", source])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[rstest::rstest]
#[case::def_f_return_2_1("def f(): return 2\n")]
#[case::def_f_int_return_2_2("def f() -> 'int': return 2\n")]
#[case::import_builtins_def_f_builtins_int_return_2_3(
    "import builtins\ndef f() -> builtins.int: return 2\n"
)]
#[case::alias_int_def_f_alias_return_2_4("Alias = int\ndef f() -> Alias: return 2\n")]
#[case::def_f_int_none_return_2_5("def f() -> int | None: return 2\n")]
#[case::def_f_float_return_2_0_6("def f() -> float: return 2.0\n")]
#[case::async_def_f_int_return_2_7("async def f() -> int: return 2\n")]
#[case::def_special_int_return_2_8("def __special__() -> int: return 2\n")]
#[case::def_f_int_yield_1_return_2_9("def f() -> int:\n    yield 1\n    return 2\n")]
#[case::def_f_int_yield_from_xs_return_2_10("def f() -> int:\n    yield from xs\n    return 2\n")]
#[case::def_f_int_return_11("def f() -> int: return\n")]
#[case::def_f_int_return_none_12("def f() -> int: return None\n")]
#[case::def_f_int_pass_13("def f() -> int: pass\n")]
#[case::def_f_int_14("def f() -> int: ...\n")]
#[case::def_f_int_def_nested_return_2_15("def f() -> int:\n    def nested(): return 2\n")]
#[case::def_f_int_class_nested_def_method_self_return_2_16(
    "def f() -> int:\n    class Nested:\n        def method(self): return 2\n"
)]
#[case::def_f_int_def_nested_x_yield_1_pass_return_2_17(
    "def f() -> int:\n    def nested(x=(yield 1)): pass\n    return 2\n"
)]
#[case::def_f_int_class_nested_yield_1_pass_return_2_18(
    "def f() -> int:\n    class Nested((yield 1)): pass\n    return 2\n"
)]
#[case::int_object_def_f_int_return_2_19("int = object\ndef f() -> int: return 2\n")]
#[case::def_f_int_return_2_int_object_20("def f() -> int: return 2\nint = object\n")]
#[case::def_f_bool_return_flag_if_condition_bool_object_21(
    "def f() -> bool: return flag\nif condition: bool = object\n"
)]
#[case::from_module_import_def_f_str_return_value_22(
    "from module import *\ndef f() -> str: return value\n"
)]
#[case::def_outer_int_def_f_int_return_2_23("def outer(int):\n    def f() -> int: return 2\n")]
#[case::class_c_def_f_self_int_return_2_int_object_24(
    "class C:\n    def f(self) -> int: return 2\n    int = object\n"
)]
#[case::class_c_metaclass_custom_def_f_self_int_return_2_25(
    "class C(metaclass=Custom):\n    def f(self) -> int: return 2\n"
)]
#[case::def_f_int_int_return_2_26("def f[int]() -> int: return 2\n")]
#[case::class_c_int_def_f_self_int_return_2_27("class C[int]:\n    def f(self) -> int: return 2\n")]
#[tokio::test]
async fn return_constant_excludes_unsupported_or_late_shadowed_annotations_and_bodies(
    #[case] source: &str,
) {
    compile(source).await;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "function_body_return_constant", "pass").await;

    assert_eq!(doc["candidates"].as_array().unwrap().len(), 0, "{source}");
}

#[rstest::rstest]
#[case::from_future_import_annotations_def_f_x_int_return_x_1(
    "from __future__ import annotations\ndef f(x) -> int: return x\n"
)]
#[case::def_f_int_int_return_int_2("def f(int) -> int: return int\n")]
#[case::def_f_x_int_def_nested_yield_1_return_x_3(
    "def f(x) -> int:\n    def nested(): yield 1\n    return x\n"
)]
#[case::def_f_x_int_class_nested_def_method_self_yield_1_return_4(
    "def f(x) -> int:\n    class Nested:\n        def method(self): yield 1\n    return x\n"
)]
#[case::def_f_x_int_nested_lambda_yield_1_return_x_5(
    "def f(x) -> int:\n    nested = lambda: (yield 1)\n    return x\n"
)]
#[case::def_f_x_int_try_return_x_finally_cleanup_6(
    "def f(x) -> int:\n    try: return x\n    finally: cleanup()\n"
)]
#[case::class_c_def_f_self_x_int_return_x_7("class C:\n    def f(self, x) -> int: return x\n")]
#[tokio::test]
async fn return_constant_tracks_own_scope_and_exact_literal_noops_1(#[case] source: &str) {
    compile(source).await;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "function_body_return_constant", "pass").await;
    let cs = doc["candidates"].as_array().unwrap();

    assert_eq!(cs.len(), 2, "{source}");
    for c in cs {
        compile(&mutated(source, c)).await;
    }
}

#[rstest::rstest]
#[case::bool_1("bool", "True", vec!["return False"])]
#[case::bool_2("bool", "(False)", vec!["return True"])]
#[case::int_3("int", "(0)", vec!["return 1"])]
#[case::int_4("int", "0x1", vec!["return 0"])]
#[case::str_5("str", "''", vec!["return \"A\""])]
#[case::str_6("str", "'\\x41' ''", vec!["return \"\""])]
#[case::int_7("int", "True", vec!["return 0", "return 1"])]
#[case::bool_8("bool", "1", vec!["return False", "return True"])]
#[case::int_9("int", "0.0", vec!["return 0", "return 1"])]
#[case::str_10("str", "b'A'", vec!["return \"\"", "return \"A\""])]
#[tokio::test]
async fn return_constant_tracks_own_scope_and_exact_literal_noops_2(
    #[case] annotation: &str,
    #[case] literal: &str,
    #[case] remaining: Vec<&str>,
) {
    let source = format!("def f() -> {annotation}:\n    'doc'\n    return {literal}\n");
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "function_body_return_constant", "pass").await;
    let actual: std::collections::BTreeSet<_> = doc["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["replacement"].as_str().unwrap())
        .collect();

    assert_eq!(actual, remaining.into_iter().collect(), "{source}");
}

#[tokio::test]
async fn return_constant_retains_headers_docstrings_and_erases_body_effects() {
    let source = "events=[]\ndef default():\n    events.append('default')\n    return 9\ndef decorate(f):\n    events.append('decorator')\n    return f\n@decorate\ndef f(x=default()) -> int:\n    'documentation'\n    events.append('body')\n    return x + 2\n";
    execute(&format!(
        "{source}\nassert f() == 11\nassert events == ['default','decorator','body']\n"
    ))
    .await;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "function_body_return_constant", "pass").await;
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 2);
    for c in doc["candidates"].as_array().unwrap() {
        let value = c["replacement"]
            .as_str()
            .unwrap()
            .strip_prefix("return ")
            .unwrap();
        execute(&format!("{}\nassert f() == {value}\nassert f.__doc__ == 'documentation'\nassert events == ['default','decorator']\n",mutated(source,c))).await;
    }
}

#[rstest::rstest]
#[case::def_f_x_int_doc_result_x_return_result_tail_1(
    "def f(x) -> int: 'doc'; result=x; return result # tail\n"
)]
#[case::def_f_x_int_t_doc_t_before_treturn_x_after_2(
    "def f(x) -> int:\n\t'doc'\n\t# before\n\treturn x # after\n"
)]
#[tokio::test]
async fn return_constant_retains_inline_and_tabbed_source_layouts(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();

    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "function_body_return_constant", "pass").await;
    for c in doc["candidates"].as_array().unwrap() {
        compile(&mutated(source, c)).await;
    }
}

#[rstest::rstest]
#[case::bool_1_weak("bool", "x > 0", "f(-1); f(1)", 0)]
#[case::bool_1_strong("bool", "x > 0", "assert f(-1) is False; assert f(1) is True", 2)]
#[case::int_2_weak("int", "x + 2", "f(5)", 0)]
#[case::int_2_strong("int", "x + 2", "assert f(5) == 7", 2)]
#[case::str_3_weak("str", "x + '!'", "f('hi')", 0)]
#[case::str_3_strong("str", "x + '!'", "assert f('hi') == 'hi!'", 2)]
#[tokio::test]
async fn return_constant_saved_plan_detects_value_assertion_gaps(
    #[case] annotation: &str,
    #[case] expression: &str,
    #[case] test: &str,
    #[case] killed: usize,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!("def f(x) -> {annotation}:\n    return {expression}\n");
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(
        root.path(),
        "function_body_return_constant",
        &format!("from subject import f; {test}"),
    )
    .await;

    assert_eq!(doc["candidates"].as_array().unwrap().len(), 2);

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
    assert_eq!(report["summary"]["counts"]["survived"], 2 - killed);
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(
        std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
        source
    );
}

#[rstest::rstest]
#[case::bool_1(
    "bool",
    "x > 0",
    "f(-1); f(1)",
    "assert f(-1) is False; assert f(1) is True"
)]
#[case::int_2("int", "x + 2", "f(5)", "assert f(5) == 7")]
#[case::str_3("str", "x + '!'", "f('hi')", "assert f('hi') == 'hi!'")]
#[tokio::test]
async fn return_constant_identity_is_independent_of_assertion_strength(
    #[case] annotation: &str,
    #[case] expression: &str,
    #[case] weak: &str,
    #[case] strong: &str,
) {
    let source = format!("def f(x) -> {annotation}:\n    return {expression}\n");

    let weak_ids = return_constant_ids(&source, weak).await;
    let strong_ids = return_constant_ids(&source, strong).await;

    assert_eq!(weak_ids.len(), 2);
    assert_eq!(weak_ids, strong_ids);
}

async fn return_constant_ids(source: &str, test: &str) -> Vec<serde_json::Value> {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(
        root.path(),
        "function_body_return_constant",
        &format!("from subject import f; {test}"),
    )
    .await;
    doc["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["id"].clone())
        .collect()
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
async fn return_constant_preserves_encoded_source_and_docstrings(
    #[case] newline: &str,
    #[case] encoding: &encoded_source::SourceEncoding,
) {
    let root = tempfile::tempdir().unwrap();
    let encoding_name = encoding.name;
    let text = format!(
        "# coding: {}{newline}def f(x) -> int:{newline}    'é'{newline}    result=x{newline}    return result{newline}",
        encoding.cookie
    );
    let bytes = encoding.source(&text);
    std::fs::write(root.path().join("subject.py"), &bytes).unwrap();

    let doc = plan(root.path(), "function_body_return_constant", "pass").await;

    assert_eq!(
        doc["candidates"].as_array().unwrap().len(),
        2,
        "{encoding_name} {newline:?}"
    );
    for c in doc["candidates"].as_array().unwrap() {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(
            &bytes[start..end],
            format!("result=x{newline}    return result").as_bytes()
        );
        let mut mutant = bytes.clone();
        let replacement = c["replacement"].as_str().unwrap();
        mutant.splice(start..end, replacement.bytes());
        let script = format!(
            "import json; ns={{}}; exec(compile(bytes(json.loads({:?})), '<encoded>', 'exec'),ns); assert ns['f'].__doc__ == 'é'; assert ns['f'](9) == {}",
            serde_json::to_string(&mutant).unwrap(),
            replacement.strip_prefix("return ").unwrap()
        );
        execute(&script).await;
    }
}

#[tokio::test]
async fn return_constant_keeps_the_shared_serialized_size_limit() {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "def f(s) -> int:\n    s.x = 1\n    #{}\n    return 2\n",
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
            "function_body_return_constant".into(),
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u8,
    id: String,
    mode: String,
    source: String,
    expected: Vec<OracleReplacement>,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct OracleReplacement {
    original: String,
    replacement: String,
}

fn oracle_cases(input: &str) -> Result<Vec<OracleCase>, String> {
    let cases = input
        .lines()
        .map(serde_json::from_str::<OracleCase>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut ids = std::collections::BTreeSet::new();
    for case in &cases {
        if case.schema != 1 || case.mode != "strict" || case.id.is_empty() || !ids.insert(&case.id)
        {
            return Err("unsupported schema/mode or duplicate/empty id".into());
        }
    }
    if cases.len() != 32 {
        return Err("incomplete corpus".into());
    }
    Ok(cases)
}

const ORACLE: &str =
    include_str!("../../../formal/HoiminOracle/corpus/function-return-constant.jsonl");

#[test]
fn oracle_accepts_the_complete_owned_case_set() {
    let result = oracle_cases(ORACLE);

    assert!(result.is_ok());
}

#[rstest::rstest]
#[case::missing_case(&ORACLE.lines().skip(1).collect::<Vec<_>>().join("\n"))]
#[case::duplicate_case(&format!("{ORACLE}{}\n",ORACLE.lines().next().unwrap()))]
#[case::unknown_mode(&ORACLE.replacen("strict","relaxed",1))]
#[case::unknown_schema(&ORACLE.replacen("\"schema\":1","\"schema\":2",1))]
#[case::unknown_field(&ORACLE.replacen('{',"{\"extra\":true,",1))]
#[case::malformed_json(&"not json".to_owned())]
fn oracle_rejects_missing_duplicate_unknown_or_malformed_rows(#[case] input: &str) {
    let result = oracle_cases(input);

    assert!(result.is_err());
}

#[tokio::test]
async fn public_candidates_match_every_lean_case_and_compile() {
    for case in oracle_cases(ORACLE).unwrap() {
        compile(&case.source).await;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &case.source).unwrap();
        let doc = plan(root.path(), "function_body_return_constant", "pass").await;
        assert_eq!(doc["truncated"], false, "{}", case.id);
        let mut actual = Vec::new();
        for candidate in doc["candidates"].as_array().unwrap() {
            actual.push(OracleReplacement {
                original: candidate["original"].as_str().unwrap().into(),
                replacement: candidate["replacement"].as_str().unwrap().into(),
            });
            compile(&mutated(&case.source, candidate)).await;
        }
        actual.sort();
        let mut expected = case.expected;
        expected.sort();
        assert_eq!(actual, expected, "{}", case.id);
    }
}
