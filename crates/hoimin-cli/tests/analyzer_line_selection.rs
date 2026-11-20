#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
//! Public selectors must preserve analyzer membership, identities and truncation.
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

async fn plan(root: &Path, selectors: &[String], limit: usize) -> Value {
    plan_operators(root, selectors, "boolean_literal", limit).await
}

async fn plan_operators(root: &Path, selectors: &[String], operators: &str, limit: usize) -> Value {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["plan", "--root"])
        .arg(root)
        .args(selectors)
        .args([
            "--operators",
            operators,
            "--max-candidates",
            &limit.to_string(),
            "--min-free-space",
            "1B",
            "--allow-best-effort-memory",
            "--",
            "python3",
            "-c",
            "pass",
        ])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(output.status.code(), Some(0 | 4)),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        output.status.code(),
        Some(if manifest["truncated"] == true { 4 } else { 0 })
    );
    manifest
}

#[tokio::test]
async fn many_line_selectors_admit_only_blank_lines() {
    // Arrange: every token candidate misses all 1,000 public selectors.
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "flag = True\n\n".repeat(1000),
    )
    .unwrap();
    let selectors = (1..=1000)
        .flat_map(|i| {
            [
                "--line".to_owned(),
                format!("subject.py:{}-{}", 2 * i, 2 * i),
            ]
        })
        .collect::<Vec<_>>();
    // Act.
    let output = plan(root.path(), &selectors, 1).await;
    // Assert.
    assert_eq!(output["candidates"], serde_json::json!([]));
    assert_eq!(output["truncated"], false);
    assert_eq!(output["diagnostics"], serde_json::json!([]));
}

#[tokio::test]
async fn equivalent_line_selectors_preserve_ids_order_and_truncation() {
    // Arrange: repeated and overlapping selectors denote the same union.
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        "a = True\nb = False\nc = True\nd = False\n",
    )
    .unwrap();
    // Act.
    let canonical = plan(root.path(), &["--line".into(), "subject.py:1-4".into()], 2).await;
    let raw = plan(
        root.path(),
        &[
            "--line".into(),
            "subject.py:3-4".into(),
            "--line".into(),
            "subject.py:1-4".into(),
            "--line".into(),
            "subject.py:2-2".into(),
        ],
        2,
    )
    .await;
    // Assert: candidate objects include IDs, ranks, spans and replacement text.
    assert_eq!(raw["candidates"], canonical["candidates"]);
    assert_eq!(raw["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(raw["truncated"], true);
}

async fn git(root: &Path, args: &[&str]) {
    let mut command = tokio::process::Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command
        .current_dir(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn noncontiguous_changed_hunks_admit_only_changed_candidates() {
    // Arrange: unchanged candidate sites separate two single-line changes.
    let root = tempfile::tempdir().unwrap();
    let source = concat!(
        "flag_0 = True\n",
        "flag_1 = True\n",
        "flag_2 = True\n",
        "flag_3 = True\n",
        "flag_4 = True\n",
        "flag_5 = True\n",
        "flag_6 = True\n",
        "flag_7 = True\n",
        "flag_8 = True\n",
        "flag_9 = True\n",
        "flag_10 = True\n",
        "flag_11 = True\n",
    );
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    git(root.path(), &["init", "-q"]).await;
    git(root.path(), &["add", "subject.py"]).await;
    git(root.path(), &["commit", "-qm", "fixture"]).await;
    let changed = source
        .replace("flag_1 = True", "flag_1 = False")
        .replace("flag_10 = True", "flag_10 = False");
    std::fs::write(root.path().join("subject.py"), changed).unwrap();
    // Act.
    let output = plan(
        root.path(),
        &["--source".into(), ".".into(), "--changed".into()],
        10,
    )
    .await;
    // Assert.
    let candidates = output["candidates"].as_array().unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|c| c["line"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [2, 11]
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["original"] == "False")
    );
    assert_eq!(output["truncated"], false);
}

const RESEARCH_SOURCE: &str = concat!(
    "import re\n",
    "from html import escape\n",
    "def target(value):\n",
    "    try:\n",
    "        cleaned = escape(value)\n",
    "        return re.search(r'\\d+', cleaned)\n",
    "    finally: pass\n",
    "def other(value):\n",
    "    return re.search(r'\\d+', value)\n",
    "def unaffected(values):\n",
    "    return [v for v in values if v]\n",
);
const RESEARCH_OPERATORS: &str =
    "finally_unwrap,stdlib_escape_remove,regex_character_set,comprehension_filter_remove";

#[rstest::rstest]
#[case::diff(
    &["--changed"],
    &[(6, "regex_character_set", "target"), (9, "regex_character_set", "other")],
)]
#[case::symbol(
    &["--symbol", "subject:target"],
    &[
        (4, "finally_unwrap", "target"),
        (5, "stdlib_escape_remove", "target"),
        (6, "regex_character_set", "target"),
    ],
)]
#[case::intersection(
    &["--changed", "--symbol", "subject:target"],
    &[(6, "regex_character_set", "target")],
)]
#[tokio::test]
async fn research_operators_respect_public_diff_and_symbol_selection(
    #[case] args: &[&str],
    #[case] expected: &[(u64, &str, &str)],
) {
    // Arrange two changed pattern lines in different functions.
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    std::fs::write(&path, RESEARCH_SOURCE.replace(r"\d+", r"\d*")).unwrap();
    git(root.path(), &["init", "-q"]).await;
    git(root.path(), &["add", "subject.py"]).await;
    git(root.path(), &["commit", "-qm", "fixture"]).await;
    std::fs::write(path, RESEARCH_SOURCE).unwrap();
    let selectors = ["--source", "."]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    // Act.
    let manifest = plan_operators(root.path(), &selectors, RESEARCH_OPERATORS, 10).await;
    let candidates = manifest["candidates"].as_array().unwrap();
    let mut sites = candidates
        .iter()
        .map(|candidate| {
            (
                candidate["line"].as_u64().unwrap(),
                candidate["operator"].as_str().unwrap(),
                candidate["symbol"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    sites.sort_unstable();

    // Assert both membership and the reason attached by Git-based ranking.
    assert_eq!(sites, expected);
    assert_eq!(manifest["truncated"], false);
    for candidate in candidates {
        let reasons = candidate["ranking_reasons"].as_array().unwrap();
        assert_eq!(
            reasons
                .iter()
                .any(|reason| reason["code"] == "changed_line"),
            args.contains(&"--changed")
        );
    }
}

#[rstest::rstest]
#[case::first_candidate(1)]
#[case::mixed_prefix(3)]
#[case::exact_capacity(5)]
#[tokio::test]
async fn research_operator_budget_keeps_the_serialized_ranked_prefix(#[case] limit: usize) {
    // Arrange.
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), RESEARCH_SOURCE).unwrap();
    let selectors = ["--file".to_owned(), "subject.py".to_owned()];
    let full = plan_operators(root.path(), &selectors, RESEARCH_OPERATORS, 10).await;

    // Act.
    let limited = plan_operators(root.path(), &selectors, RESEARCH_OPERATORS, limit).await;
    let saved = serde_json::to_vec(&limited).unwrap();
    let loaded: hoimin_cli::plan::PlanManifest = serde_json::from_slice(&saved).unwrap();

    // Assert that clipping and round-tripping preserve candidate IDs, ranks and edits.
    let all_candidates = full["candidates"].as_array().unwrap();
    assert_eq!(all_candidates.len(), 5);
    assert_eq!(full["truncated"], false);
    assert_eq!(limited["truncated"], limit < all_candidates.len());
    assert_eq!(
        limited["candidates"].as_array().unwrap(),
        &all_candidates[..limit]
    );
    assert_eq!(serde_json::to_value(loaded).unwrap(), limited);
}
