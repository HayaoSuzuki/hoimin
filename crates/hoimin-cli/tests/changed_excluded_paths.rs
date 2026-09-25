#![cfg(unix)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

async fn git(root: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .current_dir(root)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("Git deadline")
        .expect("Git spawn");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

async fn plan(root: &Path, changed: bool, extra: &[&str]) -> (i32, Value, String) {
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python");
    let mut args: Vec<OsString> = ["hoimin", "plan", "--root"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(root.into());
    args.extend(["--source".into(), "selected".into()]);
    args.extend(
        [
            "--exclude",
            "data/**",
            "--operators",
            "boolean_literal",
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
        ]
        .into_iter()
        .map(OsString::from),
    );
    if changed {
        args.push("--changed".into());
    }
    args.extend(extra.iter().map(OsString::from));
    args.extend([
        OsString::from("--"),
        python.into(),
        "-c".into(),
        "pass".into(),
    ]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = tokio::time::timeout(
        Duration::from_secs(30),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .expect("plan deadline");
    (
        code,
        serde_json::from_slice(&stdout).unwrap_or(Value::Null),
        String::from_utf8(stderr).unwrap(),
    )
}

fn identities(manifest: &Value) -> Vec<Value> {
    manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| {
            let mut candidate = candidate.clone();
            for key in ["rank", "score", "ranking_reasons"] {
                candidate.as_object_mut().unwrap().remove(key);
            }
            candidate
        })
        .collect()
}

#[tokio::test]
async fn excluded_git_names_do_not_change_public_plan_in_three_repository_states() {
    let mut failures = Vec::new();
    for state in ["tracked", "untracked", "unborn-indexed"] {
        for extension in ["txt", "py"] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            git(root, &["init", "-q"]).await;
            let bad = format!(r"data/fixture\asset.{extension}");
            if state != "unborn-indexed" {
                write(root, "selected/app.py", "value = False\n");
                if state == "tracked" {
                    write(root, &bad, "old\n");
                }
                git(root, &["add", "."]).await;
                git(root, &["commit", "-qm", "before"]).await;
            }
            write(root, "selected/app.py", "value = True\n");
            write(root, &bad, "new\n");
            if state == "unborn-indexed" {
                git(root, &["add", "."]).await;
            }
            let (plain_code, baseline, plain_error) = plan(root, false, &[]).await;
            assert_eq!(plain_code, 0, "{state}/{extension}: {plain_error}");
            assert_eq!(identities(&baseline).len(), 1);
            let (changed_code, changed, error) = plan(root, true, &[]).await;
            if changed_code != 0 {
                failures.push(format!("{state}/{extension}: exit {changed_code}: {error}"));
            } else {
                assert_eq!(
                    identities(&changed),
                    identities(&baseline),
                    "{state}/{extension}"
                );
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn selected_unsupported_python_names_still_fail_plain_and_changed_plan() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q"]).await;
    write(root, r"selected/literal\module.py", "value = True\n");
    for changed in [false, true] {
        let (code, _, error) = plan(root, changed, &[]).await;
        assert_eq!(code, 2);
        assert!(
            error.contains("target path cannot be represented portably"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn excluded_binary_names_and_rename_origins_do_not_reject_selected_destination() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q"]).await;
    let header = "# unchanged rename evidence\n".repeat(40);
    write(
        root,
        r"data/old\module.py",
        format!("{header}value = False\n"),
    );
    write(root, r"data/binary\asset.py", b"\0old\n");
    git(root, &["add", "."]).await;
    git(root, &["commit", "-qm", "before"]).await;
    std::fs::create_dir(root.join("selected")).unwrap();
    git(root, &["mv", r"data/old\module.py", "selected/app.py"]).await;
    write(root, "selected/app.py", format!("{header}value = True\n"));
    write(root, r"data/binary\asset.py", b"\0new\n");
    let patch = git(root, &["diff", "HEAD", "--find-renames"]).await;
    assert!(patch.contains("rename to selected/app.py"), "{patch}");
    let numstat = git(root, &["diff", "HEAD", "--numstat"]).await;
    assert!(numstat.contains("-\t-\t"), "{numstat}");
    let (plain_code, baseline, error) = plan(root, false, &[]).await;
    assert_eq!(plain_code, 0, "{error}");
    let (code, changed, error) = plan(root, true, &[]).await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(identities(&changed), identities(&baseline));
    assert_eq!(identities(&changed).len(), 1);
}

#[tokio::test]
async fn excluded_backslash_spelling_cannot_alias_a_selected_slash_path() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q"]).await;
    write(root, "selected/app.py", "value = True\n");
    write(root, r"selected\app.py", "value = False\n");
    let mut selection = hoimin_core::Selection {
        root: camino::Utf8PathBuf::from_path_buf(root.to_path_buf()).unwrap(),
        files: vec!["selected/app.py".into()],
        ..hoimin_core::Selection::default()
    };
    let baseline = hoimin_cli::target::TargetHandler::resolve(&selection)
        .await
        .unwrap();
    assert_eq!(baseline.len(), 1);
    selection.changed = true;
    let changed = hoimin_cli::target::TargetHandler::resolve(&selection)
        .await
        .unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].path, baseline[0].path);
}

#[tokio::test]
async fn an_empty_resolved_scope_ignores_unsupported_git_names() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q"]).await;
    write(root, "selected/app.py", "value = True\n");
    write(root, r"data/bad\name.py", "value = False\n");
    let (code, manifest, error) = plan(root, true, &["--exclude", "selected/**"]).await;
    assert_eq!(code, 0, "{error}");
    assert!(identities(&manifest).is_empty());
}
