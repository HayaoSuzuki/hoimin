//! Public subprocess regressions: a native stack overflow must not kill the test runner.
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn python() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    })
}

fn bounded_output(command: &mut Command) -> Output {
    // Files avoid pipe backpressure while the parent enforces the deadline.
    let stdout = tempfile::NamedTempFile::new().unwrap();
    let stderr = tempfile::NamedTempFile::new().unwrap();
    let mut child = command
        .stdout(Stdio::from(stdout.reopen().unwrap()))
        .stderr(Stdio::from(stderr.reopen().unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "analysis subprocess timed out: {}",
                std::fs::read_to_string(stderr.path()).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(stdout.path()).unwrap(),
        stderr: std::fs::read(stderr.path()).unwrap(),
    }
}

fn assert_python_valid(label: &str, path: &Path) {
    let python_output = bounded_output(Command::new(python()).args([
        "-c", "import pathlib,sys; compile(pathlib.Path(sys.argv[1]).read_bytes(), 'calc.py', 'exec')",
    ]).arg(path));
    assert!(
        python_output.status.success(),
        "{label}: fixture must compile under CPython: {python_output:?}"
    );
}

fn assert_plan_and_run_depth_rejection(
    label: &str,
    source: &str,
    validate_fixture: fn(&str, &Path),
) {
    type OperationCase = (&'static str, &'static [&'static str], fn(&Output));
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("calc.py");
    std::fs::write(&path, source).unwrap();
    validate_fixture(label, &path);
    let operations: [OperationCase; 2] = [
        ("plan", &[], |output| {
            assert!(output.stdout.is_empty(), "failed analysis emitted a plan");
        }),
        ("run", &[], |output| {
            let report: serde_json::Value = jsonl_report::from_slice(&output.stdout).unwrap();
            assert_eq!(report["summary"]["complete"], false);
            assert!(
                !report["baseline"].is_null(),
                "run still executes its baseline first"
            );
        }),
    ];
    for (operation, format_args, assert_output) in operations {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args([operation, "--root"])
            .arg(root.path())
            .args([
                "--file",
                "calc.py",
                "--operators",
                "binary_add_sub",
                "--max-candidates",
                "1",
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
            ])
            .env_remove("RUST_MIN_STACK");
        command.args(format_args);
        command.arg("--").arg(python()).args(["-c", "pass"]);
        let output = bounded_output(&mut command);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.code().is_some_and(|code| code != 0),
            "{operation}/{label}: {output:?}"
        );
        assert!(
            stderr.contains("calc.py")
                && stderr.contains("analysis depth")
                && stderr.contains("128"),
            "{operation}/{label}: {stderr}"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("calc.py")).unwrap(),
            source
        );
        assert_output(&output);
    }
}

#[test]
fn plan_and_run_reject_deep_ast_with_path_and_cause() {
    for terms in [2_000, 20_000, 25_000] {
        assert_plan_and_run_depth_rejection(
            &format!("addition-{terms}"),
            &format!("value = {}\n", vec!["1"; terms].join("+")),
            // These existing parser/disposal cases exceed some CPython builds' limits.
            |_, _| {},
        );
    }
}

#[test]
fn unary_parser_recursion_reaches_controlled_depth_rejection() {
    assert_plan_and_run_depth_rejection(
        "unary",
        &format!("value = {}1\n", "-".repeat(1_000)),
        assert_python_valid,
    );
}

#[test]
fn power_parser_recursion_reaches_controlled_depth_rejection() {
    assert_plan_and_run_depth_rejection(
        "power",
        &format!("value = {}\n", vec!["1"; 2_000].join("**")),
        assert_python_valid,
    );
}

#[test]
fn lambda_parser_recursion_reaches_controlled_depth_rejection() {
    assert_plan_and_run_depth_rejection(
        "lambda",
        &format!("value = {}1\n", "lambda: ".repeat(2_000)),
        assert_python_valid,
    );
}

#[test]
fn conditional_parser_recursion_reaches_controlled_depth_rejection() {
    assert_plan_and_run_depth_rejection(
        "conditional",
        &format!("value = {}1\n", "1 if x else ".repeat(2_000)),
        assert_python_valid,
    );
}

#[test]
fn speculative_with_expression_reclaims_the_discarded_parse() {
    // Parser/disposal ownership fixture, without a cross-platform CPython limit claim.
    let deep = vec!["1"; 25_000].join("+");
    assert_plan_and_run_depth_rejection(
        "speculative-with",
        &format!("with ({deep}) as context:\n    pass\n"),
        |_, _| {},
    );
}

#[test]
fn speculative_match_call_reclaims_the_discarded_subject() {
    // `match` is a soft keyword here; speculative statement parsing must rewind.
    let deep = vec!["1"; 25_000].join("+");
    assert_plan_and_run_depth_rejection(
        "speculative-match",
        &format!("match({deep})\n"),
        |_, _| {},
    );
}

#[test]
fn invalid_deep_partial_trees_keep_invalid_syntax_behavior() {
    let deep = vec!["1"; 25_000].join("+");
    for source in [
        format!("value = {deep}+\n"),
        format!("value = {deep}\nbroken =\n"),
        format!("value = {}1\nbroken =\n", "-".repeat(1_000)),
        format!("{}x = 1\n", "-".repeat(10_000)),
        format!("match x:\n case C(a{}=other): pass\n", ".b".repeat(25_000)),
        format!("match x:\n case (a{} as y)(): pass\n", ".b".repeat(25_000)),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("calc.py"), source).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(["plan", "--root"])
            .arg(root.path())
            .args([
                "--file",
                "calc.py",
                "--min-free-space",
                "1B",
                "--allow-best-effort-memory",
                "--",
            ])
            .arg(python())
            .args(["-c", "pass"])
            .env_remove("RUST_MIN_STACK");
        let output = bounded_output(&mut command);
        assert!(
            output.status.code().is_some_and(|code| code != 0),
            "{output:?}"
        );
        assert!(output.stdout.is_empty(), "invalid syntax emitted a plan");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("calc.py") && stderr.contains("syntax"),
            "{stderr}"
        );
        assert!(
            !stderr.contains("analysis depth"),
            "invalid syntax semantics changed: {stderr}"
        );
    }
}

fn parser_fixture(case: &str) -> (String, bool) {
    // Stress ownership and recursion, not CPython's independent grammar/resource limits.
    match case {
        "assignment-context" => (format!("{}x = 1\n", "-".repeat(10_000)), false),
        "assignment-target" => (
            format!("{}x{} = items\n", "[".repeat(10_000), "]".repeat(10_000)),
            true,
        ),
        "delete-target" => (
            format!("del {}x{}\n", "[".repeat(10_000), "]".repeat(10_000)),
            true,
        ),
        "discard-keyword-pattern" => (
            format!("match x:\n case C(a{}=other): pass\n", ".b".repeat(25_000)),
            false,
        ),
        "discard-as-pattern" => (
            format!("match x:\n case (a{} as y)(): pass\n", ".b".repeat(25_000)),
            false,
        ),
        "convert-pattern" => (
            format!(
                "match x:\n case {}a{}(): pass\n",
                "[".repeat(10_000),
                "]".repeat(10_000)
            ),
            false,
        ),
        "format-spec" => (
            format!("value = f'{}x{}'\n", "{x:".repeat(2_000), "}".repeat(2_000)),
            true,
        ),
        "suite" => {
            let mut source = String::new();
            for n in 0..500 {
                source.push_str(&" ".repeat(n));
                source.push_str("if x:\n");
            }
            source.push_str(&" ".repeat(500));
            source.push_str("pass\n");
            (source, true)
        }
        "old-decorator" => (format!("@a{}\ndef f(): pass\n", ".b".repeat(25_000)), true),
        "async-recovery" => (format!("{}pass\n", "async ".repeat(2_000)), false),
        "mixed" => (
            format!(
                "value = {}1{}\n",
                "(lambda: -1 ** (1 if x else ".repeat(500),
                "))".repeat(500)
            ),
            true,
        ),
        _ => panic!("unknown parser fixture: {case}"),
    }
}

#[test]
fn parser_small_stack_child_fixture() {
    const CASE_ENV: &str = "HOIMIN_TEST_PARSER_STACK_CASE";
    for case in parser_stack_child_cases(CASE_ENV) {
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || {
                let (source, valid) = parser_fixture(&case);
                let parsed = ruff_python_parser::parse_unchecked(
                    &source,
                    ruff_python_parser::ParseOptions::from(ruff_python_ast::PySourceType::Python)
                        .with_target_version(ruff_python_ast::PythonVersion::PY38),
                )
                .try_into_module()
                .unwrap();
                let actual_valid = parsed.has_valid_syntax();
                ruff_python_parser::ast_cleanup::dispose_module(parsed.into_syntax());
                assert_eq!(actual_valid, valid, "{case}: syntax semantics changed");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

fn parser_stack_child_cases(variable: &str) -> Vec<String> {
    std::env::var(variable).into_iter().collect()
}

#[test]
fn parser_and_recovery_survive_small_stack() {
    const CASE_ENV: &str = "HOIMIN_TEST_PARSER_STACK_CASE";
    for case in [
        "assignment-context",
        "assignment-target",
        "delete-target",
        "discard-keyword-pattern",
        "discard-as-pattern",
        "convert-pattern",
        "format-spec",
        "suite",
        "async-recovery",
        "mixed",
        "old-decorator",
    ] {
        let output = bounded_output(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "parser_small_stack_child_fixture", "--nocapture"])
                .env(CASE_ENV, case)
                .env_remove("RUST_MIN_STACK"),
        );
        assert!(output.status.success(), "{case}: {output:?}");
    }
}
