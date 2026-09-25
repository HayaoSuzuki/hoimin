use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use camino::Utf8PathBuf;
use hoimin_cli::target::TargetHandler;
use hoimin_cli::target::git::{GitChangesResolved, ResolveGitChanges, handle_git};
use hoimin_core::{
    EffectId, LineRange, LineSelection, ResolveTargets, Selection, TargetError, TargetSlice,
};
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

#[cfg(unix)]
#[test]
fn explicit_discovery_rejects_a_literal_backslash_path() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(r"foo\bar.py"), "x = 1\n").unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::new()],
        ..Selection::default()
    };

    let error = hoimin_cli::target::fs::discover_explicit(&selection).unwrap_err();

    assert!(error.to_string().contains(r"foo\bar.py"), "{error}");
}

#[cfg(unix)]
#[test]
fn explicit_discovery_rejects_before_a_backslash_path_can_collide() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(r"foo\bar.py"), "literal = 1\n").unwrap();
    fs::create_dir(temp.path().join("foo")).unwrap();
    fs::write(temp.path().join("foo/bar.py"), "nested = 1\n").unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        sources: vec![Utf8PathBuf::new()],
        ..Selection::default()
    };

    let error = hoimin_cli::target::fs::discover_explicit(&selection).unwrap_err();

    assert!(error.to_string().contains(r"foo\bar.py"), "{error}");
}

#[cfg(unix)]
#[test]
fn exact_file_discovery_does_not_diagnose_an_unrelated_backslash_path() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("calc.py"), "value = 1\n").unwrap();
    fs::write(temp.path().join(r"unrelated\fixture.py"), "value = 2\n").unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        files: vec!["calc.py".into()],
        ..Selection::default()
    };

    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "calc.py");
}

#[cfg(unix)]
#[test]
fn exact_file_discovery_still_diagnoses_a_selected_backslash_path() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(r"selected\fixture.py"), "value = 1\n").unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        files: vec![Utf8PathBuf::from(r"selected\fixture.py")],
        ..Selection::default()
    };

    let error = hoimin_cli::target::fs::discover_explicit(&selection).unwrap_err();

    assert!(
        error.to_string().contains(r"selected\fixture.py"),
        "{error}"
    );
}

#[test]
fn exact_file_include_restores_only_the_requested_ignored_file() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".gitignore"), "*.py\n").unwrap();
    fs::write(temp.path().join("calc.py"), "value = 1\n").unwrap();
    fs::write(temp.path().join("other.py"), "value = 2\n").unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        files: vec!["calc.py".into()],
        includes: vec!["calc.py".into()],
        ..Selection::default()
    };

    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();

    assert_eq!(files, [hoimin_core::DiscoveredFile::python("calc.py")]);
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
async fn file_and_line_narrow_one_real_discovered_target() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("pkg")).unwrap();
    fs::write(temp.path().join("pkg/a.py"), "first\nsecond\nthird\n").unwrap();
    fs::write(temp.path().join("pkg/other.py"), "other = True\n").unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();

    let targets = TargetHandler::resolve(&Selection {
        root,
        files: vec![Utf8PathBuf::from("pkg/./a.py")],
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("pkg/a.py"),
            range: LineRange { start: 2, end: 3 },
        }],
        ..Selection::default()
    })
    .await
    .unwrap();

    assert_eq!(
        targets,
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/a.py"),
            lines: vec![LineRange { start: 2, end: 3 }],
            symbols: Vec::new(),
        }]
    );
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
async fn changed_root_source_selects_changed_python_lines() {
    let repo = FixtureRepo::new();
    repo.write("root.py", "root_one\nroot_two\nroot_three\n");
    repo.write(
        "pkg/nested.py",
        "nested_one\nnested_two\nnested_three\nnested_four\n",
    );
    repo.commit_all("initial");

    repo.write("root.py", "root_one\nROOT_TWO\nroot_three\n");
    repo.write(
        "pkg/nested.py",
        "nested_one\nnested_two\nnested_three\nNESTED_FOUR\n",
    );

    let targets = TargetHandler::resolve(&Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from(".")],
        changed: true,
        ..Selection::default()
    })
    .await
    .unwrap();

    assert_eq!(
        targets,
        vec![
            TargetSlice {
                path: Utf8PathBuf::from("pkg/nested.py"),
                lines: vec![LineRange { start: 4, end: 4 }],
                symbols: Vec::new(),
            },
            TargetSlice {
                path: Utf8PathBuf::from("root.py"),
                lines: vec![LineRange { start: 2, end: 2 }],
                symbols: Vec::new(),
            },
        ]
    );
}

#[cfg(windows)]
#[tokio::test]
async fn changed_selection_matches_git_index_case_to_discovered_path() {
    let repo = FixtureRepo::new();
    repo.write("Src/App.py", "one\ntwo\nthree\n");
    repo.commit_all("initial");
    fs::rename(
        repo.temp.path().join("Src"),
        repo.temp.path().join("case-transition"),
    )
    .unwrap();
    fs::rename(
        repo.temp.path().join("case-transition"),
        repo.temp.path().join("src"),
    )
    .unwrap();
    repo.write("src/App.py", "one\nchanged two\nthree\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("Src/App.py"),
            vec![LineRange { start: 2, end: 2 }],
        )])
    );

    let targets = TargetHandler::resolve(&Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from("src")],
        changed: true,
        ..Selection::default()
    })
    .await
    .unwrap();

    assert_eq!(
        targets,
        vec![TargetSlice {
            path: Utf8PathBuf::from("src/App.py"),
            lines: vec![LineRange { start: 2, end: 2 }],
            symbols: Vec::new(),
        }]
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

#[cfg(unix)]
#[tokio::test]
async fn git_handler_rejects_an_untracked_literal_backslash_path() {
    let repo = FixtureRepo::new();
    repo.write(r"literal\calc.py", "one\ntwo\n");

    let error = handle_git(ResolveGitChanges {
        id: EffectId(7),
        root: repo.root(),
        diff_base: None,
    })
    .await
    .unwrap_err();

    assert_eq!(error.failure.code(), "target.git");
    assert!(
        error.failure.message().contains(r"literal\calc.py"),
        "{:?}",
        error.failure
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

#[cfg(unix)]
#[tokio::test]
async fn standalone_git_skips_an_untracked_python_self_link() {
    let repo = FixtureRepo::new();
    repo.write("pkg/regular.py", "one\ntwo\n");
    std::os::unix::fs::symlink("self.py", repo.temp.path().join("pkg/self.py")).unwrap();

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/regular.py"),
            vec![LineRange { start: 1, end: 2 }],
        )])
    );
}

#[cfg(unix)]
#[tokio::test]
async fn standalone_git_skips_an_untracked_link_to_an_external_regular_python_file() {
    let repo = FixtureRepo::new();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.py"), "outside\n").unwrap();
    repo.write("pkg/regular.py", "one\ntwo\n");
    std::os::unix::fs::symlink(
        external.path().join("outside.py"),
        repo.temp.path().join("pkg/external.py"),
    )
    .unwrap();

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/regular.py"),
            vec![LineRange { start: 1, end: 2 }],
        )])
    );
}

#[cfg(unix)]
#[tokio::test]
async fn standalone_git_skips_an_indexed_unborn_path_replaced_by_an_external_link() {
    let repo = FixtureRepo::new();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.py"), "outside\n").unwrap();
    repo.write("pkg/indexed.py", "one\ntwo\n");
    repo.git(&["add", "pkg/indexed.py"]);
    repo.remove("pkg/indexed.py");
    std::os::unix::fs::symlink(
        external.path().join("outside.py"),
        repo.temp.path().join("pkg/indexed.py"),
    )
    .unwrap();

    assert!(repo.changed_lines(None).await.changed.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn changed_target_skips_an_excluded_untracked_python_self_link() {
    let repo = FixtureRepo::new();
    repo.write("pkg/selected.py", "one\ntwo\n");
    std::os::unix::fs::symlink("excluded.py", repo.temp.path().join("pkg/excluded.py")).unwrap();

    let targets = TargetHandler::resolve(&Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from("pkg")],
        excludes: vec!["pkg/excluded.py".into()],
        changed: true,
        ..Selection::default()
    })
    .await
    .unwrap();

    assert_eq!(
        targets,
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/selected.py"),
            lines: vec![LineRange { start: 1, end: 2 }],
            symbols: Vec::new(),
        }]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn changed_target_skips_an_excluded_unreadable_regular_file() {
    use std::os::unix::fs::PermissionsExt;

    let repo = FixtureRepo::new();
    repo.write("pkg/selected.py", "one\ntwo\n");
    repo.write("pkg/excluded.py", "one\ntwo\n");
    let excluded = repo.temp.path().join("pkg/excluded.py");
    fs::set_permissions(&excluded, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&excluded).is_ok() {
        return;
    }

    let targets = TargetHandler::resolve(&Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from("pkg")],
        excludes: vec!["pkg/excluded.py".into()],
        changed: true,
        ..Selection::default()
    })
    .await
    .unwrap();

    assert_eq!(
        targets,
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/selected.py"),
            lines: vec![LineRange { start: 1, end: 2 }],
            symbols: Vec::new(),
        }]
    );
    let error = handle_git(ResolveGitChanges {
        id: EffectId(8),
        root: repo.root(),
        diff_base: None,
    })
    .await
    .unwrap_err();
    assert_eq!(error.failure.code(), "target.git");
}

#[tokio::test]
async fn changed_target_with_an_empty_explicit_scope_does_not_read_untracked_files() {
    let repo = FixtureRepo::new();
    repo.write("pkg/excluded.py", "one\ntwo\n");

    let targets = TargetHandler::resolve(&Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from("pkg")],
        excludes: vec!["pkg/excluded.py".into()],
        changed: true,
        ..Selection::default()
    })
    .await
    .unwrap();

    assert!(targets.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn changed_target_with_only_an_excluded_unreadable_file_skips_current_file_reads() {
    use std::os::unix::fs::PermissionsExt;

    let repo = FixtureRepo::new();
    repo.write("pkg/excluded.py", "one\ntwo\n");
    let excluded = repo.temp.path().join("pkg/excluded.py");
    fs::set_permissions(&excluded, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&excluded).is_ok() {
        return;
    }

    let targets = TargetHandler::resolve(&Selection {
        root: repo.root(),
        sources: vec![Utf8PathBuf::from("pkg")],
        excludes: vec!["pkg/excluded.py".into()],
        changed: true,
        ..Selection::default()
    })
    .await
    .unwrap();

    assert!(targets.is_empty());
}

#[tokio::test]
async fn standalone_git_skips_an_indexed_unborn_path_replaced_by_a_directory() {
    let repo = FixtureRepo::new();
    repo.write("pkg/indexed.py", "one\ntwo\n");
    repo.git(&["add", "pkg/indexed.py"]);
    repo.remove("pkg/indexed.py");
    fs::create_dir(repo.temp.path().join("pkg/indexed.py")).unwrap();

    assert!(repo.changed_lines(None).await.changed.is_empty());
}

#[tokio::test]
async fn standalone_git_skips_an_indexed_path_when_its_parent_disappears() {
    let repo = FixtureRepo::new();
    repo.write("pkg/missing.py", "one\ntwo\n");
    repo.git(&["add", "pkg/missing.py"]);
    repo.remove("pkg/missing.py");
    fs::remove_dir(repo.temp.path().join("pkg")).unwrap();

    assert!(repo.changed_lines(None).await.changed.is_empty());
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
async fn binary_path_with_and_does_not_exclude_an_unrelated_text_path() {
    let repo = FixtureRepo::new();
    repo.write("pkg/x and y.py", b"before\0bytes\n");
    repo.write("y.py", "one\ntwo\n");
    repo.commit_all("initial");
    repo.write("pkg/x and y.py", b"after\0bytes\n");
    repo.write("y.py", "one\nTWO\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("y.py"),
            vec![LineRange { start: 2, end: 2 }],
        )])
    );
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
async fn changed_content_cannot_replace_the_diff_destination() {
    let repo = FixtureRepo::new();
    repo.write(
        "pkg/doc.py",
        "one\nold two\nthree\nfour\nfive\nsix\nseven\nold eight\n",
    );
    repo.write("pkg/evil.py", "unchanged\n");
    repo.commit_all("initial");
    repo.write(
        "pkg/doc.py",
        "one\n++ b/pkg/evil.py\nthree\nfour\nfive\nsix\nseven\nnew eight\n",
    );

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([(
            Utf8PathBuf::from("pkg/doc.py"),
            vec![
                LineRange { start: 2, end: 2 },
                LineRange { start: 8, end: 8 },
            ],
        )])
    );
}

#[tokio::test]
async fn changed_content_cannot_exclude_another_python_file() {
    let repo = FixtureRepo::new();
    repo.write(
        "pkg/decoy.py",
        "one\n-- a/pkg/innocent.py\nthree\nfour\nfive\n",
    );
    repo.write("pkg/innocent.py", "one\ntwo\nthree\n");
    repo.commit_all("initial");
    repo.write("pkg/decoy.py", "one\n++ /dev/null\nthree\nfour\nfive\n");
    repo.write("pkg/innocent.py", "one\nTWO\nthree\n");

    assert_eq!(
        repo.changed_lines(None).await.changed,
        BTreeMap::from([
            (
                Utf8PathBuf::from("pkg/decoy.py"),
                vec![LineRange { start: 2, end: 2 }],
            ),
            (
                Utf8PathBuf::from("pkg/innocent.py"),
                vec![LineRange { start: 2, end: 2 }],
            ),
        ])
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

#[test]
fn automatic_discovery_shares_workspace_exclusions_even_with_includes() {
    let temp = tempfile::tempdir().unwrap();
    let paths = [
        "calc.py",
        "venv/dep.py",
        "nested/env/dep.py",
        "__pycache__/dep.py",
        ".venv/dep.py",
        ".pytest_cache/dep.py",
    ];
    for path in paths {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "value = 1 + 2\n").unwrap();
    }
    for includes in [vec![], vec!["**/*.py".to_owned()]] {
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
            sources: vec![Utf8PathBuf::from(".")],
            includes,
            ..Selection::default()
        };
        let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
        // pins: issue #452 — candidates must be present in the worker manifest.
        assert_eq!(
            files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["calc.py"]
        );
    }
}

#[tokio::test]
async fn explicit_excluded_targets_explain_the_path_and_remediation() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("venv")).unwrap();
    fs::write(temp.path().join("venv/dep.py"), "value = 1 + 2\n").unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    for files in [true, false] {
        let mut selection = Selection {
            root: root.clone(),
            ..Selection::default()
        };
        if files {
            selection.files.push(root.join("venv/./dep.py"));
        } else {
            selection.lines.push(LineSelection {
                path: "venv/dep.py".into(),
                range: LineRange { start: 1, end: 1 },
            });
        }
        let error = TargetHandler::resolve(&selection)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("venv/dep.py"), "{error}");
        assert!(error.contains("built-in"), "{error}");
        assert!(error.contains("outside"), "{error}");
    }
}

#[test]
fn non_git_discovery_honors_ignore_files_and_restores_only_eligible_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".gitignore"), "ignored.py\n").unwrap();
    for name in ["calc.py", "ignored.py", ".hidden.py"] {
        fs::write(temp.path().join(name), "value = 1 + 2\n").unwrap();
    }
    let mut selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
        ..Selection::default()
    };
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert_eq!(
        files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["calc.py"]
    );
    selection.includes = vec!["ignored.py".into(), ".hidden.py".into()];
    selection.excludes = vec!["ignored.py".into()];
    let files = hoimin_cli::target::fs::discover_explicit(&selection).unwrap();
    assert_eq!(
        files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [".hidden.py", "calc.py"]
    );
}

#[tokio::test]
async fn a_root_named_venv_is_not_itself_excluded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("venv");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("calc.py"), "value = 1 + 2\n").unwrap();
    let targets = TargetHandler::resolve(&Selection {
        root: Utf8PathBuf::from_path_buf(root).unwrap(),
        files: vec!["calc.py".into()],
        ..Selection::default()
    })
    .await
    .unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "calc.py");
}

#[tokio::test]
async fn exclusion_case_rules_follow_the_platform_for_walks_and_selectors() {
    for actual in ["venv", "VENV"] {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(actual)).unwrap();
        fs::write(temp.path().join(actual).join("dep.py"), "value = 1 + 2\n").unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        let files = hoimin_cli::target::fs::discover_explicit(&Selection {
            root: root.clone(),
            includes: vec!["**/*.py".into()],
            ..Selection::default()
        })
        .unwrap();
        let excluded = cfg!(windows) || actual == "venv";
        assert_eq!(files.is_empty(), excluded);
        let spellings: &[&str] = if cfg!(windows) {
            &["venv", "VENV"]
        } else {
            &[actual]
        };
        for spelling in spellings {
            let result = TargetHandler::resolve(&Selection {
                root: root.clone(),
                files: vec![format!("{spelling}/dep.py").into()],
                ..Selection::default()
            })
            .await;
            if excluded {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains("built-in") && error.contains("outside"),
                    "{error}"
                );
            } else {
                assert_eq!(result.unwrap().len(), 1);
            }
        }
    }
}

#[tokio::test]
async fn changed_context_expands_current_lines_and_intersects_explicit_ranges() {
    for (before, after, context, expected) in [
        (
            "a = 1\nb = 2\nc = 3\nd = 4\n",
            "a = 1\nb = 20\nc = 3\nd = 4\n",
            0,
            vec![LineRange { start: 2, end: 2 }],
        ),
        (
            "a = 1\nb = 2\nc = 3\nd = 4\n",
            "a = 1\nb = 20\nc = 3\nd = 4\n",
            1,
            vec![LineRange { start: 1, end: 3 }],
        ),
        ("a = 1\nb = 2\nc = 3\n", "a = 1\nc = 3\n", 0, vec![]),
        (
            "a = 1\nb = 2\nc = 3\n",
            "a = 1\nc = 3\n",
            1,
            vec![LineRange { start: 1, end: 2 }],
        ),
        (
            "a = 1\nb = 2\nc = 3\n",
            "b = 2\nc = 3\n",
            1,
            vec![LineRange { start: 1, end: 1 }],
        ),
        (
            "a = 1\nb = 2\nc = 3\n",
            "a = 1\nb = 2\n",
            1,
            vec![LineRange { start: 2, end: 2 }],
        ),
        ("a = 1\n", "", 1, vec![]),
        (
            "a = 1\nb = 2\nc = 3\n",
            "a = 1\nb = 20\nc = 3\n",
            1_073_741_823,
            vec![LineRange { start: 1, end: 3 }],
        ),
    ] {
        let repo = FixtureRepo::new();
        repo.write("src/calc.py", before);
        repo.write("src/unchanged.py", "untouched = 1\n");
        repo.commit_all("initial");
        repo.write("src/calc.py", after);
        let mut value = serde_json::to_value(Selection {
            root: repo.root(),
            sources: vec!["src".into()],
            changed: true,
            ..Selection::default()
        })
        .unwrap();
        value["changed_context"] = serde_json::json!(context);
        let mut selection: Selection = serde_json::from_value(value).unwrap();
        let targets = TargetHandler::resolve(&selection).await.unwrap();
        if expected.is_empty() {
            assert!(targets.is_empty(), "context={context}: {targets:?}");
        } else {
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].path, "src/calc.py");
            assert_eq!(targets[0].lines, expected);
        }
        selection.lines = vec![LineSelection {
            path: "src/calc.py".into(),
            range: LineRange { start: 2, end: 2 },
        }];
        let restricted = TargetHandler::resolve(&selection).await.unwrap();
        let includes_second = expected.iter().any(|r| r.start <= 2 && r.end >= 2);
        if includes_second {
            assert_eq!(restricted[0].lines, vec![LineRange { start: 2, end: 2 }]);
        } else {
            assert!(restricted.is_empty());
        }
    }
}

#[tokio::test]
async fn changed_cr_rows_are_translated_before_explicit_python_line_intersection() {
    let repo = FixtureRepo::new();
    repo.write(
        "src/calc.py",
        b"# header\rdef f():\r    return 1 - 2\ndef g():\n    return 3 + 4\n",
    );
    repo.commit_all("initial");
    repo.write(
        "src/calc.py",
        b"# header\rdef f():\r    return 1 + 2\ndef g():\n    return 3 + 4\n",
    );
    assert_eq!(
        repo.changed_lines(None).await.changed[&Utf8PathBuf::from("src/calc.py")],
        [LineRange { start: 1, end: 3 }]
    );
    for (line, selected) in [(3, true), (5, false)] {
        let targets = TargetHandler::resolve(&Selection {
            root: repo.root(),
            sources: vec!["src".into()],
            changed: true,
            lines: vec![LineSelection {
                path: "src/calc.py".into(),
                range: LineRange {
                    start: line,
                    end: line,
                },
            }],
            ..Selection::default()
        })
        .await
        .unwrap();
        assert_eq!(!targets.is_empty(), selected);
        if selected {
            assert_eq!(
                targets[0].lines,
                [LineRange {
                    start: line,
                    end: line
                }]
            );
        }
    }
}

#[tokio::test]
async fn changed_cr_deletion_context_preserves_surviving_byte_coverage() {
    let repo = FixtureRepo::new();
    repo.write(
        "src/calc.py",
        b"left = 1 + 2\r# left\nremoved = 3\n# right\rright = 4 + 5\n",
    );
    repo.commit_all("initial");
    repo.write(
        "src/calc.py",
        b"left = 1 + 2\r# left\n# right\rright = 4 + 5\n",
    );
    for context in [0, 1] {
        let targets = TargetHandler::resolve(&Selection {
            root: repo.root(),
            sources: vec!["src".into()],
            changed: true,
            changed_context: context,
            ..Selection::default()
        })
        .await
        .unwrap();
        if context == 0 {
            assert!(targets.is_empty());
        } else {
            assert_eq!(targets[0].lines, [LineRange { start: 1, end: 4 }]);
        }
    }
}

#[tokio::test]
async fn changed_mixed_rows_preserve_rename_binary_and_text_attribute_behavior() {
    let repo = FixtureRepo::new();
    repo.write(".gitattributes", "*.py text eol=lf\n");
    let header = format!("# {}\r\n", "unchanged ".repeat(32));
    repo.write(
        "src/old.py",
        format!("{header}# longer unchanged comment\rdef f():\r    return 1 - 2\r\n"),
    );
    repo.write("src/binary.py", b"old\0binary\n");
    repo.commit_all("initial");
    repo.git(&["mv", "src/old.py", "src/new.py"]);
    repo.write(
        "src/new.py",
        format!("{header}# longer unchanged comment\rdef f():\r    return 1 + 2\r\n"),
    );
    repo.write("src/binary.py", b"new\0binary\n");
    let patch = repo.git(&["diff", "--find-renames", "-l0", "HEAD"]);
    assert!(
        patch.contains("rename from src/old.py"),
        "fixture premise: {patch}"
    );
    let changed = repo.changed_lines(None).await.changed;
    assert_eq!(changed.len(), 1);
    assert_eq!(
        changed[&Utf8PathBuf::from("src/new.py")],
        [LineRange { start: 2, end: 4 }]
    );
}
