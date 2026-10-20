#![cfg(unix)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

async fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .env_remove("GIT_DIFF_OPTS")
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
        .current_dir(root)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn plan(root: &Path, context: Option<u32>, diff_opts: Option<&str>) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command.args(["plan", "--root"]).arg(root).args([
        "--source",
        "selected",
        "--changed",
        "--operators",
        "boolean_literal",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
    ]);
    if let Some(context) = context {
        command.args(["--changed-context", &context.to_string()]);
    }
    command.env_remove("GIT_DIFF_OPTS");
    if let Some(diff_opts) = diff_opts {
        command.env("GIT_DIFF_OPTS", diff_opts);
    }
    command
        .arg("--")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python"))
        .args(["-c", "pass"])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

async fn context_case(context: Option<u32>, diff_opts: Option<&str>, rename: bool) {
    // Arrange: the sole edit is row 6; the rename retains enough identical bytes for detection.
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    std::fs::create_dir(root.join("selected")).unwrap();
    let mut before = String::new();
    for row in 1..=11 {
        writeln!(
            &mut before,
            "v{row} = True # unchanged fixture padding for rename detection"
        )
        .unwrap();
    }
    std::fs::write(root.join("selected/before.py"), &before).unwrap();
    git(root, &["init", "-q"]).await;
    git(root, &["add", "."]).await;
    git(root, &["commit", "-qm", "fixture"]).await;
    let path = if rename {
        git(root, &["mv", "selected/before.py", "selected/after.py"]).await;
        "selected/after.py"
    } else {
        "selected/before.py"
    };
    std::fs::write(root.join(path), before.replace("v6 = True", "v6 = False")).unwrap();
    let baseline = plan(root, context, None).await;

    // Act: only the Git presentation environment changes.
    let actual = plan(root, context, diff_opts).await;

    // Assert: IDs, ordering and ranking are preserved as well as the selected rows and saved context.
    let wanted: Vec<u64> = if context == Some(2) {
        vec![4, 5, 6, 7, 8]
    } else {
        vec![6]
    };
    let candidates = actual["candidates"].as_array().unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate["line"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        wanted
    );
    assert_eq!(actual["candidates"], baseline["candidates"]);
    assert_eq!(
        actual["normalized_config"]["selection"]["changed"]["context"],
        context.unwrap_or(0)
    );
    for candidate in candidates {
        assert!(
            candidate["ranking_reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason["code"] == "changed_line")
        );
    }
}

macro_rules! context_test {
    ($name:ident, $context:expr, $opts:expr, $rename:expr) => {
        #[tokio::test]
        async fn $name() {
            context_case($context, $opts, $rename).await;
        }
    };
}
context_test!(scoped_default_unset, None, None, false);
context_test!(scoped_default_env_zero, None, Some("--unified=0"), false);
context_test!(scoped_default_env_four, None, Some("--unified=4"), false);
context_test!(scoped_zero_unset, Some(0), None, false);
context_test!(scoped_zero_env_zero, Some(0), Some("--unified=0"), false);
context_test!(scoped_zero_env_four, Some(0), Some("--unified=4"), false);
context_test!(scoped_two_unset, Some(2), None, false);
context_test!(scoped_two_env_zero, Some(2), Some("--unified=0"), false);
context_test!(scoped_two_env_four, Some(2), Some("--unified=4"), false);
context_test!(rename_default_unset, None, None, true);
context_test!(rename_default_env_zero, None, Some("--unified=0"), true);
context_test!(rename_default_env_four, None, Some("--unified=4"), true);
context_test!(rename_zero_unset, Some(0), None, true);
context_test!(rename_zero_env_zero, Some(0), Some("--unified=0"), true);
context_test!(rename_zero_env_four, Some(0), Some("--unified=4"), true);
context_test!(rename_two_unset, Some(2), None, true);
context_test!(rename_two_env_zero, Some(2), Some("--unified=0"), true);
context_test!(rename_two_env_four, Some(2), Some("--unified=4"), true);

#[tokio::test]
async fn target_git_sanitization_preserves_user_test_environment() {
    // Arrange: a changed target and a baseline that can prove its inherited environment.
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("app.py"), "value = True\n").unwrap();
    git(&root, &["init", "-q"]).await;
    git(&root, &["add", "."]).await;
    git(&root, &["commit", "-qm", "fixture"]).await;
    std::fs::write(root.join("app.py"), "value = False\n").unwrap();
    let marker = fixture.path().join("baseline-environment");
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python");
    let script = "import os,pathlib,sys; value=os.environ['GIT_DIFF_OPTS']; assert value == '--unified=4'; pathlib.Path(sys.argv[1]).write_text(value)";

    // Act: the target selector and test process inherit the same parent environment.
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--root"])
            .arg(&root)
            .args([
                "--source",
                ".",
                "--changed",
                "--changed-context",
                "0",
                "--operators",
                "boolean_literal",
                "--total-timeout",
                "20s",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--",
            ])
            .arg(python)
            .args(["-c", script])
            .arg(&marker)
            .env("GIT_DIFF_OPTS", "--unified=4")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();

    // Assert: a successful real baseline executed with the original value.
    assert_eq!(
        output.status.code(),
        Some(1), // The baseline passes and the deliberately untested mutant survives.
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "--unified=4");
}
