use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use camino::Utf8PathBuf;
use hoimin_cli::target::TargetHandler;
use hoimin_cli::target::git::{GitChangesResolved, ResolveGitChanges, handle_git};
use hoimin_core::{EffectId, LineRange, ResolveTargets, Selection, TargetError, TargetSlice};
use tempfile::TempDir;

struct FixtureRepo {
    temp: TempDir,
}

impl FixtureRepo {
    fn new() -> Self {
        let repo = Self {
            temp: tempfile::tempdir().unwrap(),
        };
        repo.git(&["init", "--quiet"]);
        repo.git(&["config", "user.email", "fixture@example.com"]);
        repo.git(&["config", "user.name", "Fixture"]);
        repo
    }

    fn root(&self) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(self.temp.path().to_path_buf()).unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.temp.path().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn remove(&self, path: &str) {
        fs::remove_file(self.temp.path().join(path)).unwrap();
    }

    fn commit_all(&self, message: &str) -> String {
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    async fn changed_lines(&self, diff_base: Option<String>) -> GitChangesResolved {
        handle_git(ResolveGitChanges {
            id: EffectId(7),
            root: self.root(),
            diff_base,
        })
        .await
        .unwrap()
    }
}

#[test]
fn discovers_python_files_and_reports_regular_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join("pkg/a.py"), "x = 1\n").unwrap();
    fs::write(temp.path().join("pkg/data.txt"), "fixture\n").unwrap();

    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert!(
        files
            .iter()
            .any(|file| file.path == "pkg/a.py" && file.is_python)
    );
    assert!(
        files
            .iter()
            .any(|file| file.path == "pkg/data.txt" && !file.is_python)
    );
}

#[test]
fn explicit_exclude_wins_over_include() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join(".gitignore"), "pkg/generated.py\n").unwrap();
    fs::write(temp.path().join("pkg/a.py"), "x = 1\n").unwrap();
    fs::write(temp.path().join("pkg/generated.py"), "x = 2\n").unwrap();

    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        includes: vec!["pkg/generated.py".into()],
        excludes: vec!["pkg/generated.py".into()],
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert!(files.iter().any(|file| file.path == "pkg/a.py"));
    assert!(!files.iter().any(|file| file.path == "pkg/generated.py"));
}

#[test]
fn include_can_restore_a_gitignored_file() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join(".gitignore"), "pkg/generated.py\n").unwrap();
    fs::write(temp.path().join("pkg/generated.py"), "x = 2\n").unwrap();

    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        includes: vec!["pkg/generated.py".into()],
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert!(files.iter().any(|file| file.path == "pkg/generated.py"));
}

#[tokio::test]
async fn changed_collects_staged_unstaged_and_untracked_python_lines() {
    let repo = FixtureRepo::new();
    repo.write(".gitignore", "build/\n");
    repo.write("pkg/a.py", "a1\na2\na3\n");
    repo.write("pkg/b.py", "b1\nb2\nb3\nb4\nb5\nb6\n");
    repo.commit_all("initial");

    repo.write("pkg/a.py", "a1\nchanged a2\na3\n");
    repo.git(&["add", "pkg/a.py"]);
    repo.write("pkg/b.py", "b1\nb2\nb3\nb4\nchanged b5\nb6\n");
    repo.write("pkg/c.py", "x = 1\n");
    repo.write("build/ignored.py", "x = 1\n");

    let resolved = repo.changed_lines(None).await;
    assert_eq!(resolved.id, EffectId(7));
    assert_eq!(
        resolved.changed,
        BTreeMap::from([
            (
                Utf8PathBuf::from("pkg/a.py"),
                vec![LineRange { start: 2, end: 2 }],
            ),
            (
                Utf8PathBuf::from("pkg/b.py"),
                vec![LineRange { start: 5, end: 5 }],
            ),
            (
                Utf8PathBuf::from("pkg/c.py"),
                vec![LineRange { start: 1, end: 1 }],
            ),
        ])
    );
}

#[tokio::test]
async fn changed_collects_untracked_python_in_unborn_repository() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/a.py"),
            vec![LineRange { start: 1, end: 2 }],
        )])
    );
}

#[tokio::test]
async fn changed_collects_staged_python_in_unborn_repository() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\nthree\n");
    repo.git(&["add", "pkg/a.py"]);

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/a.py"),
            vec![LineRange { start: 1, end: 3 }],
        )])
    );
}

#[tokio::test]
async fn uses_diff_base_against_worktree() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\nthree\n");
    let base = repo.commit_all("base");
    repo.write("pkg/a.py", "one\nTWO\nthree\n");
    repo.commit_all("head");
    repo.write("pkg/a.py", "one\nTWO\nTHREE\n");

    assert_eq!(
        repo.changed_lines(Some(base)).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/a.py"),
            vec![LineRange { start: 2, end: 3 }],
        )])
    );
}

#[tokio::test]
async fn diff_base_uses_merge_base_against_worktree() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\nthree\n");
    let common = repo.commit_all("common");
    repo.git(&["checkout", "--quiet", "-b", "base"]);
    repo.write("pkg/a.py", "one\nBASE\nthree\n");
    repo.commit_all("base branch");
    repo.git(&["checkout", "--quiet", "-b", "feature", &common]);
    repo.write("pkg/a.py", "one\ntwo\nFEATURE\n");

    assert_eq!(
        repo.changed_lines(Some("base".into())).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/a.py"),
            vec![LineRange { start: 3, end: 3 }],
        )])
    );
}

#[tokio::test]
async fn diff_base_starting_with_dash_is_rejected_before_diff() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\n");
    repo.commit_all("initial");
    let selection = Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from("pkg")],
        changed: true,
        diff_base: Some("--stat".into()),
        ..Selection::default()
    };

    let error = TargetHandler::resolve(&selection).await.unwrap_err();
    assert!(
        matches!(error, TargetError::GitFailed(ref message) if message.contains("git rev-parse --verify --end-of-options")),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn changed_cancels_staged_edit_reverted_in_worktree() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\nthree\n");
    repo.commit_all("initial");
    repo.write("pkg/a.py", "one\nSTAGED\nthree\n");
    repo.git(&["add", "pkg/a.py"]);
    repo.write("pkg/a.py", "one\ntwo\nthree\n");

    assert!(repo.changed_lines(None).await.changed.is_empty());
}

#[tokio::test]
async fn ignores_deleted_and_binary_paths() {
    let repo = FixtureRepo::new();
    repo.write("pkg/deleted.py", "x = 1\n");
    repo.write("pkg/binary.py", b"before\0bytes\n");
    repo.commit_all("initial");
    repo.remove("pkg/deleted.py");
    repo.write("pkg/binary.py", b"after\0bytes\n");

    let changed = repo.changed_lines(None).await.changed;
    assert!(changed.is_empty(), "unexpected changed lines: {changed:?}");
}

#[tokio::test]
async fn ignores_files_deleted_or_made_binary_after_staging() {
    let repo = FixtureRepo::new();
    repo.write("pkg/deleted.py", "one\ntwo\n");
    repo.write("pkg/binary.py", "one\ntwo\n");
    repo.commit_all("initial");
    repo.write("pkg/deleted.py", "one\nTWO\n");
    repo.write("pkg/binary.py", "one\nTWO\n");
    repo.git(&["add", "pkg/deleted.py", "pkg/binary.py"]);
    repo.remove("pkg/deleted.py");
    repo.write("pkg/binary.py", b"now\0binary\n");

    let changed = repo.changed_lines(None).await.changed;
    assert!(changed.is_empty(), "unexpected changed lines: {changed:?}");
}

#[tokio::test]
async fn changed_accepts_non_utf8_text_in_python_patch_bodies() {
    let repo = FixtureRepo::new();
    repo.write("pkg/encoded.py", b"one\n\xff\nthree\n");
    repo.commit_all("initial");
    repo.write("pkg/encoded.py", b"one\n\xfe\nthree\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/encoded.py"),
            vec![LineRange { start: 2, end: 2 }],
        )])
    );
}

#[tokio::test]
async fn tracks_renamed_python_destination() {
    let repo = FixtureRepo::new();
    repo.write("pkg/old.py", "one\ntwo\nthree\nfour\nfive\n");
    repo.commit_all("initial");
    repo.git(&["mv", "pkg/old.py", "pkg/new.py"]);
    repo.write("pkg/new.py", "one\nTWO\nthree\nfour\nfive\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/new.py"),
            vec![LineRange { start: 2, end: 2 }],
        )])
    );
}

#[tokio::test]
async fn changed_pins_diff_format_and_rename_detection() {
    let repo = FixtureRepo::new();
    repo.write("pkg/old.py", "one\ntwo\nthree\nfour\nfive\n");
    repo.commit_all("initial");
    repo.git(&["config", "color.ui", "always"]);
    repo.git(&["config", "diff.noprefix", "true"]);
    repo.git(&["config", "diff.renames", "false"]);
    repo.git(&["mv", "pkg/old.py", "pkg/new.py"]);
    repo.write("pkg/new.py", "one\nTWO\nthree\nfour\nfive\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/new.py"),
            vec![LineRange { start: 2, end: 2 }],
        )])
    );
}

#[tokio::test]
async fn changed_pins_hunk_boundaries_under_hostile_config() {
    let repo = FixtureRepo::new();
    repo.write(
        "pkg/a.py",
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    repo.commit_all("initial");
    repo.git(&["config", "diff.interHunkContext", "100"]);
    repo.git(&["config", "diff.algorithm", "histogram"]);
    repo.git(&["config", "diff.indentHeuristic", "true"]);
    repo.git(&["config", "diff.renameLimit", "1"]);
    repo.write(
        "pkg/a.py",
        "one\nTWO\nthree\nfour\nfive\nsix\nseven\nEIGHT\nnine\nten\n",
    );

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/a.py"),
            vec![
                LineRange { start: 2, end: 2 },
                LineRange { start: 8, end: 8 },
            ],
        )])
    );
}

#[tokio::test]
async fn rejects_non_repository() {
    let root = tempfile::tempdir().unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(root.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        changed: true,
        ..Selection::default()
    };

    assert_eq!(
        TargetHandler::resolve(&selection).await,
        Err(TargetError::GitRepositoryRequired)
    );
}

#[tokio::test]
async fn rejects_bare_repository_as_git_repository_required() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new("git")
        .args(["init", "--bare", "--quiet"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(root.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::from("pkg")],
        changed: true,
        ..Selection::default()
    };

    assert_eq!(
        TargetHandler::resolve(&selection).await,
        Err(TargetError::GitRepositoryRequired)
    );
}

#[tokio::test]
async fn combined_handler_preserves_effect_id_and_resolves_changed_targets() {
    let repo = FixtureRepo::new();
    repo.write("pkg/a.py", "one\ntwo\nthree\n");
    repo.commit_all("initial");
    repo.write("pkg/a.py", "one\nTWO\nthree\n");

    let resolved = TargetHandler::handle(ResolveTargets {
        id: EffectId(41),
        selection: Selection {
            root: repo.root(),
            sources: vec![Utf8PathBuf::from("pkg")],
            changed: true,
            ..Selection::default()
        },
    })
    .await
    .unwrap();

    assert_eq!(resolved.id, EffectId(41));
    assert_eq!(
        resolved.targets,
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/a.py"),
            lines: vec![LineRange { start: 2, end: 2 }],
            symbols: Vec::new(),
        }]
    );
}
