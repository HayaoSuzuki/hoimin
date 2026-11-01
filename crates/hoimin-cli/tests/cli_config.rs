use std::fs;
use std::num::NonZeroUsize;

use hoimin_cli::cli::{
    ParsedCommand, TopSelectionPolicy, VerifySelection, parse_config_from, parse_from,
};
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};

type ConfigParser = fn(
    &[&str],
) -> Result<
    hoimin_core::RunConfig<hoimin_core::EnvironmentRequest>,
    hoimin_cli::cli::CliError,
>;

const CONFIG_PARSERS: [(&str, ConfigParser); 2] = [
    ("run", |args| parse_config_from(args.iter().copied())),
    ("plan", |args| {
        let ParsedCommand::Plan(plan) = parse_from(args.iter().copied())? else {
            panic!("expected plan");
        };
        plan.into_configs().map(|(config, _)| config)
    }),
];

#[rstest::rstest]
#[case::run("run", CONFIG_PARSERS[0].1)]
#[case::plan("plan", CONFIG_PARSERS[1].1)]
fn mutation_commands_accept_independent_worker_and_process_limits(
    #[case] command: &str,
    #[case] parse: ConfigParser,
) {
    let args = [
        "hoimin",
        command,
        "--file",
        "calc.py",
        "--jobs",
        "4",
        "--max-processes",
        "1",
        "--",
        "python",
    ];

    let config = parse(&args).unwrap();

    assert_eq!(config.limits.jobs.get(), 4, "{command}");
    assert_eq!(config.limits.max_processes.get(), 1, "{command}");
}

#[cfg(any(unix, windows))]
fn native_mutation_config(
    command: &str,
    options: Vec<std::ffi::OsString>,
) -> Result<hoimin_core::RunConfig<hoimin_core::EnvironmentRequest>, hoimin_cli::cli::CliError> {
    let mut args = vec![std::ffi::OsString::from("hoimin"), command.into()];
    args.extend(options);
    if command == "run" {
        parse_config_from(args)
    } else {
        let ParsedCommand::Plan(plan) = parse_from(args)? else {
            panic!("expected plan");
        };
        plan.into_configs().map(|(config, _)| config)
    }
}

const RUNTIME_DEFAULT_OPERATOR_IDS: [&str; 85] = [
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
    "statement_delete",
    "integer_literal_neighbor",
    "augmented_to_assignment",
    "return_tuple_swap",
    "string_literal_empty",
    "while_condition_false",
    "conversion_call_remove",
    "method_call_remove",
    "function_body_return_constant",
    "string_segment_empty",
    "stdlib_conversion_call_remove",
    "loop_break_insert",
    "iterator_stop_insert",
    "async_iterator_stop_insert",
    "list_comprehension_to_generator",
    "generator_return_insert",
    "generator_skip_first",
    "generator_empty",
    "none_truthiness",
    "mutable_default_share",
    "nonlocal_remove",
    "yield_from_to_yield",
    "mapping_setdefault_to_get",
    "bisect_boundary_swap",
    "deepcopy_to_copy",
    "deque_popleft_to_pop",
    "gather_return_exceptions",
    "itertools_take_drop_swap",
    "fstring_conversion_swap",
    "binary_div_floor",
    "structure_slice_bound_omit",
    "structure_slice_bound_move",
    "continue_remove",
    "comparison_not_implemented",
    "exception_cause_remove",
    "dataclass_factory_share",
    "sort_key_swap",
    "lambda_capture_late",
    "encoding_errors_ignore",
    "base_order_shuffle",
    "override_method_delete",
    "super_call_move",
];

#[rstest::rstest]
#[case::method_call_remove_1("method_call_remove")]
#[case::function_body_return_constant_2("function_body_return_constant")]
#[case::compare_eq_ne_3("compare_eq_ne")]
#[case::compare_order_4("compare_order")]
#[case::membership_5("membership")]
#[case::identity_6("identity")]
#[case::boolean_and_or_7("boolean_and_or")]
#[case::binary_add_sub_8("binary_add_sub")]
#[case::augmented_add_sub_9("augmented_add_sub")]
#[case::augmented_mul_div_10("augmented_mul_div")]
#[case::augmented_floor_mod_11("augmented_floor_mod")]
#[case::binary_mul_div_12("binary_mul_div")]
#[case::binary_floor_mod_13("binary_floor_mod")]
#[case::unary_sign_14("unary_sign")]
#[case::remove_not_15("remove_not")]
#[case::boolean_literal_16("boolean_literal")]
#[case::break_continue_17("break_continue")]
#[case::collection_any_all_18("collection_any_all")]
#[case::collection_list_tuple_19("collection_list_tuple")]
#[case::collection_set_frozenset_20("collection_set_frozenset")]
#[case::collection_append_insert_21("collection_append_insert")]
#[case::collection_min_max_22("collection_min_max")]
#[case::collection_set_add_discard_23("collection_set_add_discard")]
#[case::collection_set_remove_discard_24("collection_set_remove_discard")]
#[case::collection_string_starts_ends_25("collection_string_starts_ends")]
#[case::collection_string_split_rsplit_26("collection_string_split_rsplit")]
#[case::bitwise_and_or_27("bitwise_and_or")]
#[case::bitwise_shift_28("bitwise_shift")]
#[case::binary_power_29("binary_power")]
#[case::binary_matmul_30("binary_matmul")]
#[case::augmented_power_31("augmented_power")]
#[case::augmented_matmul_32("augmented_matmul")]
#[case::bitwise_xor_33("bitwise_xor")]
#[case::bitwise_invert_34("bitwise_invert")]
#[case::augmented_bitwise_and_or_35("augmented_bitwise_and_or")]
#[case::augmented_bitwise_xor_36("augmented_bitwise_xor")]
#[case::augmented_bitwise_shift_37("augmented_bitwise_shift")]
#[case::operator_function_38("operator_function")]
#[case::structure_append_extend_39("structure_append_extend")]
#[case::structure_mapping_get_subscript_40("structure_mapping_get_subscript")]
#[case::structure_sort_reverse_41("structure_sort_reverse")]
#[case::structure_sorted_reversed_42("structure_sorted_reversed")]
#[case::structure_index_neighbor_43("structure_index_neighbor")]
#[case::structure_slice_neighbor_44("structure_slice_neighbor")]
#[case::exception_type_pair_45("exception_type_pair")]
#[case::exception_bare_to_exception_46("exception_bare_to_exception")]
#[case::exception_exception_to_bare_47("exception_exception_to_bare")]
#[case::exception_base_boundary_48("exception_base_boundary")]
#[case::exception_tuple_add_pair_49("exception_tuple_add_pair")]
#[case::exception_tuple_remove_member_50("exception_tuple_remove_member")]
#[case::type_nullable_remove_51("type_nullable_remove")]
#[case::type_nullable_add_52("type_nullable_add")]
#[case::type_list_sequence_53("type_list_sequence")]
#[case::type_set_abstract_set_54("type_set_abstract_set")]
#[case::type_dict_mapping_55("type_dict_mapping")]
#[case::type_iterable_iterator_56("type_iterable_iterator")]
#[case::type_sequence_iterable_57("type_sequence_iterable")]
#[case::binary_div_floor_58("binary_div_floor")]
#[case::structure_slice_bound_omit_59("structure_slice_bound_omit")]
#[case::structure_slice_bound_move_60("structure_slice_bound_move")]
#[case::type_nullable_61("type_nullable")]
#[case::type_collections_62("type_collections")]
#[case::type_iterables_63("type_iterables")]
fn usage_documents_all_mutation_operator_ids_and_selector_families_1(#[case] name: &str) {
    let usage =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/usage.md")).unwrap();

    assert!(usage.contains(name), "usage reference is missing {name}");
}

#[rstest::rstest]
#[case::input_85_default_runtime_operators_1("85 default runtime operators")]
#[case::input_104_operator_ids_2("104 operator IDs")]
#[case::collection_ops_3("collection_ops")]
#[case::structure_ops_4("structure_ops")]
#[case::bitwise_ops_5("bitwise_ops")]
#[case::exception_ops_6("exception_ops")]
#[case::exception_risky_7("exception_risky")]
#[case::exception_type_pair_8("exception_type_pair")]
#[case::baseexception_9("BaseException")]
#[case::except_10("except*")]
#[case::exclude_operators_collection_ops_11("--exclude-operators collection_ops")]
#[case::append_pop_12("`append`/`pop`")]
#[case::comprehensions_13("comprehensions")]
#[case::set_literals_14("set literals")]
fn usage_documents_all_mutation_operator_ids_and_selector_families_2(#[case] expected: &str) {
    let usage =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/usage.md")).unwrap();

    assert!(
        usage.contains(expected),
        "usage reference is missing {expected}"
    );
}

#[rstest::rstest]
#[case::extending_python_exception_mutations_1("Extending Python exception mutations")]
#[case::excepthandler_2("ExceptHandler")]
#[case::exception_type_pair_3("exception_type_pair")]
#[case::exception_risky_4("exception_risky")]
#[case::baseexception_5("BaseException")]
#[case::except_6("except*")]
#[case::shadowing_7("shadowing")]
#[case::both_its_source_and_replacement_names_8("both its source and replacement names")]
#[case::scope_aware_resolver_9("scope-aware resolver")]
#[case::class_non_closure_10("class non-closure")]
#[case::bare_exec_11("bare `exec`")]
#[case::unknown_12("`Unknown`")]
#[case::apply_candidate_and_reparse_13("apply_candidate_and_reparse")]
fn development_docs_explain_exception_mutation_policy(#[case] expected: &str) {
    let development = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/development.md"
    ))
    .unwrap();

    assert!(
        development.contains(expected),
        "development docs missing {expected}"
    );
}

#[rstest::rstest]
#[case::max_workspace_size_8gib_1("`--max-workspace-size` | `8GiB`")]
#[case::min_free_space_10gib_2("`--min-free-space` | `10GiB`")]
#[case::raising_a_consumption_limit_or_lowering_the_reserve_is_3(
    "raising a consumption limit or lowering the reserve is explicit risk acceptance"
)]
fn docs_publish_mandatory_disk_defaults(#[case] expected: &str) {
    let usage =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/usage.md")).unwrap();

    assert!(
        usage.contains(expected),
        "usage reference is missing {expected}"
    );
}

#[rstest::rstest]
#[case::help_1("--help")]
#[case::version_2("--version")]
fn real_binary_help_and_version_use_stdout(#[case] argument: &str) {
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

#[rstest::rstest]
#[case::run_1("run")]
#[case::plan_2("plan")]
fn mutation_command_help_lists_all_operator_ids_and_selectors(#[case] command: &str) {
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

#[rstest::rstest]
#[case::bash("bash")]
#[case::zsh("zsh")]
#[case::fish("fish")]
#[case::powershell("powershell")]
fn removed_completions_command_is_rejected(#[case] shell: &str) {
    let argv = ["hoimin", "completions", shell];

    let result = parse_from(argv);

    assert!(result.is_err());
}

#[test]
fn removed_completions_command_is_absent_from_help() {
    let help = hoimin_cli::cli::root_command()
        .render_long_help()
        .to_string();

    assert!(!help.contains("completions"));
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
fn plan_rejects_metrics() {
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
}

#[test]
fn verify_resolves_metrics_path_from_invocation_directory() {
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
fn progress_defaults_to_three_stalls() {
    let ParsedCommand::Progress(args) =
        parse_from(["hoimin", "progress", "before.json", "after.json"]).unwrap()
    else {
        panic!("expected progress");
    };

    assert_eq!(args.patience.get(), 3);
}

#[rstest::rstest]
#[case::hoimin_1(&["hoimin", "progress", "one.json"])]
#[case::hoimin_2(&["hoimin", "progress", "--patience", "0", "a", "b"])]
fn progress_requires_two_reports_and_positive_patience(#[case] argv: &[&str]) {
    let result = parse_from(argv.iter().copied());

    assert!(result.is_err(), "{argv:?}");
}

#[test]
fn plan_normalizes_selection_with_json_output_and_disabled_session() {
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
    let config = args.into_configs().unwrap().0;
    assert_eq!(config.session, hoimin_core::SessionMode::Disabled);
    assert_eq!(config.fingerprint_includes, ["pyproject.toml"]);
    assert_eq!(config.fingerprint_files, ["pyproject.toml"]);
    assert_eq!(config.test_argv.len(), 3);
    assert_eq!(config.output.metrics, None);
}

#[rstest::rstest]
#[case::format_1("format", &["--format", "json"][..])]
#[case::session_2("session", &["--session", "session.sqlite"][..])]
#[case::resume_3("resume", &["--resume"][..])]
fn plan_rejects_run_output_and_session_options(#[case] case: &str, #[case] option: &[&str]) {
    let mut args = vec!["hoimin", "plan", "--file", "src/calc.py"];
    args.extend_from_slice(option);
    args.extend(["--", "python"]);

    let error = parse_from(args).unwrap_err();

    assert!(
        matches!(error, hoimin_cli::cli::CliError::Clap(_)),
        "{case}"
    );
    assert!(error.to_string().contains(option[0]), "{case}: {error}");
}

#[test]
fn verify_accepts_candidate_selection_and_format_override() {
    let ParsedCommand::Verify(args) = parse_from([
        "hoimin",
        "verify",
        "plan.json",
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
}

#[test]
fn verify_requires_an_explicit_selection() {
    assert!(parse_from(["hoimin", "verify", "plan.json"]).is_err());
}

#[rstest::rstest]
#[case::vec_1(vec![])]
#[case::selection_policy_2(vec!["--selection-policy", "strict"])]
#[case::selection_policy_3(vec!["--selection-policy", "diverse"])]
#[case::selection_policy_4(vec!["--selection-policy", "line-diverse"])]
fn verify_top_normalizes_omitted_and_zero_offsets_to_the_same_request(
    #[case] policy_args: Vec<&str>,
) {
    let mut omitted = vec!["hoimin", "verify", "plan.json", "--top", "30"];
    omitted.extend(policy_args);
    let mut explicit = omitted.clone();
    explicit.extend(["--offset", "0"]);

    let ParsedCommand::Verify(omitted) = parse_from(omitted).unwrap() else {
        panic!("expected verify");
    };

    let ParsedCommand::Verify(explicit) = parse_from(explicit).unwrap() else {
        panic!("expected verify");
    };

    assert_eq!(omitted.selection, explicit.selection);
}

#[rstest::rstest]
#[case::input_0_1("0")]
#[case::input_1_2("1")]
fn verify_offset_requires_top_even_when_zero(#[case] offset: &str) {
    let args = ["hoimin", "verify", "plan.json", "--offset", offset];

    let result = parse_from(args);

    assert!(result.is_err(), "offset={offset}");
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
            offset: 0,
        }
    );
}

#[rstest::rstest]
#[case::diverse("diverse", TopSelectionPolicy::Diverse)]
#[case::strict("strict", TopSelectionPolicy::Strict)]
fn verify_top_accepts_explicit_selection_policy(
    #[case] policy: &str,
    #[case] expected: TopSelectionPolicy,
) {
    let argv = [
        "hoimin",
        "verify",
        "plan.json",
        "--top",
        "30",
        "--selection-policy",
        policy,
    ];

    let parsed = parse_from(argv).unwrap();

    let ParsedCommand::Verify(args) = parsed else {
        panic!("expected verify")
    };
    assert_eq!(
        args.selection,
        VerifySelection::Top {
            count: NonZeroUsize::new(30).unwrap(),
            policy: expected,
            offset: 0,
        }
    );
}

#[rstest::rstest]
#[case::hoimin_1(&[
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1_a",
            "--top",
            "30",
        ])]
#[case::hoimin_2(&[
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1",
            "--selection-policy",
            "strict",
        ])]
#[case::hoimin_3(&[
            "hoimin",
            "verify",
            "plan.json",
            "--top",
            "3",
            "--selection-policy",
            "weighted",
        ])]
#[case::hoimin_4(&["hoimin", "verify", "plan.json", "--top", "0"])]
fn verify_rejects_conflicting_or_invalid_selection_options(#[case] argv: &[&str]) {
    let result = parse_from(argv.iter().copied());

    assert!(result.is_err(), "{argv:?}");
}

#[rstest::rstest]
#[case::hoimin_1(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--source",
                "x",
            ])]
#[case::hoimin_2(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--operators",
                "x",
            ])]
#[case::hoimin_3(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--exclude-operators",
                "x",
            ])]
#[case::hoimin_4(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--profile",
                "x",
            ])]
#[case::hoimin_5(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--session",
                "x",
            ])]
#[case::hoimin_6(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--fingerprint-file",
                "x",
            ])]
#[case::hoimin_7(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--max-workspace-size",
                "x",
            ])]
#[case::hoimin_8(&[
                "hoimin",
                "verify",
                "plan.json",
                "--candidate",
                "m1_a",
                "--min-free-space",
                "x",
            ])]
#[case::hoimin_9(&[
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1_a",
            "--",
            "python",
        ])]
fn verify_rejects_run_only_options_and_test_argv(#[case] argv: &[&str]) {
    let result = parse_from(argv.iter().copied());

    assert!(result.is_err(), "{argv:?}");
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
    let err = parse_from(["hoimin", "run", "--", "python", "-m", "pytest"]).unwrap_err();
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
fn line_is_an_independent_target_selector() {
    let config =
        parse_config_from(["hoimin", "run", "--line", "pkg/a.py:4-7", "--", "python"]).unwrap();

    assert_eq!(config.selection.lines.len(), 1);
}

#[test]
fn symbol_satisfies_required_selector_before_config_validation() {
    let args = ["hoimin", "run", "--symbol", "pkg.a:run", "--", "python"];

    let command = parse_from(args).unwrap();

    assert!(matches!(command, ParsedCommand::Run(_)));
}

#[test]
fn source_and_symbol_selectors_can_be_combined() {
    let config = parse_config_from([
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

    assert_eq!(config.selection.sources, ["pkg"]);
    assert_eq!(config.selection.symbols.len(), 1);
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

#[rstest::rstest]
#[case::run("run", CONFIG_PARSERS[0].1)]
#[case::plan("plan", CONFIG_PARSERS[1].1)]
fn mutation_commands_apply_mandatory_disk_safety_defaults(
    #[case] command: &str,
    #[case] parse: ConfigParser,
) {
    let argv = ["hoimin", command, "--file", "pkg/a.py", "--", "python"];

    let config = parse(&argv).unwrap();

    assert_eq!(
        config.limits.max_workspace_size.get(),
        8 * 1024 * 1024 * 1024
    );
    assert_eq!(config.limits.min_free_space.get(), 10 * 1024 * 1024 * 1024);
}

#[rstest::rstest]
#[case::run("run", CONFIG_PARSERS[0].1)]
#[case::plan("plan", CONFIG_PARSERS[1].1)]
fn mutation_commands_parse_explicit_disk_safety_limits(
    #[case] command: &str,
    #[case] parse: ConfigParser,
) {
    let argv = [
        "hoimin",
        command,
        "--file",
        "pkg/a.py",
        "--max-workspace-size",
        "768MiB",
        "--min-free-space",
        "12GiB",
        "--",
        "python",
    ];

    let config = parse(&argv).unwrap();

    assert_eq!(config.limits.max_workspace_size.get(), 768 * 1024 * 1024);
    assert_eq!(config.limits.min_free_space.get(), 12 * 1024 * 1024 * 1024);
}

#[rstest::rstest]
#[case::run_max_workspace_size_1("run", CONFIG_PARSERS[0].1, "--max-workspace-size", "0")]
#[case::run_max_workspace_size_2("run", CONFIG_PARSERS[0].1, "--max-workspace-size", "bogus")]
#[case::run_max_workspace_size_3("run", CONFIG_PARSERS[0].1, "--max-workspace-size", "18446744073709551616B")]
#[case::run_min_free_space_4("run", CONFIG_PARSERS[0].1, "--min-free-space", "0")]
#[case::run_min_free_space_5("run", CONFIG_PARSERS[0].1, "--min-free-space", "bogus")]
#[case::run_min_free_space_6("run", CONFIG_PARSERS[0].1, "--min-free-space", "18446744073709551616B")]
#[case::plan_max_workspace_size_1("plan", CONFIG_PARSERS[1].1, "--max-workspace-size", "0")]
#[case::plan_max_workspace_size_2("plan", CONFIG_PARSERS[1].1, "--max-workspace-size", "bogus")]
#[case::plan_max_workspace_size_3("plan", CONFIG_PARSERS[1].1, "--max-workspace-size", "18446744073709551616B")]
#[case::plan_min_free_space_4("plan", CONFIG_PARSERS[1].1, "--min-free-space", "0")]
#[case::plan_min_free_space_5("plan", CONFIG_PARSERS[1].1, "--min-free-space", "bogus")]
#[case::plan_min_free_space_6("plan", CONFIG_PARSERS[1].1, "--min-free-space", "18446744073709551616B")]
fn disk_safety_limits_reject_zero_malformed_and_overflow_with_the_exact_flag(
    #[case] command: &str,
    #[case] parse_config: ConfigParser,
    #[case] flag: &str,
    #[case] value: &str,
) {
    let args = [
        "hoimin", command, "--file", "pkg/a.py", flag, value, "--", "python",
    ];
    let error = parse_config(&args).unwrap_err().to_string();

    assert!(
        error.contains(flag),
        "missing {flag} from {command}: {error}"
    );
}

#[rstest::rstest]
#[case::run("run", &["--max-workspace-size", "including generated files", "--min-free-space", "Mandatory minimum available bytes"])]
#[case::plan("plan", &["--max-workspace-size", "including generated files", "--min-free-space", "Mandatory minimum available bytes"])]
#[case::verify("verify", &["--max-workspace-size", "--min-free-space", "inherited from PLAN"])]
fn command_help_explains_disk_scope_reserve_and_plan_inheritance(
    #[case] command: &str,
    #[case] expected: &[&str],
) {
    let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    process.args([command, "--help"]);

    let output = process.output().unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    for fragment in expected {
        assert!(
            help.contains(fragment),
            "missing {fragment} from {command}:\n{help}"
        );
    }
}

#[rstest::rstest]
#[case::b_1("B")]
#[case::kb_2("KB")]
#[case::mb_3("MB")]
#[case::gb_4("GB")]
#[case::kib_5("KiB")]
#[case::mib_6("MiB")]
#[case::gib_7("GiB")]
fn invalid_memory_value_lists_case_sensitive_byte_suffixes(#[case] suffix: &str) {
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

    assert!(error.contains(suffix), "missing {suffix} in {error}");
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
    let config = parse_config_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--",
        "python",
        "tests.py",
    ])
    .unwrap();

    assert_eq!(config.limits.jobs.get(), 1);
    assert_eq!(config.limits.max_mutants.get(), 100);
    assert_eq!(config.limits.max_candidates.get(), 10_000);
    assert_eq!(config.limits.baseline_timeout.get().as_secs(), 60);
    assert_eq!(
        config.limits.mutant_timeout,
        hoimin_budget_math::MutantTimeout::Auto
    );
    assert_eq!(config.limits.total_timeout.get().as_secs(), 300);
    assert_eq!(config.limits.max_memory.get(), 1024 * 1024 * 1024);
    assert_eq!(config.limits.max_output.get(), 1024 * 1024);
    assert_eq!(config.limits.max_copy_size.get(), 1024 * 1024 * 1024);
    assert_eq!(
        config.limits.max_workspace_size.get(),
        8 * 1024 * 1024 * 1024
    );
    assert_eq!(config.limits.min_free_space.get(), 10 * 1024 * 1024 * 1024);
    assert_eq!(config.limits.max_processes.get(), 64);
}

#[rstest::rstest]
#[case::default_full(&[], MutationProfile::Full)]
#[case::explicit_focused(&["--profile", "focused"], MutationProfile::Focused)]
fn mutation_profile_defaults_to_full_and_accepts_focused(
    #[case] options: &[&str],
    #[case] expected: MutationProfile,
) {
    let mut argv = vec!["hoimin", "run", "--file", "x.py"];
    argv.extend_from_slice(options);
    argv.extend(["--", "check"]);

    let config = parse_config_from(argv).unwrap();

    assert_eq!(config.profile, expected);
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

#[rstest::rstest]
#[case::one_or_more_candidates_1("one or more candidates")]
#[case::repeat_2("repeat")]
#[case::top_3("--top")]
#[case::highest_ranked_4("highest-ranked")]
#[case::execution_and_resource_settings_come_from_plan_5(
    "Execution and resource settings come from PLAN"
)]
#[case::cannot_be_overridden_6("cannot be overridden")]
#[case::create_a_new_plan_7("Create a new plan")]
fn verify_help_explains_repeatable_candidates_and_plan_inheritance(#[case] expected: &str) {
    let error = parse_from(["hoimin", "verify", "--help"]).unwrap_err();
    let help = error.to_string();

    assert!(
        help.contains(expected),
        "missing `{expected}` from help:\n{help}"
    );
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
        "--",
        "python",
        "-m",
        "pytest",
    ])
    .unwrap();

    assert_eq!(config.limits.jobs.get(), 2);
    assert_eq!(config.limits.max_memory.get(), 2 * 1024 * 1024 * 1024);
    assert_eq!(
        config.selection.lines[0].range,
        hoimin_core::LineRange { start: 4, end: 7 }
    );
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

#[rstest::rstest]
#[case::src_a_py_1_1("src/a.py:1", hoimin_core::LineRange { start: 1, end: 1 })]
#[case::src_a_py_1_4294967295_2(
            "src/a.py:1-4294967295",
            hoimin_core::LineRange {
                start: 1,
                end: u32::MAX,
            },
        )]
#[case::src_a_py_4294967295_3(
            "src/a.py:4294967295",
            hoimin_core::LineRange {
                start: u32::MAX,
                end: u32::MAX,
            },
        )]
fn line_selector_accepts_one_based_u32_boundaries(
    #[case] value: &str,
    #[case] expected: hoimin_core::LineRange,
) {
    let config = parse_config_from(["hoimin", "run", "--line", value, "--", "python"]).unwrap();

    assert_eq!(config.selection.lines[0].range, expected, "{value}");
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
    assert_eq!(config.fingerprint_inputs, Vec::new());
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
#[rstest::rstest]
#[case::run_1("run")]
#[case::plan_2("plan")]
fn test_argv_preserves_non_utf8_bytes(#[case] command: &str) {
    use std::os::unix::ffi::OsStringExt;

    let options = vec![
        "--file".into(),
        "a.py".into(),
        "--".into(),
        std::ffi::OsString::from_vec(vec![0xff, b'x']),
    ];

    let config = native_mutation_config(command, options).unwrap();

    assert_eq!(
        config.test_argv,
        [hoimin_core::CommandArg::Unix(vec![0xff, b'x'])],
        "{command}"
    );
}

#[cfg(windows)]
#[rstest::rstest]
#[case::run_1("run")]
#[case::plan_2("plan")]
fn test_argv_preserves_non_utf8_wide_units(#[case] command: &str) {
    use std::os::windows::ffi::OsStringExt;

    let options = vec![
        "--file".into(),
        "a.py".into(),
        "--".into(),
        std::ffi::OsString::from_wide(&[0xd800, 0x0078]),
    ];

    let config = native_mutation_config(command, options).unwrap();

    assert_eq!(
        config.test_argv,
        [hoimin_core::CommandArg::Windows(vec![0xd800, 0x0078])],
        "{command}"
    );
}

#[cfg(any(unix, windows))]
#[rstest::rstest]
#[case::run_1_root_1("run", "--root")]
#[case::run_1_source_2("run", "--source")]
#[case::run_1_import_root_3("run", "--import-root")]
#[case::run_1_file_4("run", "--file")]
#[case::plan_2_root_1("plan", "--root")]
#[case::plan_2_source_2("plan", "--source")]
#[case::plan_2_import_root_3("plan", "--import-root")]
#[case::plan_2_file_4("plan", "--file")]
fn mutation_paths_reject_non_utf8_values(#[case] command: &str, #[case] option: &str) {
    #[cfg(unix)]
    let invalid = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![0xff])
    };
    #[cfg(windows)]
    let invalid = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0xd800])
    };

    let options = vec![
        "--file".into(),
        "a.py".into(),
        option.into(),
        invalid.clone(),
        "--".into(),
        "python".into(),
    ];

    let error = native_mutation_config(command, options).unwrap_err();

    assert!(
        matches!(error, hoimin_cli::cli::CliError::NonUtf8Value(name) if name == option),
        "{command} {option}: {error}"
    );
}

#[cfg(any(unix, windows))]
#[rstest::rstest]
#[case::run_1_line_1("run", "--line")]
#[case::run_1_symbol_2("run", "--symbol")]
#[case::plan_2_line_1("plan", "--line")]
#[case::plan_2_symbol_2("plan", "--symbol")]
fn mutation_text_selectors_reject_non_utf8_values(#[case] command: &str, #[case] option: &str) {
    #[cfg(unix)]
    let invalid = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![0xff])
    };
    #[cfg(windows)]
    let invalid = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0xd800])
    };

    let options = vec![option.into(), invalid.clone(), "--".into(), "python".into()];

    let error = native_mutation_config(command, options).unwrap_err();

    assert!(
        matches!(error, hoimin_cli::cli::CliError::Clap(ref clap) if clap.kind() == clap::error::ErrorKind::InvalidUtf8),
        "{command} {option}: {error}"
    );
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
}

#[test]
fn operator_flags_expand_explicit_groups_and_honor_exclusion() {
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
}

#[rstest::rstest]
#[case::collection_ops_1(
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
        )]
#[case::structure_ops_2(
            "structure_ops",
            &[
                "structure_append_extend",
                "structure_mapping_get_subscript",
                "structure_sort_reverse",
                "structure_sorted_reversed",
                "structure_index_neighbor",
                "structure_slice_neighbor",
                "structure_slice_bound_omit",
                "structure_slice_bound_move",
            ][..],
        )]
#[case::bitwise_ops_3(
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
        )]
fn collection_operator_flags_expand_families(#[case] selector: &str, #[case] expected: &[&str]) {
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

#[test]
fn collection_operator_flags_exclude_structure_family() {
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

#[rstest::rstest]
#[case::collection_any_all_1("collection_any_all")]
#[case::collection_list_tuple_2("collection_list_tuple")]
#[case::collection_set_frozenset_3("collection_set_frozenset")]
#[case::collection_append_insert_4("collection_append_insert")]
#[case::collection_min_max_5("collection_min_max")]
#[case::collection_set_add_discard_6("collection_set_add_discard")]
#[case::collection_set_remove_discard_7("collection_set_remove_discard")]
#[case::collection_string_starts_ends_8("collection_string_starts_ends")]
#[case::collection_string_split_rsplit_9("collection_string_split_rsplit")]
#[case::structure_append_extend_10("structure_append_extend")]
#[case::structure_mapping_get_subscript_11("structure_mapping_get_subscript")]
#[case::structure_sort_reverse_12("structure_sort_reverse")]
#[case::structure_sorted_reversed_13("structure_sorted_reversed")]
#[case::structure_index_neighbor_14("structure_index_neighbor")]
#[case::structure_slice_neighbor_15("structure_slice_neighbor")]
#[case::bitwise_and_or_16("bitwise_and_or")]
#[case::bitwise_shift_17("bitwise_shift")]
fn collection_operator_validation_lists_every_new_canonical_id(#[case] name: &str) {
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

    assert!(error.contains(name), "missing {name} from {error}");
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

#[rstest::rstest]
#[case::run("run", CONFIG_PARSERS[0].1)]
#[case::plan("plan", CONFIG_PARSERS[1].1)]
fn import_roots_are_ordered_normalized_and_independent_of_selection(
    #[case] command: &str,
    #[case] parse_config: ConfigParser,
) {
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
    let config = parse_config(&args).unwrap();
    assert_eq!(config.import_roots, ["vendor", "src", "."]);
    assert_eq!(config.selection.sources, Vec::<camino::Utf8PathBuf>::new());
    assert_eq!(config.selection.files, ["src/pkg/a.py"]);
    config.validate().unwrap();
}

#[rstest::rstest]
#[case::absolute_1("/absolute")]
#[case::outside_2("../outside")]
#[case::src_outside_3("src/../../outside")]
#[case::input_4("")]
#[case::src_5("./../src")]
fn import_roots_reject_absolute_and_escaping_paths(#[case] path: &str) {
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

proptest::proptest! {
    #[test]
    fn changed_context_normalizes_supported_values(
        parser in 0..2_usize,
        context in proptest::prop_oneof![proptest::strategy::Just(0_u32), proptest::strategy::Just(1_073_741_823_u32), 0..=1_073_741_823_u32],
    ) {
        let (command, parse_config) = CONFIG_PARSERS[parser];
        let context_arg = context.to_string();
        let command_line = ["hoimin", command, "--source", "src", "--changed", "--changed-context", &context_arg, "--", "python"];

        let config = parse_config(&command_line).unwrap();

        proptest::prop_assert_eq!(config.selection.changed.as_ref().unwrap().context(), context);
    }
}

#[rstest::rstest]
#[case::run_input_1_1("run", "-1")]
#[case::run_input_1073741824_2("run", "1073741824")]
#[case::run_input_4294967296_3("run", "4294967296")]
#[case::run_abc_4("run", "abc")]
#[case::plan_input_1_1("plan", "-1")]
#[case::plan_input_1073741824_2("plan", "1073741824")]
#[case::plan_input_4294967296_3("plan", "4294967296")]
#[case::plan_abc_4("plan", "abc")]
fn changed_context_rejects_invalid_cli_values(#[case] command: &str, #[case] context: &str) {
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
    let result = parse_from(command_line);
    assert!(result.is_err(), "{command}: {context}");
}

#[rstest::rstest]
#[case::run("run")]
#[case::plan("plan")]
fn explicit_zero_changed_context_requires_changed_flag(#[case] command: &str) {
    let command_line = [
        "hoimin",
        command,
        "--source",
        "src",
        "--changed-context",
        "0",
        "--",
        "python",
    ];
    let result = parse_from(command_line);
    assert!(result.is_err(), "{command}");
}

#[test]
fn run_and_plan_normalize_all_shared_options_equally() {
    let shared = [
        "--root",
        ".",
        "--source",
        "pkg",
        "--import-root",
        "vendor",
        "--file",
        "pkg/a.py",
        "--line",
        "pkg/a.py:4-7",
        "--symbol",
        "pkg.a:run",
        "--changed",
        "--changed-context",
        "2",
        "--diff-base",
        "main",
        "--include",
        "fixtures/**",
        "--exclude",
        "build/**",
        "--fingerprint-include",
        "fixtures/**/*.json",
        "--fingerprint-file",
        "pyproject.toml",
        "--fingerprint-env",
        "HOIMIN_TEST",
        "--operators",
        "compare_eq_ne,compare_order",
        "--exclude-operators",
        "compare_order",
        "--profile",
        "focused",
        "--jobs",
        "2",
        "--max-mutants",
        "12",
        "--max-candidates",
        "200",
        "--baseline-timeout",
        "21s",
        "--mutant-timeout",
        "7s",
        "--total-timeout",
        "2m",
        "--max-memory",
        "2GiB",
        "--max-output",
        "2MiB",
        "--max-copy-size",
        "3GiB",
        "--max-workspace-size",
        "4GiB",
        "--min-free-space",
        "5GiB",
        "--max-processes",
        "8",
        "--allow-best-effort-memory",
        "--",
        "python",
        "-m",
        "pytest",
        "-q",
    ];
    let mut run_args = vec!["hoimin", "run"];
    run_args.extend(shared);
    let mut plan_args = vec!["hoimin", "plan"];
    plan_args.extend(shared);

    let run = parse_config_from(run_args).unwrap();
    let ParsedCommand::Plan(plan) = parse_from(plan_args).unwrap() else {
        panic!("expected plan");
    };
    let plan = plan.into_configs().unwrap().0;

    assert_eq!(run, plan);
    assert_eq!(run.limits.max_output.get(), 2 * 1024 * 1024);
    assert_eq!(run.operators.names(), ["compare_eq_ne"]);
    assert_eq!(run.profile, MutationProfile::Focused);
}

#[rstest::rstest]
#[case::run_1("run")]
#[case::plan_2("plan")]
fn missing_selector_precedes_missing_test_argv(#[case] command: &str) {
    let args = ["hoimin", command];

    let error = parse_from(args).unwrap_err();

    assert!(
        matches!(error, hoimin_cli::cli::CliError::MissingTargetSelector),
        "{command}: {error}"
    );
}

#[rstest::rstest]
#[case::run_1("run")]
#[case::plan_2("plan")]
fn selected_mutation_command_requires_test_argv(#[case] command: &str) {
    let args = ["hoimin", command, "--file", "pkg/a.py"];

    let error = parse_from(args).unwrap_err();

    assert!(
        matches!(error, hoimin_cli::cli::CliError::MissingTestArgv),
        "{command}: {error}"
    );
}

#[rstest::rstest]
#[case::run_baseline_timeout_1("run", CONFIG_PARSERS[0].1, "--baseline-timeout")]
#[case::run_mutant_timeout_2("run", CONFIG_PARSERS[0].1, "--mutant-timeout")]
#[case::run_total_timeout_3("run", CONFIG_PARSERS[0].1, "--total-timeout")]
#[case::plan_baseline_timeout_1("plan", CONFIG_PARSERS[1].1, "--baseline-timeout")]
#[case::plan_mutant_timeout_2("plan", CONFIG_PARSERS[1].1, "--mutant-timeout")]
#[case::plan_total_timeout_3("plan", CONFIG_PARSERS[1].1, "--total-timeout")]
fn common_duration_diagnostics_name_the_invalid_option(
    #[case] command: &str,
    #[case] parse_config: ConfigParser,
    #[case] option: &str,
) {
    let args = [
        "hoimin", command, "--file", "pkg/a.py", option, "nonsense", "--", "python",
    ];

    let error = parse_config(&args).unwrap_err();

    assert_eq!(
        error.to_string(),
        format!("invalid {option}: nonsense; expected a duration such as 90s or 5m"),
        "{command}"
    );
}

#[rstest::rstest]
#[case::run_max_memory_1("run", CONFIG_PARSERS[0].1, "--max-memory")]
#[case::run_max_output_2("run", CONFIG_PARSERS[0].1, "--max-output")]
#[case::run_max_copy_size_3("run", CONFIG_PARSERS[0].1, "--max-copy-size")]
#[case::run_max_workspace_size_4("run", CONFIG_PARSERS[0].1, "--max-workspace-size")]
#[case::run_min_free_space_5("run", CONFIG_PARSERS[0].1, "--min-free-space")]
#[case::plan_max_memory_1("plan", CONFIG_PARSERS[1].1, "--max-memory")]
#[case::plan_max_output_2("plan", CONFIG_PARSERS[1].1, "--max-output")]
#[case::plan_max_copy_size_3("plan", CONFIG_PARSERS[1].1, "--max-copy-size")]
#[case::plan_max_workspace_size_4("plan", CONFIG_PARSERS[1].1, "--max-workspace-size")]
#[case::plan_min_free_space_5("plan", CONFIG_PARSERS[1].1, "--min-free-space")]
fn common_byte_diagnostics_name_the_invalid_option(
    #[case] command: &str,
    #[case] parse_config: ConfigParser,
    #[case] option: &str,
) {
    let args = [
        "hoimin", command, "--file", "pkg/a.py", option, "2gb", "--", "python",
    ];

    let error = parse_config(&args).unwrap_err();

    assert_eq!(
        error.to_string(),
        format!("invalid {option}: 2gb; expected bytes with one of: B, KB, MB, GB, KiB, MiB, GiB"),
        "{command}"
    );
}

#[test]
fn run_resume_requires_a_session() {
    let args = [
        "hoimin", "run", "--file", "pkg/a.py", "--resume", "--", "python",
    ];

    let error = parse_config_from(args).unwrap_err();

    assert_eq!(
        error.config_error(),
        Some(&hoimin_core::ConfigError::ResumeRequiresSession)
    );
}

fn assert_run_rejects_plan_only_analyzer_timeout(options: &[&str]) {
    let mut argv = vec!["hoimin", "run", "--file", "src/calc.py"];
    argv.extend_from_slice(options);
    argv.extend(["--", "python"]);
    let result = parse_from(argv);
    assert!(
        result.is_err(),
        "run accepted discovery-only option {options:?}"
    );
}

#[test]
fn run_rejects_plan_only_analyzer_timeout_separate_tokens() {
    assert_run_rejects_plan_only_analyzer_timeout(&["--analyzer-timeout", "19s"]);
}

#[test]
fn run_rejects_plan_only_analyzer_timeout_equals_spelling() {
    assert_run_rejects_plan_only_analyzer_timeout(&["--analyzer-timeout=19s"]);
}

#[test]
fn plan_normalizes_its_discovery_timeout_separately_from_execution() {
    let ParsedCommand::Plan(plan) = parse_from([
        "hoimin",
        "plan",
        "--file",
        "calc.py",
        "--analyzer-timeout",
        "19s",
        "--",
        "python",
    ])
    .unwrap() else {
        panic!("expected plan")
    };
    let (execution, discovery) = plan.into_configs().unwrap();
    let execution = serde_json::to_value(execution).unwrap();
    assert_eq!(
        discovery.analyzer_timeout.get(),
        std::time::Duration::from_secs(19)
    );
    assert!(execution.get("discovery").is_none());
    assert!(execution["limits"].get("analyzer_timeout").is_none());
}

fn assert_plan_rejects_discovery_timeout_with_flag_attribution(value: &str) {
    let ParsedCommand::Plan(plan) = parse_from([
        "hoimin",
        "plan",
        "--file",
        "calc.py",
        "--analyzer-timeout",
        value,
        "--",
        "python",
    ])
    .unwrap() else {
        panic!("expected plan")
    };
    let error = plan.into_configs().unwrap_err();
    assert!(
        error.to_string().contains("--analyzer-timeout"),
        "{value}: {error}"
    );
}

#[test]
fn plan_rejects_malformed_discovery_timeout_with_flag_attribution() {
    assert_plan_rejects_discovery_timeout_with_flag_attribution("nonsense");
}

#[test]
fn plan_rejects_zero_discovery_timeout_with_flag_attribution() {
    assert_plan_rejects_discovery_timeout_with_flag_attribution("0s");
}

#[test]
fn plan_rejects_oversized_discovery_timeout_with_flag_attribution() {
    assert_plan_rejects_discovery_timeout_with_flag_attribution("10000000000s");
}

#[rstest::rstest]
#[case::hoimin_1(vec!["hoimin", "--help"], false)]
#[case::hoimin_2(vec!["hoimin", "run", "--help"], false)]
#[case::hoimin_3(vec!["hoimin", "plan", "--help"], true)]
#[case::hoimin_4(vec!["hoimin", "verify", "--help"], false)]
fn help_limits_discovery_timeout_to_plan(#[case] argv: Vec<&str>, #[case] advertised: bool) {
    // Arrange: root describes run, while verify inherits the saved plan.

    let help = parse_from(argv.clone()).unwrap_err().to_string();
    assert_eq!(help.contains("--analyzer-timeout"), advertised, "{argv:?}");
}

#[rstest::rstest]
#[case::hoimin_1(&[
            "hoimin",
            "run",
            "--file",
            "src/foo.py",
            "--format",
            "human",
            "--",
            "python"
        ])]
#[case::hoimin_2(&[
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "m1",
            "--format",
            "human"
        ])]
#[case::hoimin_3(&[
                "hoimin",
                "progress",
                "--format",
                "human",
                "before.json",
                "after.json"
            ])]
#[case::hoimin_4(&[
                "hoimin",
                "progress",
                "--format",
                "json",
                "before.json",
                "after.json"
            ])]
#[case::hoimin_5(&[
                "hoimin",
                "review",
                "list",
                "plan.json",
                "--store",
                "review.sqlite3",
                "--format",
                "text"
            ])]
#[case::hoimin_6(&[
                "hoimin",
                "review",
                "list",
                "plan.json",
                "--store",
                "review.sqlite3",
                "--format",
                "json"
            ])]
fn agent_output_rejects_retired_formats_and_redundant_selectors(#[case] argv: &[&str]) {
    let result = parse_from(argv.iter().copied());

    assert!(result.is_err(), "{argv:?}");
}

#[test]
fn removed_typo_suggestions_preserve_context_and_usage() {
    let error = hoimin_cli::cli::parse_from([
        "hoimin",
        "plan",
        "--max-mutant",
        "1",
        "--file",
        "target.py",
        "--",
        "python",
        "-c",
        "pass",
    ])
    .unwrap_err()
    .to_string();
    assert!(error.contains("--max-mutant"), "{error}");
    assert!(error.contains("Usage:"), "{error}");
    assert!(!error.contains("similar argument"), "{error}");
}

#[test]
fn removed_terminal_color_preserves_help_content() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .arg("--help")
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env_remove("RUST_LOG")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Usage:"), "{help}");
    assert!(help.contains("Commands:"), "{help}");
    assert!(!help.contains('\u{1b}'), "{help}");
}

#[rstest::rstest]
#[case::run_json(&["hoimin", "run", "--file", "calc.py", "--format", "json", "--", "python"])]
#[case::run_jsonl(&["hoimin", "run", "--file", "calc.py", "--format", "jsonl", "--", "python"])]
#[case::verify_json(&["hoimin", "verify", "plan.json", "--top", "1", "--format", "json"])]
#[case::verify_jsonl(&["hoimin", "verify", "plan.json", "--top", "1", "--format", "jsonl"])]
fn run_and_verify_reject_removed_format_option(#[case] argv: &[&str]) {
    let result = parse_from(argv.iter().copied());

    assert!(result.is_err());
}

#[rstest::rstest]
#[case::hoimin_1(vec![
            "hoimin",
            "run",
            "--file",
            "calc.py",
            "--verbose",
            "--",
            "python",
        ])]
#[case::hoimin_2(vec!["hoimin", "verify", "plan.json", "--top", "1", "--verbose"])]
fn run_and_verify_reject_removed_verbose_option(#[case] args: Vec<&str>) {
    let error = parse_from(args).unwrap_err().to_string();

    assert!(error.contains("--verbose"), "{error}");
}

#[rstest::rstest]
#[case::run("run")]
#[case::plan("plan")]
fn import_root_does_not_replace_target_selection(#[case] command: &str) {
    let argv = ["hoimin", command, "--import-root", "src", "--", "python"];

    let result = parse_from(argv);

    assert!(result.is_err());
}
