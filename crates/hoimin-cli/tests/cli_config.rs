use std::fs;
use std::num::NonZeroUsize;

use hoimin_cli::cli::{
    ParsedCommand, ProgressOutputFormat, TopSelectionPolicy, VerifySelection, parse_config_from,
    parse_from,
};
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};

const RUNTIME_DEFAULT_OPERATOR_IDS: [&str; 43] = [
    "compare_eq_ne",
    "compare_order",
    "membership",
    "identity",
    "boolean_and_or",
    "binary_add_sub",
    "augmented_add_sub",
    "binary_mul_div",
    "augmented_mul_div",
    "binary_floor_mod",
    "augmented_floor_mod",
    "unary_sign",
    "remove_not",
    "boolean_literal",
    "break_continue",
    "collection_any_all",
    "collection_list_tuple",
    "collection_set_frozenset",
    "collection_append_insert",
    "collection_min_max",
    "collection_set_add_discard",
    "collection_set_remove_discard",
    "collection_string_starts_ends",
    "collection_string_split_rsplit",
    "bitwise_and_or",
    "bitwise_shift",
    "binary_power",
    "binary_matmul",
    "augmented_power",
    "augmented_matmul",
    "bitwise_xor",
    "bitwise_invert",
    "augmented_bitwise_and_or",
    "augmented_bitwise_xor",
    "augmented_bitwise_shift",
    "operator_function",
    "structure_append_extend",
    "structure_mapping_get_subscript",
    "structure_sort_reverse",
    "structure_sorted_reversed",
    "structure_index_neighbor",
    "structure_slice_neighbor",
    "exception_type_pair",
];

#[test]
fn readme_documents_all_mutation_operator_ids_and_selector_families() {
    let readme =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
    for name in [
        "compare_eq_ne",
        "compare_order",
        "membership",
        "identity",
        "boolean_and_or",
        "binary_add_sub",
        "augmented_add_sub",
        "augmented_mul_div",
        "augmented_floor_mod",
        "binary_mul_div",
        "binary_floor_mod",
        "unary_sign",
        "remove_not",
        "boolean_literal",
        "break_continue",
        "collection_any_all",
        "collection_list_tuple",
        "collection_set_frozenset",
        "collection_append_insert",
        "collection_min_max",
        "collection_set_add_discard",
        "collection_set_remove_discard",
        "collection_string_starts_ends",
        "collection_string_split_rsplit",
        "bitwise_and_or",
        "bitwise_shift",
        "binary_power",
        "binary_matmul",
        "augmented_power",
        "augmented_matmul",
        "bitwise_xor",
        "bitwise_invert",
        "augmented_bitwise_and_or",
        "augmented_bitwise_xor",
        "augmented_bitwise_shift",
        "operator_function",
        "structure_append_extend",
        "structure_mapping_get_subscript",
        "structure_sort_reverse",
        "structure_sorted_reversed",
        "structure_index_neighbor",
        "structure_slice_neighbor",
        "exception_type_pair",
        "exception_bare_to_exception",
        "exception_exception_to_bare",
        "exception_base_boundary",
        "exception_tuple_add_pair",
        "exception_tuple_remove_member",
        "type_nullable_remove",
        "type_nullable_add",
        "type_list_sequence",
        "type_set_abstract_set",
        "type_dict_mapping",
        "type_iterable_iterator",
        "type_sequence_iterable",
        "type_nullable",
        "type_collections",
        "type_iterables",
    ] {
        assert!(readme.contains(name), "README is missing {name}");
    }
    for expected in [
        "all 43 runtime operators",
        "55 operator IDs",
        "collection_ops",
        "structure_ops",
        "bitwise_ops",
        "exception_ops",
        "exception_risky",
        "exception_type_pair",
        "BaseException",
        "except*",
        "--exclude-operators collection_ops",
        "`append`/`pop`",
        "comprehensions",
        "set literals",
    ] {
        assert!(readme.contains(expected), "README is missing {expected}");
    }
}

#[test]
fn development_docs_explain_exception_mutation_policy() {
    let development = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/development.md"
    ))
    .unwrap();
    for expected in [
        "Extending Python exception mutations",
        "ExceptHandler",
        "exception_type_pair",
        "exception_risky",
        "BaseException",
        "except*",
        "shadowing",
        "both its source and replacement names",
        "scope-aware resolver",
        "class non-closure",
        "bare `exec`",
        "`Unknown`",
        "apply_candidate_and_reparse",
    ] {
        assert!(
            development.contains(expected),
            "development docs missing {expected}"
        );
    }
}

#[test]
fn docs_publish_mandatory_disk_defaults() {
    let readme =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
    for expected in [
        "`--max-workspace-size` | `8GiB`",
        "`--min-free-space` | `10GiB`",
        "raising a consumption limit or lowering the reserve is explicit risk acceptance",
    ] {
        assert!(readme.contains(expected), "README is missing {expected}");
    }
}

#[test]
fn real_binary_help_and_version_use_stdout() {
    for argument in ["--help", "--version"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .arg(argument)
            .output()
            .unwrap();

        assert!(output.status.success(), "argument: {argument}");
        assert!(!output.stdout.is_empty(), "argument: {argument}");
        assert!(
            output.stderr.is_empty(),
            "argument: {argument}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn mutation_command_help_lists_all_operator_ids_and_selectors() {
    for command in ["run", "plan"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args([command, "--help"])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{command} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{command} stderr: {:?}",
            output.stderr
        );
        let help = String::from_utf8(output.stdout).unwrap();
        for name in MutationOperatorSelection::valid_names() {
            assert!(
                help.contains(name),
                "missing {name} from {command}:\n{help}"
            );
        }
    }
}

#[test]
fn completions_accept_supported_shells() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        assert!(matches!(
            parse_from(["hoimin", "completions", shell]).unwrap(),
            ParsedCommand::Completions(_)
        ));
    }
    assert!(parse_from(["hoimin", "completions", "unsupported"]).is_err());
}

#[test]
fn run_resolves_metrics_path_from_invocation_directory() {
    let invocation_dir = std::env::current_dir().unwrap();
    let config = parse_config_from([
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
fn metrics_is_rejected_for_plan_and_resolved_for_verify() {
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
    let ParsedCommand::Verify(args) = parse_from([
        "hoimin",
        "verify",
        "plan.json",
        "--candidate",
        "m1_a",
        "--metrics",
        "metrics.json",
    ])
    .unwrap() else {
        panic!("expected verify")
    };
    let expected = camino::Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap())
        .unwrap()
        .join("metrics.json");
    assert_eq!(args.metrics.as_ref(), Some(&expected));
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
    for option in [
        "--source",
        "--operators",
        "--exclude-operators",
        "--profile",
        "--session",
        "--fingerprint-file",
        "--max-workspace-size",
        "--min-free-space",
    ] {
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
fn verify_help_does_not_advertise_mutation_selection_options() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["verify", "--help"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for unexpected in [
        "--operators",
        "--exclude-operators",
        "--profile",
        "Mutation operator IDs and selector families",
    ] {
        assert!(
            !help.contains(unexpected),
            "unexpected {unexpected} in verify help:\n{help}"
        );
    }
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
    let line =
        parse_config_from(["hoimin", "run", "--line", "pkg/a.py:4-7", "--", "python"]).unwrap();
    assert_eq!(line.selection.lines.len(), 1);

    let ParsedCommand::Run(symbol_only) =
        parse_from(["hoimin", "run", "--symbol", "pkg.a:run", "--", "python"]).unwrap()
    else {
        panic!("expected run");
    };
    assert_eq!(symbol_only.symbol, ["pkg.a:run"]);

    let symbol = parse_config_from([
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
    let config = parse_config_from([
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
fn run_and_plan_apply_mandatory_disk_safety_defaults() {
    let run = parse_config_from(["hoimin", "run", "--file", "pkg/a.py", "--", "python"]).unwrap();
    let ParsedCommand::Plan(plan) =
        parse_from(["hoimin", "plan", "--file", "pkg/a.py", "--", "python"]).unwrap()
    else {
        panic!("expected plan");
    };
    let plan = plan.into_run_config().unwrap();

    for limits in [&run.limits, &plan.limits] {
        assert_eq!(limits.max_workspace_size.get(), 8 * 1024 * 1024 * 1024);
        assert_eq!(limits.min_free_space.get(), 10 * 1024 * 1024 * 1024);
    }
}

#[test]
fn run_and_plan_parse_explicit_disk_safety_limits() {
    let run = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "pkg/a.py",
        "--max-workspace-size",
        "768MiB",
        "--min-free-space",
        "12GiB",
        "--",
        "python",
    ])
    .unwrap();
    let ParsedCommand::Plan(plan) = parse_from([
        "hoimin",
        "plan",
        "--file",
        "pkg/a.py",
        "--max-workspace-size",
        "768MiB",
        "--min-free-space",
        "12GiB",
        "--",
        "python",
    ])
    .unwrap() else {
        panic!("expected plan");
    };
    let plan = plan.into_run_config().unwrap();

    for limits in [&run.limits, &plan.limits] {
        assert_eq!(limits.max_workspace_size.get(), 768 * 1024 * 1024);
        assert_eq!(limits.min_free_space.get(), 12 * 1024 * 1024 * 1024);
    }
}

#[test]
fn disk_safety_limits_reject_zero_malformed_and_overflow_with_the_exact_flag() {
    for command in ["run", "plan"] {
        for (flag, value) in [
            ("--max-workspace-size", "0"),
            ("--max-workspace-size", "bogus"),
            ("--max-workspace-size", "18446744073709551616B"),
            ("--min-free-space", "0"),
            ("--min-free-space", "bogus"),
            ("--min-free-space", "18446744073709551616B"),
        ] {
            let args = [
                "hoimin", "plan", "--file", "pkg/a.py", flag, value, "--", "python",
            ];
            let error = if command == "run" {
                let mut args = args;
                args[1] = "run";
                parse_config_from(args).unwrap_err()
            } else {
                let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
                    panic!("expected plan");
                };
                plan.into_run_config().unwrap_err()
            }
            .to_string();

            assert!(
                error.contains(flag),
                "missing {flag} from {command}: {error}"
            );
        }
    }
}

#[test]
fn command_help_explains_disk_scope_reserve_and_plan_inheritance() {
    for command in ["run", "plan"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "--max-workspace-size",
            "including generated files",
            "--min-free-space",
            "Mandatory minimum available bytes",
        ] {
            assert!(
                help.contains(expected),
                "missing {expected} from {command}:\n{help}"
            );
        }
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["verify", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "--max-workspace-size",
        "--min-free-space",
        "inherited from PLAN",
    ] {
        assert!(
            help.contains(expected),
            "missing {expected} from verify:\n{help}"
        );
    }
}

#[test]
fn invalid_memory_value_lists_case_sensitive_byte_suffixes() {
    let error = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--max-memory",
        "2gb",
        "--",
        "python",
    ])
    .unwrap_err()
    .to_string();

    for suffix in ["B", "KB", "MB", "GB", "KiB", "MiB", "GiB"] {
        assert!(error.contains(suffix), "missing {suffix} in {error}");
    }
}

#[test]
fn invalid_duration_value_shows_duration_example() {
    let error = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--total-timeout",
        "nonsense",
        "--",
        "python",
    ])
    .unwrap_err()
    .to_string();

    assert!(error.contains("--total-timeout"), "{error}");
    assert!(error.contains("90s"), "{error}");
    assert!(error.contains("5m"), "{error}");
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
    assert_eq!(cli.max_workspace_size, "8GiB");
    assert_eq!(cli.min_free_space, "10GiB");
    assert_eq!(cli.max_processes, 64);
}

#[test]
fn mutation_profile_defaults_to_full_and_accepts_focused() {
    let full = parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"]).unwrap();
    assert_eq!(full.profile, MutationProfile::Full);

    let focused = parse_config_from([
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
    let error = parse_from([
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
        "--changed-context",
        "--jobs",
        "--max-memory",
        "--max-workspace-size",
        "--min-free-space",
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
    let error = parse_config_from([
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
    let config = parse_config_from([
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

fn assert_invalid_line_selector(value: &str) {
    let error = parse_config_from(["hoimin", "run", "--line", value, "--", "python"]).unwrap_err();

    assert_eq!(
        error.to_string(),
        format!("invalid --line: {value}; expected PATH:START-END with 1-based START <= END")
    );
}

#[test]
fn line_selector_rejects_an_empty_path_during_cli_validation() {
    assert_invalid_line_selector(":5-10");
}

#[test]
fn line_selector_rejects_line_zero_during_cli_validation() {
    assert_invalid_line_selector("src/a.py:0-5");
}

#[test]
fn line_selector_rejects_a_reversed_range_during_cli_validation() {
    assert_invalid_line_selector("src/a.py:10-5");
}

#[test]
fn line_selector_splits_on_the_final_colon() {
    let config = parse_config_from([
        "hoimin",
        "run",
        "--line",
        "C:/repo/pkg/a.py:4-7",
        "--",
        "python",
    ])
    .unwrap();

    assert_eq!(config.selection.lines[0].path, "C:/repo/pkg/a.py");
    assert_eq!(
        config.selection.lines[0].range,
        hoimin_core::LineRange { start: 4, end: 7 }
    );
}

#[test]
fn line_selector_accepts_one_based_u32_boundaries() {
    for (value, expected) in [
        ("src/a.py:1", hoimin_core::LineRange { start: 1, end: 1 }),
        (
            "src/a.py:1-4294967295",
            hoimin_core::LineRange {
                start: 1,
                end: u32::MAX,
            },
        ),
        (
            "src/a.py:4294967295",
            hoimin_core::LineRange {
                start: u32::MAX,
                end: u32::MAX,
            },
        ),
    ] {
        let config = parse_config_from(["hoimin", "run", "--line", value, "--", "python"]).unwrap();

        assert_eq!(config.selection.lines[0].range, expected, "{value}");
    }
}

#[cfg(unix)]
fn assert_cli_rejects_line_selector_before_test_command(value: &str) {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir(project.path().join("src")).unwrap();
    fs::write(project.path().join("src/a.py"), "value = 1\n").unwrap();
    let marker = project.path().join("test-command.ran");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["run", "--root"])
        .arg(project.path())
        .args(["--line", value, "--allow-best-effort-memory", "--"])
        .args(["/bin/sh", "-c", "printf ran > \"$1\"", "line-test"])
        .arg(&marker)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "{value}");
    assert!(!marker.exists(), "test command ran for {value}");
    assert!(output.stdout.is_empty(), "{value}");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("invalid --line: {value}; expected PATH:START-END with 1-based START <= END\n")
    );
}

#[cfg(unix)]
#[test]
fn binary_rejects_an_empty_line_selector_path_before_the_test_command() {
    assert_cli_rejects_line_selector_before_test_command(":5-10");
}

#[cfg(unix)]
#[test]
fn binary_rejects_line_zero_before_the_test_command() {
    assert_cli_rejects_line_selector_before_test_command("src/a.py:0-5");
}

#[cfg(unix)]
#[test]
fn binary_rejects_a_reversed_line_range_before_the_test_command() {
    assert_cli_rejects_line_selector_before_test_command("src/a.py:10-5");
}

#[cfg(unix)]
#[test]
fn line_selector_rejects_a_literal_backslash_path() {
    let error = parse_config_from([
        "hoimin",
        "run",
        "--line",
        r"pkg\calc.py:4-7",
        "--",
        "python",
    ])
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        r"invalid --line: pkg\calc.py:4-7; expected PATH:START-END with 1-based START <= END"
    );
}

#[test]
fn run_preserves_repeated_fingerprint_include_patterns() {
    let config = parse_config_from([
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
    let error = parse_config_from([
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
    let config = parse_config_from([
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
fn operator_flags_expand_groups_and_preserve_runtime_default() {
    let default = parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"]).unwrap();
    assert!(
        !default
            .operators
            .contains(MutationOperator::TypeNullableRemove)
    );
    assert!(
        default
            .operators
            .contains(MutationOperator::CollectionAnyAll)
    );
    assert!(default.operators.contains(MutationOperator::BitwiseShift));
    assert!(
        default
            .operators
            .contains(MutationOperator::ExceptionTypePair)
    );
    assert!(
        !default
            .operators
            .contains(MutationOperator::ExceptionBaseBoundary)
    );
    assert!(
        default
            .operators
            .contains(MutationOperator::StructureSliceNeighbor)
    );
    let selected = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--operators",
        "type_nullable,type_collections,collection_ops,structure_ops,bitwise_ops",
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
    assert!(
        selected
            .operators
            .contains(MutationOperator::CollectionStringSplitRsplit)
    );
    assert!(
        selected
            .operators
            .contains(MutationOperator::StructureMappingGetSubscript)
    );
    assert!(selected.operators.contains(MutationOperator::BitwiseAndOr));
}

#[test]
fn exception_operator_flags_keep_risky_mutations_explicit() {
    let selected = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--operators",
        "exception_risky",
        "--",
        "check",
    ])
    .unwrap();

    assert!(
        !selected
            .operators
            .contains(MutationOperator::ExceptionTypePair)
    );
    for operator in [
        MutationOperator::ExceptionBareToException,
        MutationOperator::ExceptionExceptionToBare,
        MutationOperator::ExceptionBaseBoundary,
        MutationOperator::ExceptionTupleAddPair,
        MutationOperator::ExceptionTupleRemoveMember,
    ] {
        assert!(
            selected.operators.contains(operator),
            "missing {operator:?}"
        );
    }
}

#[test]
fn collection_operator_flags_expand_families_and_preserve_runtime_default() {
    let default = parse_config_from(["hoimin", "run", "--file", "x.py", "--", "check"]).unwrap();
    assert_eq!(
        default.operators.names(),
        RUNTIME_DEFAULT_OPERATOR_IDS.map(str::to_owned)
    );
    assert!(
        !default
            .operators
            .contains(MutationOperator::TypeNullableRemove)
    );

    for (selector, expected) in [
        (
            "collection_ops",
            &[
                "collection_any_all",
                "collection_list_tuple",
                "collection_set_frozenset",
                "collection_append_insert",
                "collection_min_max",
                "collection_set_add_discard",
                "collection_set_remove_discard",
                "collection_string_starts_ends",
                "collection_string_split_rsplit",
            ][..],
        ),
        (
            "structure_ops",
            &[
                "structure_append_extend",
                "structure_mapping_get_subscript",
                "structure_sort_reverse",
                "structure_sorted_reversed",
                "structure_index_neighbor",
                "structure_slice_neighbor",
            ][..],
        ),
        (
            "bitwise_ops",
            &[
                "bitwise_and_or",
                "bitwise_shift",
                "bitwise_xor",
                "bitwise_invert",
                "augmented_bitwise_and_or",
                "augmented_bitwise_xor",
                "augmented_bitwise_shift",
            ][..],
        ),
    ] {
        let selected = parse_config_from([
            "hoimin",
            "run",
            "--file",
            "x.py",
            "--operators",
            selector,
            "--",
            "check",
        ])
        .unwrap();
        assert_eq!(
            selected.operators.names(),
            expected
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
            "selector: {selector}"
        );
    }

    let without_structure = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--exclude-operators",
        "structure_ops",
        "--",
        "check",
    ])
    .unwrap();
    assert_eq!(
        without_structure.operators.names(),
        RUNTIME_DEFAULT_OPERATOR_IDS
            .into_iter()
            .filter(|name| !name.starts_with("structure_"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
}

#[test]
fn collection_operator_validation_lists_every_new_canonical_id() {
    let error = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "x.py",
        "--operators",
        "unknown_operator",
        "--",
        "check",
    ])
    .unwrap_err()
    .to_string();

    for name in [
        "collection_any_all",
        "collection_list_tuple",
        "collection_set_frozenset",
        "collection_append_insert",
        "collection_min_max",
        "collection_set_add_discard",
        "collection_set_remove_discard",
        "collection_string_starts_ends",
        "collection_string_split_rsplit",
        "structure_append_extend",
        "structure_mapping_get_subscript",
        "structure_sort_reverse",
        "structure_sorted_reversed",
        "structure_index_neighbor",
        "structure_slice_neighbor",
        "bitwise_and_or",
        "bitwise_shift",
    ] {
        assert!(error.contains(name), "missing {name} from {error}");
    }
}

#[test]
fn operator_flags_reject_unknown_names() {
    let error = parse_config_from([
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
    let error = parse_config_from([
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

#[test]
fn import_roots_are_ordered_normalized_and_independent_of_selection() {
    for command in ["run", "plan"] {
        let args = [
            "hoimin",
            command,
            "--file",
            "src/pkg/a.py",
            "--import-root",
            "./vendor",
            "--import-root",
            "src/./",
            "--import-root",
            "vendor",
            "--import-root",
            ".",
            "--import-root",
            "src/../src",
            "--",
            "python",
        ];
        let config = match parse_from(args).unwrap() {
            ParsedCommand::Run(_) => parse_config_from(args).unwrap(),
            ParsedCommand::Plan(args) => args.into_run_config().unwrap(),
            _ => panic!("unexpected command"),
        };
        assert_eq!(config.import_roots, ["vendor", "src", "."]);
        assert!(config.selection.sources.is_empty());
        assert_eq!(config.selection.files, ["src/pkg/a.py"]);
        config.validate().unwrap();
        assert!(parse_from(["hoimin", command, "--import-root", "src", "--", "python"]).is_err());
    }
}

#[test]
fn import_roots_reject_absolute_and_escaping_paths() {
    for path in [
        "/absolute",
        "../outside",
        "src/../../outside",
        "",
        "./../src",
    ] {
        let error = parse_config_from([
            "hoimin",
            "run",
            "--file",
            "a.py",
            "--import-root",
            path,
            "--",
            "python",
        ])
        .unwrap_err();
        assert!(error.to_string().contains("--import-root"), "{error}");
    }
}

#[test]
fn run_help_explains_windows_per_root_resource_scope() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["run", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for contract in [
        "Windows: per root process tree",
        "committed memory",
        "including the root",
        "jobs multiplies",
    ] {
        assert!(help.contains(contract), "missing {contract}: {help}");
    }
}

#[test]
fn changed_context_parses_for_run_and_plan_and_checks_bounds() {
    for command in ["run", "plan"] {
        for context in ["0", "1", "1073741823"] {
            let command_line = [
                "hoimin",
                command,
                "--source",
                "src",
                "--changed",
                "--changed-context",
                context,
                "--",
                "python",
            ];
            let config = if command == "run" {
                parse_config_from(command_line).unwrap()
            } else {
                let ParsedCommand::Plan(args) = parse_from(command_line).unwrap() else {
                    panic!("expected plan")
                };
                args.into_run_config().unwrap()
            };
            assert_eq!(
                serde_json::to_value(config).unwrap()["selection"]["changed_context"],
                context.parse::<u32>().unwrap()
            );
        }
        for context in ["-1", "1073741824", "4294967296", "abc"] {
            assert!(
                parse_from([
                    "hoimin",
                    command,
                    "--source",
                    "src",
                    "--changed",
                    "--changed-context",
                    context,
                    "--",
                    "python"
                ])
                .is_err()
            );
        }
        assert!(
            parse_from([
                "hoimin",
                command,
                "--source",
                "src",
                "--changed-context",
                "0",
                "--",
                "python"
            ])
            .is_err()
        );
    }
}
