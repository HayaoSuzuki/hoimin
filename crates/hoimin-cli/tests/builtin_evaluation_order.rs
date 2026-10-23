#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use hoimin_cli::plan::PlanManifest;

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

fn command(root: &Path, mode: &str, check: &str) -> Vec<OsString> {
    let mut args = vec![
        "hoimin".into(),
        mode.into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        "collection_any_all".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
    ];
    args.extend([
        "--".into(),
        python().into_os_string(),
        "-c".into(),
        check.into(),
    ]);
    args
}

#[tokio::test]
async fn public_plan_observes_evaluation_order_and_preserves_source_spans() {
    let cases = [
        (
            "any, slots[any([])] = custom, 7",
            "assert slots == {'custom': 7}",
            0,
        ),
        (
            "any = slots[any([])] = custom",
            "assert slots == {'custom': custom}",
            0,
        ),
        (
            "slots[any([])] = (any := custom)",
            "assert slots == {'custom': custom}",
            0,
        ),
        (
            "all, slots[any([])] = custom, 7",
            "assert slots == {False: 7}",
            0,
        ),
        (
            "observed = sink(flag=any([]), *[(any := custom)])",
            "assert observed == 'custom'",
            0,
        ),
        (
            "observed = sink(flag=any([]), *[(all := custom)])",
            "assert observed is False",
            0,
        ),
        ("any, slots[0] = any([]), 7", "assert any is False", 1),
        ("any = slots[0] = any([])", "assert any is False", 1),
        (
            "(any, (slots[any([])], other)) = custom, (7, 8)",
            "assert slots == {'custom': 7}",
            0,
        ),
        (
            "try:\n    any, (other, slots[any([])]) = custom, ()\nexcept ValueError:\n    observed = any([])",
            "assert observed == 'custom'",
            0,
        ),
        (
            "try:\n    any, other = ()\nexcept ValueError:\n    observed = any([])",
            "assert observed is False",
            0,
        ),
        (
            "slots[False] = 1\nslots[any([])] += (all := 2)",
            "assert slots == {False: 3}",
            1,
        ),
    ];
    for (body, check, expected) in cases {
        let source = format!(
            "def custom(values): return 'custom'\ndef sink(*args, **kwargs): return kwargs['flag']\nslots = {{}}\n{body}\n"
        );
        let execution = Command::new(python())
            .args(["-c", &format!("{source}\n{check}\n")])
            .output()
            .expect("CPython must be available in .venv");
        assert!(
            execution.status.success(),
            "{body}: {}",
            String::from_utf8_lossy(&execution.stderr)
        );
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("subject.py"), &source).unwrap();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let code = hoimin_cli::run_with_writers(
            command(directory.path(), "plan", "pass"),
            stdout.clone(),
            stderr.clone(),
        )
        .await;
        assert_eq!(
            code,
            0,
            "{body}: {}",
            String::from_utf8_lossy(&stderr.snapshot())
        );
        let manifest: PlanManifest = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
        assert_eq!(manifest.candidates.len(), expected, "{body}");
        for candidate in &manifest.candidates {
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            assert_eq!(&source[start..end], "any");
            assert_eq!(candidate.original, "any");
            assert_eq!(candidate.replacement, "all");
        }
    }
}

#[tokio::test]
async fn public_run_does_not_count_a_custom_callable_as_a_killed_mutant() {
    let source = "def custom(values): return 'custom'\nslots = {}\nany, slots[any([])] = custom, 7\nobserved = list(slots)\n";
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("subject.py"), source).unwrap();
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(
        command(
            directory.path(),
            "run",
            "import subject; assert subject.observed == ['custom']",
        ),
        stdout.clone(),
        stderr.clone(),
    )
    .await;
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let report: serde_json::Value = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["summary"]["counts"]["killed"], 0);
    assert!(
        report["mutants"].as_array().unwrap().is_empty(),
        "unexpected mutants: {}",
        report["mutants"]
    );
}
