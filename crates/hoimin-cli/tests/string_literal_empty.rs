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
async fn string_empty_changes_complete_nonempty_literal_tokens() {
    for literal in [
        "'ready'",
        "\"ready\"",
        "r'\\n'",
        "u'é前'",
        "'''line1\nline2'''",
        "'\\u2603'",
        "'\\ud800'",
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = format!("é='outside'\ndef f():\n    return {literal}\n");
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "string_literal_empty", "pass").await;
        let cs = doc["candidates"].as_array().unwrap();
        assert_eq!(cs.len(), 2, "{doc}");
        let c = cs.iter().find(|c| c["symbol"] == "f").unwrap();
        assert_eq!(c["original"], literal);
        assert_eq!(c["replacement"], "\"\"");
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..end, "\"\"");
        compile(&mutant).await;
        let output = tokio::process::Command::new(python())
            .args([
                "-c",
                "import sys;ns={};exec(sys.argv[1],ns);assert ns['f']()==''",
                &mutant,
            ])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "string_literal_empty", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn string_empty_distinguishes_runtime_roles() {
    let source = concat!(
        "'module doc'\nfrom typing import Literal, TypeAlias as TA\nimport typing as ty\n",
        "type Modern = Literal['modern']\nOld: TA = Literal['old']\nOther: ty.TypeAlias = 'Forward'\nQuoted: 'TA' = Literal['quoted alias']\nParen: '(TA)' = 'str'\nQualified: '(ty . TypeAlias)' = 'str'\n",
        "x: Literal['annotation'] = 'value'\n",
        "'ordinary module expression'\n",
        "class C:\n    'class doc'\n    'ordinary class expression'\n",
        "    def f(self, x: 'Forward' = 'default') -> Literal['return annotation']:\n",
        "        'function doc'\n        'ordinary function expression'\n",
        "        if x:\n            'not a docstring'\n",
        "        print('message', end='end')\n        return 'ready'\n",
        "match x:\n    case 'pattern' if x == 'guard': pass\n",
        "empty = ''; raw_empty = r''; triple_empty = ''''''\n",
        "binary=b'bytes'; formatted=f\"{'field'}\"; template=t\"{'field'}\"\n",
        "joined='a' 'b'\n"
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "string_literal_empty", "pass").await;
    let mut actual: Vec<_> = doc["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["original"].as_str().unwrap())
        .collect();
    actual.sort_unstable();
    let mut expected = vec![
        "'value'",
        "'guard'",
        "'ordinary module expression'",
        "'ordinary class expression'",
        "'default'",
        "'ordinary function expression'",
        "'not a docstring'",
        "'message'",
        "'end'",
        "'ready'",
    ];
    expected.sort_unstable();
    assert_eq!(actual, expected, "{doc}");
    for c in doc["candidates"].as_array().unwrap() {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, "\"\"");
        compile(&mutant).await;
    }
}

#[tokio::test]
async fn string_empty_saved_plan_requires_content_observation() {
    for (test, killed, survived) in [
        (
            "from subject import state;assert isinstance(state(),str)",
            0,
            1,
        ),
        ("from subject import state;assert state()=='ready'", 1, 0),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def state():\n    return 'ready'\n",
        )
        .unwrap();
        let doc = plan(root.path(), "string_literal_empty", test).await;
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
        let (_, report) = cli(vec![
            "hoimin".into(),
            "verify".into(),
            manifest.into_os_string(),
            "--top".into(),
            "1".into(),
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], survived);
    }
}

#[tokio::test]
async fn string_empty_uses_decoded_value_and_exact_encoded_occurrence() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "x='\\\n'\n").unwrap();
    assert!(
        plan(root.path(), "string_literal_empty", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for newline in ["\n", "\r\n", "\r"] {
        for encoding in ["utf8", "bom", "latin1"] {
            let root = tempfile::tempdir().unwrap();
            let text = format!(
                "# coding: {}{newline}# é{newline}first='same'; second='same'{newline}",
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
            let doc = plan(root.path(), "string_literal_empty", "pass").await;
            let cs = doc["candidates"].as_array().unwrap();
            assert_eq!(cs.len(), 2);
            assert_ne!(cs[0]["span"]["start"], cs[1]["span"]["start"]);
            for c in cs {
                let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
                let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
                assert_eq!(&bytes[start..end], b"'same'");
                let mut mutant = bytes.clone();
                mutant.splice(start..end, b"\"\"".iter().copied());
                let output=tokio::process::Command::new(python()).args(["-c","import json,sys;ns={};exec(compile(bytes(json.loads(sys.argv[1])),'<encoded>','exec'),ns);assert sorted([ns['first'],ns['second']])==['','same']",&serde_json::to_string(&mutant).unwrap()]).output().await.unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
