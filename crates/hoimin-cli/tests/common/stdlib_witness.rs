use camino::Utf8Path;
use hoimin_core::MutationProfile;
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
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

pub async fn assert_python(source: &str, assertion: &str) {
    let script = format!("{source}\n{assertion}");
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args(["-B", "-c", &script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython execution deadline")
    .expect("set HOIMIN_OPERATOR_TEST_PYTHON to a CPython executable");
    assert!(
        output.status.success(),
        "{script}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn mutant(source: &str, id: &str, destination: &str) -> String {
    let operators = serde_json::from_value(serde_json::json!([id])).unwrap();
    let request = AnalyzeRequest {
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
        payload: PayloadBudget::default(),
    };
    let output = analyze_source_cancellable(&request, source, || false).unwrap();
    let mut matching = output
        .candidates
        .iter()
        .filter(|c| c.replacement.contains(destination));
    let candidate = matching.next().expect("matching mutation");
    assert!(
        matching.next().is_none(),
        "ambiguous mutation for {id}: {destination}"
    );
    let start = usize::try_from(candidate.span.start).unwrap();
    let length = usize::try_from(candidate.span.length).unwrap();
    let mut script = source.to_owned();
    script.replace_range(start..start + length, &candidate.replacement);

    script
}
