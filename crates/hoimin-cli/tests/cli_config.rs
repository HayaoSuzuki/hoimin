#[test]
fn run_requires_a_selector_and_test_argv() {
    let err = hoimin_cli::cli::parse_from(["hoimin", "run", "--", "python", "-m", "unittest"])
        .unwrap_err();
    assert!(err.to_string().contains("target selector"));
}

#[test]
fn command_after_separator_is_preserved_without_shell_parsing() {
    let cli = hoimin_cli::cli::parse_from([
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
    .unwrap();
    assert_eq!(cli.test_argv, ["python", "-m", "pytest", "-q"]);
}

#[test]
fn run_requires_test_argv() {
    let err = hoimin_cli::cli::parse_from(["hoimin", "run", "--file", "src/calc.py"]).unwrap_err();
    assert!(err.to_string().contains("test argv"));
}

#[test]
fn documented_defaults_are_applied() {
    let cli = hoimin_cli::cli::parse_from([
        "hoimin",
        "run",
        "--file",
        "src/calc.py",
        "--",
        "python",
        "tests.py",
    ])
    .unwrap();

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
fn root_help_exposes_the_run_contract() {
    let error = hoimin_cli::cli::parse_from(["hoimin", "--help"]).unwrap_err();
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
}
