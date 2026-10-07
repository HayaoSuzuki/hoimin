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
                "import sys; assert sys.version_info[:2] == (3, 14), sys.version; compile(sys.argv[1], '<neighbor>', 'exec')",
                source,
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython compiler deadline")
    .expect("CPython 3.14; set HOIMIN_OPERATOR_TEST_PYTHON to its executable");
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert!(
        code <= 1,
        "exit={code}, stderr={}",
        String::from_utf8_lossy(&stderr)
    );
    (
        code,
        serde_json::from_slice(&stdout).expect("one JSON document"),
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

#[tokio::test]
async fn return_constant_emits_complementary_builtin_literals() {
    for (annotation, values) in [
        ("bool", ["False", "True"]),
        ("int", ["0", "1"]),
        ("str", ["\"\"", "\"A\""]),
    ] {
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

#[tokio::test]
async fn return_constant_excludes_unsupported_or_late_shadowed_annotations_and_bodies() {
    for source in [
        "def f(): return 2\n",
        "def f() -> 'int': return 2\n",
        "import builtins\ndef f() -> builtins.int: return 2\n",
        "Alias = int\ndef f() -> Alias: return 2\n",
        "def f() -> int | None: return 2\n",
        "def f() -> float: return 2.0\n",
        "async def f() -> int: return 2\n",
        "def __special__() -> int: return 2\n",
        "def f() -> int:\n    yield 1\n    return 2\n",
        "def f() -> int:\n    yield from xs\n    return 2\n",
        "def f() -> int: return\n",
        "def f() -> int: return None\n",
        "def f() -> int: pass\n",
        "def f() -> int: ...\n",
        "def f() -> int:\n    def nested(): return 2\n",
        "def f() -> int:\n    class Nested:\n        def method(self): return 2\n",
        "def f() -> int:\n    def nested(x=(yield 1)): pass\n    return 2\n",
        "def f() -> int:\n    class Nested((yield 1)): pass\n    return 2\n",
        "int = object\ndef f() -> int: return 2\n",
        "def f() -> int: return 2\nint = object\n",
        "def f() -> bool: return flag\nif condition: bool = object\n",
        "from module import *\ndef f() -> str: return value\n",
        "def outer(int):\n    def f() -> int: return 2\n",
        "class C:\n    def f(self) -> int: return 2\n    int = object\n",
        "class C(metaclass=Custom):\n    def f(self) -> int: return 2\n",
        "def f[int]() -> int: return 2\n",
        "class C[int]:\n    def f(self) -> int: return 2\n",
    ] {
        compile(source).await;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "function_body_return_constant", "pass").await;
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 0, "{source}");
    }
}

#[tokio::test]
async fn return_constant_tracks_own_scope_and_exact_literal_noops() {
    for source in [
        "from __future__ import annotations\ndef f(x) -> int: return x\n",
        "def f(int) -> int: return int\n",
        "def f(x) -> int:\n    def nested(): yield 1\n    return x\n",
        "def f(x) -> int:\n    class Nested:\n        def method(self): yield 1\n    return x\n",
        "def f(x) -> int:\n    nested = lambda: (yield 1)\n    return x\n",
        "def f(x) -> int:\n    try: return x\n    finally: cleanup()\n",
        "class C:\n    def f(self, x) -> int: return x\n",
    ] {
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
    for (annotation, literal, remaining) in [
        ("bool", "True", vec!["return False"]),
        ("bool", "(False)", vec!["return True"]),
        ("int", "(0)", vec!["return 1"]),
        ("int", "0x1", vec!["return 0"]),
        ("str", "''", vec!["return \"A\""]),
        ("str", "'\\x41' ''", vec!["return \"\""]),
        ("int", "True", vec!["return 0", "return 1"]),
        ("bool", "1", vec!["return False", "return True"]),
        ("int", "0.0", vec!["return 0", "return 1"]),
        ("str", "b'A'", vec!["return \"\"", "return \"A\""]),
    ] {
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
    for source in [
        "def f(x) -> int: 'doc'; result=x; return result # tail\n",
        "def f(x) -> int:\n\t'doc'\n\t# before\n\treturn x # after\n",
    ] {
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "function_body_return_constant", "pass").await;
        for c in doc["candidates"].as_array().unwrap() {
            compile(&mutated(source, c)).await;
        }
    }
}

#[tokio::test]
async fn return_constant_saved_plan_detects_value_assertion_gaps() {
    for (annotation, expression, weak, strong) in [
        (
            "bool",
            "x > 0",
            "f(-1); f(1)",
            "assert f(-1) is False; assert f(1) is True",
        ),
        ("int", "x + 2", "f(5)", "assert f(5) == 7"),
        ("str", "x + '!'", "f('hi')", "assert f('hi') == 'hi!'"),
    ] {
        let mut ids = Vec::new();
        for (test, killed) in [(weak, 0), (strong, 2)] {
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
            ids.push(
                doc["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c["id"].clone())
                    .collect::<Vec<_>>(),
            );
            let manifest = root.path().join("plan.json");
            std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
            let (_, report) = cli(vec![
                "hoimin".into(),
                "verify".into(),
                manifest.into_os_string(),
                "--top".into(),
                "2".into(),
                "--format".into(),
                "json".into(),
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
        assert_eq!(ids[0], ids[1]);
    }
}

#[tokio::test]
async fn return_constant_preserves_encoded_source_and_docstrings() {
    for newline in ["\n", "\r\n", "\r"] {
        for encoding in ["utf8", "bom", "latin1"] {
            let root = tempfile::tempdir().unwrap();
            let text = format!(
                "# coding: {}{newline}def f(x) -> int:{newline}    'é'{newline}    result=x{newline}    return result{newline}",
                if encoding == "latin1" {
                    "latin-1"
                } else {
                    "utf-8"
                }
            );
            let mut bytes = if encoding == "latin1" {
                text.chars()
                    .map(|c| u8::try_from(u32::from(c)).unwrap())
                    .collect::<Vec<_>>()
            } else {
                text.as_bytes().to_vec()
            };
            if encoding == "bom" {
                bytes.splice(0..0, [0xef, 0xbb, 0xbf]);
            }
            std::fs::write(root.path().join("subject.py"), &bytes).unwrap();
            let doc = plan(root.path(), "function_body_return_constant", "pass").await;
            assert_eq!(doc["candidates"].as_array().unwrap().len(), 2);
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
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(
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
        &mut stdout,
        &mut stderr,
    )
    .await;
    assert_eq!(code, 2);
    assert!(String::from_utf8_lossy(&stderr).contains("candidate spool record exceeds"));
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
fn oracle_rejects_missing_duplicate_unknown_or_malformed_rows() {
    assert!(oracle_cases(ORACLE).is_ok());
    assert!(oracle_cases(&ORACLE.lines().skip(1).collect::<Vec<_>>().join("\n")).is_err());
    assert!(oracle_cases(&format!("{ORACLE}{}\n", ORACLE.lines().next().unwrap())).is_err());
    assert!(oracle_cases(&ORACLE.replacen("strict", "relaxed", 1)).is_err());
    assert!(oracle_cases(&ORACLE.replacen("\"schema\":1", "\"schema\":2", 1)).is_err());
    assert!(oracle_cases(&ORACLE.replacen('{', "{\"extra\":true,", 1)).is_err());
    assert!(oracle_cases("not json").is_err());
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
