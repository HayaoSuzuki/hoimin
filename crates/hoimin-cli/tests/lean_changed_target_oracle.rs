use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use hoimin_core::{ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateIdentity, stable_mutant_id};
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/changed-target-composition.jsonl");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u32,
    id: String,
    mode: String,
    scenario: String,
    path: Option<String>,
    eligible_lines: Vec<u32>,
}

fn expected_contract() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    [
        ("modified-overlap", ("strict", "modified_overlap")),
        ("explicit-line", ("strict", "explicit_intersection")),
        ("symbol-line", ("strict", "symbol_intersection")),
        ("rename-destination", ("strict", "rename")),
        ("deleted-binary", ("strict", "excluded")),
        ("untracked-unterminated", ("strict", "untracked")),
        ("diff-base-worktree", ("strict", "diff_base")),
        ("hostile-parser", ("internal-fixture", "parser_isolation")),
        ("non-utf8-path", ("model-only", "non_utf8_path")),
        ("git-failure", ("infrastructure-error", "git_failure")),
    ]
    .into_iter()
    .collect()
}

fn parse_corpus(input: &str) -> Vec<OracleCase> {
    let cases = input
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<OracleCase>(line)
                .unwrap_or_else(|error| panic!("line {}: {error}", index + 1))
        })
        .collect::<Vec<_>>();
    let contract = cases
        .iter()
        .map(|case| {
            assert_eq!(case.schema, 1, "{}", case.id);
            assert!(
                case.eligible_lines.windows(2).all(|pair| pair[0] < pair[1]),
                "{}",
                case.id
            );
            (
                case.id.as_str(),
                (case.mode.as_str(), case.scenario.as_str()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(contract, expected_contract());
    assert_eq!(contract.len(), cases.len());
    for case in &cases {
        match case.mode.as_str() {
            "strict" => {
                if case.scenario == "excluded" {
                    assert!(case.path.is_none() && case.eligible_lines.is_empty());
                } else {
                    assert!(case.path.is_some() && !case.eligible_lines.is_empty());
                }
            }
            "internal-fixture" => assert_eq!(case.scenario, "parser_isolation"),
            "model-only" => assert_eq!(case.scenario, "non_utf8_path"),
            "infrastructure-error" => assert_eq!(case.scenario, "git_failure"),
            mode => panic!("unknown mode {mode}"),
        }
    }
    cases
}

fn render_rows(rows: Vec<serde_json::Value>) -> String {
    rows.into_iter().fold(String::new(), |mut output, row| {
        writeln!(output, "{row}").unwrap();
        output
    })
}

#[test]
fn changed_target_corpus_is_closed_and_typed() {
    assert_eq!(parse_corpus(CORPUS).len(), 10);
}

#[test]
fn changed_target_corpus_rejects_unknown_crossed_and_duplicate_rows() {
    let rows = CORPUS
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut unknown = rows.clone();
    unknown[0]["future"] = serde_json::json!(true);
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(unknown))).is_err());
    let mut crossed = rows.clone();
    crossed[0]["mode"] = serde_json::json!("model-only");
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(crossed))).is_err());
    let mut duplicate = rows.clone();
    duplicate[0]["id"] = duplicate[1]["id"].clone();
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(duplicate))).is_err());
    let mut unsorted = rows;
    unsorted[0]["eligible_lines"] = serde_json::json!([3, 2]);
    assert!(std::panic::catch_unwind(|| parse_corpus(&render_rows(unsorted))).is_err());
}

struct Repo {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

impl Repo {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_owned();
        let repo = Self {
            _directory: directory,
            root,
        };
        repo.git(&["init", "--quiet"]);
        repo.git(&["config", "user.email", "fixture@example.com"]);
        repo.git(&["config", "user.name", "Fixture"]);
        repo
    }

    fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn git(&self, args: &[&str]) {
        let _ = self.git_output(args);
    }

    fn git_output(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", "fixture"]);
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn python_executable() -> PathBuf {
    if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    }
}

async fn plan(repo: &Repo, extra: &[&str]) -> serde_json::Value {
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        repo.root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("pkg"),
        OsString::from("--changed"),
        OsString::from("--operators"),
        OsString::from("binary_add_sub"),
        OsString::from("--allow-best-effort-memory"),
    ];
    args.extend(extra.iter().map(OsString::from));
    args.extend([
        OsString::from("--"),
        python_executable().into_os_string(),
        OsString::from("-c"),
        OsString::from("pass"),
    ]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    serde_json::from_slice(&stdout).unwrap()
}

fn candidates(manifest: &serde_json::Value) -> &[serde_json::Value] {
    manifest["candidates"].as_array().unwrap()
}

fn assert_candidate_ids(candidates: &[serde_json::Value]) {
    for candidate in candidates {
        let expected = stable_mutant_id(&CandidateIdentity {
            schema_version: CANDIDATE_SCHEMA_VERSION,
            file_hash: candidate["file_hash"].as_str().unwrap().to_owned(),
            path: candidate["path"].as_str().unwrap().into(),
            span: ByteSpan {
                start: candidate["span"]["start"].as_u64().unwrap(),
                length: candidate["span"]["length"].as_u64().unwrap(),
            },
            operator: candidate["operator"].as_str().unwrap().to_owned(),
            replacement: candidate["replacement"].as_str().unwrap().to_owned(),
        });
        assert_eq!(candidate["id"], expected.as_str());
    }
}

fn assert_candidate_spans(repo: &Repo, candidates: &[serde_json::Value]) {
    for candidate in candidates {
        let source = fs::read(repo.root.join(candidate["path"].as_str().unwrap())).unwrap();
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        let end = start.checked_add(length).unwrap();
        assert_eq!(
            &source[start..end],
            candidate["original"].as_str().unwrap().as_bytes()
        );
        let line = source[..start].split(|byte| *byte == b'\n').count();
        assert_eq!(
            candidate["line"].as_u64().unwrap(),
            u64::try_from(line).unwrap()
        );
    }
}

fn observed_lines(manifest: &serde_json::Value, path: &str) -> BTreeSet<u32> {
    candidates(manifest)
        .iter()
        .filter(|candidate| candidate["path"] == path)
        .map(|candidate| u32::try_from(candidate["line"].as_u64().unwrap()).unwrap())
        .collect()
}

fn arithmetic_source(lines: usize) -> String {
    (1..=lines).fold(String::new(), |mut source, line| {
        writeln!(source, "value_{line} = {line} + 1").unwrap();
        source
    })
}

#[tokio::test]
async fn public_plan_matches_changed_range_and_explicit_line_intersection() {
    let repo = Repo::new();
    repo.write("pkg/a.py", arithmetic_source(8));
    repo.commit();
    let mut changed = arithmetic_source(8);
    for line in 2..=7 {
        changed = changed.replace(
            &format!("value_{line} = {line} + 1"),
            &format!("value_{line} = {line} + 2"),
        );
    }
    repo.write("pkg/a.py", changed);

    let manifest = plan(&repo, &[]).await;
    assert_eq!(
        observed_lines(&manifest, "pkg/a.py"),
        BTreeSet::from([2, 3, 4, 5, 6, 7])
    );
    assert_candidate_ids(candidates(&manifest));
    assert_candidate_spans(&repo, candidates(&manifest));

    let intersected = plan(&repo, &["--line", "pkg/a.py:4-5"]).await;
    assert_eq!(
        observed_lines(&intersected, "pkg/a.py"),
        BTreeSet::from([4, 5])
    );
    assert_candidate_ids(candidates(&intersected));
    assert_candidate_spans(&repo, candidates(&intersected));
}

#[tokio::test]
async fn public_plan_preserves_symbol_restrictions_under_changed_intersection() {
    let repo = Repo::new();
    repo.write(
        "pkg/a.py",
        "class Widget:\n    def run(self):\n        return 1 + 1\n\ndef other():\n    return 2 + 2\n",
    );
    repo.commit();
    repo.write(
        "pkg/a.py",
        "class Widget:\n    def run(self):\n        return 1 + 2\n\ndef other():\n    return 2 + 3\n",
    );

    let manifest = plan(&repo, &["--symbol", "a:Widget.run"]).await;
    let actual = candidates(&manifest);
    assert!(!actual.is_empty());
    assert!(actual.iter().all(|candidate| candidate["line"] == 3));
    assert_candidate_ids(actual);
    assert_candidate_spans(&repo, actual);
}

#[tokio::test]
async fn public_plan_uses_rename_destination_and_excludes_deleted_and_binary() {
    let repo = Repo::new();
    repo.write("pkg/old.py", arithmetic_source(3));
    repo.write("pkg/deleted.py", "gone = 1 + 1\n");
    repo.write("pkg/binary.py", b"before\0bytes\n");
    repo.commit();
    repo.git(&["mv", "pkg/old.py", "pkg/new.py"]);
    repo.write(
        "pkg/new.py",
        "value_1 = 1 + 1\nvalue_2 = 2 + 2\nvalue_3 = 3 + 1\n",
    );
    fs::remove_file(repo.root.join("pkg/deleted.py")).unwrap();
    repo.write("pkg/binary.py", b"after\0bytes\n");

    let manifest = plan(&repo, &[]).await;
    assert_eq!(observed_lines(&manifest, "pkg/new.py"), BTreeSet::from([2]));
    assert!(candidates(&manifest).iter().all(|candidate| {
        candidate["path"] != "pkg/old.py"
            && candidate["path"] != "pkg/deleted.py"
            && candidate["path"] != "pkg/binary.py"
    }));
    assert_candidate_ids(candidates(&manifest));
    assert_candidate_spans(&repo, candidates(&manifest));
}

#[tokio::test]
async fn public_plan_counts_the_last_unterminated_untracked_line() {
    let repo = Repo::new();
    repo.write("pkg/base.py", "base = 1\n");
    repo.commit();
    repo.write("pkg/new.py", "first = 1 + 1\nsecond = 2 + 2");

    let manifest = plan(&repo, &[]).await;
    assert_eq!(
        observed_lines(&manifest, "pkg/new.py"),
        BTreeSet::from([1, 2])
    );
    assert_candidate_ids(candidates(&manifest));
    assert_candidate_spans(&repo, candidates(&manifest));
}

#[tokio::test]
async fn public_plan_diff_base_composes_head_and_worktree_destination_lines() {
    let repo = Repo::new();
    repo.write("pkg/a.py", arithmetic_source(4));
    repo.commit();
    let base = repo.git_output(&["rev-parse", "HEAD"]);
    repo.write(
        "pkg/a.py",
        "value_1 = 1 + 1\nvalue_2 = 2 + 2\nvalue_3 = 3 + 1\nvalue_4 = 4 + 1\n",
    );
    repo.commit();
    repo.write(
        "pkg/a.py",
        "value_1 = 1 + 1\nvalue_2 = 2 + 2\nvalue_3 = 3 + 2\nvalue_4 = 4 + 1\n",
    );

    let manifest = plan(&repo, &["--diff-base", &base]).await;
    assert_eq!(
        observed_lines(&manifest, "pkg/a.py"),
        BTreeSet::from([2, 3])
    );
    assert_candidate_ids(candidates(&manifest));
    assert_candidate_spans(&repo, candidates(&manifest));
}
