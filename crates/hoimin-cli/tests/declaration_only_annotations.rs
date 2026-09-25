use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use hoimin_cli::plan::PlanManifest;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u8,
    id: String,
    mode: String,
    source: String,
    operator: String,
    pairs: Vec<(String, String)>,
}

fn evaluate(source: &str, id: &str) -> Vec<u8> {
    let python = Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    });
    let output = Command::new(python)
        .args([
            "-c",
            "import sys; assert sys.version_info[:2] == (3, 14); ns = {}; exec(compile(sys.argv[1], '<declaration-oracle>', 'exec'), ns); print(repr(ns['observed']))",
            source,
        ])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("infrastructure error: CPython 3.14 must be installed in .venv");
    assert!(
        output.status.success(),
        "{id}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[tokio::test]
async fn declaration_only_lean_corpus_matches_public_plan() {
    let corpus = include_str!("../../../formal/HoiminOracle/corpus/declaration-only.jsonl");
    let cases: Vec<Case> = corpus
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 14);
    for mut case in cases {
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "strict", "{}", case.id);
        let baseline = evaluate(&case.source, &case.id);
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("subject.py"), &case.source).unwrap();
        let args = [
            OsString::from("hoimin"),
            "plan".into(),
            "--root".into(),
            project.path().as_os_str().to_owned(),
            "--file".into(),
            "subject.py".into(),
            "--operators".into(),
            case.operator.into(),
            "--allow-best-effort-memory".into(),
            "--jobs".into(),
            "1".into(),
            "--max-workspace-size".into(),
            "8GiB".into(),
            "--min-free-space".into(),
            "10GiB".into(),
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
        .expect("public plan timed out");
        assert_eq!(exit, 0, "{}: {}", case.id, String::from_utf8_lossy(&stderr));
        let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
        let mut actual = Vec::new();
        for candidate in manifest.candidates {
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            assert_eq!(&case.source[start..end], candidate.original, "{}", case.id);
            let mutated = format!(
                "{}{}{}",
                &case.source[..start],
                candidate.replacement,
                &case.source[end..]
            );
            ruff_python_parser::parse_module(&mutated).expect("replacement is valid Python");
            assert_ne!(
                evaluate(&mutated, &case.id),
                baseline,
                "{}: mutation changes observed value",
                case.id
            );
            actual.push((candidate.original.clone(), candidate.replacement.clone()));
        }
        actual.sort();
        case.pairs.sort();
        assert_eq!(actual, case.pairs, "{}", case.id);
    }
}
