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
async fn while_false_replaces_each_loop_test_and_compiles() {
    for source in [
        "while[]: pass\n",
        "while x < 2: x += 1\n",
        "while a and b: break\n",
        "while ready():\n    while nested(): break\n    break\nelse: done()\n",
        "é = '前'\r\nwhile (\r\n    a and # comment\r\n    b\r\n): break\r\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "while_condition_false", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(
            candidates.len(),
            if source.contains("nested") { 2 } else { 1 }
        );
        for c in candidates {
            assert!(matches!(c["replacement"].as_str().unwrap(), "(False)"));
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            let mut mutant = source.to_owned();
            mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
            compile(&mutant).await;
        }
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "while_condition_false", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn while_false_exclude_other_predicates_and_binding_changes() {
    for source in [
        "while (True): break\nwhile False: pass\n",
        "for x in xs: pass\nif x: pass\nz = [a for a in xs if a]\n",
        "while f(x := 1): break\n",
        "while (lambda: (x := 1))(): break\n",
        "async def f():\n    while g(await h()): break\n",
        "def f():\n    while g((yield 1)): break\n",
        "def f():\n    while g((yield from h())): break\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "while_condition_false", "pass").await;
        assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
    }
}

#[tokio::test]
async fn while_false_saved_queue_plan() {
    for (test, killed, survived) in [
        ("from subject import drain; drain([1,2])", 0, 1),
        (
            "from subject import drain; assert drain([1,2]) == [2,4]",
            1,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            "def drain(queue):\n    out = []\n    while queue:\n        out.append(queue.pop(0)*2)\n    return out\n",
        )
        .unwrap();
        let doc = plan(root.path(), "while_condition_false", test).await;
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
async fn while_false_skips_effects_and_runs_else() {
    let source = "calls = []\ndef predicate():\n    calls.append('condition')\n    return True\nwhile predicate():\n    outcome = 'yes'; break\nelse:\n    outcome = 'no'\n";
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "while_condition_false", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        let forced = c["replacement"].as_str().unwrap();
        mutant.replace_range(start..end, forced);
        mutant.push_str(if forced == "(True)" {
            "assert calls == [] and outcome == 'yes'\n"
        } else {
            "assert calls == [] and outcome == 'no'\n"
        });
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
}

#[tokio::test]
async fn while_false_has_no_boolean_literal_duplicate() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "while True: break\nwhile False: pass\nwhile enabled: break\n",
    )
    .unwrap();
    let doc = plan(root.path(), "while_condition_false,boolean_literal", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(
        candidates
            .iter()
            .filter(|c| c["operator"] == "while_condition_false")
            .count(),
        1
    );
}
