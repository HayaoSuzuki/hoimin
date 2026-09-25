//! Lean owns the expected line sets; the adapter executes real Git and public plan.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/changed-context.jsonl");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u32,
    id: String,
    before: String,
    after: String,
    context: u32,
    explicit: Vec<u32>,
    untracked: bool,
    eligible_lines: Vec<u32>,
}

fn cases(input: &str) -> Vec<Case> {
    let cases: Vec<Case> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 44);
    let mut ids = BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert!(ids.insert(&case.id), "duplicate {}", case.id);
        assert!(case.eligible_lines.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            case.eligible_lines
                .iter()
                .all(|&line| line > 0 && line as usize <= case.after.lines().count())
        );
    }
    cases
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn changed_context_corpus_is_closed_and_rejects_duplicates() {
    cases(CORPUS);
    let mut lines: Vec<_> = CORPUS.lines().collect();
    lines[1] = lines[0];
    assert!(std::panic::catch_unwind(|| cases(&lines.join("\n"))).is_err());
}

#[tokio::test]
async fn changed_context_public_plan_matches_lean() {
    for case in cases(CORPUS) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        git(root, &["init", "--quiet"]);
        git(root, &["config", "user.email", "fixture@example.com"]);
        git(root, &["config", "user.name", "Fixture"]);
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("tracked.txt"), "fixture\n").unwrap();
        if !case.untracked {
            fs::write(root.join("src/calc.py"), &case.before).unwrap();
        }
        git(root, &["add", "."]);
        git(root, &["commit", "--quiet", "-m", "fixture"]);
        fs::write(root.join("src/calc.py"), &case.after).unwrap();
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("plan"),
            OsString::from("--root"),
            root.as_os_str().to_owned(),
            OsString::from("--source"),
            OsString::from("src"),
            OsString::from("--changed"),
            OsString::from("--changed-context"),
            OsString::from(case.context.to_string()),
            OsString::from("--operators"),
            OsString::from("binary_add_sub"),
            OsString::from("--allow-best-effort-memory"),
        ];
        for line in &case.explicit {
            args.extend([
                OsString::from("--line"),
                OsString::from(format!("src/calc.py:{line}")),
            ]);
        }
        args.extend([
            OsString::from("--"),
            OsString::from("python3"),
            OsString::from("-c"),
            OsString::from("pass"),
        ]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
        assert_eq!(code, 0, "{}: {}", case.id, String::from_utf8_lossy(&stderr));
        let manifest: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let candidates = manifest["candidates"].as_array().unwrap();
        let mut observed = Vec::new();
        for candidate in candidates {
            assert_eq!(candidate["path"], "src/calc.py", "{}", case.id);
            assert_eq!(candidate["operator"], "binary_add_sub", "{}", case.id);
            assert_eq!(candidate["original"], "+", "{}", case.id);
            observed.push(u32::try_from(candidate["line"].as_u64().unwrap()).unwrap());
        }
        observed.sort_unstable();
        assert_eq!(observed, case.eligible_lines, "{}", case.id);
        assert_eq!(manifest["truncated"], false, "{}", case.id);
    }
}
