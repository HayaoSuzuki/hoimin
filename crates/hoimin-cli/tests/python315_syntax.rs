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
                "import sys; assert sys.version_info[:2] == (3, 15), sys.version; compile(sys.argv[1], '<neighbor>', 'exec')",
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

fn python315_available() -> bool {
    let output = std::process::Command::new(python()).args(["-c", "import sys; assert sys.version_info[:2] in ((3,14),(3,15)); print(sys.version_info.minor)"]).output().expect("supported CPython required");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim() == "15"
}

#[tokio::test]
async fn python315_comprehension_and_lazy_import_candidates_compile() {
    if !python315_available() {
        eprintln!("new syntax compiler oracle runs in the Python 3.15 lane");
        return;
    }
    for source in [
        "lazy import json\né = True\n",
        "lazy from pathlib import Path\né = True\n",
        "é = [*xs for xs in [[1,2],[3]] if True]\n",
        "é = {*xs for xs in [[1,2],[3]] if True}\n",
        "é = {**d for d in [{'a':1},{'b':2}] if True}\n",
        "é = (*xs for xs in [[1,2],[3]] if True)\n",
        "async def f(rows):\n    return [*xs async for xs in rows if True]\n",
        "async def f(rows):\n    return {*xs async for xs in rows if True}\n",
        "async def f(rows):\n    return {**d async for d in rows if True}\n",
        "async def f(rows):\n    return (*xs async for xs in rows if True)\n",
        "missing = sentinel('MISSING')\nvalue = frozendict(active=True)\n",
    ] {
        compile(source).await;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "boolean_literal", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{source}: {doc}");
        let candidate = &candidates[0];
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..end], "True");
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, candidate["replacement"].as_str().unwrap());
        compile(&mutant).await;
    }
}

#[tokio::test]
async fn python315_unpacking_saved_plan_runs_and_kills_mutant() {
    if !python315_available() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "lazy import json\ndef selected():\n    return [*xs for xs in [[1,2],[3]] if True]\n",
    )
    .unwrap();
    let doc = plan(
        root.path(),
        "boolean_literal",
        "from subject import selected; assert selected() == [1,2,3]",
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
    ])
    .await;
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], 1);
}
