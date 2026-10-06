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
async fn clauses_preserve_nested_structure_and_compile() {
    for (source, count) in [
        ("if a and b: pass\n", 2),
        ("if a or b or c: pass\n", 3),
        ("if a and (b or c): pass\n", 2),
        ("if a and a: pass\n", 1),
        ("if a: pass\nelif b and c: pass\n", 2),
        ("é = 1\r\nif (a and # comment\r\n b and c): pass\r\n", 3),
        ("if('x')and(lambda: False): pass\n", 2),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "condition_clause_delete", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), count, "{source}");
        for c in candidates {
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            let mut mutant = source.to_owned();
            mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
            compile(&mutant).await;
        }
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "condition_clause_delete", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn clauses_exclude_other_contexts_and_binding_changes() {
    for source in [
        "while a and b: break\nassert a and b\nx = a and b\ny = 1 if a and b else 0\n",
        "if a < b < c: pass\n",
        "if a and (b := f()): pass\n",
        "async def f():\n    if a and await g(): pass\n",
        "def f():\n    if a and (yield 1): pass\n",
        "def f():\n    if a and (yield from g()): pass\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        assert!(
            plan(root.path(), "condition_clause_delete", "pass").await["candidates"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{source}"
        );
    }
}

#[tokio::test]
async fn clauses_saved_plan_checks_both_negative_cases() {
    for (test, killed, survived) in [
        (
            "from subject import allowed; assert not allowed(True, False)",
            1,
            1,
        ),
        (
            "from subject import allowed; assert not allowed(True, False); assert not allowed(False, True)",
            2,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), "def allowed(authenticated, permitted):\n    if authenticated and permitted: return True\n    return False\n").unwrap();
        let doc = plan(root.path(), "condition_clause_delete", test).await;
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
        assert_eq!(report["summary"]["counts"]["survived"], survived);
    }
}

#[tokio::test]
async fn clauses_keep_operator_origin_and_short_circuit() {
    let root = tempfile::tempdir().unwrap();
    let source = "calls=[]\ndef check(n, result):\n    calls.append(n)\n    return result\nif check(1,False) and (check(2,True) or check(3,True)): pass\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(
        root.path(),
        "condition_clause_delete,boolean_and_or",
        "pass",
    )
    .await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 4);
    for c in candidates
        .iter()
        .filter(|c| c["operator"] == "condition_clause_delete")
    {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let replacement = c["replacement"].as_str().unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, replacement);
        mutant.push_str(if replacement.contains("check(1") {
            "assert calls == [1]\n"
        } else {
            "assert calls == [2]\n"
        });
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
