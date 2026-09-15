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
    #[serde(default)]
    runtime_typing: Vec<bool>,
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
            let start = usize::try_from(candidate.span.start).expect("span start fits usize");
            let length = usize::try_from(candidate.span.length).expect("span length fits usize");
            let end = start.checked_add(length).expect("span end fits usize");
            assert_eq!(&case.source[start..end], candidate.original);
        }
        observed += 1;
    }
    assert_eq!(observed, 9, "all Lean cases must execute");
}

fn cases() -> Vec<Case> {
    include_str!("../../../formal/HoiminOracle/corpus/implicit-finally.jsonl")
        .lines()
        .map(|row| serde_json::from_str(row).expect("valid Lean case"))
        .collect()
}

fn python() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

#[tokio::test(flavor = "current_thread")]
async fn implicit_finally_runtime_observations_match_lean() {
    let mut observations = 0;
    for case in cases()
        .into_iter()
        .filter(|case| !case.runtime_typing.is_empty())
    {
        assert_eq!(case.runtime_typing.len(), 2, "{}", case.id);
        for (raises, expected) in case.runtime_typing.iter().enumerate() {
            let probe = r"import json, sys, typing
source, raises = sys.argv[1:]
def hazard():
    if raises == '1':
        raise KeyError()
ns = {'hazard': hazard}
exec(compile(source, '<audit>', 'exec'), ns)
print(json.dumps(ns['observed'] == typing.Sequence[int]))
";
            let output = tokio::time::timeout(
                Duration::from_secs(10),
                tokio::process::Command::new(python())
                    .args(["-B", "-c", probe, &case.source, &raises.to_string()])
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .expect("infrastructure error: Python deadline")
            .expect("infrastructure error: Python launch");
            assert!(output.status.success(), "{}: {:?}", case.id, output.stderr);
            let observed: bool = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(observed, *expected, "{}: raises={raises}", case.id);
            observations += 1;
        }
    }
    assert_eq!(
        observations, 6,
        "all three audit cases and both runtime paths execute"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn implicit_exception_run_does_not_count_non_typing_mutation_as_killed() {
    let case = cases()
        .into_iter()
        .find(|case| case.id == "audit_implicit_before_import")
        .expect("the audit's minimal counterexample is retained");
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    std::fs::write(&path, &case.source).unwrap();
    let probe = r"import runpy
def hazard():
    raise KeyError()
ns = runpy.run_path('subject.py', init_globals={'hazard': hazard})
assert ns['observed'] == set[int]
";
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--root"])
            .arg(root.path())
            .args([
                "--file",
                "subject.py",
                "--operators",
                "type_list_sequence",
                "--format",
                "json",
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
                "--",
            ])
            .arg(python())
            .args(["-B", "-c", probe])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("infrastructure error: run deadline")
    .expect("infrastructure error: run launch");
    assert!(output.status.success(), "{:?}", output.stderr);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0, "{report}");
    assert_eq!(report["summary"]["counts"]["killed"], 0, "{report}");
    assert_eq!(report["summary"]["complete"], true, "{report}");
    assert!(report["mutants"].as_array().unwrap().is_empty(), "{report}");
    assert_eq!(std::fs::read_to_string(path).unwrap(), case.source);
}
