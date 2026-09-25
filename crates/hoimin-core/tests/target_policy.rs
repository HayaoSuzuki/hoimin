use std::collections::BTreeMap;
use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_core::{
    ConfigError, DiscoveredFile, LineRange, LineSelection, MAX_JOBS, MAX_TIMEOUT, RawRunConfig,
    RunConfig, Selection, SymbolSelection, TargetError, TargetSlice, auto_mutant_timeout,
    changed_is_normalized, intersect_changed, resolve_explicit, targets_are_normalized,
};
use hoimin_core::{MutationOperator, MutationProfile};

fn raw_config() -> RawRunConfig {
    RawRunConfig {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        test_argv: vec![hoimin_core::CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    }
}

#[test]
fn mutation_profile_defaults_to_full_and_is_normalized() {
    let raw = raw_config();
    assert_eq!(raw.profile, MutationProfile::Full);
    assert_eq!(
        RunConfig::try_from(raw.clone()).unwrap().profile,
        MutationProfile::Full
    );

    let mut raw = raw;
    raw.profile = MutationProfile::Focused;
    assert_eq!(
        RunConfig::try_from(raw).unwrap().profile,
        MutationProfile::Focused
    );
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

    let mut raw = raw_config();
    raw.limits.max_processes = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::InvalidLimit("max_processes"))
    );
}

#[test]
fn invalid_limit_messages_use_cli_flag_spellings() {
    for (internal, flag) in [
        ("jobs", "--jobs"),
        ("baseline_timeout", "--baseline-timeout"),
        ("max_memory", "--max-memory"),
    ] {
        let message = ConfigError::InvalidLimit(internal).to_string();
        assert!(message.contains(flag), "{message} does not mention {flag}");
    }
}

#[test]
fn rejects_jobs_before_any_jobs_sized_allocation_is_possible() {
    let mut raw = raw_config();
    raw.limits.jobs = MAX_JOBS + 1;
    raw.limits.max_processes = MAX_JOBS + 1;
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::JobsExceedsMaximum {
            jobs: MAX_JOBS + 1,
            maximum: MAX_JOBS,
        })
    );

    let mut raw = raw_config();
    raw.limits.jobs = 4;
    raw.limits.max_processes = 3;
    assert_eq!(
        RunConfig::try_from(raw),
        Err(ConfigError::JobsExceedsProcesses {
            jobs: 4,
            max_processes: 3,
        })
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
fn raw_timeout_limits_accept_the_ceiling_and_reject_one_nanosecond_more() {
    for name in [
        "analyzer_timeout",
        "baseline_timeout",
        "mutant_timeout",
        "total_timeout",
    ] {
        let mut accepted = raw_config();
        accepted.limits.mutant_timeout = Some(Duration::from_secs(5));
        match name {
            "analyzer_timeout" => accepted.limits.analyzer_timeout = MAX_TIMEOUT,
            "baseline_timeout" => accepted.limits.baseline_timeout = MAX_TIMEOUT,
            "mutant_timeout" => accepted.limits.mutant_timeout = Some(MAX_TIMEOUT),
            "total_timeout" => accepted.limits.total_timeout = MAX_TIMEOUT,
            _ => unreachable!(),
        }
        assert!(RunConfig::try_from(accepted).is_ok(), "accepted {name}");

        let mut rejected = raw_config();
        rejected.limits.mutant_timeout = Some(Duration::from_secs(5));
        let oversized = MAX_TIMEOUT + Duration::from_nanos(1);
        match name {
            "analyzer_timeout" => rejected.limits.analyzer_timeout = oversized,
            "baseline_timeout" => rejected.limits.baseline_timeout = oversized,
            "mutant_timeout" => rejected.limits.mutant_timeout = Some(oversized),
            "total_timeout" => rejected.limits.total_timeout = oversized,
            _ => unreachable!(),
        }
        assert_eq!(
            RunConfig::try_from(rejected),
            Err(ConfigError::InvalidLimit(name)),
            "rejected {name}"
        );
    }
}

#[test]
fn raw_auto_mutant_timeout_bounds_the_derived_deadline() {
    let maximum_baseline = MAX_TIMEOUT.checked_sub(Duration::from_secs(1)).unwrap() / 2;
    assert_eq!(auto_mutant_timeout(maximum_baseline), MAX_TIMEOUT);

    let mut accepted = raw_config();
    accepted.limits.baseline_timeout = maximum_baseline;
    accepted.limits.mutant_timeout = None;
    assert!(RunConfig::try_from(accepted).is_ok());

    let mut rejected = raw_config();
    rejected.limits.baseline_timeout = maximum_baseline + Duration::from_nanos(1);
    rejected.limits.mutant_timeout = None;
    assert_eq!(
        RunConfig::try_from(rejected),
        Err(ConfigError::InvalidLimit("baseline_timeout"))
    );
}

#[test]
fn maximum_timeout_is_supported_by_standard_deadlines() {
    assert!(std::time::Instant::now().checked_add(MAX_TIMEOUT).is_some());
}

#[test]
fn changed_normalization_rejects_empty_invalid_adjacent_and_overlapping_ranges() {
    let path = Utf8PathBuf::from("pkg/a.py");
    let changed = |ranges| BTreeMap::from([(path.clone(), ranges)]);

    assert!(changed_is_normalized(&changed(vec![
        LineRange { start: 1, end: 3 },
        LineRange { start: 5, end: 5 },
    ])));
    assert!(!changed_is_normalized(&changed(Vec::new())));
    assert!(!changed_is_normalized(&changed(vec![LineRange {
        start: 0,
        end: 3,
    }])));
    assert!(!changed_is_normalized(&changed(vec![LineRange {
        start: 4,
        end: 3,
    }])));
    assert!(!changed_is_normalized(&changed(vec![
        LineRange { start: 1, end: 3 },
        LineRange { start: 4, end: 5 },
    ])));
    assert!(!changed_is_normalized(&changed(vec![
        LineRange { start: 1, end: 3 },
        LineRange { start: 3, end: 5 },
    ])));
}

#[test]
fn target_normalization_requires_sorted_paths_and_valid_disjoint_ranges() {
    let target = |path, lines| TargetSlice {
        path: Utf8PathBuf::from(path),
        lines,
        symbols: Vec::new(),
    };

    assert!(targets_are_normalized(&[
        target("pkg/a.py", Vec::new()),
        target(
            "pkg/b.py",
            vec![
                LineRange { start: 1, end: 3 },
                LineRange { start: 4, end: 5 }
            ],
        ),
    ]));
    assert!(!targets_are_normalized(&[
        target("pkg/b.py", Vec::new()),
        target("pkg/a.py", Vec::new()),
    ]));
    assert!(!targets_are_normalized(&[
        target("pkg/a.py", Vec::new()),
        target("pkg/a.py", Vec::new()),
    ]));
    assert!(!targets_are_normalized(&[target(
        "pkg/a.py",
        vec![
            LineRange { start: 1, end: 3 },
            LineRange { start: 3, end: 5 }
        ],
    )]));
    assert!(!targets_are_normalized(&[target(
        "pkg/a.py",
        vec![LineRange { start: 0, end: 3 }],
    )]));
    assert!(!targets_are_normalized(&[target(
        "pkg/a.py",
        vec![LineRange { start: 4, end: 3 }],
    )]));
}

#[test]
fn whole_file_and_subfile_selectors_resolve_sorted_targets() {
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
    assert!(targets[0].lines.is_empty());
    assert_eq!(targets[1].lines, [LineRange { start: 4, end: 7 }]);
    assert!(targets[2].lines.is_empty());
}

#[test]
fn root_equal_source_selects_every_python_file() {
    let root = if cfg!(windows) {
        Utf8PathBuf::from("C:/project")
    } else {
        Utf8PathBuf::from("/project")
    };
    let discovered = [
        DiscoveredFile::regular("notes.txt"),
        DiscoveredFile::python("pkg/nested.py"),
        DiscoveredFile::python("root.py"),
    ];

    for source in [
        Utf8PathBuf::new(),
        Utf8PathBuf::from("."),
        Utf8PathBuf::from("./"),
        Utf8PathBuf::from("pkg/.."),
        root.clone(),
    ] {
        let selection = Selection {
            root: root.clone(),
            sources: vec![source.clone()],
            ..Selection::default()
        };

        let targets = resolve_explicit(&selection, &discovered).unwrap();
        assert_eq!(
            targets
                .iter()
                .map(|target| target.path.as_str())
                .collect::<Vec<_>>(),
            ["pkg/nested.py", "root.py"],
            "root-equivalent source {source:?} did not select the project"
        );
    }
}

#[test]
fn root_source_contains_root_relative_file_and_line_selectors() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from(".")],
        files: vec![Utf8PathBuf::from("root.py")],
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("pkg/nested.py"),
            range: LineRange { start: 2, end: 3 },
        }],
        ..Selection::default()
    };
    let discovered = [
        DiscoveredFile::python("pkg/nested.py"),
        DiscoveredFile::python("root.py"),
    ];

    let targets = resolve_explicit(&selection, &discovered).unwrap();
    assert_eq!(
        targets
            .iter()
            .map(|target| target.path.as_str())
            .collect::<Vec<_>>(),
        ["pkg/nested.py", "root.py"]
    );
}

#[test]
fn file_and_line_match_source_and_line_on_the_same_path() {
    let line = LineSelection {
        path: Utf8PathBuf::from("pkg/a.py"),
        range: LineRange { start: 4, end: 7 },
    };
    let file_selection = Selection {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/./a.py")],
        lines: vec![line.clone()],
        ..Selection::default()
    };
    let source_selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        lines: vec![line],
        ..Selection::default()
    };
    let discovered = [DiscoveredFile::python("pkg/a.py")];

    let from_file = resolve_explicit(&file_selection, &discovered).unwrap();
    let from_source = resolve_explicit(&source_selection, &discovered).unwrap();

    assert_eq!(from_file, from_source);
    assert_eq!(
        from_file,
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/a.py"),
            lines: vec![LineRange { start: 4, end: 7 }],
            symbols: Vec::new(),
        }]
    );
}

#[test]
fn file_and_symbol_match_source_and_symbol_on_the_same_path() {
    let symbol = SymbolSelection {
        module: "a".into(),
        qualname: "Widget.run".into(),
    };
    let file_selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        symbols: vec![symbol.clone()],
        ..Selection::default()
    };
    let source_selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        symbols: vec![symbol],
        ..Selection::default()
    };
    let discovered = [
        DiscoveredFile::python("pkg/b.py"),
        DiscoveredFile::python("pkg/a.py"),
    ];

    let from_file = resolve_explicit(&file_selection, &discovered).unwrap();
    let from_source = resolve_explicit(&source_selection, &discovered).unwrap();

    assert_eq!(from_file, from_source);
    assert_eq!(
        from_file,
        vec![
            TargetSlice {
                path: Utf8PathBuf::from("pkg/a.py"),
                lines: Vec::new(),
                symbols: vec!["Widget.run".into()],
            },
            TargetSlice {
                path: Utf8PathBuf::from("pkg/b.py"),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
        ]
    );
}

#[test]
fn duplicate_file_selectors_normalize_before_line_narrowing() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        files: vec![
            Utf8PathBuf::from("z.py"),
            Utf8PathBuf::from("pkg/../a.py"),
            Utf8PathBuf::from("./a.py"),
        ],
        lines: vec![
            LineSelection {
                path: Utf8PathBuf::from("a.py"),
                range: LineRange { start: 6, end: 7 },
            },
            LineSelection {
                path: Utf8PathBuf::from("./a.py"),
                range: LineRange { start: 2, end: 5 },
            },
        ],
        ..Selection::default()
    };
    let discovered = [
        DiscoveredFile::python("z.py"),
        DiscoveredFile::python("a.py"),
    ];

    let targets = resolve_explicit(&selection, &discovered).unwrap();

    assert_eq!(
        targets,
        vec![
            TargetSlice {
                path: Utf8PathBuf::from("a.py"),
                lines: vec![LineRange { start: 2, end: 7 }],
                symbols: Vec::new(),
            },
            TargetSlice {
                path: Utf8PathBuf::from("z.py"),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
        ]
    );
}

#[test]
fn changed_lines_intersect_a_file_line_selector_after_resolution() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        files: vec![Utf8PathBuf::from("pkg/a.py")],
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("pkg/a.py"),
            range: LineRange { start: 4, end: 8 },
        }],
        ..Selection::default()
    };
    let explicit = resolve_explicit(&selection, &[DiscoveredFile::python("pkg/a.py")]).unwrap();
    let changed = BTreeMap::from([
        (
            Utf8PathBuf::from("pkg/a.py"),
            vec![
                LineRange { start: 1, end: 5 },
                LineRange { start: 7, end: 9 },
            ],
        ),
        (
            Utf8PathBuf::from("pkg/other.py"),
            vec![LineRange { start: 4, end: 8 }],
        ),
    ]);

    assert_eq!(
        intersect_changed(&explicit, &changed),
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/a.py"),
            lines: vec![
                LineRange { start: 4, end: 5 },
                LineRange { start: 7, end: 8 },
            ],
            symbols: Vec::new(),
        }]
    );
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
fn resolver_consumes_inventory_after_discovery_exclusion() {
    let selection = Selection {
        root: Utf8PathBuf::from("project"),
        sources: vec![Utf8PathBuf::from("pkg")],
        includes: vec!["pkg/generated.py".into()],
        excludes: vec!["pkg/generated.py".into()],
        ..Selection::default()
    };
    // Discovery owns glob precedence; the excluded file is absent from its result.
    let discovered = [DiscoveredFile::python("pkg/a.py")];
    let targets = resolve_explicit(&selection, &discovered).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "pkg/a.py");
}

#[test]
fn discovered_bracket_filename_is_not_reinterpreted_as_a_literal_exclude() {
    for selector in ["source", "file", "line"] {
        let mut selection = Selection {
            root: "project".into(),
            excludes: vec!["src/[ab].py".into()],
            ..Selection::default()
        };
        match selector {
            "source" => selection.sources.push("src".into()),
            "file" => selection.files.push("src/[ab].py".into()),
            _ => selection.lines.push(LineSelection {
                path: "src/[ab].py".into(),
                range: LineRange { start: 1, end: 1 },
            }),
        }
        let targets =
            resolve_explicit(&selection, &[DiscoveredFile::python("src/[ab].py")]).unwrap();
        assert_eq!(targets.len(), 1, "{selector}");
        assert_eq!(targets[0].path, "src/[ab].py");
    }
}

#[test]
fn changed_intersects_file_line_symbol_and_source() {
    let explicit = vec![
        TargetSlice {
            path: Utf8PathBuf::from("pkg/file.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
        TargetSlice {
            path: Utf8PathBuf::from("pkg/line.py"),
            lines: vec![LineRange { start: 3, end: 6 }],
            symbols: Vec::new(),
        },
        TargetSlice {
            path: Utf8PathBuf::from("pkg/source.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
        TargetSlice {
            path: Utf8PathBuf::from("pkg/symbol.py"),
            lines: Vec::new(),
            symbols: vec!["Widget.run".into()],
        },
    ];
    let changed = BTreeMap::from([
        (
            Utf8PathBuf::from("pkg/file.py"),
            vec![LineRange { start: 8, end: 9 }],
        ),
        (
            Utf8PathBuf::from("pkg/line.py"),
            vec![
                LineRange { start: 1, end: 4 },
                LineRange { start: 6, end: 8 },
            ],
        ),
        (
            Utf8PathBuf::from("pkg/source.py"),
            vec![LineRange { start: 11, end: 12 }],
        ),
        (
            Utf8PathBuf::from("pkg/symbol.py"),
            vec![LineRange { start: 20, end: 20 }],
        ),
        (
            Utf8PathBuf::from("tests/not_selected.py"),
            vec![LineRange { start: 1, end: 1 }],
        ),
    ]);

    assert_eq!(
        intersect_changed(&explicit, &changed),
        vec![
            TargetSlice {
                path: Utf8PathBuf::from("pkg/file.py"),
                lines: vec![LineRange { start: 8, end: 9 }],
                symbols: Vec::new(),
            },
            TargetSlice {
                path: Utf8PathBuf::from("pkg/line.py"),
                lines: vec![
                    LineRange { start: 3, end: 4 },
                    LineRange { start: 6, end: 6 },
                ],
                symbols: Vec::new(),
            },
            TargetSlice {
                path: Utf8PathBuf::from("pkg/source.py"),
                lines: vec![LineRange { start: 11, end: 12 }],
                symbols: Vec::new(),
            },
            TargetSlice {
                path: Utf8PathBuf::from("pkg/symbol.py"),
                lines: vec![LineRange { start: 20, end: 20 }],
                symbols: vec!["Widget.run".into()],
            },
        ]
    );
}

#[cfg(windows)]
#[test]
fn windows_changed_intersection_uses_discovered_path_case() {
    let explicit = vec![TargetSlice {
        path: Utf8PathBuf::from("src/App.py"),
        lines: Vec::new(),
        symbols: Vec::new(),
    }];
    let changed = BTreeMap::from([(
        Utf8PathBuf::from("Src/App.py"),
        vec![LineRange { start: 2, end: 3 }],
    )]);

    assert_eq!(
        intersect_changed(&explicit, &changed),
        vec![TargetSlice {
            path: Utf8PathBuf::from("src/App.py"),
            lines: vec![LineRange { start: 2, end: 3 }],
            symbols: Vec::new(),
        }]
    );
}

#[cfg(windows)]
#[test]
fn windows_changed_intersection_uses_unicode_path_equality() {
    let explicit = vec![TargetSlice {
        path: Utf8PathBuf::from("pkg/ς/App.py"),
        lines: Vec::new(),
        symbols: Vec::new(),
    }];
    let changed = BTreeMap::from([(
        Utf8PathBuf::from("PKG/Σ/App.py"),
        vec![LineRange { start: 5, end: 5 }],
    )]);

    assert_eq!(
        intersect_changed(&explicit, &changed),
        vec![TargetSlice {
            path: Utf8PathBuf::from("pkg/ς/App.py"),
            lines: vec![LineRange { start: 5, end: 5 }],
            symbols: Vec::new(),
        }]
    );
}

#[cfg(windows)]
#[test]
fn windows_changed_intersection_merges_case_equivalent_ranges() {
    let explicit = vec![TargetSlice {
        path: Utf8PathBuf::from("SRC/App.py"),
        lines: Vec::new(),
        symbols: Vec::new(),
    }];
    let changed = BTreeMap::from([
        (
            Utf8PathBuf::from("Src/App.py"),
            vec![LineRange { start: 2, end: 3 }],
        ),
        (
            Utf8PathBuf::from("src/app.py"),
            vec![LineRange { start: 4, end: 5 }],
        ),
    ]);

    assert_eq!(
        intersect_changed(&explicit, &changed),
        vec![TargetSlice {
            path: Utf8PathBuf::from("SRC/App.py"),
            lines: vec![LineRange { start: 2, end: 5 }],
            symbols: Vec::new(),
        }]
    );
}

#[cfg(not(windows))]
#[test]
fn unix_changed_intersection_keeps_backslash_as_a_filename_character() {
    let explicit = vec![TargetSlice {
        path: Utf8PathBuf::from(r"pkg\App.py"),
        lines: Vec::new(),
        symbols: Vec::new(),
    }];
    let changed = BTreeMap::from([(
        Utf8PathBuf::from("pkg/App.py"),
        vec![LineRange { start: 2, end: 3 }],
    )]);

    assert!(intersect_changed(&explicit, &changed).is_empty());
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
#[test]
fn run_config_normalizes_operator_selectors() {
    let mut raw = raw_config();
    raw.operators = vec!["type_iterables".to_owned(), "type_dict_mapping".to_owned()];
    raw.exclude_operators = vec!["type_iterable_iterator".to_owned()];
    let config = RunConfig::try_from(raw).unwrap();
    assert!(
        config
            .operators
            .contains(MutationOperator::TypeSequenceIterable)
    );
    assert!(config.operators.contains(MutationOperator::TypeMapping));
    assert!(
        !config
            .operators
            .contains(MutationOperator::TypeIterableIterator)
    );
}
