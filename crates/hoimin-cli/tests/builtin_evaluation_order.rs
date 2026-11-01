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

#[rstest::rstest]
#[case::any_slots_any_custom_7_1(0)]
#[case::any_slots_any_custom_2(1)]
#[case::slots_any_any_custom_3(2)]
#[case::all_slots_any_custom_7_4(3)]
#[case::observed_sink_flag_any_any_custom_5(4)]
#[case::observed_sink_flag_any_all_custom_6(5)]
#[case::any_slots_0_any_7_7(6)]
#[case::any_slots_0_any_8(7)]
#[case::any_slots_any_other_custom_7_8_9(8)]
#[case::try_any_other_slots_any_custom_except_valueerror_observed_any_10(9)]
#[case::try_any_other_except_valueerror_observed_any_11(10)]
#[case::slots_false_1_slots_any_all_2_12(11)]
#[tokio::test]
async fn public_plan_observes_evaluation_order_and_preserves_source_spans(
    #[case] scenario_index: usize,
) {
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

    let (body, check, expected) = cases.into_iter().nth(scenario_index).unwrap();

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
