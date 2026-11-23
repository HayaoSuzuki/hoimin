use std::collections::BTreeSet;

use hoimin_core::{MutationOperatorSelection, MutationSelector};
use rstest::rstest;

fn advertised_section(help: &str, heading: &str) -> BTreeSet<String> {
    help.split_once(heading)
        .unwrap_or_else(|| panic!("missing help section {heading}"))
        .1
        .trim_start()
        .split("\n\n")
        .next()
        .unwrap()
        .split([',', '\n'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

#[rstest]
#[case::run("run")]
#[case::plan("plan")]
fn help_separates_defaults_opt_in_ids_and_selector_families(#[case] name: &str) {
    // Arrange
    let mut command = hoimin_cli::cli::root_command()
        .find_subcommand(name)
        .unwrap()
        .clone();
    let defaults: BTreeSet<_> = MutationOperatorSelection::default()
        .names()
        .into_iter()
        .collect();
    let selectors = MutationOperatorSelection::selectors();
    let explicit: BTreeSet<_> = selectors
        .iter()
        .filter_map(|selector| match selector {
            MutationSelector::Operator(operator) if !defaults.contains(operator.as_str()) => {
                Some(operator.as_str().to_owned())
            }
            MutationSelector::Operator(_) | MutationSelector::Family { .. } => None,
        })
        .collect();
    let families: BTreeSet<_> = selectors
        .iter()
        .filter_map(|selector| match selector {
            MutationSelector::Family { name, .. } => Some((*name).to_owned()),
            MutationSelector::Operator(_) => None,
        })
        .collect();

    // Act
    let help = command.render_long_help().to_string();

    // Assert
    assert_eq!(
        advertised_section(&help, "Default runtime IDs:\n"),
        defaults
    );
    assert_eq!(advertised_section(&help, "Opt-in IDs:\n"), explicit);
    assert_eq!(advertised_section(&help, "Selector families:\n"), families);
}

#[rstest]
#[case::run("run")]
#[case::plan("plan")]
fn help_links_to_the_package_versions_existing_usage_path(#[case] name: &str) {
    // Arrange
    let mut command = hoimin_cli::cli::root_command()
        .find_subcommand(name)
        .unwrap()
        .clone();
    let expected = format!(
        "https://github.com/HayaoSuzuki/hoimin/blob/v{}/docs/user/guides/usage.md#mutation-operators",
        env!("CARGO_PKG_VERSION"),
    );

    // Act
    let help = command.render_long_help().to_string();

    // Assert
    assert!(help.contains(&expected));
    assert!(help.contains("docs/user/reference/operators.md"));
}

#[test]
fn reference_has_exactly_one_entry_for_every_public_operator() {
    // Arrange
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/user/reference/operators.md");
    let expected: BTreeSet<_> = MutationOperatorSelection::selectors()
        .into_iter()
        .filter_map(|selector| match selector {
            MutationSelector::Operator(operator) => Some(operator.as_str().to_owned()),
            MutationSelector::Family { .. } => None,
        })
        .collect();

    // Act
    let reference = std::fs::read_to_string(path).unwrap();
    let entries: Vec<_> = reference
        .lines()
        .filter_map(|line| {
            line.strip_prefix("### `")
                .and_then(|value| value.strip_suffix('`'))
        })
        .map(str::to_owned)
        .collect();
    let unique: BTreeSet<_> = entries.iter().cloned().collect();

    // Assert
    assert_eq!(entries.len(), unique.len());
    assert_eq!(unique, expected);
}

#[rstest]
#[case::run_selection("run", "operators")]
#[case::plan_selection("plan", "operators")]
#[case::run_exclusion("run", "exclude_operators")]
#[case::plan_exclusion("plan", "exclude_operators")]
fn website_operator_index_uses_the_cli_argument_explanation(
    #[case] command: &str,
    #[case] argument: &str,
) {
    // Arrange
    let cli = hoimin_cli::cli::root_command();
    let argument = cli
        .find_subcommand(command)
        .unwrap()
        .get_arguments()
        .find(|arg| arg.get_id() == argument)
        .unwrap();
    let explanation = argument
        .get_long_help()
        .or_else(|| argument.get_help())
        .unwrap()
        .to_string();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user/reference/cli.md");

    // Act
    let reference = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    let index = reference
        .split_once("\n## Operator selection\n")
        .unwrap()
        .1
        .split("\n## Selector families\n")
        .next()
        .unwrap();

    // Assert: compare independently consumed outputs, not a copy of manual prose.
    assert!(
        index
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(&explanation.split_whitespace().collect::<Vec<_>>().join(" "))
    );
}

#[rstest]
#[case::root(None)]
#[case::run(Some("run"))]
#[case::plan(Some("plan"))]
#[case::verify(Some("verify"))]
#[case::progress(Some("progress"))]
#[case::review(Some("review"))]
#[case::reduce(Some("reduce"))]
fn website_publishes_the_actual_command_help(#[case] subcommand: Option<&str>) {
    // Arrange
    let directory = tempfile::tempdir().unwrap();
    let reference = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user/reference/cli.md"),
    )
    .unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command.current_dir(directory.path());
    if let Some(subcommand) = subcommand {
        command.arg(subcommand);
    }

    // Act
    let output = command.arg("--help").output().unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    // Assert
    assert!(output.status.success());
    assert_eq!(output.stderr.as_slice(), b"");
    assert!(
        reference
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(&help.split_whitespace().collect::<Vec<_>>().join(" "))
    );
}

#[rstest]
#[case::slice(
    "structure_slice_bound_move",
    "result = items[:n]\n",
    Some("result = items[n:]\n")
)]
#[case::cause(
    "exception_cause_remove",
    "raise Error() from cause\n",
    Some("raise Error() from None\n")
)]
#[case::conversion_statement("conversion_call_remove", "int(value)\n", Some("(value)\n"))]
#[case::ascii_conversion_excluded("fstring_conversion_swap", "result = f'{value!a}'\n", None)]
#[case::set_deletion_excluded("container_element_delete", "result = {1, 2}\n", None)]
#[case::ternary_excluded("condition_constant", "result = left if ready else right\n", None)]
#[case::valued_return_excluded("function_body_erase", "def f():\n    return 1\n", None)]
#[case::explicit_gather_option_excluded(
    "gather_return_exceptions",
    "import asyncio\nasyncio.gather(task, return_exceptions=False)\n",
    None
)]
#[case::strict_encoding_excluded(
    "encoding_errors_ignore",
    "text.encode('ascii', errors='strict')\n",
    None
)]
fn reference_example_has_the_documented_candidate(
    #[case] id: &str,
    #[case] source: &str,
    #[case] expected: Option<&str>,
) {
    // Arrange
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!([id])).unwrap();
    let request = hoimin_python::AnalyzeRequest {
        payload: hoimin_python::payload::PayloadBudget::default(),
        path: camino::Utf8Path::new("example.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: hoimin_core::MutationProfile::Full,
        max_candidates: 100,
    };

    // Act
    let output = hoimin_python::analyze_source_cancellable(&request, source, || false).unwrap();
    let actual: Vec<_> = output
        .candidates
        .iter()
        .map(|candidate| {
            let mut mutated = source.to_owned();
            let start = usize::try_from(candidate.span.start).unwrap();
            let end = start + usize::try_from(candidate.span.length).unwrap();
            mutated.replace_range(start..end, &candidate.replacement);
            mutated
        })
        .collect();

    // Assert
    assert_eq!(output.diagnostics, []);
    assert!(!output.truncated);
    assert_eq!(
        actual,
        expected.into_iter().map(str::to_owned).collect::<Vec<_>>()
    );
}

fn documented_members(reference: &str, selector: &str) -> BTreeSet<String> {
    let heading = format!("### `{selector}`\n");
    reference.split_once(&heading).map_or_else(
        || BTreeSet::from([selector.to_owned()]),
        |(_, section)| {
            section
                .split("\n### ")
                .next()
                .unwrap()
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("- [`")?
                        .split_once("`](")
                        .map(|(id, _)| id.to_owned())
                })
                .collect()
        },
    )
}

#[derive(Clone, Copy, Debug)]
enum MutationCommand {
    Run,
    Plan,
}

proptest::proptest! {
    #[test]
    fn documented_selection_rules_match_cli_for_arbitrary_selector_combinations(
        command in proptest::sample::select(vec![MutationCommand::Run, MutationCommand::Plan]),
        windows_newlines in proptest::bool::ANY,
        use_defaults in proptest::bool::ANY,
        included in proptest::collection::vec(proptest::sample::select(MutationOperatorSelection::valid_names()), 1..8),
        excluded in proptest::collection::vec(proptest::sample::select(MutationOperatorSelection::valid_names()), 0..8),
    ) {
        // Arrange: use the published table/family lists as the selection oracle.
        let reference = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/user/reference/cli.md")).unwrap().replace("\r\n", "\n");
        let reference = if windows_newlines { reference.replace('\n', "\r\n") } else { reference };
        let reference = reference.replace("\r\n", "\n");
        let mut expected: BTreeSet<_> = if use_defaults {
            reference.lines().filter(|line| line.starts_with("| [`") && line.contains(" | yes |"))
                .map(|line| line.split('`').nth(1).unwrap().to_owned()).collect()
        } else {
            included.iter().flat_map(|name| documented_members(&reference, name)).collect()
        };
        for id in excluded.iter().flat_map(|name| documented_members(&reference, name)) {
            expected.remove(&id);
        }
        let command_name = match command {
            MutationCommand::Run => "run",
            MutationCommand::Plan => "plan",
        };
        let mut argv = vec!["hoimin".to_owned(), command_name.to_owned(), "--source".to_owned(), "src".to_owned()];
        if !use_defaults {
            argv.extend(["--operators".to_owned(), included.join(",")]);
        }
        if !excluded.is_empty() {
            argv.extend(["--exclude-operators".to_owned(), excluded.join(",")]);
        }
        argv.extend(["--".to_owned(), "python".to_owned()]);

        // Act
        let actual = match command {
            MutationCommand::Run => hoimin_cli::cli::parse_config_from(argv).map(|config| config.operators.names()),
            MutationCommand::Plan => match hoimin_cli::cli::parse_from(argv).unwrap() {
                hoimin_cli::cli::ParsedCommand::Plan(plan) => plan.into_configs().map(|(config, _)| config.operators.names()),
                _ => unreachable!(),
            },
        };

        // Assert
        if expected.is_empty() {
            proptest::prop_assert!(matches!(actual, Err(hoimin_cli::cli::CliError::Config(hoimin_core::ConfigError::EmptyMutationOperatorSelection))));
        } else {
            proptest::prop_assert_eq!(actual.unwrap().into_iter().collect::<BTreeSet<_>>(), expected);
        }
    }
}
