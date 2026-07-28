use std::num::NonZeroUsize;

use hoimin_cli::cli::{
    ParsedCommand, ProgressOutputFormat, TopSelectionPolicy, VerifySelection, parse_from,
};
use hoimin_core::{MutationOperator, MutationProfile};

#[test]
fn run_resolves_metrics_path_from_invocation_directory() {
    let invocation_dir = std::env::current_dir().unwrap();
    let config = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--metrics",
        "metrics.json",
        "--",
        "python",
    ])
    .unwrap();

    let expected = camino::Utf8PathBuf::from_path_buf(invocation_dir)
        .unwrap()
        .join("metrics.json");
    assert_eq!(config.output.metrics.as_ref(), Some(&expected));
}

#[test]
fn metrics_is_rejected_outside_run() {
    assert!(
        parse_from([
            "hoimin",
            "plan",
            "--file",
            "src/calc.py",
            "--metrics",
            "metrics.json",
            "--",
            "python",
        ])
        .is_err()
    );
    assert!(
        parse_from([
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1_a",
            "--metrics",
            "metrics.json",
        ])
        .is_err()
    );
}

#[test]
fn progress_defaults_to_human_and_three_stalls() {
    let ParsedCommand::Progress(args) =
        parse_from(["hoimin", "progress", "before.json", "after.json"]).unwrap()
    else {
        panic!("expected progress");
    };

    assert_eq!(args.patience.get(), 3);
    assert_eq!(args.format, ProgressOutputFormat::Human);
}

#[test]
fn progress_requires_two_reports_and_positive_patience() {
    assert!(parse_from(["hoimin", "progress", "one.json"]).is_err());
    assert!(parse_from(["hoimin", "progress", "--patience", "0", "a", "b"]).is_err());
}

#[test]
fn plan_accepts_run_selection_but_rejects_report_and_session_options() {
    let ParsedCommand::Plan(args) = parse_from([
        "hoimin",
        "plan",
        "--file",
        "src/calc.py",
        "--fingerprint-include",
        "pyproject.toml",
        "--fingerprint-file",
        "pyproject.toml",
        "--",
        "python",
        "-m",
        "pytest",
    ])
    .unwrap() else {
        panic!("expected plan");
    };
    let config = args.into_run_config().unwrap();
    assert_eq!(config.output.format, hoimin_core::OutputFormat::Json);
    assert_eq!(config.session, None);
    assert!(!config.resume);
    assert_eq!(config.fingerprint_includes, ["pyproject.toml"]);
    assert_eq!(config.fingerprint_files, ["pyproject.toml"]);
    assert_eq!(config.test_argv.len(), 3);
    for option in ["--format", "--session", "--resume"] {
        assert!(
            parse_from([
                "hoimin",
                "plan",
                "--file",
                "src/calc.py",
                option,
                "x",
                "--",
                "python",
            ])
            .is_err(),
            "{option}"
        );
    }
}

#[test]
fn verify_accepts_candidate_selection_and_format_override() {
    let ParsedCommand::Verify(args) = parse_from([
        "hoimin",
        "verify",
        "plan.json",
        "--format",
        "jsonl",
        "--candidate",
        "m1_a",
        "--candidate",
        "m2_b",
    ])
    .unwrap() else {
        panic!("expected verify");
    };
    assert_eq!(args.manifest, std::path::PathBuf::from("plan.json"));
    assert_eq!(
        args.selection,
        VerifySelection::CandidateIds(vec!["m1_a".into(), "m2_b".into()])
    );
    assert_eq!(args.format, hoimin_cli::cli::OutputFormat::Jsonl);
}

#[test]
fn verify_requires_an_explicit_selection() {
    assert!(parse_from(["hoimin", "verify", "plan.json"]).is_err());
}

#[test]
fn verify_top_defaults_to_strict_selection() {
    let ParsedCommand::Verify(args) =
        parse_from(["hoimin", "verify", "plan.json", "--top", "30"]).unwrap()
    else {
        panic!("expected verify");
    };
    assert_eq!(
        args.selection,
        VerifySelection::Top {
            count: NonZeroUsize::new(30).unwrap(),
            policy: TopSelectionPolicy::Strict,
        }
    );
}

#[test]
fn verify_top_accepts_diverse_and_explicit_strict_selection() {
    let ParsedCommand::Verify(args) = parse_from([
        "hoimin",
        "verify",
        "plan.json",
        "--top",
        "30",
        "--selection-policy",
        "diverse",
    ])
    .unwrap() else {
        panic!("expected verify");
    };
    assert_eq!(
        args.selection,
        VerifySelection::Top {
            count: NonZeroUsize::new(30).unwrap(),
            policy: TopSelectionPolicy::Diverse,
        }
    );

    let ParsedCommand::Verify(args) = parse_from([
        "hoimin",
        "verify",
        "plan.json",
        "--top",
        "30",
        "--selection-policy",
        "strict",
    ])
    .unwrap() else {
        panic!("expected verify");
    };
    assert_eq!(
        args.selection,
        VerifySelection::Top {
            count: NonZeroUsize::new(30).unwrap(),
            policy: TopSelectionPolicy::Strict,
        }
    );
}

#[test]
fn verify_rejects_conflicting_or_invalid_selection_options() {
    assert!(
        parse_from([
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1_a",
            "--top",
            "30",
        ])
        .is_err()
    );
    assert!(
        parse_from([
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1",
            "--selection-policy",
            "strict",
        ])
        .is_err()
    );
    assert!(
        parse_from([
            "hoimin",
            "verify",
            "plan.json",
            "--top",
            "3",
            "--selection-policy",
            "weighted",
        ])
        .is_err()
    );
    assert!(parse_from(["hoimin", "verify", "plan.json", "--top", "0"]).is_err());
}

#[test]
fn verify_rejects_run_only_options_and_test_argv() {
    for option in ["--source", "--session", "--fingerprint-file"] {
        assert!(
            parse_from([
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                option,
                "x",
            ])
            .is_err(),
            "{option}"
        );
    }
    assert!(
        parse_from([
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1_a",
            "--",
            "python",
        ])
        .is_err()
    );
}

#[test]
fn run_requires_a_selector_and_test_argv() {
    let err = parse_from(["hoimin", "run", "--", "python", "-m", "unittest"]).unwrap_err();
    assert!(err.to_string().contains("target selector"));
}

#[test]
fn command_after_separator_is_preserved_without_shell_parsing() {
    let ParsedCommand::Run(cli) = parse_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--",
        "python",
        "-m",
        "pytest",
        "-q",
    ])
    .unwrap() else {
        panic!("expected run");
    };
    assert_eq!(cli.test_argv, ["python", "-m", "pytest", "-q"]);
}

#[test]
fn run_requires_test_argv() {
    let err = parse_from(["hoimin", "run", "--file", "src/calc.py"]).unwrap_err();
    assert!(err.to_string().contains("test argv"));
}

#[test]
fn line_and_symbol_are_independent_target_selectors() {
    let line = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--line",
        "pkg/a.py:4-7",
        "--",
        "python",
    ])
    .unwrap();
    assert_eq!(line.selection.lines.len(), 1);

    let ParsedCommand::Run(symbol_only) =
        parse_from(["hoimin", "run", "--symbol", "pkg.a:run", "--", "python"]).unwrap()
    else {
        panic!("expected run");
    };
    assert_eq!(symbol_only.symbol, ["pkg.a:run"]);

    let symbol = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--source",
        "pkg",
        "--symbol",
        "pkg.a:run",
        "--",
        "python",
    ])
    .unwrap();
    assert_eq!(symbol.selection.symbols.len(), 1);
}

#[test]
fn binary_byte_units_preserve_their_1024_multiplier() {
    let config = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "pkg/a.py",
        "--max-memory",
        "2MiB",
        "--",
        "python",
    ])
    .unwrap();

    assert_eq!(config.limits.max_memory.get(), 2 * 1024 * 1024);
}

#[test]
fn documented_defaults_are_applied() {
    let ParsedCommand::Run(cli) = parse_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--",
        "python",
        "tests.py",
    ])
    .unwrap() else {
        panic!("expected run");
    };

    assert_eq!(cli.jobs, 1);
    assert_eq!(cli.max_mutants, 100);
    assert_eq!(cli.max_candidates, 10_000);
    assert_eq!(cli.analyzer_timeout, "30s");
    assert_eq!(cli.baseline_timeout, "60s");
    assert_eq!(cli.mutant_timeout, "auto");
    assert_eq!(cli.total_timeout, "5m");
    assert_eq!(cli.max_memory, "1GiB");
    assert_eq!(cli.max_output, "1MiB");
    assert_eq!(cli.max_copy_size, "1GiB");
    assert_eq!(cli.max_processes, 64);
}

#[test]
fn mutation_profile_defaults_to_full_and_accepts_focused() {
    let full =
        hoimin_cli::cli::parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"])
            .unwrap();
    assert_eq!(full.profile, MutationProfile::Full);

    let focused = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--profile",
        "focused",
        "--",
        "check",
    ])
    .unwrap();
    assert_eq!(focused.profile, MutationProfile::Focused);
}

#[test]
fn mutation_profile_rejects_unknown_value() {
    let error = hoimin_cli::cli::parse_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--profile",
        "sampled",
        "--",
        "check",
    ])
    .unwrap_err();
    assert!(error.to_string().contains("sampled"));
}

#[test]
fn root_help_exposes_the_run_contract() {
    let error = parse_from(["hoimin", "--help"]).unwrap_err();
    let help = error.to_string();

    for expected in [
        "run",
        "--file",
        "--changed",
        "--jobs",
        "--max-memory",
        "--format",
        "--session",
        "-- <TEST_ARGV>",
    ] {
        assert!(
            help.contains(expected),
            "missing `{expected}` from help:\n{help}"
        );
    }
    assert!(
        !help.contains("--python"),
        "obsolete option in help:\n{help}"
    );
}

#[test]
fn verify_help_explains_repeatable_candidates_and_plan_inheritance() {
    let error = parse_from(["hoimin", "verify", "--help"]).unwrap_err();
    let help = error.to_string();

    for expected in [
        "one or more candidates",
        "repeat",
        "--top",
        "highest-ranked",
        "Execution and resource settings come from PLAN",
        "cannot be overridden",
        "Create a new plan",
    ] {
        assert!(
            help.contains(expected),
            "missing `{expected}` from help:\n{help}"
        );
    }
}

#[test]
fn parse_run_config_validates_cross_flag_rules() {
    let error = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "a.py",
        "--diff-base",
        "main",
        "--",
        "python",
    ])
    .unwrap_err();
    assert_eq!(
        error.config_error(),
        Some(&hoimin_core::ConfigError::DiffBaseRequiresChanged)
    );
}

#[tokio::test]
async fn runtime_entrypoint_applies_typed_validation() {
    let code = hoimin_cli::run_from([
        "hoimin",
        "run",
        "--file",
        "a.py",
        "--diff-base",
        "main",
        "--",
        "python",
    ])
    .await;
    assert_eq!(code, 2);
}

#[test]
fn parse_run_config_parses_ranges_limits_and_output() {
    let config = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--source",
        "pkg",
        "--line",
        "pkg/a.py:4-7",
        "--jobs",
        "2",
        "--max-memory",
        "2GiB",
        "--format",
        "jsonl",
        "--",
        "python",
        "-m",
        "unittest",
    ])
    .unwrap();

    assert_eq!(config.limits.jobs.get(), 2);
    assert_eq!(config.limits.max_memory.get(), 2 * 1024 * 1024 * 1024);
    assert_eq!(
        config.selection.lines[0].range,
        hoimin_core::LineRange { start: 4, end: 7 }
    );
    assert_eq!(config.output.format, hoimin_core::OutputFormat::Jsonl);
    assert_eq!(config.test_argv.len(), 3);
}

#[test]
fn run_preserves_repeated_fingerprint_include_patterns() {
    let config = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--fingerprint-include",
        "pyproject.toml",
        "--fingerprint-include",
        "fixtures/**/*.json",
        "--fingerprint-file",
        "pyproject.toml",
        "--fingerprint-file",
        "config/settings[prod].toml",
        "--include",
        "fixtures/**",
        "--",
        "python",
        "-m",
        "pytest",
    ])
    .unwrap();

    assert_eq!(
        config.fingerprint_includes,
        ["pyproject.toml", "fixtures/**/*.json"]
    );
    assert_eq!(config.selection.includes, ["fixtures/**"]);
    assert_eq!(
        config.fingerprint_files,
        ["pyproject.toml", "config/settings[prod].toml"]
    );
    assert!(config.fingerprint_inputs.is_empty());
}

#[test]
fn parse_run_config_rejects_obsolete_python_option() {
    let error = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "pkg/a.py",
        "--python",
        "tools/python",
        "--allow-best-effort-memory",
        "--",
        "python",
    ])
    .unwrap_err();
    assert!(error.to_string().contains("--python"));
}

#[cfg(unix)]
#[test]
fn test_argv_preserves_non_utf8_bytes() {
    use std::os::unix::ffi::OsStringExt;

    let invalid = std::ffi::OsString::from_vec(vec![0xff, b'x']);
    let config = hoimin_cli::cli::parse_config_from([
        std::ffi::OsString::from("hoimin"),
        std::ffi::OsString::from("run"),
        std::ffi::OsString::from("--file"),
        std::ffi::OsString::from("a.py"),
        std::ffi::OsString::from("--"),
        invalid.clone(),
    ])
    .unwrap();
    assert_eq!(
        config.test_argv,
        [hoimin_core::CommandArg::Unix(invalid.into_vec())]
    );
}

#[cfg(windows)]
#[test]
fn test_argv_preserves_non_utf8_wide_units() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let invalid = std::ffi::OsString::from_wide(&[0xd800, u16::from(b'x')]);
    let expected: Vec<u16> = invalid.encode_wide().collect();
    let config = hoimin_cli::cli::parse_config_from([
        std::ffi::OsString::from("hoimin"),
        std::ffi::OsString::from("run"),
        std::ffi::OsString::from("--file"),
        std::ffi::OsString::from("a.py"),
        std::ffi::OsString::from("--"),
        invalid,
    ])
    .unwrap();
    assert_eq!(
        config.test_argv,
        [hoimin_core::CommandArg::Windows(expected)]
    );
}

#[cfg(windows)]
#[test]
fn selector_rejects_non_utf8_wide_units() {
    use std::os::windows::ffi::OsStringExt;

    let invalid = std::ffi::OsString::from_wide(&[0xd800]);
    let error = hoimin_cli::cli::parse_config_from([
        std::ffi::OsString::from("hoimin"),
        std::ffi::OsString::from("run"),
        std::ffi::OsString::from("--file"),
        invalid,
        std::ffi::OsString::from("--"),
        std::ffi::OsString::from("python"),
    ])
    .unwrap_err();
    assert!(matches!(
        error,
        hoimin_cli::cli::CliError::NonUtf8Value("--file")
    ));
}
#[test]
fn operator_flags_expand_groups_and_preserve_legacy_default() {
    let default =
        hoimin_cli::cli::parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"])
            .unwrap();
    assert!(
        !default
            .operators
            .contains(MutationOperator::TypeNullableRemove)
    );
    let selected = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--operators",
        "type_nullable,type_collections",
        "--exclude-operators",
        "type_dict_mapping",
        "--",
        "check",
    ])
    .unwrap();
    assert!(
        selected
            .operators
            .contains(MutationOperator::TypeNullableRemove)
    );
    assert!(
        selected
            .operators
            .contains(MutationOperator::TypeListSequence)
    );
    assert!(!selected.operators.contains(MutationOperator::TypeMapping));
}

#[test]
fn operator_flags_reject_unknown_names() {
    let error = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--operators",
        "unknown_operator",
        "--",
        "check",
    ])
    .unwrap_err();
    assert_eq!(
        error.config_error(),
        Some(&hoimin_core::ConfigError::UnknownMutationOperator {
            value: "unknown_operator".to_owned(),
        })
    );
}

#[test]
fn operator_flags_reject_type_mapping_alias() {
    let error = hoimin_cli::cli::parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--operators",
        "type_mapping",
        "--",
        "check",
    ])
    .unwrap_err();
    assert_eq!(
        error.config_error(),
        Some(&hoimin_core::ConfigError::UnknownMutationOperator {
            value: "type_mapping".to_owned(),
        })
    );
}
