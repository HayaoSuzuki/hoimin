#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;

use collecting_writer::CollectingWriter;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

const OP: &str = "base_order_shuffle";
const COOPERATIVE: &str = "class Root:\n def run(self, emit): pass\nclass A(Root):\n def run(self, emit):\n  emit('a')\n  super().run(emit)\nclass B(Root):\n def run(self, emit):\n  emit('b')\n  super().run(emit)\nclass Child(A, B): pass\n";

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn cli(args: Vec<OsString>) -> serde_json::Value {
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    jsonl_report::from_slice(&stdout.snapshot()).unwrap()
}

async fn plan(root: &Path, extra: &[&str], check: &str) -> serde_json::Value {
    let mut args: Vec<OsString> = [
        "hoimin",
        "plan",
        "--root",
        root.to_str().unwrap(),
        "--source",
        ".",
        "--jobs",
        "1",
        "--min-free-space",
        "1B",
        "--max-candidates",
        "128",
        "--baseline-timeout",
        "10s",
        "--mutant-timeout",
        "10s",
        "--total-timeout",
        "30s",
        "--allow-best-effort-memory",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.extend(extra.iter().map(OsString::from));
    args.extend([
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        check.into(),
    ]);
    cli(args).await
}

fn candidates(document: &serde_json::Value) -> Vec<serde_json::Value> {
    document["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|candidate| candidate["operator"] == OP)
        .map(|candidate| {
            serde_json::json!({
                "id": candidate["id"], "operator": candidate["operator"],
                "span": candidate["span"], "original": candidate["original"],
                "replacement": candidate["replacement"], "symbol": candidate["symbol"],
            })
        })
        .collect()
}

#[tokio::test]
async fn base_order_shuffle_is_default_opt_out_and_stable_under_selectors() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), COOPERATIVE).unwrap();
    let defaults = plan(root.path(), &[], "pass").await;
    let expected = candidates(&defaults);
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0]["original"], "A, B");
    assert_eq!(expected[0]["replacement"], "B, A");
    assert_eq!(
        candidates(&plan(root.path(), &["--operators", OP], "pass").await),
        expected
    );
    assert_eq!(
        candidates(
            &plan(
                root.path(),
                &["--operators", OP, "--symbol", "subject:Child"],
                "pass"
            )
            .await
        ),
        expected
    );
    assert_eq!(
        candidates(&plan(root.path(), &["--exclude-operators", OP], "pass").await),
        Vec::<serde_json::Value>::new()
    );
    assert_eq!(
        std::fs::read(root.path().join("subject.py")).unwrap(),
        COOPERATIVE.as_bytes()
    );
}

#[tokio::test]
async fn base_order_shuffle_saved_plan_exposes_missing_order_assertions() {
    for (check, killed, survived) in [
        (
            "from subject import Child; seen=[]; Child().run(seen.append); assert sorted(seen)==['a','b']",
            0,
            1,
        ),
        (
            "from subject import Child; seen=[]; Child().run(seen.append); assert seen==['a','b']",
            1,
            0,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), COOPERATIVE).unwrap();
        let saved = plan(root.path(), &["--operators", OP], check).await;
        assert_eq!(candidates(&saved).len(), 1);
        let manifest = root.path().join("plan.json");
        std::fs::write(&manifest, serde_json::to_vec(&saved).unwrap()).unwrap();
        let report = cli(vec![
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
        assert_eq!(
            std::fs::read(root.path().join("subject.py")).unwrap(),
            COOPERATIVE.as_bytes()
        );
    }
}

#[tokio::test]
async fn base_order_shuffle_preserves_encoded_multiline_grouped_source() {
    for newline in ["\n", "\r\n", "\r"] {
        let root = tempfile::tempdir().unwrap();
        let source = "# coding: latin-1\n# café\nclass A: pass\nclass B: pass\nclass Child(\n (A), # first\n (B), # second\n):\n 'doc'; pass\n".replace('\n', newline);
        let bytes: Vec<u8> = source
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap())
            .collect();
        std::fs::write(root.path().join("subject.py"), &bytes).unwrap();
        let saved = plan(root.path(), &["--operators", OP], "pass").await;
        let cs = candidates(&saved);
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = bytes.clone();
        mutant.splice(
            start..start + length,
            c["replacement"].as_str().unwrap().bytes(),
        );
        std::fs::write(root.path().join("mutant.py"), &mutant).unwrap();
        let output = tokio::time::timeout(Duration::from_secs(15), tokio::process::Command::new(python())
            .args(["-B", "-c", "from pathlib import Path; exec(compile(Path('mutant.py').read_bytes(), 'mutant.py', 'exec')); assert Child.__bases__ == (B, A); assert Child.__doc__ == 'doc'"])
            .current_dir(root.path()).kill_on_drop(true).output()).await.unwrap().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(root.path().join("subject.py")).unwrap(),
            bytes
        );
    }
}

#[tokio::test]
async fn base_order_shuffle_planning_does_not_import_target() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "raise RuntimeError('do not import')\nclass C(A,B): pass\n",
    )
    .unwrap();
    assert_eq!(
        candidates(&plan(root.path(), &["--operators", OP], "pass").await).len(),
        1
    );
}

#[tokio::test]
async fn base_order_shuffle_multi_base_mutants_execute_with_every_base_once() {
    for count in 3..=6 {
        let root = tempfile::tempdir().unwrap();
        let names: Vec<_> = (0..count).map(|index| format!("Base{index}")).collect();
        let mut definitions = String::new();
        for name in &names {
            writeln!(definitions, "class {name}: pass").unwrap();
        }
        let source = format!("{definitions}class Child({}): pass\n", names.join(", "));
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let saved = plan(root.path(), &["--operators", OP], "pass").await;
        let cs = candidates(&saved);
        assert_eq!(cs.len(), 1);
        let c = &cs[0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source;
        mutant.replace_range(start..start + length, c["replacement"].as_str().unwrap());
        std::fs::write(root.path().join("mutant.py"), mutant).unwrap();
        let check = format!(
            "from mutant import *; original=({}); assert Child.__bases__ != original; assert len(Child.__bases__)==len(original); assert set(Child.__bases__)==set(original)",
            names.join(", ")
        );
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::process::Command::new(python())
                .args(["-B", "-c", &check])
                .current_dir(root.path())
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
}
