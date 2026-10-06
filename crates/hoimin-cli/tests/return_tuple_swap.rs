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
async fn return_tuple_swap_preserves_tuple_source_structure() {
    for (body, expected) in [
        ("return lower, upper", "upper, lower"),
        ("return (lower, upper,)", "(upper, lower,)"),
        (
            "return (lower, # first\n            upper,)",
            "(upper, # first\n            lower,)",
        ),
        ("return ('a,b', 3)", "(3, 'a,b')"),
        ("return'x', upper", "(upper, 'x')"),
        ("return-1, upper", "(upper, -1)"),
        ("return (None, True)", "(True, None)"),
    ] {
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
}

#[tokio::test]
async fn return_tuple_swap_checks_atomic_elements_and_function_scope() {
    for (source, count) in [
        ("def f(): return 1, 2\n", 1),
        ("async def f(): return 1, 2\n", 0),
        ("def f():\n    yield 0\n    return 1, 2\n", 0),
        ("def f():\n    yield from []\n    return 1, 2\n", 0),
        ("def f():\n    def g(): yield 0\n    return 1, 2\n", 1),
        (
            "def f():\n    def g(x=(yield 0)): return 3, 4\n    return 1, 2\n",
            1,
        ),
        (
            "async def f():\n    def g(): return 3, 4\n    return 1, 2\n",
            1,
        ),
        ("def f(): return x, x\n", 0),
        ("def f(\u{212a}): return \u{212a}, K\n", 0),
        ("def f(): return [x for x in xs], y\n", 0),
        ("def f(): return 'x', 'x'\n", 0),
        ("def f(): return (),\n", 0),
        ("def f(): return 1, 2, 3\n", 0),
        ("def f(): return pair\n", 0),
        ("def f(): return f(), x\n", 0),
        ("def f(): return obj.x, x\n", 0),
        ("def f(): return obj[0], x\n", 0),
        ("def f(): return (x:=1), x\n", 0),
        ("def f(): return *xs, x\n", 0),
        ("def f(): return (1,2), x\n", 0),
        ("def f(): return 'a' 'b', x\n", 0),
        ("def f(): return f'x', x\n", 0),
        ("def f(): return b'x', x\n", 0),
        ("def f(): pair=1,2\n", 0),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "return_tuple_swap", "pass").await;
        assert_eq!(
            doc["candidates"].as_array().unwrap().len(),
            count,
            "{source} {doc}"
        );
    }
}

#[tokio::test]
async fn return_tuple_swap_requires_positional_assertions() {
    for (test, killed, survived) in [
        ("from subject import bounds; assert len(bounds())==2", 0, 1),
        ("from subject import bounds; assert bounds()==(3,9)", 1, 0),
    ] {
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
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], survived);
    }
}
