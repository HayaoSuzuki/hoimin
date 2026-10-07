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
async fn method_removal_preserves_receiver_source_and_compiles() {
    let root = tempfile::tempdir().unwrap();
    let source = "def clean(text):\n    return text.strip()\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "method_call_remove", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    let c = &candidates[0];
    assert_eq!(c["original"], "text.strip()");
    assert_eq!(c["replacement"], "(text)");
    assert_eq!(c["line"], 2);
    let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
    let mut mutant = source.to_owned();
    mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
    compile(&mutant).await;
}

fn mutated(source: &str, candidate: &serde_json::Value) -> String {
    let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
    let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
    let mut output = source.to_owned();
    assert_eq!(&source[start..end], candidate["original"].as_str().unwrap());
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
async fn method_removal_preserves_precedence_chains_and_receiver_effects() {
    for (expression, expected) in [
        ("(left + right).strip()", "(left + right)"),
        (
            "(left if condition else right).strip()",
            "(left if condition else right)",
        ),
        ("factory().strip()", "(factory())"),
        ("'a'.strip()", "('a')"),
        ("(\n    text # comment\n).strip(\n)", "(text)"),
    ] {
        let source = format!("é = 1\r\nvalue = 2 * {expression}\r\n");
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "method_call_remove", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{expression}");
        assert_eq!(candidates[0]["replacement"], expected);
        compile(&mutated(&source, &candidates[0])).await;
    }
    let source = "events=[]\nclass Box:\n    def __getattribute__(self, name):\n        if name == 'clean': events.append('lookup')\n        return object.__getattribute__(self, name)\n    def clean(self):\n        events.append('call')\n        return self\ndef make():\n    events.append('receiver')\n    return Box()\nvalue=make().clean()\n";
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "method_call_remove", "pass").await;
    assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
    execute(&format!(
        "{source}\nassert events == ['receiver', 'lookup', 'call']\n"
    ))
    .await;
    execute(&format!(
        "{}\nassert events == ['receiver'] and type(value) is Box\n",
        mutated(source, &doc["candidates"][0])
    ))
    .await;
    let source = "value = text.strip().lower()\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "method_call_remove", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    let pairs = candidates
        .iter()
        .map(|c| {
            (
                c["original"].as_str().unwrap(),
                c["replacement"].as_str().unwrap(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        pairs,
        std::collections::BTreeSet::from([
            ("text.strip()", "(text)"),
            ("text.strip().lower()", "(text.strip())")
        ])
    );
    for candidate in candidates {
        compile(&mutated(source, candidate)).await;
    }
}

#[tokio::test]
async fn method_removal_excludes_arguments_bindings_suspension_and_type_positions() {
    for source in [
        "value = text.strip('x')\nvalue = text.clean(flag=True)\nvalue = text.clean(*args)\nvalue = text.clean(**kw)\n",
        "value = (text := factory()).clean()\nvalue = (x for x in values).clean()\n",
        "async def f(): return (await factory()).clean()\n",
        "def f(): return (yield factory()).clean()\n",
        "def f(): return (yield from factory()).clean()\n",
        "from typing import TypeAlias\nA: TypeAlias = text.clean()\nx: text.clean()\ntype B = text.clean()\nitems[text.clean()] = 1\ntext.clean().attr = 1\n",
    ] {
        compile(source).await;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        assert!(
            plan(root.path(), "method_call_remove", "pass").await["candidates"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{source}"
        );
    }
}

#[tokio::test]
async fn method_removal_saved_plan_distinguishes_missing_result_assertions() {
    let source = "def clean(text): return text.strip()\n";
    let mut ids = Vec::new();
    for (test, killed) in [
        (
            "from subject import clean; assert isinstance(clean(' x '), str)",
            0,
        ),
        ("from subject import clean; assert clean(' x ') == 'x'", 1),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "method_call_remove", test).await;
        ids.push(doc["candidates"][0]["id"].clone());
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
        let mut args = vec![
            "hoimin".into(),
            "verify".into(),
            manifest.into_os_string(),
            "--top".into(),
            "1".into(),
            "--format".into(),
            "json".into(),
        ];
        args.push("--dry-run".into());
        let (_, preview) = cli(args.clone()).await;
        assert_eq!(preview["candidates"][0]["id"], doc["candidates"][0]["id"]);
        args.pop();
        let (_, report) = cli(args).await;
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], 1 - killed);
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(
            std::fs::read_to_string(root.path().join("subject.py")).unwrap(),
            source
        );
    }
    assert_eq!(ids[0], ids[1]);
}

#[tokio::test]
async fn method_removal_preserves_encoded_source_and_outer_bytes() {
    for source in [
        b"\xef\xbb\xbf# coding: utf-8\r\ntext = ' x '\r\nvalue = text.strip()\r\n".as_slice(),
        b"# coding: latin-1\r\nname = '\xe9'\r\ntext = ' x '\r\nvalue = text.strip()\r\n"
            .as_slice(),
        b"text = ' x '\rvalue = text.strip()\r".as_slice(),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "method_call_remove", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..end], b"text.strip()");
        let mut output = source.to_vec();
        output.splice(start..end, c["replacement"].as_str().unwrap().bytes());
        let path = root.path().join("mutant.py");
        std::fs::write(&path, output).unwrap();
        let status = tokio::process::Command::new(python()).args(["-B", "-c", "import pathlib,sys; exec(compile(pathlib.Path(sys.argv[1]).read_bytes(), '<mutant>', 'exec')); assert value == ' x '"])
            .arg(path).status().await.unwrap();
        assert!(status.success());
    }
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
    if cases.len() != 23 {
        return Err("incomplete corpus".into());
    }
    Ok(cases)
}

const ORACLE: &str = include_str!("../../../formal/HoiminOracle/corpus/method-call-remove.jsonl");

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
        let doc = plan(root.path(), "method_call_remove", "pass").await;
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
