use std::collections::BTreeMap;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_cli::target::{
    TargetHandler,
    git::{ResolveGitChanges, handle_git},
};
use hoimin_core::{EffectId, LineRange, Selection, TargetSlice, intersect_changed};
use tokio::process::Command;

struct Repository {
    directory: tempfile::TempDir,
}

impl Repository {
    async fn new() -> Self {
        let result = Self {
            directory: tempfile::tempdir().unwrap(),
        };
        result.git(&["init", "-q"]).await;
        result
    }
    fn root(&self) -> &Path {
        self.directory.path()
    }
    fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.root().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    async fn git(&self, args: &[&str]) -> Vec<u8> {
        let mut command = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(self.root())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(20), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
    async fn commit(&self) {
        self.git(&["add", "."]).await;
        self.git(&["commit", "-qm", "fixture"]).await;
    }
    fn selection(&self) -> Selection {
        Selection {
            root: Utf8PathBuf::from_path_buf(self.root().to_owned()).unwrap(),
            sources: vec!["selected".into()],
            changed: true,
            ..Selection::default()
        }
    }
    async fn compare(&self, context: u32, base: Option<String>) -> Vec<TargetSlice> {
        let mut selection = self.selection();
        selection.changed_context = context;
        selection.diff_base.clone_from(&base);
        let changed = TargetHandler::resolve(&selection).await.unwrap();
        selection.changed = false;
        let eligible = TargetHandler::resolve(&selection).await.unwrap();
        // The unscoped public API retains the historical whole-repository algorithm.
        if context == 0 {
            let all = handle_git(ResolveGitChanges {
                id: EffectId(1),
                root: selection.root,
                diff_base: base,
            })
            .await
            .unwrap();
            assert_eq!(changed, intersect_changed(&eligible, &all.changed));
        }
        changed
    }
}

#[cfg(unix)]
fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

#[cfg(unix)]
#[tokio::test]
async fn public_plan_does_not_receive_unselected_tracked_patch_bodies() {
    use std::os::unix::fs::PermissionsExt;
    let evidence = tempfile::tempdir().unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let bin = evidence.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let trace = evidence.path().join("git-output.jsonl");
    let wrapper = bin.join("git");
    std::fs::write(&wrapper, format!(r"#!/usr/bin/env python3
import json, pathlib, subprocess, sys
result = subprocess.run([{}, *sys.argv[1:]], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)
with pathlib.Path({}).open('a') as log:
    log.write(json.dumps(dict(args=sys.argv[1:], bytes=len(result.stdout))) + '\n')
sys.stdout.buffer.write(result.stdout)
sys.stderr.buffer.write(result.stderr)
raise SystemExit(result.returncode)
", serde_json::to_string(real_git.to_str().unwrap()).unwrap(), serde_json::to_string(trace.to_str().unwrap()).unwrap())).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let mut ids = Vec::new();
    let mut received = Vec::new();
    for rows in [2_048, 16_384] {
        let repo = Repository::new().await;
        repo.write("selected/app.py", "def f(a,b):\n    return a - b\n");
        repo.write(
            "data/export.csv",
            format!("a,{}\n", "x".repeat(125)).repeat(rows),
        );
        repo.commit().await;
        repo.write("selected/app.py", "def f(a,b):\n    return a + b\n");
        repo.write(
            "data/export.csv",
            format!("b,{}\n", "x".repeat(125)).repeat(rows),
        );
        std::fs::write(&trace, "").unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["plan", "--root"])
            .arg(repo.root())
            .args([
                "--source",
                "selected",
                "--file",
                "selected/app.py",
                "--changed",
                "--operators",
                "binary_add_sub",
                "--allow-best-effort-memory",
                "--min-free-space",
                "1B",
                "--",
            ])
            .arg(python())
            .args(["-c", "pass"])
            .env("PATH", &path)
            .env("GIT_GLOB_PATHSPECS", "1")
            .env("TMPDIR", temporary.path())
            .env("TMP", temporary.path())
            .env("TEMP", temporary.path())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(40), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(manifest["candidates"].as_array().unwrap().len(), 1);
        ids.push(manifest["candidates"][0]["id"].clone());
        let mut patch_bytes = 0;
        for line in std::fs::read_to_string(&trace).unwrap().lines() {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            let args = record["args"].as_array().unwrap();
            if args.iter().any(|arg| arg == "diff")
                && !args.iter().any(|arg| arg == "--name-status")
            {
                patch_bytes += record["bytes"].as_u64().unwrap();
            }
        }
        received.push(patch_bytes);
    }
    assert_eq!(ids[0], ids[1]);
    eprintln!("public scoped diff bytes: {received:?}");
    assert!(
        received.iter().all(|bytes| *bytes < 4096),
        "unselected diff bytes reached public plan: {received:?}"
    );
}

#[tokio::test]
async fn scoped_changes_preserve_modification_deletion_binary_and_untracked_selection() {
    let repo = Repository::new().await;
    for path in [
        "selected/keep.py",
        "selected/deleted.py",
        "selected/binary.py",
        "data/ignored.csv",
    ] {
        repo.write(path, "a = 1\nb = 2\nc = 3\n");
    }
    repo.commit().await;
    repo.write("selected/keep.py", "a = 1\nb = 9\nc = 3\n");
    std::fs::remove_file(repo.root().join("selected/deleted.py")).unwrap();
    repo.write("selected/binary.py", b"\0binary");
    repo.write("selected/untracked.py", "x = 1\ny = 2\n");
    repo.write("data/ignored.csv", "changed\n".repeat(100));
    let changes = repo.compare(0, None).await;
    let paths: BTreeMap<_, _> = changes
        .into_iter()
        .map(|target| (target.path, target.lines))
        .collect();
    assert_eq!(
        paths,
        BTreeMap::from([
            (
                "selected/keep.py".into(),
                vec![LineRange { start: 2, end: 2 }]
            ),
            (
                "selected/untracked.py".into(),
                vec![LineRange { start: 1, end: 2 }]
            ),
        ])
    );
    let expanded = repo.compare(1, None).await;
    assert_eq!(
        expanded
            .iter()
            .find(|target| target.path == "selected/keep.py")
            .unwrap()
            .lines,
        [LineRange { start: 1, end: 3 }]
    );
}

#[tokio::test]
async fn scoped_diff_base_and_unborn_sources_match_unscoped_resolution() {
    let repo = Repository::new().await;
    repo.write("selected/staged.py", "x = 1\ny = 2\n");
    repo.git(&["add", "."]).await;
    repo.write("selected/untracked.py", "x = 3\n");
    assert_eq!(repo.compare(0, None).await.len(), 2);
    repo.commit().await;
    let base = String::from_utf8(repo.git(&["rev-parse", "HEAD"]).await)
        .unwrap()
        .trim()
        .to_owned();
    repo.write("selected/staged.py", "x = 8\ny = 2\n");
    repo.commit().await;
    assert_eq!(
        repo.compare(0, Some(base)).await[0].lines,
        [LineRange { start: 1, end: 1 }]
    );
}

#[tokio::test]
async fn selected_and_unrelated_rename_directions_preserve_hunk_ranges() {
    for direction in ["into", "out", "unrelated"] {
        let repo = Repository::new().await;
        let header = "# stable rename evidence\n".repeat(40);
        repo.write("selected/ordinary.py", "x = 1\n");
        let old = if direction == "out" {
            "selected/old.py"
        } else {
            "data/old.py"
        };
        let new = if direction == "into" {
            "selected/new.py"
        } else {
            "data/new.py"
        };
        repo.write(old, format!("{header}x = 1\n"));
        repo.commit().await;
        std::fs::create_dir_all(repo.root().join("data")).unwrap();
        repo.git(&["mv", old, new]).await;
        repo.write(new, format!("{header}x = 2\n"));
        repo.write("selected/ordinary.py", "x = 9\n");
        let status = String::from_utf8(
            repo.git(&["diff", "--name-status", "--find-renames", "HEAD"])
                .await,
        )
        .unwrap();
        assert!(status.lines().any(|line| line.starts_with('R')), "{status}");
        let changed = repo.compare(0, None).await;
        assert_eq!(changed.len(), if direction == "into" { 2 } else { 1 });
        if direction == "into" {
            assert_eq!(
                changed
                    .iter()
                    .find(|target| target.path == new)
                    .unwrap()
                    .lines,
                [LineRange { start: 41, end: 41 }]
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn literal_pathspecs_preserve_supported_names_and_existing_colon_rejection() {
    let repo = Repository::new().await;
    let names = [
        ":(exclude)*.py",
        "--option.py",
        "brackets[1].py",
        "spaces and\tquoted\".py",
    ];
    for name in names.iter().chain(["ordinary.py"].iter()) {
        repo.write(name, "x = 1\n");
    }
    repo.commit().await;
    for name in names.iter().chain(["ordinary.py"].iter()) {
        repo.write(name, "x = 9\n");
    }
    let mut selection = repo.selection();
    selection.sources.clear();
    selection.files = names.iter().map(|name| (*name).into()).collect();
    selection.changed = false;
    let eligible = TargetHandler::resolve(&selection).await.unwrap();
    assert_eq!(eligible.len(), names.len());
    selection.changed = true;
    let actual = TargetHandler::resolve(&selection).await.unwrap();
    let historical = handle_git(ResolveGitChanges {
        id: EffectId(2),
        root: selection.root,
        diff_base: None,
    })
    .await
    .unwrap();
    assert_eq!(actual, intersect_changed(&eligible, &historical.changed));
    // Mutation targets reject colon-containing path components. Keep this
    // literal magic-looking name in Git's inventory to ensure it cannot exclude
    // the other valid selected paths; do not broaden the portable-path contract.
    assert_eq!(actual.len(), names.len() - 1);
    assert!(actual.iter().all(|target| target.path != names[0]));
}

#[tokio::test]
async fn many_long_selected_paths_preserve_all_batch_results() {
    let repo = Repository::new().await;
    let names: Vec<_> = (0..140)
        .map(|index| format!("selected/{index:03}_{}.py", "x".repeat(90)))
        .collect();
    for name in &names {
        repo.write(name, "x = 1\ny = 2\n");
    }
    repo.commit().await;
    for name in &names {
        repo.write(name, "x = 1\ny = 9\n");
    }
    let changed = repo.compare(0, None).await;
    assert_eq!(changed.len(), names.len());
    assert!(
        changed
            .iter()
            .all(|target| target.lines == [LineRange { start: 2, end: 2 }])
    );
}

#[tokio::test]
async fn empty_scope_still_validates_diff_base_and_repository() {
    let repo = Repository::new().await;
    std::fs::create_dir(repo.root().join("selected")).unwrap();
    repo.git(&["commit", "--allow-empty", "-qm", "initial"])
        .await;
    let mut selection = repo.selection();
    selection.diff_base = Some("no-such-revision".into());
    assert!(matches!(
        TargetHandler::resolve(&selection).await,
        Err(hoimin_core::TargetError::GitFailed(_))
    ));
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(outside.path().join("selected")).unwrap();
    selection.root = Utf8PathBuf::from_path_buf(outside.path().to_owned()).unwrap();
    selection.diff_base = None;
    assert!(matches!(
        TargetHandler::resolve(&selection).await,
        Err(hoimin_core::TargetError::GitRepositoryRequired)
    ));
}
