use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_core::{
    ConfigError, DiscoveredFile, LineRange, LineSelection, RawRunConfig, RunConfig, Selection,
    SymbolSelection, TargetError, auto_mutant_timeout, resolve_explicit,
};

fn raw_config() -> RawRunConfig {
    RawRunConfig {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        test_argv: vec![hoimin_core::CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    }
}

#[test]
fn rejects_missing_selector() {
    let mut raw = raw_config();
    raw.files.clear();
    assert_eq!(RunConfig::try_from(raw), Err(ConfigError::MissingSelector));
}

#[test]
fn diff_base_without_changed_is_rejected() {
    let mut raw = raw_config();
    raw.diff_base = Some("main".into());
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::DiffBaseRequiresChanged)
    );
}

#[test]
fn changed_requires_source() {
    let mut raw = raw_config();
    raw.changed = true;
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::ChangedRequiresSource)
    );
}

#[test]
fn symbol_requires_source() {
    let mut raw = raw_config();
    raw.files.clear();
    raw.symbols.push(SymbolSelection {
        module: "pkg.a".into(),
        qualname: "run".into(),
    });
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::SymbolRequiresSource)
    );
}

#[test]
fn resume_requires_session() {
    let mut raw = raw_config();
    raw.resume = true;
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::ResumeRequiresSession)
    );
}

#[test]
fn rejects_zero_or_overflow_limit() {
    let mut raw = raw_config();
    raw.limits.jobs = 0;
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::InvalidLimit("jobs"))
    );

    let mut raw = raw_config();
    raw.limits.baseline_timeout = Duration::MAX;
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::InvalidLimit("baseline_timeout"))
    );
}

#[test]
fn automatic_mutant_timeout_uses_baseline_duration() {
    assert_eq!(
        auto_mutant_timeout(Duration::from_secs(1)),
        Duration::from_secs(5)
    );
    assert_eq!(
        auto_mutant_timeout(Duration::from_secs(8)),
        Duration::from_secs(17)
    );
}

#[test]
fn explicit_selectors_form_a_union_and_are_normalized() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("pkg/b.py"),
            range: LineRange { start: 4, end: 7 },
        }],
        ..Selection::default()
    };
    let discovered = [
        DiscoveredFile::python("pkg/c.py"),
        DiscoveredFile::python("pkg/b.py"),
        DiscoveredFile::python("pkg/a.py"),
    ];
    let targets = resolve_explicit(&selection, &discovered).unwrap();

    assert_eq!(
        targets
            .iter()
            .map(|target| target.path.as_str())
            .collect::<Vec<_>>(),
        ["pkg/a.py", "pkg/b.py", "pkg/c.py"]
    );
    assert_eq!(targets[1].lines, [LineRange { start: 4, end: 7 }]);
}

#[test]
fn whole_file_union_wins_over_line_on_the_same_path() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("pkg/a.py"),
            range: LineRange { start: 4, end: 7 },
        }],
        ..Selection::default()
    };
    let targets = resolve_explicit(&selection, &[DiscoveredFile::python("pkg/a.py")]).unwrap();
    assert!(targets[0].lines.is_empty());
}

#[test]
fn rejects_path_outside_root() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("../outside.py")],
        ..Selection::default()
    };
    assert_eq!(
        resolve_explicit(&selection, &[]),
        Err(TargetError::PathOutsideRoot(Utf8PathBuf::from(
            "../outside.py"
        )))
    );
}

#[test]
fn rejects_file_outside_source() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        files: vec![Utf8PathBuf::from("tests/test_a.py")],
        ..Selection::default()
    };
    let discovered = [DiscoveredFile::python("tests/test_a.py")];
    assert_eq!(
        resolve_explicit(&selection, &discovered),
        Err(TargetError::FileOutsideSource(Utf8PathBuf::from(
            "tests/test_a.py"
        )))
    );
}

#[test]
fn rejects_invalid_line_range() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("pkg/a.py"),
            range: LineRange { start: 7, end: 4 },
        }],
        ..Selection::default()
    };
    let discovered = [DiscoveredFile::python("pkg/a.py")];
    assert_eq!(
        resolve_explicit(&selection, &discovered),
        Err(TargetError::InvalidLineRange(LineRange {
            start: 7,
            end: 4
        }))
    );
}

#[test]
fn rejects_missing_or_non_python_file() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.txt")],
        ..Selection::default()
    };
    assert_eq!(
        resolve_explicit(&selection, &[DiscoveredFile::regular("pkg/a.txt")]),
        Err(TargetError::MissingOrNonPythonFile(Utf8PathBuf::from(
            "pkg/a.txt"
        )))
    );
}

#[test]
fn explicit_exclude_wins_over_include() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        includes: vec!["pkg/generated.py".into()],
        excludes: vec!["pkg/generated.py".into()],
        ..Selection::default()
    };
    let discovered = [
        DiscoveredFile::python("pkg/a.py"),
        DiscoveredFile::python("pkg/generated.py"),
    ];
    let targets = resolve_explicit(&selection, &discovered).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "pkg/a.py");
}

#[cfg(windows)]
#[test]
fn windows_paths_use_mixed_slashes_and_case_insensitive_containment() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("PKG")],
        files: vec![Utf8PathBuf::from(r"PKG\A.py")],
        ..Selection::default()
    };
    let targets = resolve_explicit(&selection, &[DiscoveredFile::python("pkg/a.py")]).unwrap();
    assert_eq!(targets[0].path, "pkg/a.py");
}

#[cfg(windows)]
#[test]
fn windows_absolute_selector_strips_root_case_insensitively() {
    let selection = Selection {
        root: Utf8PathBuf::from("C:/Project"),
        files: vec![Utf8PathBuf::from("c:/project/pkg/a.py")],
        ..Selection::default()
    };
    let targets = resolve_explicit(&selection, &[DiscoveredFile::python("pkg/a.py")]).unwrap();
    assert_eq!(targets[0].path, "pkg/a.py");
}

#[cfg(windows)]
#[test]
fn windows_non_ascii_path_outside_root_returns_typed_error() {
    let selection = Selection {
        root: Utf8PathBuf::from("C:/a"),
        files: vec![Utf8PathBuf::from("C:/€x/file.py")],
        ..Selection::default()
    };
    assert_eq!(
        resolve_explicit(&selection, &[]),
        Err(TargetError::PathOutsideRoot(Utf8PathBuf::from(
            "C:/€x/file.py"
        )))
    );
}

#[cfg(windows)]
#[test]
fn windows_unicode_case_variants_resolve_under_root() {
    let selection = Selection {
        root: Utf8PathBuf::from("C:/Ä"),
        sources: vec![Utf8PathBuf::from("PKG")],
        files: vec![Utf8PathBuf::from("c:/ä/pkg/a.py")],
        ..Selection::default()
    };
    let targets = resolve_explicit(&selection, &[DiscoveredFile::python("pkg/a.py")]).unwrap();
    assert_eq!(targets[0].path, "pkg/a.py");
}

#[cfg(windows)]
#[test]
fn windows_final_sigma_uses_uppercase_case_equivalence() {
    let selection = Selection {
        root: Utf8PathBuf::from("C:/Σ"),
        files: vec![Utf8PathBuf::from("c:/ς/pkg/a.py")],
        ..Selection::default()
    };
    let targets = resolve_explicit(&selection, &[DiscoveredFile::python("pkg/a.py")]).unwrap();
    assert_eq!(targets[0].path, "pkg/a.py");
}

#[cfg(windows)]
#[test]
fn windows_multi_character_uppercase_does_not_overmatch_root() {
    let selection = Selection {
        root: Utf8PathBuf::from("C:/ß"),
        files: vec![Utf8PathBuf::from("C:/SS/file.py")],
        ..Selection::default()
    };
    assert_eq!(
        resolve_explicit(&selection, &[]),
        Err(TargetError::PathOutsideRoot(Utf8PathBuf::from(
            "C:/SS/file.py"
        )))
    );
}
