#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;

use collecting_writer::CollectingWriter;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const OPS: [&str; 2] = ["override_method_delete", "super_call_move"];
const SOURCE: &str = "class Base:\n def run(self, emit): emit(1)\nclass Child(Base):\n def run(self, emit):\n  super().run(emit)\n  emit(2)\n";

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

fn candidates(document: &serde_json::Value, operator: &str) -> Vec<serde_json::Value> {
    document["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|candidate| candidate["operator"] == operator)
        .map(|candidate| {
            serde_json::json!({
                "id": candidate["id"], "operator": candidate["operator"],
                "span": candidate["span"], "original": candidate["original"],
                "replacement": candidate["replacement"], "symbol": candidate["symbol"],
            })
        })
        .collect()
}

#[rstest::rstest]
#[case::override_delete("override_method_delete")]
#[case::super_move("super_call_move")]
#[tokio::test]
async fn inheritance_defaults_opt_out_and_selected_ids_are_stable(#[case] operator: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), SOURCE).unwrap();
    let defaults = plan(root.path(), &[], "pass").await;

    let expected = candidates(&defaults, operator);

    assert_eq!(expected.len(), 1, "{operator}");
    assert_eq!(
        candidates(
            &plan(root.path(), &["--operators", operator], "pass").await,
            operator
        ),
        expected
    );
    assert_eq!(
        candidates(
            &plan(
                root.path(),
                &["--operators", operator, "--symbol", "subject:Child.run"],
                "pass"
            )
            .await,
            operator
        ),
        expected
    );
    let excluded = plan(root.path(), &["--exclude-operators", operator], "pass").await;
    assert_eq!(
        candidates(&excluded, operator),
        Vec::<serde_json::Value>::new()
    );
    let other = OPS.into_iter().find(|name| *name != operator).unwrap();
    assert_eq!(candidates(&excluded, other), candidates(&defaults, other));

    assert_eq!(
        std::fs::read(root.path().join("subject.py")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[rstest::rstest]
#[case::delete_weak(
    "override_method_delete",
    "from subject import Child; seen=[]; Child().run(seen.append); assert seen[0]==1",
    0,
    1
)]
#[case::delete_strong(
    "override_method_delete",
    "from subject import Child; seen=[]; Child().run(seen.append); assert seen==[1,2]",
    1,
    0
)]
#[case::move_weak(
    "super_call_move",
    "from subject import Child; seen=[]; Child().run(seen.append); assert sorted(seen)==[1,2]",
    0,
    1
)]
#[case::move_strong(
    "super_call_move",
    "from subject import Child; seen=[]; Child().run(seen.append); assert seen==[1,2]",
    1,
    0
)]
#[tokio::test]
async fn inheritance_saved_plans_expose_missing_override_and_order_assertions(
    #[case] operator: &str,
    #[case] check: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), SOURCE).unwrap();
    let saved = plan(root.path(), &["--operators", operator], check).await;

    assert_eq!(candidates(&saved, operator).len(), 1);
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
    assert_eq!(report["summary"]["counts"]["killed"], killed, "{operator}");
    assert_eq!(
        report["summary"]["counts"]["survived"], survived,
        "{operator}"
    );
    assert_eq!(
        std::fs::read(root.path().join("subject.py")).unwrap(),
        SOURCE.as_bytes()
    );
}

#[rstest::rstest]
#[case::override_delete("override_method_delete")]
#[case::super_move("super_call_move")]
#[tokio::test]
async fn inheritance_planning_does_not_import_target(#[case] operator: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        format!("raise RuntimeError('do not import')\n{SOURCE}"),
    )
    .unwrap();

    assert_eq!(
        candidates(
            &plan(root.path(), &["--operators", operator], "pass").await,
            operator
        )
        .len(),
        1
    );
}

#[rstest::rstest]
#[case::input_1("\n")]
#[case::input_2("\r\n")]
#[case::input_3("\r")]
#[tokio::test]
async fn inheritance_saved_plans_preserve_encoded_sources_and_newlines(
    #[case] newline: &str,
    #[values("override_method_delete", "super_call_move")] operator: &str,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!("# coding: latin-1\n# café\n{SOURCE}")
        .replace(
            "  super().run(emit)",
            "  'method doc'\n  (super().run(\n   emit\n  )) # source slot",
        )
        .replace('\n', newline);
    let bytes: Vec<u8> = source
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap())
        .collect();
    std::fs::write(root.path().join("subject.py"), &bytes).unwrap();

    let saved = plan(
        root.path(),
        &["--operators", operator],
        "from subject import Child; seen=[]; Child().run(seen.append); assert seen==[1,2]",
    )
    .await;

    assert_eq!(candidates(&saved, operator).len(), 1);
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
    assert_eq!(report["summary"]["counts"]["killed"], 1, "{operator}");
    assert_eq!(
        std::fs::read(root.path().join("subject.py")).unwrap(),
        bytes
    );
}

#[rstest::rstest]
#[case::equivalent_override(OPS[0], "class Base:\n def run(self, emit): emit(1)\nclass Child(Base):\n def run(self, emit): emit(1)\n", "from subject import Child; seen=[]; Child().run(seen.append); assert seen==[1]", 0, 1)]
#[case::removed_override_breaks_dependency(OPS[0], "class Base:\n def run(self, emit): emit(self.ready)\nclass Child(Base):\n def prepare(self): self.ready = 1\n def run(self, emit): self.prepare(); super().run(emit)\n", "from subject import Child; Child().run(print)", 1, 0)]
#[case::moved_super_breaks_dependency(OPS[1], "class Base:\n def run(self, emit): emit(self.ready)\nclass Child(Base):\n def prepare(self): self.ready = 1\n def run(self, emit): self.prepare(); super().run(emit)\n", "from subject import Child; Child().run(print)", 1, 0)]
#[tokio::test]
async fn inheritance_equivalence_and_runtime_failure_controls(
    #[case] operator: &str,
    #[case] source: &str,
    #[case] check: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let saved = plan(root.path(), &["--operators", operator], check).await;

    assert_eq!(candidates(&saved, operator).len(), 1);
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
        source.as_bytes()
    );
}
