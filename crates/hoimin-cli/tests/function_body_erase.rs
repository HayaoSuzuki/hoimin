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
async fn body_erasure_preserves_headers_docstrings_and_valid_suites() {
    for source in [
        "@decorate\ndef initialize(state, value=3):\n    'documentation'\n    state.ready = True\n    state.count = value\n",
        "def initialize(state): 'doc'; state.ready = True; state.count = 0 # tail\n",
        "é = '前'\r\ndef initialize(state):\r\n    # before\r\n    state.ready = True\r\n    state.count = 0 # tail\r\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "function_body_erase", "pass").await;
        let cs = doc["candidates"].as_array().unwrap();
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        assert_eq!(c["replacement"], "pass");
        assert!(
            c["original"]
                .as_str()
                .unwrap()
                .starts_with("state.ready = True")
        );
        assert!(c["original"].as_str().unwrap().contains("state.count"));
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, "pass");
        compile(&mutant).await;
        assert_eq!(&mutant[..start], &source[..start]);
    }
}

#[tokio::test]
async fn body_erasure_checks_function_scope_and_excludes_noops() {
    for (source, count) in [
        ("def f(s):\n    s.x=1\n    return\n", 1),
        ("def f(s):\n    s.x=1\n    return None\n", 1),
        ("def f(s):\n    s.x=1\n    return 1\n", 0),
        ("def f():\n    yield 1\n", 0),
        ("def f():\n    yield from g()\n", 0),
        ("async def f(s):\n    s.x=1\n", 0),
        ("class C:\n    def __init__(self):\n        self.x=1\n", 0),
        ("def f():\n    'doc'\n    pass\n    return None\n", 0),
        ("def f(): ...\n", 0),
        (
            "def f():\n    def inner():\n        return 1\n    call()\n",
            1,
        ),
        (
            "def f():\n    def inner():\n        yield 1\n    call()\n",
            1,
        ),
        (
            "def f():\n    def inner(x=(yield 1)):\n        return 1\n    call()\n",
            0,
        ),
        ("def f():\n    value = lambda: (yield 1)\n    call()\n", 1),
        (
            "def f():\n    value = lambda x=(yield 1): x\n    call()\n",
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "function_body_erase", "pass").await;
        assert_eq!(
            doc["candidates"].as_array().unwrap().len(),
            count,
            "{source}"
        );
        if count == 1 {
            assert_eq!(doc["candidates"][0]["symbol"], "f");
        }
    }
}

#[tokio::test]
async fn body_erasure_saved_plan_requires_state_observation() {
    for (test, killed, survived) in [
        ("from subject import initialize; s={}; initialize(s)", 0, 1),
        (
            "from subject import initialize; s={}; initialize(s); assert s == {'ready': True, 'count': 3}",
            1,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def initialize(s):\n    s['ready'] = True\n    s['count'] = 3\n",
        )
        .unwrap();
        let doc = plan(root.path(), "function_body_erase", test).await;
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
async fn body_erasure_keeps_the_shared_serialized_size_limit() {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "def f(s):\n    s.x = 1\n    #{}\n    s.y = 2\n",
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
            "function_body_erase".into(),
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

#[tokio::test]
async fn body_erasure_preserves_encoded_docstrings_and_newlines() {
    for newline in ["\n", "\r\n", "\r"] {
        for encoding in ["utf8", "bom", "latin1"] {
            let root = tempfile::tempdir().unwrap();
            let text = format!(
                "# coding: {}{newline}def f(s):{newline}    'é'{newline}    s.x=1{newline}    s.y=2{newline}",
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
            let document = plan(root.path(), "function_body_erase", "pass").await;
            let cs = document["candidates"].as_array().unwrap();
            assert_eq!(cs.len(), 1);
            let c = &cs[0];
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            assert_eq!(
                &bytes[start..end],
                format!("s.x=1{newline}    s.y=2").as_bytes()
            );
            bytes.splice(start..end, b"pass".iter().copied());
            let result = tokio::process::Command::new(python()).args(["-c",
                "import json,sys; assert sys.version_info[:2] == (3,14); ns={}; exec(compile(bytes(json.loads(sys.argv[1])), '<encoded>', 'exec'),ns); assert ns['f'].__doc__ == 'é'",
                &serde_json::to_string(&bytes).unwrap()]).output().await.unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[tokio::test]
async fn body_erasure_does_not_replace_finer_mutations() {
    for (operator, killed, survived) in
        [("function_body_erase", 0, 1), ("collection_min_max", 1, 0)]
    {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def cap(s):\n    s['x'] = min(s['x'], 3)\n",
        )
        .unwrap();
        let doc = plan(
            root.path(),
            operator,
            "from subject import cap; s={'x': 2}; cap(s); assert s['x'] == 2",
        )
        .await;
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
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
