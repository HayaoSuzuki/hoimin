use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;
use std::ffi::OsString;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    id: String,
    source: String,
    candidate_count: usize,
}

#[tokio::test(flavor = "current_thread")]
async fn implicit_finally_matches_lean_public_plan() {
    let corpus = include_str!("../../../formal/HoiminOracle/corpus/implicit-finally.jsonl");
    let mut observed = 0;
    for row in corpus.lines() {
        let case: Case = serde_json::from_str(row).expect("valid Lean fixture");
        assert_eq!(case.schema, 1);
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("subject.py"), &case.source).unwrap();
        let args: Vec<OsString> = vec![
            "hoimin".into(),
            "plan".into(),
            "--root".into(),
            directory.path().into(),
            "--file".into(),
            "subject.py".into(),
            "--operators".into(),
            "type_list_sequence".into(),
            "--allow-best-effort-memory".into(),
            "--".into(),
            "true".into(),
        ];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = tokio::time::timeout(
            Duration::from_secs(10),
            hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
        )
        .await
        .expect("plan deadline");
        assert_eq!(exit, 0, "infrastructure error: {}: {:?}", case.id, stderr);
        let manifest: PlanManifest = serde_json::from_slice(&stdout).expect("plan JSON");
        assert_eq!(
            manifest.candidates.len(),
            case.candidate_count,
            "semantic mismatch: {}\n{}",
            case.id,
            case.source
        );
        for candidate in manifest.candidates {
            assert_eq!(candidate.original, "Sequence[int]");
            assert_eq!(candidate.replacement, "list[int]");
            let start = candidate.span.start as usize;
            let end = start + candidate.span.length as usize;
            assert_eq!(&case.source[start..end], candidate.original);
        }
        observed += 1;
    }
    assert_eq!(observed, 6, "all Lean cases must execute");
}
