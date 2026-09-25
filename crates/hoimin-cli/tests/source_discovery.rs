use std::fs;

use camino::Utf8PathBuf;
use hoimin_cli::target::fs::discover_explicit;
use hoimin_core::{LineRange, LineSelection, Selection, SymbolSelection, resolve_explicit};

fn fixture() -> (tempfile::TempDir, Selection) {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "alpha/mod.py",
        "alpha/nested/deep.py",
        "alpha/data.txt",
        "alpha/ignored/restored.py",
        "alpha/blocked/drop.py",
        "alpha/.venv/private.py",
        "alpha_extra/out.py",
        "beta/mod.py",
        "beta/.hidden.py",
    ] {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "def f():\n    return 1 + 2\n").unwrap();
    }
    fs::create_dir(temp.path().join("empty")).unwrap();
    fs::write(temp.path().join(".gitignore"), "alpha/ignored/\n").unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
        excludes: vec!["alpha/blocked/**".into()],
        ..Selection::default()
    };
    (temp, selection)
}

fn assert_broad_correspondence(
    selection: &Selection,
) -> Result<Vec<hoimin_core::TargetSlice>, hoimin_core::TargetError> {
    let scoped = discover_explicit(selection).unwrap();
    let broad = discover_explicit(&Selection {
        sources: vec![".".into()],
        files: vec![],
        lines: vec![],
        symbols: vec![],
        ..selection.clone()
    })
    .unwrap();
    let expected = resolve_explicit(selection, &broad);
    let actual = resolve_explicit(selection, &scoped);
    assert_eq!(actual, expected);
    actual
}

#[test]
fn source_unions_normalization_and_mixed_selectors_match_broad_discovery() {
    let (_temp, base) = fixture();
    for (sources, expected) in [
        (vec!["alpha"], vec!["alpha/mod.py", "alpha/nested/deep.py"]),
        (
            vec!["alpha", "alpha/nested"],
            vec!["alpha/mod.py", "alpha/nested/deep.py"],
        ),
        (
            vec!["./alpha/unused/..", "beta"],
            vec!["alpha/mod.py", "alpha/nested/deep.py", "beta/mod.py"],
        ),
        (vec!["alpha/mod.py"], vec!["alpha/mod.py"]),
        (vec!["empty"], vec![]),
    ] {
        let mut selection = Selection {
            sources: sources.into_iter().map(Into::into).collect(),
            ..base.clone()
        };
        let targets = assert_broad_correspondence(&selection).unwrap();
        assert_eq!(
            targets
                .iter()
                .map(|target| target.path.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        if expected.contains(&"alpha/mod.py") {
            selection.files = vec!["alpha/mod.py".into()];
            selection.lines = vec![LineSelection {
                path: "alpha/mod.py".into(),
                range: LineRange { start: 1, end: 2 },
            }];
            let mixed = assert_broad_correspondence(&selection).unwrap();
            assert_eq!(mixed[0].lines, [LineRange { start: 1, end: 2 }]);
        }
    }
    for source in [
        base.root.join("alpha"),
        base.root.clone(),
        ".".into(),
        "".into(),
    ] {
        let selection = Selection {
            sources: vec![source],
            ..base.clone()
        };
        assert!(!assert_broad_correspondence(&selection).unwrap().is_empty());
    }
    let outside = Selection {
        sources: vec!["alpha".into()],
        files: vec!["beta/mod.py".into()],
        ..base
    };
    assert!(assert_broad_correspondence(&outside).is_err());
    assert!(matches!(
        resolve_explicit(&outside, &discover_explicit(&outside).unwrap()),
        Err(hoimin_core::TargetError::FileOutsideSource(_))
    ));
}

#[test]
fn include_exclude_and_source_order_preserve_symbol_resolution() {
    let (_temp, base) = fixture();
    for (sources, symbol_path) in [
        (["alpha", "beta"], "alpha/mod.py"),
        (["beta", "alpha"], "beta/mod.py"),
    ] {
        let selection = Selection {
            sources: sources.into_iter().map(Into::into).collect(),
            symbols: vec![SymbolSelection {
                module: "mod".into(),
                qualname: "f".into(),
            }],
            includes: vec![
                "alpha/ignored/**".into(),
                "alpha_extra/**".into(),
                "alpha/blocked/**".into(),
                "alpha/.venv/**".into(),
                "beta/.hidden.py".into(),
            ],
            ..base.clone()
        };
        let targets = assert_broad_correspondence(&selection).unwrap();
        assert_eq!(
            targets
                .iter()
                .filter(|target| !target.symbols.is_empty())
                .map(|target| target.path.as_str())
                .collect::<Vec<_>>(),
            [symbol_path]
        );
        assert_eq!(
            targets
                .iter()
                .map(|target| target.path.as_str())
                .collect::<Vec<_>>(),
            [
                "alpha/ignored/restored.py",
                "alpha/mod.py",
                "alpha/nested/deep.py",
                "beta/.hidden.py",
                "beta/mod.py"
            ]
        );
    }
}

#[test]
fn source_subtree_case_rules_match_the_platform() {
    let (_temp, base) = fixture();
    let selection = Selection {
        sources: vec!["ALPHA".into()],
        ..base
    };
    let targets = assert_broad_correspondence(&selection).unwrap();
    assert_eq!(targets.len(), if cfg!(windows) { 2 } else { 0 });
}

#[cfg(unix)]
#[test]
fn malformed_paths_are_diagnosed_only_in_the_selected_source_scope() {
    use std::os::unix::ffi::OsStringExt;
    // APFS rejects non-UTF-8 directory entry names before discovery can observe them.
    let names = [
        Some(std::ffi::OsString::from(r"bad\name.py")),
        cfg!(not(target_os = "macos"))
            .then(|| std::ffi::OsString::from_vec(b"bad\xff.py".to_vec())),
    ];
    for name in names.into_iter().flatten() {
        for selected in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir_all(temp.path().join("selected")).unwrap();
            fs::create_dir_all(temp.path().join("unrelated")).unwrap();
            fs::write(temp.path().join("selected/app.py"), "x = 1\n").unwrap();
            let parent = if selected { "selected" } else { "unrelated" };
            fs::write(temp.path().join(parent).join(&name), "x = 2\n").unwrap();
            let selection = Selection {
                root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
                sources: vec!["selected".into()],
                ..Selection::default()
            };
            let result = discover_explicit(&selection);
            if selected {
                assert!(result.is_err());
            } else {
                assert_eq!(
                    result.unwrap(),
                    [hoimin_core::DiscoveredFile::python("selected/app.py")]
                );
            }
            assert!(
                discover_explicit(&Selection {
                    sources: vec![".".into()],
                    ..selection
                })
                .is_err()
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn source_discovery_does_not_follow_links() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("selected")).unwrap();
    fs::write(temp.path().join("selected/app.py"), "x = 1\n").unwrap();
    fs::write(outside.path().join("outside.py"), "x = 2\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("selected/link")).unwrap();
    let selection = Selection {
        root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
        sources: vec!["selected".into()],
        includes: vec!["selected/**".into()],
        ..Selection::default()
    };
    assert_eq!(
        discover_explicit(&selection).unwrap(),
        [hoimin_core::DiscoveredFile::python("selected/app.py")]
    );
}
