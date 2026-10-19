//! Public selectors must preserve analyzer membership, identities and truncation.
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

async fn plan(root: &Path, selectors: &[String], limit: usize) -> Value {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(["plan", "--root"])
        .arg(root)
        .args(selectors)
        .args([
            "--operators",
            "boolean_literal",
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
    serde_json::from_slice(&output.stdout).unwrap()
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
