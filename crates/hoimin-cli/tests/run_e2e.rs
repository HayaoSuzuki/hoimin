use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::{collections::BTreeSet, str};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use hoimin_core::{
    VerificationSelection, VerificationSelectionMode, VerificationSelectionPolicy,
    VerificationSelectionScope,
};

const TEST_MIN_FREE_SPACE: &str = "1B";

#[cfg(unix)]
fn full_report_transport() -> (std::os::unix::net::UnixStream, std::os::fd::OwnedFd, usize) {
    let (consumer, mut producer) = std::os::unix::net::UnixStream::pair().unwrap();
    producer.set_nonblocking(true).unwrap();
    let mut filled = 0;
    loop {
        match producer.write(&[b'x'; 4096]) {
            Ok(0) => panic!("report transport closed while filling it"),
            Ok(bytes) => filled += bytes,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("could not fill report transport: {error}"),
        }
    }
    producer.set_nonblocking(false).unwrap();
    (consumer, producer.into(), filled)
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_report_consumer_cannot_outlive_total_timeout_and_grace() {
    use std::process::Stdio;

    for format in ["jsonl", "json", "human"] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), "pass\n").unwrap();
        let (consumer, producer, _) = full_report_transport();
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--min-free-space", TEST_MIN_FREE_SPACE, "--root"])
            .arg(project.path())
            .args([
                "--file",
                "target.py",
                "--format",
                format,
                "--allow-best-effort-memory",
                "--total-timeout",
                "1s",
                "--",
            ])
            .arg(python_executable())
            .args(["-c", "pass"])
            .stdout(Stdio::from(producer))
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let diagnostics = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).await.unwrap();
            String::from_utf8(bytes).unwrap()
        });
        let status = tokio::time::timeout(Duration::from_secs(6), child.wait()).await;
        if status.is_err() {
            child.kill().await.unwrap();
            child.wait().await.unwrap();
        }
        drop(consumer);
        let diagnostics = diagnostics.await.unwrap();
        let status = status
            .unwrap_or_else(|_| {
                panic!("format={format}: stalled report exceeded timeout plus grace: {diagnostics}")
            })
            .unwrap();
        assert_eq!(status.code(), Some(2), "format={format}: {diagnostics}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_stderr_cannot_reenter_an_unbounded_terminal_diagnostic() {
    use std::process::Stdio;

    for format in ["json", "jsonl", "human"] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), "def broken(:\n").unwrap();
        let (consumer, producer, _) = full_report_transport();
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--min-free-space", TEST_MIN_FREE_SPACE, "--root"])
            .arg(project.path())
            .args([
                "--file",
                "target.py",
                "--format",
                format,
                "--allow-best-effort-memory",
                "--total-timeout",
                "1s",
                "--",
            ])
            .arg(python_executable())
            .args(["-c", "pass"])
            .stdout(Stdio::null())
            .stderr(Stdio::from(producer))
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(6), child.wait()).await;
        reap_test_child(&mut child).await.unwrap();
        drop(consumer);
        assert_eq!(
            result
                .expect("stalled stderr exceeded shutdown grace")
                .unwrap()
                .code(),
            Some(2),
            "format={format}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn first_sigint_bounds_a_started_report_write() {
    use std::io::Read;
    use std::process::Stdio;

    for format in ["jsonl", "json"] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), "pass\n").unwrap();
        let (mut consumer, producer, filled) = full_report_transport();
        let padding = "padding".repeat(2048);
        let arguments = filled / padding.len() + 4;
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(["run", "--min-free-space", TEST_MIN_FREE_SPACE, "--root"])
            .arg(project.path())
            .args([
                "--file",
                "target.py",
                "--format",
                format,
                "--allow-best-effort-memory",
                "--total-timeout",
                "30s",
                "--",
            ])
            .arg(python_executable())
            .args(["-c", "pass"])
            .args(std::iter::repeat_n(padding, arguments))
            .stdout(Stdio::from(producer))
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let ready = tokio::task::spawn_blocking(move || {
            consumer.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut prefix = vec![0; filled + 1];
            consumer.read_exact(&mut prefix)?;
            if prefix[filled] != b'{' {
                return Err(io::Error::other("report did not begin with a JSON object"));
            }
            Ok::<_, io::Error>(consumer)
        });
        let outcome: Result<_, String> = async {
            let consumer = ready
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            send_fixture_interrupt(child.id()).await?;
            let status = tokio::time::timeout(Duration::from_secs(4), child.wait())
                .await
                .map_err(|_| "first interrupt exceeded shutdown grace".to_owned())?
                .map_err(|error| error.to_string())?;
            drop(consumer);
            Ok(status)
        }
        .await;
        reap_test_child(&mut child).await.unwrap();
        assert_eq!(
            outcome
                .unwrap_or_else(|error| panic!("format={format}: {error}"))
                .code(),
            Some(2)
        );
    }
}

#[test]
fn cli_entrypoint_future_keeps_large_run_state_out_of_line() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let future = hoimin_cli::run_with_io(["hoimin", "--help"], &mut stdout, &mut stderr);

    assert!(
        size_of_val(&future) <= 14 * 1024,
        "CLI future grew to {} bytes; large run state must remain out of line",
        size_of_val(&future)
    );
}

#[tokio::test]
async fn successful_clap_displays_use_stdout() {
    for (args, expected) in [
        (["hoimin", "--help"], "Usage:"),
        (["hoimin", "--version"], env!("CARGO_PKG_VERSION")),
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

        assert_eq!(exit, 0, "args: {args:?}");
        assert!(stderr.is_empty(), "args: {args:?}: {stderr:?}");
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(
            stdout.contains(expected),
            "args: {args:?}, missing {expected:?} from:\n{stdout}"
        );
    }
}

#[tokio::test]
async fn completions_command_writes_shell_script_to_stdout() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit =
        hoimin_cli::run_with_io(["hoimin", "completions", "bash"], &mut stdout, &mut stderr).await;

    assert_eq!(exit, 0);
    assert!(stderr.is_empty(), "{stderr:?}");
    let stdout = String::from_utf8(stdout).unwrap();
    assert!(
        stdout.contains("_hoimin"),
        "missing hoimin completion function"
    );
    assert!(stdout.contains("--changed-context"));
}

#[tokio::test]
async fn invalid_clap_arguments_remain_on_stderr() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit =
        hoimin_cli::run_with_io(["hoimin", "--definitely-invalid"], &mut stdout, &mut stderr).await;

    assert_ne!(exit, 0);
    assert!(stdout.is_empty(), "{stdout:?}");
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("--definitely-invalid")
    );
}

#[tokio::test]
async fn oversized_runtime_timeouts_are_rejected_before_project_execution() {
    assert!(
        tokio::time::Instant::now()
            .checked_add(hoimin_core::MAX_TIMEOUT)
            .is_some()
    );

    for flag in ["--total-timeout", "--mutant-timeout"] {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("test-command-ran");
        let command = format!(
            "from pathlib import Path; Path({:?}).write_text('ran')",
            marker.to_string_lossy()
        );
        let root = fixture_root();
        let python = python_executable();
        let args = vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--min-free-space"),
            OsString::from(TEST_MIN_FREE_SPACE),
            OsString::from("--root"),
            root.as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from(flag),
            OsString::from("18446744073709551615s"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            python.as_os_str().to_owned(),
            OsString::from("-c"),
            OsString::from(command),
        ];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

        assert_eq!(exit, 2, "flag={flag}");
        assert!(stdout.is_empty(), "flag={flag}");
        let stderr = String::from_utf8(stderr).unwrap();
        assert_eq!(
            stderr,
            format!("invalid zero or overflowing limit: {flag}\n")
        );
        assert!(!marker.exists(), "{flag} reached the test command");
    }
}

#[tokio::test]
async fn maximum_total_timeout_completes_without_overflowing_finalization_grace() {
    let total_timeout = format!("{}s", hoimin_core::MAX_TIMEOUT.as_secs());
    let run = run_fixture_options_extra(
        &["-m", "pytest", "-q", "tests"],
        None,
        false,
        &["--max-mutants", "1", "--total-timeout", &total_timeout],
    )
    .await;

    assert_ne!(run.exit_code, 2, "stderr={}", run.stderr);
    assert_eq!(
        run.document["run"]["normalized_config"]["limits"]["total_timeout"]["secs"],
        hoimin_core::MAX_TIMEOUT.as_secs()
    );
}

#[tokio::test]
async fn pytest_command_produces_the_expected_mutant_statuses() {
    let pytest = run_fixture(&["-m", "pytest", "-q", "tests"]).await;

    assert_eq!(pytest.exit_code, 0);
    assert_eq!(pytest.statuses, ["killed"]);
    assert_eq!(
        pytest.document["run"]["versions"],
        serde_json::json!({
            "os": std::env::consts::OS,
            "hoimin": env!("CARGO_PKG_VERSION"),
        })
    );
}

#[tokio::test]
async fn invalid_syntax_warns_and_prevents_a_complete_zero_candidate_run() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(source.join("calc.py"), "def broken(:\n").unwrap();

    let run = run_project(project.path(), 1, "print('baseline succeeds')").await;

    assert_eq!(
        run.exit_code, 4,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    let diagnostic = run
        .stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|record| record["code"] == "analyzer.invalid_syntax")
        .unwrap_or_else(|| panic!("missing analyzer diagnostic in stderr: {}", run.stderr));
    assert_eq!(diagnostic["level"], "warning");
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("src/calc.py"),
        "{diagnostic}"
    );
    assert_eq!(run.document["summary"]["complete"], false);
    assert!(run.document["mutants"].as_array().unwrap().is_empty());
    assert!(
        !run.stdout.contains("\"complete\":true"),
        "invalid syntax must not produce a complete zero-candidate report: {}",
        run.stdout
    );
}

#[tokio::test]
async fn mutation_timeout_propagates_to_the_final_report_and_exit_policy() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def total(left, right):\n    return left + right\n",
    )
    .unwrap();
    let original = "return left + right";
    let command = format!(
        "from pathlib import Path; import time; source=Path('src/calc.py').read_text(); time.sleep(3) if {original:?} not in source else exec('from src.calc import total; assert total(1,2) == 3')",
    );

    let run = run_project_options(
        project.path(),
        1,
        &command,
        &["--operators", "binary_add_sub", "--mutant-timeout", "1s"],
    )
    .await;

    assert_eq!(
        run.exit_code, 4,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    assert_eq!(run.statuses, ["timeout"]);
    assert_eq!(run.document["summary"]["counts"]["timeout"], 1);
    assert_eq!(run.document["mutants"][0]["termination"], "Timeout");
    assert_eq!(run.document["summary"]["complete"], false);
}

#[tokio::test]
async fn changed_selection_real_run_uses_only_the_edited_function() {
    let project = tempfile::tempdir().unwrap();
    let _base_revision = write_changed_git_project(project.path());

    let run = run_project_options(
        project.path(),
        1,
        "pass",
        &["--changed", "--operators", "binary_add_sub"],
    )
    .await;

    assert_eq!(run.exit_code, 1, "stderr={}", run.stderr);
    let mutants = run.document["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), 1);
    let expected = BTreeSet::from([(
        "src/calc.py",
        2_u64,
        13_u64,
        "binary_add_sub",
        "+",
        "-",
        "changed",
    )]);
    assert_eq!(json_candidate_tuples(mutants), expected);
}

#[tokio::test]
async fn collection_default_run_reports_canonical_ids_with_stable_spans() {
    let project = tempfile::tempdir().unwrap();
    write_collection_operator_project(project.path());
    let command = "from src.calc import bitwise, collection, structure; assert collection() == (1, 2); assert structure([]) == [3]; assert bitwise(3, 1) == 1";

    let first = run_project_options(project.path(), 1, command, &["--max-mutants", "100"]).await;
    let second = run_project_options(project.path(), 1, command, &["--max-mutants", "100"]).await;

    assert_eq!(
        first.document["summary"]["complete"], true,
        "{}",
        first.stderr
    );
    assert_eq!(
        second.document["summary"]["complete"], true,
        "{}",
        second.stderr
    );

    for (operator, start, length) in [
        ("collection_list_tuple", 29, 6),
        ("structure_append_extend", 64, 16),
        ("bitwise_and_or", 142, 1),
    ] {
        let first_candidate = report_candidate_at(&first.document, operator, start, length);
        let first_id = first_candidate["id"].as_str().unwrap();
        assert!(!first_id.is_empty(), "operator: {operator}");

        let second_candidate = report_candidate_at(&second.document, operator, start, length);
        assert_eq!(second_candidate["id"].as_str(), Some(first_id));
        assert_eq!(second_candidate["span"], first_candidate["span"]);
    }
}

#[tokio::test]
async fn protocol_contracts_distinguish_generated_mutants_in_external_tests() {
    // Each weak command passes both versions. The stronger command observes a
    // contract that the actual Rust-generated replacement breaks. Test doubles
    // live only in the external command, outside the selected mutation source.
    let cases = [
        (
            "boolean_and_or",
            "def probe(left, right):\n    return left and right()\n",
            "assert probe(False, lambda: False) is False",
            "calls = []\ndef right():\n    calls.append('called')\n    return False\nassert probe(False, right) is False\nassert calls == []",
            1,
        ),
        (
            "collection_any_all",
            "def probe(items):\n    return any(items)\n",
            "assert probe([True, True]) is True",
            "items = iter([True, True])\nassert probe(items) is True\nassert list(items) == [True]",
            1,
        ),
        (
            "structure_mapping_get_subscript",
            "def probe(mapping, key):\n    return mapping.get(key)\n",
            "assert probe({'present': 42}, 'present') == 42",
            "class Missing(dict):\n    def __missing__(self, key):\n        return 42\nassert probe(Missing(), 'absent') is None",
            1,
        ),
        (
            "structure_append_extend",
            "def probe(items, value):\n    items.append(value)\n",
            "items = []\nprobe(items, 7)\nassert items == [7]",
            "class Logged(list):\n    def append(self, value):\n        super().append(('append', value))\n    def extend(self, values):\n        super().extend(('extend', value) for value in values)\nitems = Logged()\nprobe(items, 7)\nassert items == [('append', 7)]",
            1,
        ),
        (
            "structure_sorted_reversed",
            "def probe(items):\n    return sorted(items)\n",
            "assert list(probe([2, 1])) == [1, 2]",
            "assert list(probe([2, 3, 1])) == [1, 2, 3]",
            1,
        ),
        (
            "structure_index_neighbor",
            "def probe(items):\n    return items[1]\n",
            "assert probe([7, 7, 7]) == 7",
            "assert probe([10, 20, 30]) == 20",
            2,
        ),
        (
            "structure_slice_neighbor",
            "def probe(items):\n    return items[:1]\n",
            "assert probe([]) == []",
            "assert probe([10, 20, 30]) == [10]",
            2,
        ),
    ];

    for (operator, source, weak, strong, count) in cases {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::write(project.path().join("src/__init__.py"), "").unwrap();
        let source_path = project.path().join("src/calc.py");
        std::fs::write(&source_path, source).unwrap();

        for (command, status, exit_code) in [(weak, "survived", 1), (strong, "killed", 0)] {
            let command = format!("from src.calc import probe\n{command}\n");
            let run =
                run_project_options(project.path(), 1, &command, &["--operators", operator]).await;
            assert_eq!(
                run.exit_code, exit_code,
                "{operator}: {} {}",
                run.stderr, run.stdout
            );
            assert_eq!(
                run.document["summary"]["complete"], true,
                "{operator}: {}",
                run.stderr
            );
            assert_eq!(
                run.statuses,
                vec![status; count],
                "{operator}: {}",
                run.stdout
            );
            for mutant in run.document["mutants"].as_array().unwrap() {
                assert_eq!(mutant["candidate"]["operator"], operator);
            }
            assert_eq!(std::fs::read_to_string(&source_path).unwrap(), source);
        }
    }
}

#[tokio::test]
async fn generic_type_parameter_destinations_cannot_create_false_kills() {
    for (source, command) in [
        (
            "def f[tuple](items):\n    return list(items)\n",
            "from src.calc import f; assert f((1, 2)) == [1, 2]",
        ),
        (
            "class C[tuple]:\n    result = list(range(1, 3))\n",
            "from src.calc import C; assert C.result == [1, 2]",
        ),
        (
            "class C[tuple]:\n    global tuple\n    def method(self):\n        return list(range(1, 3))\n",
            "from src.calc import C; assert C().method() == [1, 2]",
        ),
    ] {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("src")).unwrap();
        let path = project.path().join("src/calc.py");
        std::fs::write(&path, source).unwrap();
        // Keep literal collections in the command, outside analyzed source:
        // tuple literals have legitimate independent list/tuple mutations.
        let run = run_project_options(
            project.path(),
            1,
            command,
            &[
                "--operators",
                "collection_list_tuple",
                "--baseline-timeout",
                "5s",
                "--total-timeout",
                "15s",
                "--max-mutants",
                "1",
            ],
        )
        .await;
        assert_eq!(run.exit_code, 0, "{}", run.stderr);
        assert_eq!(run.document["baseline"]["termination"]["Exit"], 0);
        assert_eq!(run.document["summary"]["complete"], true);
        assert_eq!(run.document["summary"]["counts"]["killed"], 0);
        assert!(run.document["mutants"].as_array().unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }
}

#[tokio::test]
async fn exception_default_run_reports_canonical_json_candidate() {
    let project = tempfile::tempdir().unwrap();
    write_exception_operator_project(project.path());
    let run = run_project_options(
        project.path(),
        1,
        "from src.calc import classify; assert classify() == 'ok'",
        &["--operators", "exception_ops"],
    )
    .await;

    assert_eq!(run.document["summary"]["complete"], true, "{}", run.stderr);
    let candidates: Vec<_> = run.document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|mutant| mutant["candidate"]["operator"] == "exception_type_pair")
        .collect();
    assert_eq!(candidates.len(), 2);
    let spans = candidates
        .iter()
        .map(|mutant| {
            let candidate = &mutant["candidate"];
            assert_eq!(candidate["original"], "ValueError");
            assert_eq!(candidate["replacement"], "TypeError");
            assert_eq!(candidate["span"]["length"], 10);
            candidate["span"]["start"].as_u64().unwrap()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(spans.len(), 2);
}

#[tokio::test]
async fn exception_handler_type_collection_candidates_are_excluded() {
    let project = tempfile::tempdir().unwrap();
    write_exception_handler_collection_project(project.path());
    let run = run_project_options(
        project.path(),
        1,
        "from src.calc import classify; assert list(classify()) == [1, 2]",
        &["--operators", "collection_list_tuple"],
    )
    .await;

    assert_eq!(
        run.exit_code, 1,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    assert_eq!(run.document["summary"]["complete"], true, "{}", run.stderr);
    let candidates: Vec<_> = run.document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| &mutant["candidate"])
        .collect();
    assert_eq!(candidates.len(), 1, "{}", run.stdout);
    assert_eq!(candidates[0]["operator"], "collection_list_tuple");
    assert_eq!(candidates[0]["original"], "(1, 2)");
    assert_eq!(candidates[0]["replacement"], "[1, 2]");
    assert!(
        candidates.iter().all(|candidate| !matches!(
            candidate["original"].as_str(),
            Some("tuple" | "(ValueError, TypeError)")
        )),
        "{}",
        run.stdout
    );
    assert_eq!(run.statuses, ["survived"]);
    assert_eq!(run.document["summary"]["counts"]["survived"], 1);
}

#[tokio::test]
async fn focused_collection_and_structure_candidates_remain_eligible_outside_arid_spans() {
    let project = tempfile::tempdir().unwrap();
    write_collection_operator_project(project.path());
    let command = "from src.calc import collection, structure; assert collection() == (1, 2); assert structure([]) == [3]";

    let run = run_project_options(
        project.path(),
        1,
        command,
        &["--profile", "focused", "--max-mutants", "100"],
    )
    .await;

    assert_eq!(run.document["summary"]["complete"], true);
    let candidates = run.document["mutants"].as_array().unwrap();
    let operators_and_lines = candidates
        .iter()
        .map(|mutant| {
            let candidate = &mutant["candidate"];
            (
                candidate["operator"].as_str().unwrap(),
                candidate["line"].as_u64().unwrap(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        operators_and_lines,
        BTreeSet::from([
            ("bitwise_and_or", 9),
            ("collection_append_insert", 5),
            ("collection_list_tuple", 2),
            ("structure_append_extend", 5),
        ])
    );
}

#[tokio::test]
async fn real_binary_json_report_and_diagnostic_use_separate_streams() {
    let project = tempfile::tempdir().unwrap();
    let fixture = fixture_root();
    std::fs::copy(
        fixture.join("pyproject.toml"),
        project.path().join("pyproject.toml"),
    )
    .unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(source.join("calc.py"), "def broken(:\n").unwrap();

    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .arg("run")
        .arg("--min-free-space")
        .arg(TEST_MIN_FREE_SPACE)
        .arg("--root")
        .arg(project.path())
        .arg("--source")
        .arg("src")
        .arg("--file")
        .arg("src/calc.py")
        .arg("--format")
        .arg("json")
        .arg("--allow-best-effort-memory")
        .arg("--")
        .arg(python_executable())
        .arg("-c")
        .arg("print('baseline succeeds')")
        .output()
        .await
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report_bytes = output
        .stdout
        .strip_suffix(b"\n")
        .expect("stdout report must end in one newline");
    assert!(
        !report_bytes.ends_with(b"\n"),
        "stdout report must end in exactly one newline"
    );
    let reports = serde_json::Deserializer::from_slice(&output.stdout)
        .into_iter::<serde_json::Value>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(reports.len(), 1, "stdout must contain exactly one report");
    assert_eq!(reports[0]["summary"]["complete"], false);
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("analyzer.invalid_syntax"),
        "diagnostic leaked to stdout"
    );

    let diagnostics = output
        .stderr
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|record| record["code"] == "analyzer.invalid_syntax"),
        "missing analyzer diagnostic in stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics
            .iter()
            .all(|record| record.get("summary").is_none()),
        "report leaked to stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn explicit_candidate_run_rejects_a_missing_candidate() {
    let project = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let missing_id = "candidate-that-is-not-in-the-spool";
    let (error, stdout) = run_missing_explicit_candidate(
        project.path(),
        missing_id,
        "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15",
    )
    .await;

    assert!(error.contains(missing_id), "{error}");
    assert!(
        !str::from_utf8(&stdout)
            .unwrap()
            .contains("\"complete\":true"),
        "a missing explicit candidate must not produce a complete report"
    );
}

#[tokio::test]
async fn explicit_candidate_run_rejects_an_empty_candidate_spool() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(source.join("calc.py"), "def noop():\n    pass\n").unwrap();
    let missing_id = "candidate-missing-from-empty-spool";

    let (error, stdout) = run_missing_explicit_candidate(
        project.path(),
        missing_id,
        "from src.calc import noop; assert noop() is None",
    )
    .await;

    assert!(error.contains(missing_id), "{error}");
    assert!(
        !str::from_utf8(&stdout)
            .unwrap()
            .contains("\"complete\":true"),
        "an empty spool must not produce a complete explicit verification report"
    );
}
#[tokio::test]
async fn ty_kills_a_nullable_contract_mutant() {
    let run = run_type_checker(&ty_executable(), "killed").await;

    assert_eq!(run.statuses, ["killed"]);
}

#[tokio::test]
async fn ty_reports_a_surviving_nullable_contract_mutant() {
    let run = run_type_checker(&ty_executable(), "survived").await;

    assert_eq!(run.exit_code, 1);
    assert_eq!(run.statuses, ["survived"]);
}

#[tokio::test]
async fn mypy_kills_a_nullable_contract_mutant() {
    let run = run_type_checker(&mypy_executable(), "killed").await;

    assert_eq!(run.statuses, ["killed"]);
}

#[tokio::test]
async fn mypy_reports_a_surviving_nullable_contract_mutant() {
    let run = run_type_checker(&mypy_executable(), "survived").await;

    assert_eq!(run.statuses, ["survived"]);
}

#[tokio::test]
async fn jobs_one_and_four_produce_the_same_candidates_and_statuses() {
    let one = tempfile::tempdir().unwrap();
    let four = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(one.path());
    write_parallel_project(four.path());
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";

    let serial = run_project(one.path(), 1, command).await;
    let parallel = run_project(four.path(), 4, command).await;
    let project_results = |run: &FixtureRun| {
        let mut results = run.document["mutants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mutant| {
                (
                    mutant["candidate"]["id"].as_str().unwrap().to_owned(),
                    mutant["status"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        results.sort();
        results
    };

    assert_eq!(serial.exit_code, parallel.exit_code);
    assert_eq!(project_results(&serial), project_results(&parallel));
    assert!(parallel.document["mutants"].as_array().unwrap().len() >= 4);
    for run in [&serial, &parallel] {
        let sequences = run.document["mutants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mutant| mutant["sequence"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

#[tokio::test]
async fn jobs_four_reaches_a_cross_process_barrier() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(directory.path());
    let markers = coordinator.path().join("markers");
    std::fs::create_dir(&markers).unwrap();
    let overlap = coordinator.path().join("overlap-proven");
    let original = "return a + b + c + d + e";
    let mutant = format!(
        "marker=markers/str(os.getpid())\nmarker.write_text('running')\ntry:\n    deadline=time.monotonic()+2\n    while len(list(markers.iterdir())) < 2 and time.monotonic() < deadline: time.sleep(.02)\n    if len(list(markers.iterdir())) >= 2: Path({:?}).write_text('proven')\n    from src.calc import total\n    assert total(1,2,3,4,5) == 15\nfinally:\n    marker.unlink(missing_ok=True)",
        overlap.to_string_lossy(),
    );
    let command = format!(
        "from pathlib import Path; import os,time; source=Path('src/calc.py').read_text(); markers=Path({:?}); exec({:?}) if {:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')",
        markers.to_string_lossy(),
        mutant,
        original,
    );

    let run = run_project(directory.path(), 4, &command).await;

    assert_eq!(
        run.exit_code, 0,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    assert!(overlap.exists(), "two live worker processes must overlap");
    assert_eq!(std::fs::read_dir(markers).unwrap().count(), 0);
    assert!(run.statuses.iter().all(|status| status == "killed"));
}

#[tokio::test]
async fn jobs_four_processes_receive_isolated_run_mutant_and_worker_metadata() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let records = coordinator.path().join("records");
    std::fs::create_dir(&records).unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let original = "return a + b + c + d + e";
    let mutant = format!(
        "assert os.path.samefile(os.environ['HOIMIN_WORKER_ROOT'], Path.cwd())\nPath({:?}, os.environ['HOIMIN_MUTANT_ID']).write_text(os.environ['HOIMIN_RUN_ID'])",
        records.to_string_lossy(),
    );
    let command = format!(
        r#"from pathlib import Path; import os; source=Path('src/calc.py').read_text(); exec({mutant:?}) if {original:?} not in source else exec("assert 'HOIMIN_MUTANT_ID' not in os.environ; assert os.environ['HOIMIN_RUN_ID']"); from src.calc import total; assert total(1,2,3,4,5) == 15"#,
    );

    let run = run_project(project.path(), 4, &command).await;

    assert_eq!(
        run.exit_code, 0,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    let run_id = run.document["run"]["run_id"].as_str().unwrap();
    for mutant in run.document["mutants"].as_array().unwrap() {
        let mutant_id = mutant["candidate"]["id"].as_str().unwrap();
        let record = records.join(mutant_id);
        assert!(
            record.is_file(),
            "missing metadata record for {mutant_id}; mutant={mutant}; records={:?}",
            std::fs::read_dir(&records)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect::<Vec<_>>()
        );
        assert_eq!(std::fs::read_to_string(record).unwrap(), run_id);
    }
}

#[tokio::test]
async fn joinset_and_completion_queue_stay_bounded_across_many_mutants() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    let expression = (1..=24)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(" + ");
    std::fs::write(
        source.join("calc.py"),
        format!("def total():\n    return {expression}\n"),
    )
    .unwrap();
    let python = python_executable();
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("4"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from src.calc import total; assert total() == 300"),
    ])
    .unwrap();
    let stdout = SharedBuffer::default();
    let control = hoimin_cli::shell::RunControl::new();

    let exit = hoimin_cli::shell::run_loop_with_control(
        config,
        stdout.clone(),
        SharedBuffer::default(),
        control.clone(),
    )
    .await
    .unwrap();

    assert_eq!(exit, 0);
    let document: serde_json::Value = serde_json::from_slice(&stdout.bytes()).unwrap();
    assert!(document["mutants"].as_array().unwrap().len() >= 20);
    assert!(control.max_process_tasks() <= 4);
    assert!(control.max_completion_in_flight() <= 5);
}

#[tokio::test]
async fn metrics_sidecar_observes_complete_parallel_run_without_changing_report() {
    let directory = tempfile::tempdir().unwrap();
    let metrics_path = directory.path().join("metrics.json");
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def total():\n    return 1 + 2 + 3\n",
    )
    .unwrap();
    let run = run_project_options(
        project.path(),
        2,
        "from src.calc import total; assert total() == 6",
        &[
            "--max-mutants",
            "2",
            "--metrics",
            metrics_path.to_str().unwrap(),
        ],
    )
    .await;

    assert_eq!(run.exit_code, 0, "stderr={}", run.stderr);
    assert_eq!(
        run.document.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["baseline", "mutants", "run", "schema_version", "summary"]
    );
    let metrics_text = std::fs::read_to_string(&metrics_path).unwrap();
    assert!(!metrics_text.contains(project.path().to_str().unwrap()));
    let metrics_value: serde_json::Value = serde_json::from_str(&metrics_text).unwrap();
    let metrics_schema: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("docs/json-schema/run-metrics.schema.json")).unwrap(),
    )
    .unwrap();
    assert_schema_valid(&metrics_schema, &metrics_value);
    let metrics: hoimin_core::RunMetrics = serde_json::from_str(&metrics_text).unwrap();
    metrics.validate().unwrap();
    assert_eq!(metrics.run_id, run.document["run"]["run_id"]);
    assert_eq!(
        metrics
            .stages
            .iter()
            .map(|stage| stage.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "analysis",
            "baseline",
            "cleanup",
            "copy",
            "mutants",
            "preflight",
            "targets"
        ]
    );
    assert_eq!(metrics.discovered, 2);
    assert_eq!(metrics.executed, 2);
    assert!(metrics.workers.iter().all(|worker| worker.processes > 0));
    assert!(
        metrics
            .workers
            .windows(2)
            .all(|workers| workers[0].worker < workers[1].worker)
    );
}

fn assert_schema_valid(schema: &serde_json::Value, instance: &serde_json::Value) {
    if let Err(error) = validate_schema(schema, instance, schema, "$") {
        panic!("schema validation failed: {error}\ninstance: {instance}");
    }
}

fn validate_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    root: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{path}: unsupported schema reference {reference}"))?;
        let target = root
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: unresolved schema reference {reference}"))?;
        return validate_schema(target, instance, root, path);
    }
    if let Some(expected) = schema.get("const")
        && instance != expected
    {
        return Err(format!("{path}: expected const {expected}, got {instance}"));
    }
    if let Some(kind) = schema.get("type").and_then(serde_json::Value::as_str) {
        let matches = match kind {
            "object" => instance.is_object(),
            "array" => instance.is_array(),
            "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
            "string" => instance.is_string(),
            _ => false,
        };
        if !matches {
            return Err(format!("{path}: {instance} does not have type {kind}"));
        }
    }
    if let Some(minimum) = schema.get("minimum").and_then(serde_json::Value::as_f64)
        && instance.as_f64().is_some_and(|value| value < minimum)
    {
        return Err(format!("{path}: number is below {minimum}"));
    }
    if let Some(object) = instance.as_object() {
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object);
        if let Some(required) = schema.get("required").and_then(serde_json::Value::as_array) {
            for name in required.iter().filter_map(serde_json::Value::as_str) {
                if !object.contains_key(name) {
                    return Err(format!("{path}: missing required property {name}"));
                }
            }
        }
        if let Some(properties) = properties {
            for (name, value) in object {
                let property_schema = properties
                    .get(name)
                    .ok_or_else(|| format!("{path}: unexpected property {name}"))?;
                validate_schema(property_schema, value, root, &format!("{path}.{name}"))?;
            }
        }
    }
    if let (Some(items), Some(values)) = (schema.get("items"), instance.as_array()) {
        for (index, value) in values.iter().enumerate() {
            validate_schema(items, value, root, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn metrics_write_failure_warns_without_changing_run_result() {
    let directory = tempfile::tempdir().unwrap();
    let ordinary = run_fixture_options_extra(
        &["-m", "pytest", "-q", "tests"],
        None,
        false,
        &["--jobs", "2", "--max-mutants", "2"],
    )
    .await;
    let failed = run_fixture_options_extra(
        &["-m", "pytest", "-q", "tests"],
        None,
        false,
        &[
            "--jobs",
            "2",
            "--max-mutants",
            "2",
            "--metrics",
            directory.path().to_str().unwrap(),
        ],
    )
    .await;

    assert_eq!(failed.exit_code, ordinary.exit_code);
    assert_eq!(failed.statuses, ordinary.statuses);
    assert_eq!(
        failed.document["summary"]["counts"],
        ordinary.document["summary"]["counts"]
    );
    assert_eq!(
        failed.document["summary"]["exit_code"],
        ordinary.document["summary"]["exit_code"]
    );
    assert_eq!(
        failed
            .document
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ordinary
            .document
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>()
    );
    assert!(directory.path().is_dir());
    assert!(failed.stderr.contains("metrics.write"), "{}", failed.stderr);
}

#[tokio::test]
async fn metrics_finish_baseline_timing_before_early_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let metrics_path = directory.path().join("metrics.json");
    let run = run_fixture_options_extra(
        &["-c", "raise SystemExit(1)"],
        None,
        false,
        &["--metrics", metrics_path.to_str().unwrap()],
    )
    .await;

    assert_eq!(run.exit_code, 3, "stderr={}", run.stderr);
    let metrics: hoimin_core::RunMetrics =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    metrics.validate().unwrap();
    assert!(metrics.stages.iter().any(|stage| stage.name == "baseline"));
    assert!(metrics.stages.iter().any(|stage| stage.name == "cleanup"));
    assert!(!metrics.stages.iter().any(|stage| stage.name == "analysis"));
    assert!(!metrics.stages.iter().any(|stage| stage.name == "mutants"));
}

#[tokio::test]
async fn failing_baseline_runs_no_mutants_and_returns_three() {
    let run = run_fixture(&["-c", "raise SystemExit(1)"]).await;

    assert_eq!(run.exit_code, 3);
    assert!(run.statuses.is_empty());
}

#[tokio::test]
async fn import_only_match_negative_literal_has_no_unary_mutant_to_kill() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("subject.py"),
        concat!(
            "def classify(value):\n",
            "    match value:\n",
            "        case -1:\n            return 'negative one'\n",
            "        case _:\n            return 'other'\n",
        ),
    )
    .unwrap();
    let python = python_executable();

    let plan = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["plan", "--root"])
        .arg(project.path())
        .args([
            "--file",
            "subject.py",
            "--operators",
            "unary_sign",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(&python)
        .args(["-c", "from subject import classify"])
        .output()
        .await
        .unwrap();
    assert!(
        plan.status.success(),
        "plan stderr={}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let manifest: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(manifest["candidates"], serde_json::json!([]));

    let run = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["run", "--min-free-space", TEST_MIN_FREE_SPACE, "--root"])
        .arg(project.path())
        .args([
            "--file",
            "subject.py",
            "--operators",
            "unary_sign",
            "--format",
            "json",
            "--allow-best-effort-memory",
            "--",
        ])
        .arg(&python)
        .args(["-c", "from subject import classify"])
        .output()
        .await
        .unwrap();
    assert!(
        run.status.success(),
        "run stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report["mutants"], serde_json::json!([]));
    assert_eq!(report["summary"]["counts"]["killed"], 0);
}

#[tokio::test]
async fn session_is_not_created_when_the_option_is_absent_and_stdout_is_one_json_document() {
    let run = run_fixture(&["-m", "pytest", "-q", "tests"]).await;

    assert!(!fixture_root().join(".hoimin.sqlite3").exists());
    assert_eq!(run.stdout.lines().count(), 1);
    assert!(
        run.stderr.is_empty(),
        "unexpected diagnostics: {}",
        run.stderr
    );
}

#[tokio::test]
async fn fingerprint_include_unmatched_fails_before_creating_session() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let root = fixture_root();
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        database.as_os_str().to_owned(),
        OsString::from("--fingerprint-include"),
        OsString::from("missing.toml"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-m"),
        OsString::from("pytest"),
        OsString::from("-q"),
        OsString::from("tests"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stderr = String::from_utf8(stderr).unwrap();

    assert_eq!(exit_code, 2);
    assert!(stderr.contains("fingerprint.include.unmatched"));
    assert!(!database.exists());
}

#[tokio::test]
async fn fingerprint_file_missing_fails_before_creating_session() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let root = fixture_root();
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        database.as_os_str().to_owned(),
        OsString::from("--fingerprint-file"),
        OsString::from("missing.toml"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-m"),
        OsString::from("pytest"),
        OsString::from("-q"),
        OsString::from("tests"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(exit_code, 2);
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("fingerprint.file.not_found")
    );
    assert!(!database.exists());
}

#[tokio::test]
async fn fingerprint_include_is_reported() {
    let options = ["--fingerprint-include", "pyproject.toml"];
    let test_args = ["-m", "pytest", "-q", "tests"];

    let json = run_fixture_options_extra(&test_args, None, false, &options).await;
    assert_eq!(json.exit_code, 0, "stderr={}", json.stderr);
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_includes"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_inputs"][0]["path"],
        "pyproject.toml"
    );

    let jsonl =
        run_fixture_options_extra_with_format(&test_args, None, false, "jsonl", &options).await;
    assert_eq!(jsonl.exit_code, 0, "stderr={}", jsonl.stderr);
    let run_started = jsonl
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["kind"] == "run_started")
        .unwrap();
    assert_eq!(
        run_started["normalized_config"]["fingerprint_includes"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        run_started["normalized_config"]["fingerprint_inputs"][0]["path"],
        "pyproject.toml"
    );
}

#[tokio::test]
async fn fingerprint_file_is_reported() {
    let options = ["--fingerprint-file", "pyproject.toml"];
    let test_args = ["-m", "pytest", "-q", "tests"];

    let json = run_fixture_options_extra(&test_args, None, false, &options).await;
    assert_eq!(json.exit_code, 0, "stderr={}", json.stderr);
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_files"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| record["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["pyproject.toml"]
    );

    let jsonl =
        run_fixture_options_extra_with_format(&test_args, None, false, "jsonl", &options).await;
    assert_eq!(jsonl.exit_code, 0, "stderr={}", jsonl.stderr);
    let run_started = jsonl
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["kind"] == "run_started")
        .unwrap();
    assert_eq!(
        run_started["normalized_config"]["fingerprint_files"],
        serde_json::json!(["pyproject.toml"])
    );
}

#[tokio::test]
async fn shell_context_construction_performs_no_project_io() {
    let directory = tempfile::tempdir().unwrap();
    let missing_root = directory.path().join("missing-project");
    let session = missing_root.join("session.sqlite3");
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        missing_root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        OsString::from("test-command"),
    ])
    .unwrap();
    let context = hoimin_cli::shell::ShellContext::new(&config, Vec::new(), Vec::new())
        .await
        .unwrap();

    assert!(!missing_root.exists());
    assert!(!session.exists());
    assert!(!missing_root.join(".session.sqlite3.hoimin-locks").exists());
    drop(context);
}

#[tokio::test]
async fn sqlite_session_saves_and_resumes_a_determinate_result_without_reexecution() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let first = run_fixture_with_session(&["-m", "pytest", "-q", "tests"], &database, false).await;
    assert_eq!(
        first.exit_code, 0,
        "stderr={} stdout={}",
        first.stderr, first.stdout
    );
    assert_eq!(first.statuses, ["killed"]);
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);

    let resumed = run_fixture_with_session(&["-m", "pytest", "-q", "tests"], &database, true).await;
    assert_eq!(resumed.exit_code, 0);
    assert_eq!(resumed.statuses, ["killed"]);
    let resumed_run_id = resumed.document["run"]["run_id"].as_str().unwrap();
    assert_eq!(
        resumed.document["baseline"]["run_id"].as_str().unwrap(),
        resumed_run_id
    );
    assert_eq!(
        resumed.document["mutants"][0]["run_id"].as_str().unwrap(),
        resumed_run_id
    );
    assert_eq!(
        resumed.document["summary"]["run_id"].as_str().unwrap(),
        resumed_run_id
    );
}

#[tokio::test]
async fn sqlite_session_reuses_results_after_jobs_and_output_retention_change() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let command = ["-m", "pytest", "-q", "tests"];
    let first = run_fixture_options_extra(
        &command,
        Some(&database),
        false,
        &["--jobs", "1", "--max-output", "1KiB"],
    )
    .await;
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    assert_eq!(first.statuses, ["killed"]);
    let first_run_id = first.document["run"]["run_id"].as_str().unwrap().to_owned();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);

    let resumed = run_fixture_options_extra(
        &command,
        Some(&database),
        true,
        &["--jobs", "2", "--max-output", "2KiB"],
    )
    .await;

    assert_eq!(resumed.exit_code, 0, "{}", resumed.stderr);
    assert_eq!(resumed.statuses, ["killed"]);
    assert_eq!(
        resumed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );
    assert_eq!(
        resumed.document["run"]["normalized_config"]["limits"]["jobs"],
        2
    );
    assert_eq!(
        resumed.document["run"]["normalized_config"]["limits"]["max_output"],
        2 * 1_024
    );
    assert_eq!(
        resumed.document["mutants"][0]["termination"],
        serde_json::Value::Null,
        "a reused result is emitted synthetically without process termination"
    );
    assert_eq!(
        resumed.document["mutants"][0]["output"],
        serde_json::Value::Null,
        "a reused result does not import output retained under the earlier limit"
    );
    let connection = rusqlite::Connection::open(database).unwrap();
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 1);
}

#[tokio::test]
async fn fresh_session_and_sessionless_results_preserve_the_same_termination() {
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    let command = ["-m", "pytest", "-q", "tests"];

    let sessionless = run_fixture_options(&command, None, false).await;
    let session = run_fixture_with_session(&command, &database, false).await;
    assert_eq!(sessionless.exit_code, 0, "{}", sessionless.stderr);
    assert_eq!(session.exit_code, 0, "{}", session.stderr);

    let normalize = |mut value: serde_json::Value| {
        value["run_id"] = serde_json::Value::Null;
        value["elapsed_ms"] = serde_json::Value::Null;
        value["output"]["token"] = serde_json::Value::Null;
        value
    };
    let sessionless_mutant = normalize(sessionless.document["mutants"][0].clone());
    let session_mutant = normalize(session.document["mutants"][0].clone());
    assert_eq!(sessionless_mutant, session_mutant);
    assert_eq!(
        session_mutant["termination"],
        serde_json::json!({ "Exit": 1 })
    );

    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);
    let reused = run_fixture_with_session(&command, &database, true).await;
    assert_eq!(reused.exit_code, 0, "{}", reused.stderr);
    assert_eq!(
        reused.document["mutants"][0]["termination"],
        serde_json::Value::Null
    );
}

#[cfg(unix)]
#[tokio::test]
async fn concurrent_real_cli_runs_refuse_live_session_ownership() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let active = coordinator.path().join("active");
    std::fs::create_dir(&active).unwrap();
    let readiness = coordinator.path().join("ready");
    let readiness_temp = coordinator.path().join("ready.tmp");
    let duplicate_execution = coordinator.path().join("duplicate-execution");
    let session = project.path().join("session.sqlite3");
    let original = "return a + b + c + d + e";
    let mutation_command = format!(
        "from pathlib import Path; import os,time; active=Path({:?},str(os.getpid())); active.write_text('running'); ready=Path({:?}); ready_temp=Path({:?}); duplicate=Path({:?})\ntry:\n    if ready.exists(): duplicate.write_text(str(os.getpid()))\n    else:\n        ready_temp.write_text(str(os.getpid()))\n        ready_temp.replace(ready)\n        while True: time.sleep(60)\nfinally:\n    active.unlink(missing_ok=True)",
        active.to_string_lossy(),
        readiness.to_string_lossy(),
        readiness_temp.to_string_lossy(),
        duplicate_execution.to_string_lossy(),
    );
    let command = format!(
        "from pathlib import Path; source=Path('src/calc.py').read_text(); exec({mutation_command:?}) if {original:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')",
    );
    let first_args = real_cli_session_args(project.path(), &session, false, &command);
    let mut first = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(&first_args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let outcome: Result<_, String> = async {
        let incomplete_runs = wait_for_live_session_readiness(
            &readiness,
            &session,
            &mut first,
            Duration::from_secs(15),
        )
        .await?;
        let second_args = real_cli_session_args(project.path(), &session, true, &command);
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
                .args(&second_args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| "resuming hoimin process did not exit".to_owned())?
        .map_err(|error| error.to_string())?;
        let second = RealCliOutput {
            exit_code: output
                .status
                .code()
                .ok_or_else(|| "resuming hoimin process exited by signal".to_owned())?,
            stdout: String::from_utf8(output.stdout).map_err(|error| error.to_string())?,
            stderr: String::from_utf8(output.stderr).map_err(|error| error.to_string())?,
        };
        send_sigint(first.id())?;
        let first_status = tokio::time::timeout(Duration::from_secs(15), first.wait())
            .await
            .map_err(|_| "first hoimin process did not exit after SIGINT".to_owned())?
            .map_err(|error| error.to_string())?;
        Ok((incomplete_runs, second, first_status))
    }
    .await;

    let child_cleanup = reap_test_child(&mut first).await;
    let process_cleanup = kill_fixture_processes(&active, &readiness).await;
    if let Err(error) = child_cleanup.and(process_cleanup) {
        panic!("test teardown failed: {error}; outcome={outcome:?}");
    }
    let (incomplete_runs, second, first_status) = outcome.unwrap_or_else(|error| {
        panic!("session ownership scenario failed after successful teardown: {error}")
    });

    assert_eq!(incomplete_runs, 1);
    assert_eq!(second.exit_code, 2);
    let second_document: serde_json::Value =
        serde_json::from_str(&second.stdout).expect("refused process must emit a JSON report");
    assert_eq!(second_document["summary"]["complete"], false);
    assert!(
        second.stderr.contains("session.resume.active"),
        "{}",
        second.stderr
    );
    assert!(
        second.stderr.contains("active in another process"),
        "{}",
        second.stderr
    );
    assert_eq!(first_status.code(), Some(130));
    assert!(
        !duplicate_execution.exists(),
        "refused process must not execute the mutation command"
    );
    assert_live_session_database_integrity(&session);
}

#[tokio::test]
async fn copy_policy_changes_do_not_reuse_old_verdicts() {
    let _parallel_test_guard = parallel_project_test_guard().await;
    for (use_include, before_present, after_present) in [
        (false, true, false),
        (false, false, true),
        (true, true, false),
        (true, false, true),
        (false, true, true),
        (false, false, false),
    ] {
        let project = tempfile::tempdir().unwrap();
        let sessions = tempfile::tempdir().unwrap();
        let database = sessions.path().join("saved.sqlite3");
        write_parallel_project(project.path());
        std::fs::write(project.path().join("strict.flag"), "strict\n").unwrap();
        if use_include {
            std::fs::write(project.path().join(".ignore"), "strict.flag\n").unwrap();
        }
        let options = |present| {
            let mut options = vec!["--jobs", "1", "--fingerprint-file", "strict.flag"];
            if use_include {
                options.extend(["--include", "src/**"]);
                if present {
                    options.extend(["--include", "strict.flag"]);
                }
            } else if !present {
                options.extend(["--exclude", "strict.flag"]);
            }
            options
        };
        let command = "from pathlib import Path; from src.calc import total; assert not Path('strict.flag').exists() or total(1, 2, 3, 4, 5) == 15";
        let first = run_project_with_session_options(
            project.path(),
            &database,
            false,
            1,
            command,
            &options(before_present),
        )
        .await;
        let resumed = run_project_with_session_options(
            project.path(),
            &database,
            true,
            1,
            command,
            &options(after_present),
        )
        .await;
        let fresh = run_project_with_session_options(
            project.path(),
            &sessions.path().join("fresh.sqlite3"),
            false,
            1,
            command,
            &options(after_present),
        )
        .await;
        for run in [&first, &resumed, &fresh] {
            assert_eq!(run.exit_code, 4, "{}", run.stderr);
            assert_eq!(
                run.document["baseline"]["termination"],
                serde_json::json!({"Exit": 0})
            );
        }
        assert_eq!(
            first.statuses[0],
            if before_present { "killed" } else { "survived" }
        );
        assert_eq!(
            fresh.statuses[0],
            if after_present { "killed" } else { "survived" }
        );
        assert_eq!(
            resumed.statuses, fresh.statuses,
            "include={use_include}, presence {before_present}->{after_present}"
        );
        assert_eq!(
            first.document["mutants"][0]["candidate"]["id"],
            resumed.document["mutants"][0]["candidate"]["id"]
        );
        if before_present == after_present {
            assert_eq!(
                first.document["run"]["run_id"],
                resumed.document["run"]["run_id"]
            );
            assert!(resumed.document["mutants"][0]["termination"].is_null());
        } else {
            assert_ne!(
                first.document["run"]["run_id"],
                resumed.document["run"]["run_id"]
            );
            assert!(!resumed.document["mutants"][0]["termination"].is_null());
        }
    }
}

#[tokio::test]
async fn fingerprint_include_change_starts_a_distinct_session_run() {
    let project = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    let watched = project.path().join("watched.toml");
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    std::fs::write(&watched, "value = 1\n").unwrap();
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";
    let fingerprint_include = ["--fingerprint-include", "watched.toml"];

    let first = run_project_with_session_options(
        project.path(),
        &database,
        false,
        1,
        command,
        &fingerprint_include,
    )
    .await;
    assert_eq!(first.exit_code, 4, "stderr={}", first.stderr);
    let first_run_id = first.document["run"]["run_id"].as_str().unwrap().to_owned();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);
    std::fs::write(&watched, "value = 2\n").unwrap();

    let resumed = run_project_with_session_options(
        project.path(),
        &database,
        true,
        1,
        command,
        &fingerprint_include,
    )
    .await;

    assert_eq!(resumed.exit_code, 4, "stderr={}", resumed.stderr);
    assert_ne!(
        resumed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );
    let connection = rusqlite::Connection::open(&database).unwrap();
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 2);
}

#[tokio::test]
async fn fingerprint_file_ignores_nested_names_but_tracks_the_exact_file() {
    let project = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    std::fs::write(project.path().join("pyproject.toml"), "value = 1\n").unwrap();
    let nested = project.path().join(".worktrees/a/pyproject.toml");
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    std::fs::write(&nested, "nested = 1\n").unwrap();
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";
    let options = ["--fingerprint-file", "pyproject.toml"];

    let first =
        run_project_with_session_options(project.path(), &database, false, 1, command, &options)
            .await;
    assert_eq!(first.exit_code, 4, "stderr={}", first.stderr);
    let first_run_id = first.document["run"]["run_id"].as_str().unwrap().to_owned();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);

    std::fs::write(&nested, "nested = 2\n").unwrap();
    let resumed =
        run_project_with_session_options(project.path(), &database, true, 1, command, &options)
            .await;
    assert_eq!(resumed.exit_code, 4, "stderr={}", resumed.stderr);
    assert_eq!(
        resumed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );

    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);
    std::fs::write(project.path().join("pyproject.toml"), "value = 2\n").unwrap();
    let changed =
        run_project_with_session_options(project.path(), &database, true, 1, command, &options)
            .await;
    assert_eq!(changed.exit_code, 4, "stderr={}", changed.stderr);
    assert_ne!(
        changed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );
}

#[tokio::test]
async fn sqlite_session_can_be_resumed_after_repeated_mutant_limits() {
    let project = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";

    let mut run_id = None;
    for resume in [false, true, true] {
        let run = run_project_with_session(project.path(), &database, resume, 1, command).await;
        assert_eq!(run.exit_code, 4, "stderr={}", run.stderr);
        assert!(!run.stderr.contains("session.finish.state"));
        let connection = rusqlite::Connection::open(&database).unwrap();
        let current_run_id = run.document["run"]["run_id"].as_str().unwrap();
        if let Some(first_run_id) = &run_id {
            assert_eq!(current_run_id, first_run_id);
        } else {
            run_id = Some(current_run_id.to_owned());
        }
        let (run_count, complete): (i64, i64) = connection
            .query_row("SELECT COUNT(*), MAX(complete) FROM runs", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(run_count, 1);
        assert_eq!(complete, 0);
    }
}

#[tokio::test]
async fn focused_profile_is_reported_and_omits_arid_candidates() {
    let project = focused_profile_project();
    let run = run_focused_profile(project.path(), "focused", "json", None, false, 100).await;

    assert_eq!(
        run.document["run"]["normalized_config"]["profile"],
        "focused"
    );
    let lines: Vec<_> = run.document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["candidate"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![4, 9]);

    let jsonl = run_focused_profile(project.path(), "focused", "jsonl", None, false, 100).await;
    let run_started = jsonl
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["kind"] == "run_started")
        .unwrap();
    assert_eq!(run_started["normalized_config"]["profile"], "focused");

    let human = run_focused_profile(project.path(), "focused", "human", None, false, 100).await;
    assert!(
        human
            .stdout
            .lines()
            .next()
            .unwrap()
            .contains("(profile: focused)"),
        "human output was: {}",
        human.stdout
    );
}

#[tokio::test]
async fn focused_profile_does_not_resume_full_profile_session() {
    let project = focused_profile_project();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");

    let full = run_focused_profile(project.path(), "full", "json", Some(&database), false, 1).await;
    assert_eq!(full.exit_code, 4, "stderr={}", full.stderr);
    assert_eq!(full.document["summary"]["complete"], false);

    let focused =
        run_focused_profile(project.path(), "focused", "json", Some(&database), true, 1).await;
    assert_eq!(focused.exit_code, 4, "stderr={}", focused.stderr);
    assert_eq!(focused.document["summary"]["complete"], false);

    let connection = rusqlite::Connection::open(database).unwrap();
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 2);
}

#[test]
fn readme_documents_focused_profile_selection_and_session_compatibility() {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let windows_readme = readme.replace("\r\n", "\n").replace('\n', "\r\n");

    for documented_readme in [&readme, &windows_readme] {
        let documented_readme = documented_readme.replace("\r\n", "\n");

        assert!(documented_readme.contains("`--profile full` / `--profile focused`"));
        assert!(documented_readme.contains(
            "`--profile full` is the default and considers every candidate produced by the selected\noperators."
        ));
        assert!(documented_readme.contains("Python `__main__` guards, bare\n`print(...)` calls, `assert` statements, and function default expressions."));
        assert!(documented_readme.contains(
            "Profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa."
        ));
    }
}

#[test]
fn readme_documents_agent_plan_workflow() {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let documented_readme = readme.replace("\r\n", "\n");

    for expected in [
        "hoimin plan",
        "--allow-best-effort-memory",
        "--total-timeout 15m",
        "hoimin verify PLAN.json --top 10",
        "hoimin verify PLAN.json --top 10 --selection-policy diverse",
        "The `strict` selection policy is the default and uses the saved rank prefix.",
        "The `diverse` selection policy round-robins production files only within equal-score tiers.",
        "Higher-score tiers are exhausted before lower-score tiers.",
        "The verification report records `file_round_robin_v1`.",
        "Verification does not rewrite the plan or change its execution limits.",
        "`ranking_reasons`",
        "mutually exclusive",
        "inherits the test command, execution limits, timeout settings, and resource policy",
        "create a new plan",
        "identical candidate-ID set",
        "After every test improvement, rerun every stable batch and save each report separately.",
        "same current test revision",
        "must not be combined",
        "`--fingerprint-include GLOB`",
        "`truncated`",
    ] {
        assert!(documented_readme.contains(expected), "missing {expected}");
    }
}

#[tokio::test]
async fn sqlite_save_failure_reports_the_classification_but_leaves_no_partial_database_result() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let first = run_fixture_with_session(&["-m", "pytest", "-q", "tests"], &database, false).await;
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "UPDATE runs SET complete=0;
         DELETE FROM results;
         DELETE FROM candidates;
         CREATE TRIGGER fail_result BEFORE INSERT ON results
         BEGIN SELECT RAISE(ABORT, 'injected save failure'); END;",
        )
        .unwrap();
    drop(connection);

    let failed = run_fixture_with_session(&["-m", "pytest", "-q", "tests"], &database, true).await;
    assert_eq!(failed.exit_code, 2);
    assert_eq!(failed.document["summary"]["counts"]["killed"], 1);
    let mutants = failed.document["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0]["status"], "killed");
    assert!(failed.stderr.contains("session"), "{}", failed.stderr);
    let connection = rusqlite::Connection::open(&database).unwrap();
    let results: i64 = connection
        .query_row("SELECT COUNT(*) FROM results", [], |row| row.get(0))
        .unwrap();
    assert_eq!(results, 0);
    let candidates: i64 = connection
        .query_row("SELECT COUNT(*) FROM candidates", [], |row| row.get(0))
        .unwrap();
    assert_eq!(candidates, 0);
}

#[tokio::test]
async fn total_timeout_cancels_and_reaps_descendants_before_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("descendant-survived");
    let child = format!(
        "import pathlib,time; time.sleep(2); pathlib.Path({:?}).write_text('leak')",
        marker.to_string_lossy()
    );
    let command = format!(
        "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',{child:?}]); time.sleep(20)"
    );
    let started = Instant::now();
    let run =
        run_fixture_options_extra(&["-c", &command], None, false, &["--total-timeout", "1s"]).await;

    assert_eq!(
        run.exit_code, 4,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert!(!marker.exists(), "timed-out descendant outlived the run");
    assert_eq!(run.document["summary"]["complete"], false);
}

#[cfg(any(unix, windows))]
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "the subprocess assertion keeps lock ownership, pipe drains, and failure-safe teardown in one scope"
)]
async fn total_timeout_exits_after_grace_when_session_finish_is_locked() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let session = coordinator.path().join("session.sqlite3");
    let (mut child, active, descendant_ready) = spawn_interrupt_fixture(
        project.path(),
        coordinator.path(),
        &session,
        "jsonl",
        &["--operators", "binary_add_sub", "--total-timeout", "5s"],
    );
    let stdout_pipe = child.stdout.take().unwrap();
    let stderr_pipe = child.stderr.take().unwrap();
    let (mutant_started_tx, mutant_started_rx) = tokio::sync::oneshot::channel();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout_pipe);
        let mut stdout = Vec::new();
        let mut mutant_started_tx = Some(mutant_started_tx);
        loop {
            let mut line = Vec::new();
            let bytes = reader.read_until(b'\n', &mut line).await?;
            if bytes == 0 {
                if let Some(mutant_started_tx) = mutant_started_tx.take() {
                    let _ = mutant_started_tx
                        .send(Err("stdout closed before mutant_started".to_owned()));
                }
                break;
            }
            if mutant_started_tx.is_some()
                && serde_json::from_slice::<serde_json::Value>(&line)
                    .is_ok_and(|event| event["kind"] == "mutant_started")
            {
                let sender = mutant_started_tx.take().expect("sender checked above");
                let _ = sender.send(Ok(Instant::now()));
            }
            stdout.extend_from_slice(&line);
        }
        Ok::<_, io::Error>(stdout)
    });
    let stderr_task = tokio::spawn(async move {
        let mut reader = stderr_pipe;
        let mut stderr = Vec::new();
        reader.read_to_end(&mut stderr).await.map(|_| stderr)
    });
    let mut fixture_processes = None;
    let outcome: Result<_, String> = async {
        // Preparation before mutant_started is outside this shutdown scenario's
        // assertion budget. Once observed, every readiness and exit wait shares
        // one absolute deadline rather than accumulating relative timeouts.
        let mutant_started_at = tokio::time::timeout(Duration::from_secs(7), mutant_started_rx)
            .await
            .map_err(|_| "timed out waiting for JSONL kind mutant_started".to_owned())?
            .map_err(|_| "stdout drain task stopped before mutant_started".to_owned())??;
        let scenario_deadline = mutant_started_at + Duration::from_secs(9);
        fixture_processes = Some(
            try_wait_for_fixture_processes(
                &active,
                &descendant_ready,
                scenario_deadline.saturating_duration_since(Instant::now()),
            )
            .await?,
        );
        let lock = begin_immediate_with_retry(&session, Duration::from_secs(1)).await?;
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("hoimin exited before the session write lock was retained".to_owned());
        }
        let status = tokio::time::timeout_at(scenario_deadline.into(), child.wait())
            .await
            .map_err(|_| "hoimin exceeded total timeout plus shutdown grace".to_owned())?
            .map_err(|error| error.to_string())?;
        let descendant_stopped = fixture_processes
            .as_ref()
            .expect("assigned above")
            .descendant
            .wait_until_stops(Duration::from_secs(2))
            .await;
        Ok((status, lock, descendant_stopped))
    }
    .await;

    let child_cleanup = reap_test_child(&mut child).await;
    #[cfg(unix)]
    let process_cleanup = kill_fixture_processes(&active, &descendant_ready).await;
    #[cfg(windows)]
    let process_cleanup = kill_fixture_processes(fixture_processes.as_ref()).await;
    let stdout = stdout_task
        .await
        .expect("stdout drain task must not panic")
        .map_err(|error| error.to_string());
    let stderr = stderr_task
        .await
        .expect("stderr drain task must not panic")
        .map_err(|error| error.to_string());
    if let Err(error) = child_cleanup.and(process_cleanup) {
        panic!("test teardown failed: {error}; outcome={outcome:?}");
    }
    let (status, lock, descendant_stopped) = outcome.unwrap_or_else(|error| {
        panic!("locked-session timeout scenario failed after successful teardown: {error}")
    });
    let stdout = String::from_utf8(
        stdout.unwrap_or_else(|error| panic!("could not drain JSONL stdout: {error}")),
    )
    .unwrap();
    let stderr = String::from_utf8(
        stderr.unwrap_or_else(|error| panic!("could not drain timeout stderr: {error}")),
    )
    .unwrap();

    assert_eq!(status.code(), Some(2), "stderr={stderr} stdout={stdout}");
    let events = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_one_incomplete_run_finished(&events, &stdout);
    let complete: i64 = lock
        .query_row("SELECT complete FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(complete, 0);
    assert!(!lock.is_autocommit());
    assert!(descendant_stopped, "timed-out descendant outlived the run");
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "the real cancellation fixture keeps process-tree and incomplete-report assertions in one scope"
)]
async fn injected_ctrl_c_finishes_incomplete_or_defers_when_tree_quiescence_is_unproven() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let descendant_ready = coordinator.path().join("descendant-ready");
    let descendant_heartbeats = coordinator.path().join("descendant-heartbeats");
    std::fs::create_dir(&descendant_ready).unwrap();
    std::fs::create_dir(&descendant_heartbeats).unwrap();
    let session = coordinator.path().join("session.sqlite3");
    let child = "from pathlib import Path\nimport sys,time\nheartbeat=Path(sys.argv[1],sys.argv[2])\nwhile True:\n heartbeat.write_text(str(time.monotonic_ns()))\n time.sleep(0.05)";
    let mutant = format!(
        "from pathlib import Path\nimport os,subprocess,sys,time\ntoken=str(os.getpid())\nchild=subprocess.Popen([sys.executable,'-c',{:?},{:?},token])\nPath({:?},token).write_text(str(child.pid))\ntime.sleep(20)",
        child,
        descendant_heartbeats.to_string_lossy(),
        descendant_ready.to_string_lossy(),
    );
    let original = "return a + b + c + d + e";
    let command = format!(
        "from pathlib import Path; source=Path('src/calc.py').read_text(); exec({mutant:?}) if {original:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')",
    );
    let python = python_executable();
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("4"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ])
    .unwrap();
    let stdout = SharedBuffer::default();
    let stderr = SharedBuffer::default();
    let control = hoimin_cli::shell::RunControl::new();
    let run = hoimin_cli::shell::run_loop_with_control(
        config,
        stdout.clone(),
        stderr.clone(),
        control.clone(),
    );
    let cancel = async {
        let descendants =
            wait_for_tokenized_descendant_processes(&descendant_ready, 4, Duration::from_secs(15))
                .await;
        let heartbeat_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while descendants
            .iter()
            .any(|(token, _)| std::fs::read(descendant_heartbeats.join(token)).is_err())
        {
            let readiness = descendants
                .iter()
                .map(|(token, descendant)| {
                    (
                        token,
                        descendant.pid(),
                        descendant.is_alive(),
                        descendant_heartbeats.join(token).exists(),
                    )
                })
                .collect::<Vec<_>>();
            assert!(
                tokio::time::Instant::now() < heartbeat_deadline,
                "every descendant did not publish its heartbeat before cancellation: {readiness:?}; stderr={}",
                String::from_utf8_lossy(&stderr.bytes())
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        control.cancel();
        descendants
    };
    let (exit, descendants) = Box::pin(tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(run, cancel)
    }))
    .await
    .expect("cancelled run must finish promptly");
    let cleanup_deferred = match exit {
        Ok(exit) => {
            assert_eq!(exit, 130);
            false
        }
        Err(error) => {
            assert!(
                error.contains(hoimin_core::WORKSPACE_CLEANUP_DEFERRED),
                "{error}"
            );
            true
        }
    };
    let document: serde_json::Value =
        serde_json::from_slice(&stdout.bytes()).expect("parseable cancelled report");
    assert_eq!(document["summary"]["complete"], false);
    if cleanup_deferred {
        assert_eq!(
            document["summary"]["disk"]["stop"]["code"],
            hoimin_core::PROCESS_LIFECYCLE_FAILED
        );
    }
    let not_run_count = document["summary"]["counts"]["not_run"]
        .as_u64()
        .expect("not_run summary count");
    let not_run_mutants = document["mutants"]
        .as_array()
        .expect("cancelled mutant array")
        .iter()
        .filter(|mutant| mutant["status"] == "not_run")
        .count() as u64;
    assert!(not_run_count > 0);
    assert_eq!(not_run_count, not_run_mutants);
    assert!(control.max_process_tasks() <= 4);
    assert!(control.max_completion_in_flight() <= 5);
    let connection = rusqlite::Connection::open(session).unwrap();
    let complete: i64 = connection
        .query_row("SELECT complete FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(complete, 0);
    drop(connection);
    for (token, descendant) in descendants {
        if !descendant
            .wait_until_stops(Duration::from_millis(500))
            .await
        {
            let heartbeat = descendant_heartbeats.join(token);
            let before = std::fs::read(&heartbeat).unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            let after = std::fs::read(&heartbeat).unwrap();
            assert_eq!(
                before,
                after,
                "cancelled descendant {} continued executing after the run",
                descendant.pid()
            );
        }
    }
}

async fn first_interrupt_scenario() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let session = coordinator.path().join("session.sqlite3");
    let (mut child, active, descendant_ready) =
        spawn_interrupt_fixture(project.path(), coordinator.path(), &session, "json", &[]);
    // Drain stdout while the child is running; a complete JSON report can exceed the pipe buffer.
    let mut stdout_reader = child.stdout.take().unwrap();
    let stdout_task = tokio::spawn(async move {
        let mut stdout = Vec::new();
        stdout_reader.read_to_end(&mut stdout).await.map(|_| stdout)
    });
    let mut fixture_processes = None;
    let outcome: Result<_, String> = async {
        fixture_processes = Some(
            try_wait_for_fixture_processes(&active, &descendant_ready, Duration::from_secs(15))
                .await?,
        );
        wait_for_live_session_readiness(
            &descendant_ready,
            &session,
            &mut child,
            Duration::from_secs(15),
        )
        .await?;

        send_fixture_interrupt(child.id()).await?;
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .map_err(|_| "hoimin did not finish after first interrupt".to_owned())?
            .map_err(|error| error.to_string())?;
        let descendant_stopped = fixture_processes
            .as_ref()
            .expect("assigned above")
            .descendant
            .wait_until_stops(Duration::from_secs(5))
            .await;
        Ok((status, descendant_stopped))
    }
    .await;

    let child_cleanup = reap_test_child(&mut child).await;
    #[cfg(unix)]
    let process_cleanup = kill_fixture_processes(&active, &descendant_ready).await;
    #[cfg(windows)]
    let process_cleanup = kill_fixture_processes(fixture_processes.as_ref()).await;
    let stdout = stdout_task
        .await
        .expect("stdout drain task must not panic")
        .map_err(|error| error.to_string())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()));
    if let Err(error) = child_cleanup.and(process_cleanup) {
        panic!("test teardown failed: {error}; outcome={outcome:?}");
    }
    let (status, descendant_stopped) = outcome.unwrap_or_else(|error| {
        panic!("first-interrupt scenario failed after successful teardown: {error}")
    });
    let stdout = stdout.unwrap_or_else(|error| panic!("cancelled stdout was not UTF-8: {error}"));

    assert_eq!(status.code(), Some(130));
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("cancelled stdout must be JSON ({error}): {stdout:?}"));
    assert_eq!(report["summary"]["complete"], false);
    assert_eq!(session_complete(&session), 0);
    assert!(
        descendant_stopped,
        "first interrupt did not reap the ready descendant"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn first_sigint_finishes_a_parseable_incomplete_session() {
    first_interrupt_scenario().await;
}

#[cfg(windows)]
#[tokio::test]
async fn first_ctrl_c_event_finishes_a_parseable_incomplete_session() {
    first_interrupt_scenario().await;
}

async fn drain_second_interrupt_stdout(
    stdout_pipe: tokio::process::ChildStdout,
    started_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    finished_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
) -> io::Result<Vec<u8>> {
    let mut reader = BufReader::new(stdout_pipe);
    let mut stdout = Vec::new();
    let mut started_tx = Some(started_tx);
    let mut finished_tx = Some(finished_tx);
    loop {
        let mut line = Vec::new();
        let bytes = reader.read_until(b'\n', &mut line).await?;
        if bytes == 0 {
            if let Some(started_tx) = started_tx.take() {
                let _ = started_tx.send(Err("stdout closed before mutant_started".to_owned()));
            }
            if let Some(finished_tx) = finished_tx.take() {
                let _ = finished_tx.send(Err("stdout closed before run_finished".to_owned()));
            }
            break;
        }
        let event = serde_json::from_slice::<serde_json::Value>(&line).ok();
        if event
            .as_ref()
            .is_some_and(|event| event["kind"] == "mutant_started")
            && let Some(started_tx) = started_tx.take()
        {
            let _ = started_tx.send(Ok(()));
        }
        if event
            .as_ref()
            .is_some_and(|event| event["kind"] == "run_finished")
            && let Some(finished_tx) = finished_tx.take()
        {
            let _ = finished_tx.send(Ok(()));
        }
        stdout.extend_from_slice(&line);
    }
    Ok(stdout)
}

async fn second_interrupt_scenario() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let session = coordinator.path().join("session.sqlite3");
    let (mut child, active, descendant_ready) =
        spawn_second_interrupt_fixture(project.path(), coordinator.path(), &session);
    // Keep draining after the readiness event so a blocked report write cannot mask the signal.
    let stdout_pipe = child.stdout.take().unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let stdout_task = tokio::spawn(drain_second_interrupt_stdout(
        stdout_pipe,
        started_tx,
        finished_tx,
    ));
    let mut fixture_processes = None;
    let outcome: Result<_, String> = async {
        tokio::time::timeout(Duration::from_secs(15), started_rx)
            .await
            .map_err(|_| "timed out waiting for JSONL kind mutant_started".to_owned())?
            .map_err(|_| "stdout drain task stopped before mutant_started".to_owned())??;
        fixture_processes = Some(
            try_wait_for_fixture_processes(&active, &descendant_ready, Duration::from_secs(15))
                .await?,
        );
        let lock = begin_immediate_with_retry(&session, Duration::from_secs(5)).await?;

        let pid = child
            .id()
            .ok_or_else(|| "hoimin exited before first interrupt".to_owned())?;
        send_fixture_interrupt(Some(pid)).await?;
        let descendant_process = &fixture_processes
            .as_ref()
            .expect("assigned above")
            .descendant;
        if !descendant_process
            .wait_until_stops(Duration::from_secs(5))
            .await
        {
            return Err(format!(
                "first interrupt did not reap descendant {}",
                descendant_process.pid()
            ));
        }
        tokio::time::timeout(Duration::from_secs(5), finished_rx)
            .await
            .map_err(|_| "timed out waiting for JSONL kind run_finished".to_owned())?
            .map_err(|_| "stdout drain task stopped before run_finished".to_owned())??;

        let forced_at = Instant::now();
        // The retained SQLite lock keeps the live child blocked in session finalization.
        send_fixture_interrupt(Some(pid)).await?;
        let status = tokio::time::timeout(Duration::from_secs(1), child.wait())
            .await
            .map_err(|_| "second interrupt must bypass blocked FinishSession".to_owned())?
            .map_err(|error| error.to_string())?;
        let forced_elapsed = forced_at.elapsed();
        Ok((status, forced_elapsed, lock))
    }
    .await;

    let child_cleanup = reap_test_child(&mut child).await;
    #[cfg(unix)]
    let process_cleanup = kill_fixture_processes(&active, &descendant_ready).await;
    #[cfg(windows)]
    let process_cleanup = kill_fixture_processes(fixture_processes.as_ref()).await;
    let stdout = stdout_task
        .await
        .expect("stdout drain task must not panic")
        .map_err(|error| error.to_string())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()));
    if let Err(error) = child_cleanup.and(process_cleanup) {
        panic!("test teardown failed: {error}; outcome={outcome:?}");
    }
    let (status, forced_elapsed, lock) = outcome.unwrap_or_else(|error| {
        panic!("second-interrupt scenario failed after successful teardown: {error}")
    });
    let stdout_lines = stdout.unwrap_or_else(|error| panic!("stdout was not UTF-8: {error}"));

    assert_eq!(status.code(), Some(130));
    assert!(forced_elapsed < Duration::from_secs(1));
    assert!(!lock.is_autocommit());
    let events: Vec<serde_json::Value> = stdout_lines
        .lines()
        .map(|line| serde_json::from_str(line).expect("complete stdout line must be JSON"))
        .collect();
    assert_one_incomplete_run_finished(&events, &stdout_lines);
    let complete: i64 = lock
        .query_row("SELECT complete FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(complete, 0);
}

fn assert_one_incomplete_run_finished(events: &[serde_json::Value], output: &str) {
    let finished = events
        .iter()
        .filter(|event| event["kind"] == "run_finished")
        .collect::<Vec<_>>();
    assert_eq!(
        finished.len(),
        1,
        "expected one run_finished before blocked FinishSession: {output}"
    );
    assert_eq!(finished[0]["complete"], false);
}

#[cfg(unix)]
#[tokio::test]
async fn second_sigint_forces_130_while_session_finish_is_blocked() {
    second_interrupt_scenario().await;
}

#[cfg(windows)]
#[tokio::test]
async fn second_ctrl_c_event_forces_130_while_session_finish_is_blocked() {
    second_interrupt_scenario().await;
}

#[tokio::test]
async fn serial_output_that_requests_stop_is_accepted_before_cancellation() {
    let project = tempfile::tempdir().unwrap();
    let metrics_directory = tempfile::tempdir().unwrap();
    let metrics_path = metrics_directory.path().join("metrics.json");
    let _parallel_test_guard = parallel_project_test_guard().await;
    write_parallel_project(project.path());
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("1"),
        OsString::from("--format"),
        OsString::from("jsonl"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--metrics"),
        metrics_path.as_os_str().to_owned(),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from src.calc import total; assert total(1,2,3,4,5) == 15"),
    ];
    let config = hoimin_cli::cli::parse_config_from(args).unwrap();
    let control = hoimin_cli::shell::RunControl::new();
    let mut stdout = CancelOnMutantFinished {
        accepted: Vec::new(),
        cancelled: false,
        control: control.clone(),
    };
    let mut stderr = Vec::new();

    let exit = hoimin_cli::shell::run_loop_with_control(config, &mut stdout, &mut stderr, control)
        .await
        .unwrap();

    assert_eq!(exit, 130);
    let events = String::from_utf8(stdout.accepted).unwrap();
    let events = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mutants = events
        .iter()
        .filter(|event| event["kind"] == "mutant_finished")
        .collect::<Vec<_>>();
    let ids = mutants
        .iter()
        .map(|event| event["candidate"]["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        mutants.len(),
        "a completed output was re-emitted"
    );
    assert!(mutants.iter().any(|event| event["status"] != "not_run"));
    let metrics: hoimin_core::RunMetrics =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    metrics.validate().unwrap();
    assert_eq!(
        metrics.executed,
        mutants
            .iter()
            .filter(|event| event["status"] != "not_run")
            .count() as u64
    );
    assert!(metrics.workers.iter().all(|worker| worker.processes > 0));
}

#[tokio::test]
async fn failed_mutant_started_output_prevents_process_start() {
    let directory = tempfile::tempdir().unwrap();
    let counter = directory.path().join("executions");
    let command = format!(
        "from pathlib import Path; p=Path({:?}); p.write_text(str(int(p.read_text())+1) if p.exists() else '1'); from src.calc import add; assert add(1, 2) == 3",
        counter.to_string_lossy()
    );
    let python = python_executable();
    let root = fixture_root();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--format"),
        OsString::from("jsonl"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ];
    let mut stdout = RejectMutantStarted::default();
    let mut stderr = Vec::new();

    let exit = tokio::time::timeout(
        Duration::from_secs(5),
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr),
    )
    .await
    .expect("report output failure must terminate without retrying the failed sink");

    assert_eq!(exit, 2);
    assert_eq!(std::fs::read_to_string(counter).unwrap(), "1");
    assert!(
        !stdout
            .accepted
            .windows(b"run_finished".len())
            .any(|window| window == b"run_finished"),
        "an irrecoverable report stream must not receive a terminal retry"
    );
}

#[tokio::test]
async fn worker_pythonpath_rewrites_an_original_src_root_to_the_mutated_copy() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let package = root.join("src/hoimin_worker_import_probe");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("__init__.py"), "").unwrap();
    std::fs::write(package.join("calc.py"), "def value():\n    return 1 + 1\n").unwrap();
    let python = python_executable();
    let args = [
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/hoimin_worker_import_probe/calc.py"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from hoimin_worker_import_probe.calc import value; assert value() == 2"),
    ];
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(args)
        .env("PYTHONPATH", root.join("src"))
        .output()
        .await
        .unwrap();

    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("complete JSON report");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(document["mutants"][0]["status"], "killed");
}

struct FixtureRun {
    exit_code: i32,
    statuses: Vec<String>,
    stdout: String,
    stderr: String,
    document: serde_json::Value,
}

fn json_candidate_tuples(
    mutants: &[serde_json::Value],
) -> BTreeSet<(&str, u64, u64, &str, &str, &str, &str)> {
    mutants
        .iter()
        .map(|mutant| {
            let candidate = &mutant["candidate"];
            (
                candidate["path"].as_str().unwrap(),
                candidate["line"].as_u64().unwrap(),
                candidate["column"].as_u64().unwrap(),
                candidate["operator"].as_str().unwrap(),
                candidate["original"].as_str().unwrap(),
                candidate["replacement"].as_str().unwrap(),
                candidate["symbol"].as_str().unwrap(),
            )
        })
        .collect()
}

fn report_candidate_at<'a>(
    document: &'a serde_json::Value,
    operator: &str,
    start: u64,
    length: u64,
) -> &'a serde_json::Value {
    let mutants = document["mutants"].as_array().unwrap();
    assert_eq!(
        mutants
            .iter()
            .filter(|mutant| {
                let candidate = &mutant["candidate"];
                candidate["operator"] == operator
                    && candidate["span"]["start"] == start
                    && candidate["span"]["length"] == length
            })
            .count(),
        1,
        "expected exactly one {operator} candidate at {start}+{length} in {document}"
    );
    &mutants
        .iter()
        .find(|mutant| {
            let candidate = &mutant["candidate"];
            candidate["operator"] == operator
                && candidate["span"]["start"] == start
                && candidate["span"]["length"] == length
        })
        .unwrap()["candidate"]
}

#[cfg(unix)]
#[derive(Debug)]
struct RealCliOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

async fn run_missing_explicit_candidate(
    project: &Path,
    missing_id: &str,
    test_code: &str,
) -> (String, Vec<u8>) {
    let python = python_executable();
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        project.as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(test_code),
    ])
    .unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let error = hoimin_cli::shell::run_selected_loop(
        config,
        BTreeSet::from([missing_id.to_owned()]),
        VerificationSelection {
            mode: VerificationSelectionMode::CandidateIds,
            policy: VerificationSelectionPolicy::ExplicitCandidates,
            requested: 1,
            selected: 1,
            scope: VerificationSelectionScope::ExplicitCandidates,
            plan_truncated: false,
        },
        &mut stdout,
        &mut stderr,
    )
    .await
    .unwrap_err();
    (error, stdout)
}

async fn run_fixture(test_args: &[&str]) -> FixtureRun {
    run_fixture_options(test_args, None, false).await
}
async fn run_type_checker(checker: &Path, expected_status: &str) -> FixtureRun {
    let root = type_checking_fixture_root();
    let line = match expected_status {
        "killed" => "src/contracts.py:1",
        "survived" => "src/contracts.py:6",
        _ => panic!("unsupported type-checker expectation: {expected_status}"),
    };
    let checker_args: &[&str] = match checker.file_stem().and_then(|name| name.to_str()) {
        Some("ty") => &["check"],
        Some("mypy") => &["src"],
        Some(name) => panic!("unsupported type checker: {name}"),
        None => panic!("type checker has no executable name: {}", checker.display()),
    };
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--line"),
        OsString::from(line),
        OsString::from("--operators"),
        OsString::from("type_nullable"),
        OsString::from("--max-mutants"),
        OsString::from("1"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        checker.as_os_str().to_owned(),
    ];
    args.extend(checker_args.iter().map(OsString::from));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    fixture_run(exit_code, stdout, stderr)
}

async fn run_fixture_with_session(test_args: &[&str], session: &Path, resume: bool) -> FixtureRun {
    run_fixture_options(test_args, Some(session), resume).await
}

async fn run_fixture_options(
    test_args: &[&str],
    session: Option<&Path>,
    resume: bool,
) -> FixtureRun {
    run_fixture_options_extra(test_args, session, resume, &[]).await
}

async fn run_fixture_options_extra(
    test_args: &[&str],
    session: Option<&Path>,
    resume: bool,
    extra_options: &[&str],
) -> FixtureRun {
    run_fixture_options_extra_with_format(test_args, session, resume, "json", extra_options).await
}

async fn run_fixture_options_extra_with_format(
    test_args: &[&str],
    session: Option<&Path>,
    resume: bool,
    format: &str,
    extra_options: &[&str],
) -> FixtureRun {
    let root = fixture_root();
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--format"),
        OsString::from(format),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
    ];
    if let Some(session) = session {
        let separator = args.iter().position(|arg| arg == "--").unwrap();
        let mut options = vec![OsString::from("--session"), session.as_os_str().to_owned()];
        if resume {
            options.push(OsString::from("--resume"));
        }
        args.splice(separator..separator, options);
    }
    let separator = args.iter().position(|arg| arg == "--").unwrap();
    args.splice(
        separator..separator,
        extra_options.iter().copied().map(OsString::from),
    );
    args.extend(test_args.iter().map(OsString::from));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document = if format == "json" {
        serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
            )
        })
    } else {
        serde_json::Value::Null
    };
    let statuses = document["mutants"]
        .as_array()
        .map(|mutants| {
            mutants
                .iter()
                .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}
fn fixture_run(exit_code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> FixtureRun {
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    });
    let statuses = document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
        .collect();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}

#[derive(Default)]
struct RejectMutantStarted {
    accepted: Vec<u8>,
}

struct CancelOnMutantFinished {
    accepted: Vec<u8>,
    cancelled: bool,
    control: hoimin_cli::shell::RunControl,
}

impl Write for CancelOnMutantFinished {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.accepted.extend_from_slice(bytes);
        if !self.cancelled
            && self
                .accepted
                .windows(b"mutant_finished".len())
                .any(|window| window == b"mutant_finished")
        {
            self.cancelled = true;
            self.control.cancel();
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for RejectMutantStarted {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut combined = self.accepted.clone();
        combined.extend_from_slice(bytes);
        if combined
            .windows(b"mutant_started".len())
            .any(|window| window == b"mutant_started")
        {
            return Err(io::Error::other("injected mutant_started failure"));
        }
        self.accepted.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

async fn parallel_project_test_guard() -> tokio::sync::OwnedMutexGuard<()> {
    static LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
    Arc::clone(LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(()))))
        .lock_owned()
        .await
}

fn write_parallel_project(root: &Path) {
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def total(a, b, c, d, e):\n    return a + b + c + d + e\n",
    )
    .unwrap();
}

fn write_collection_operator_project(root: &Path) {
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def collection():\n    return (1, 2)\n\ndef structure(values):\n    values.append(3)\n    return values\n\ndef bitwise(left, right):\n    return left & right\n\nif __name__ == \"__main__\":\n    hidden = (4, 5)\n",
    )
    .unwrap();
}

fn write_exception_operator_project(root: &Path) {
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def classify():\n    try:\n        raise ValueError\n    except ValueError:\n        return 'ok'\n",
    )
    .unwrap();
}

fn write_exception_handler_collection_project(root: &Path) {
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def classify():\n    try:\n        raise ValueError\n    except tuple((ValueError, TypeError)):\n        return (1, 2)\n",
    )
    .unwrap();
}

fn write_changed_git_project(root: &Path) -> String {
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def changed(a, b):\n    return a + b\n\ndef untouched(a, b):\n    return a + b\n",
    )
    .unwrap();
    run_git(root, &["init", "--quiet"]);
    run_git(root, &["config", "user.name", "Hoimin Test"]);
    run_git(
        root,
        &["config", "user.email", "hoimin-test@example.invalid"],
    );
    run_git(root, &["add", "src/calc.py"]);
    run_git(root, &["commit", "--quiet", "-m", "fixture base"]);
    let base_revision = run_git(root, &["rev-parse", "HEAD"]);
    std::fs::write(
        source.join("calc.py"),
        "def changed(a, b):\n    return a + b  # changed\n\ndef untouched(a, b):\n    return a + b\n",
    )
    .unwrap();
    base_revision
}

fn run_git(root: &Path, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn real_cli_session_args(
    root: &Path,
    session: &Path,
    resume: bool,
    command: &str,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("1"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
    ];
    if resume {
        args.push(OsString::from("--resume"));
    }
    args.extend([
        OsString::from("--max-mutants"),
        OsString::from("1"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python_executable().into_os_string(),
        OsString::from("-c"),
        OsString::from(command),
    ]);
    args
}

#[cfg(unix)]
fn assert_live_session_database_integrity(session: &Path) {
    let connection = rusqlite::Connection::open(session).unwrap();
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    {
        let mut statement = connection.prepare("PRAGMA foreign_key_check").unwrap();
        let mut rows = statement.query([]).unwrap();
        assert!(rows.next().unwrap().is_none());
    }
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 1);
    let run_id: String = connection
        .query_row("SELECT run_id FROM runs", [], |row| row.get(0))
        .unwrap();
    let (result_count, distinct_mutants): (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT mutant_id) FROM results WHERE run_id=?1",
            [&run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(result_count, distinct_mutants);
    let invalid_candidate_joins: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM results r
             WHERE r.run_id=?1 AND (
                 SELECT COUNT(*) FROM candidates c
                 WHERE c.run_id=r.run_id AND c.mutant_id=r.mutant_id
             ) != 1",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(invalid_candidate_joins, 0);
}

fn focused_profile_project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("focused.py"),
        "def decide(value=True):\n    print(1 + 2)\n    assert value is True\n    return 3 + 4\n\nif __name__ == \"__main__\":\n    launch = 5 + 6\nelse:\n    fallback = 7 + 8\n",
    )
    .unwrap();
    project
}

async fn run_focused_profile(
    root: &Path,
    profile: &str,
    format: &str,
    session: Option<&Path>,
    resume: bool,
    max_mutants: usize,
) -> FixtureRun {
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/focused.py"),
        OsString::from("--profile"),
        OsString::from(profile),
        OsString::from("--max-mutants"),
        OsString::from(max_mutants.to_string()),
        OsString::from("--format"),
        OsString::from(format),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from src.focused import decide; assert decide() == 7"),
    ];
    let separator = args.iter().position(|argument| argument == "--").unwrap();
    if let Some(session) = session {
        args.splice(
            separator..separator,
            [OsString::from("--session"), session.as_os_str().to_owned()],
        );
    }
    if resume {
        let separator = args.iter().position(|argument| argument == "--").unwrap();
        args.splice(separator..separator, [OsString::from("--resume")]);
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document = if format == "json" {
        serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
            )
        })
    } else {
        serde_json::Value::Null
    };
    let statuses = document["mutants"]
        .as_array()
        .map(|mutants| {
            mutants
                .iter()
                .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}

async fn run_project_with_session(
    root: &Path,
    session: &Path,
    resume: bool,
    max_mutants: usize,
    command: &str,
) -> FixtureRun {
    run_project_with_session_options(root, session, resume, max_mutants, command, &[]).await
}

async fn run_project_with_session_options(
    root: &Path,
    session: &Path,
    resume: bool,
    max_mutants: usize,
    command: &str,
    options: &[&str],
) -> FixtureRun {
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
    ];
    if resume {
        args.push(OsString::from("--resume"));
    }
    args.extend(options.iter().copied().map(OsString::from));
    args.extend([
        OsString::from("--max-mutants"),
        OsString::from(max_mutants.to_string()),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    });
    let statuses = document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
        .collect();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}
async fn run_project(root: &Path, jobs: usize, command: &str) -> FixtureRun {
    run_project_options(root, jobs, command, &[]).await
}

async fn run_project_options(
    root: &Path,
    jobs: usize,
    command: &str,
    extra_options: &[&str],
) -> FixtureRun {
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from(jobs.to_string()),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ];
    let separator = args.iter().position(|arg| arg == "--").unwrap();
    args.splice(
        separator..separator,
        extra_options.iter().copied().map(OsString::from),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    });
    let statuses = document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
        .collect();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}

fn fixture_root() -> PathBuf {
    repo_root().join("tests/fixtures/projects/basic")
}
fn type_checking_fixture_root() -> PathBuf {
    repo_root().join("tests/fixtures/projects/type-checking")
}

fn python_executable() -> PathBuf {
    let executable = if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    };
    assert!(
        executable.is_file(),
        "missing controlled test Python interpreter: {}",
        executable.display()
    );
    executable
}
fn ty_executable() -> PathBuf {
    checker_executable("ty")
}

fn mypy_executable() -> PathBuf {
    checker_executable("mypy")
}

fn checker_executable(name: &str) -> PathBuf {
    let executable = if cfg!(windows) {
        repo_root().join(format!(".venv/Scripts/{name}.exe"))
    } else {
        repo_root().join(format!(".venv/bin/{name}"))
    };
    assert!(
        executable.is_file(),
        "missing controlled type checker: {}",
        executable.display()
    );
    executable
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

#[cfg(unix)]
struct FixtureProcess {
    // `libc::kill` accepts a signed Unix process ID. Validate the marker value
    // once when opening it and retain that native representation thereafter.
    pid: i32,
}

#[cfg(windows)]
struct FixtureProcess {
    pid: u32,
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl FixtureProcess {
    fn pid(&self) -> u32 {
        #[cfg(unix)]
        {
            self.pid.unsigned_abs()
        }

        #[cfg(windows)]
        {
            self.pid
        }
    }

    #[cfg(unix)]
    fn open(pid: u32) -> Option<Self> {
        i32::try_from(pid).ok().map(|pid| Self { pid })
    }

    #[cfg(windows)]
    fn open(pid: u32) -> Option<Self> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };

        // SAFETY: the fixture PID came from the child process and the handle is owned on success.
        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                0,
                pid,
            );
            (!handle.is_null()).then_some(Self { pid, handle })
        }
    }

    #[cfg(unix)]
    fn is_alive(&self) -> bool {
        // SAFETY: signal 0 performs no mutation and the process ID was validated in `open`.
        unsafe { libc::kill(self.pid, 0) == 0 }
    }

    #[cfg(windows)]
    fn is_alive(&self) -> bool {
        use windows_sys::Win32::Foundation::STILL_ACTIVE;
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;

        // SAFETY: handle is retained by this fixture and valid until Drop.
        unsafe {
            let mut exit_code = 0;
            GetExitCodeProcess(self.handle, &raw mut exit_code) != 0
                && exit_code == STILL_ACTIVE as u32
        }
    }

    async fn wait_until_stops(&self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while self.is_alive() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        !self.is_alive()
    }

    #[cfg(windows)]
    fn terminate(&self) -> Result<(), String> {
        use windows_sys::Win32::System::Threading::TerminateProcess;

        // SAFETY: this retained handle belongs to the observed test fixture
        // process and was opened with PROCESS_TERMINATE.
        if unsafe { TerminateProcess(self.handle, 1) } == 0 {
            return Err(format!(
                "failed to terminate fixture process {}: {}",
                self.pid,
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for FixtureProcess {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        // SAFETY: this instance owns the successful OpenProcess handle.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

async fn wait_for_tokenized_descendant_processes(
    directory: &Path,
    expected: usize,
    timeout: Duration,
) -> Vec<(String, FixtureProcess)> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let processes = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let token = entry.file_name().to_str()?.to_owned();
                let pid = std::fs::read_to_string(entry.path())
                    .ok()?
                    .trim()
                    .parse::<u32>()
                    .ok()?;
                FixtureProcess::open(pid).map(|process| (token, process))
            })
            .collect::<Vec<_>>();
        if processes.len() >= expected {
            return processes;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "tokenized descendant directory yielded {} processes, expected {expected}",
            processes.len()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(any(unix, windows))]
fn spawn_second_interrupt_fixture(
    project: &Path,
    coordinator: &Path,
    session: &Path,
) -> (tokio::process::Child, PathBuf, PathBuf) {
    spawn_interrupt_fixture(project, coordinator, session, "jsonl", &[])
}

#[cfg(any(unix, windows))]
fn spawn_interrupt_fixture(
    project: &Path,
    coordinator: &Path,
    session: &Path,
    format: &str,
    extra_run_args: &[&str],
) -> (tokio::process::Child, PathBuf, PathBuf) {
    let active = coordinator.join("active");
    std::fs::create_dir(&active).unwrap();
    let descendant_ready = coordinator.join("descendant-ready");
    let descendant_ready_temp = coordinator.join("descendant-ready.tmp");
    let descendant_command = format!(
        "from pathlib import Path; import os,signal,time; signal.signal(signal.SIGINT, signal.SIG_IGN); ready_temp=Path({:?}); ready_temp.write_text(str(os.getpid())); ready_temp.replace({:?}); time.sleep(20)",
        descendant_ready_temp.to_string_lossy(),
        descendant_ready.to_string_lossy(),
    );
    let mutant_command = format!(
        "from pathlib import Path; import os,signal,subprocess,sys,time; signal.signal(signal.SIGINT, signal.SIG_IGN); Path({:?},str(os.getpid())).write_text('running'); time.sleep(0.5); subprocess.Popen([sys.executable,'-c',{:?}]); time.sleep(20)",
        active.to_string_lossy(),
        descendant_command,
    );
    let original = "return a + b + c + d + e";
    let test_command = format!(
        "from pathlib import Path; source=Path('src/calc.py').read_text(); exec({mutant_command:?}) if {original:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')",
    );
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .arg("run")
        .arg("--min-free-space")
        .arg(TEST_MIN_FREE_SPACE)
        .arg("--root")
        .arg(project)
        .arg("--source")
        .arg("src")
        .arg("--file")
        .arg("src/calc.py")
        .arg("--jobs")
        .arg("1")
        .arg("--session")
        .arg(session)
        .arg("--format")
        .arg(format)
        .arg("--allow-best-effort-memory")
        .args(extra_run_args)
        .arg("--")
        .arg(python_executable())
        .arg("-c")
        .arg(test_command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;

        command.as_std_mut().creation_flags(CREATE_NEW_CONSOLE);
    }
    let child = command.spawn().unwrap();
    (child, active, descendant_ready)
}

#[cfg(any(unix, windows))]
fn session_complete(session: &Path) -> i64 {
    rusqlite::Connection::open(session)
        .unwrap()
        .query_row("SELECT complete FROM runs", [], |row| row.get(0))
        .unwrap()
}

#[cfg(any(unix, windows))]
async fn begin_immediate_with_retry(
    path: &Path,
    timeout: Duration,
) -> Result<rusqlite::Connection, String> {
    let connection = rusqlite::Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|error| error.to_string())?;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match connection.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => return Ok(connection),
            Err(error) if tokio::time::Instant::now() < deadline => {
                if !matches!(
                    error.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
                ) {
                    return Err(format!("unexpected BEGIN IMMEDIATE failure: {error}"));
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(format!("could not retain session write lock: {error}")),
        }
    }
}

#[cfg(any(unix, windows))]
struct FixtureProcesses {
    #[cfg(windows)]
    active_mutant: FixtureProcess,
    descendant: FixtureProcess,
}

#[cfg(windows)]
fn try_open_active_fixture_process(active: &Path) -> Option<FixtureProcess> {
    std::fs::read_dir(active)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .find_map(FixtureProcess::open)
}

#[cfg(any(unix, windows))]
async fn try_wait_for_fixture_processes(
    active: &Path,
    descendant_marker: &Path,
    timeout: Duration,
) -> Result<FixtureProcesses, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    #[cfg(unix)]
    let _ = active;
    #[cfg(windows)]
    let mut active_mutant = None;
    let mut descendant = None;
    loop {
        #[cfg(windows)]
        if active_mutant.is_none() {
            active_mutant = try_open_active_fixture_process(active);
        }
        if descendant.is_none() {
            descendant = std::fs::read_to_string(descendant_marker)
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .and_then(FixtureProcess::open);
        }
        #[cfg(unix)]
        if let Some(descendant) = descendant {
            return Ok(FixtureProcesses { descendant });
        }
        #[cfg(windows)]
        if let Some(active_process) = active_mutant.take() {
            if let Some(descendant_process) = descendant.take() {
                return Ok(FixtureProcesses {
                    active_mutant: active_process,
                    descendant: descendant_process,
                });
            }
            active_mutant = Some(active_process);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(
                "fixture readiness did not yield active-mutant and descendant processes".to_owned(),
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(any(unix, windows))]
async fn wait_for_live_session_readiness(
    marker: &Path,
    session: &Path,
    child: &mut tokio::process::Child,
    timeout: Duration,
) -> Result<i64, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!(
                "first hoimin process exited before readiness: {status}"
            ));
        }
        if marker.exists() {
            let connection = rusqlite::Connection::open_with_flags(
                session,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|error| error.to_string())?;
            let (runs, incomplete): (i64, i64) = connection
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(complete=0), 0) FROM runs",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|error| error.to_string())?;
            if runs != 1 || incomplete != 1 {
                return Err(format!(
                    "readiness observed with runs={runs}, incomplete={incomplete}"
                ));
            }
            return Ok(incomplete);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("first hoimin process did not publish readiness".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
fn send_sigint(pid: Option<u32>) -> Result<(), String> {
    let pid = pid.ok_or_else(|| "hoimin exited before SIGINT".to_owned())?;
    let pid = i32::try_from(pid).map_err(|error| error.to_string())?;
    // SAFETY: `pid` belongs to the live child spawned by this test.
    if unsafe { libc::kill(pid, libc::SIGINT) } != 0 {
        return Err(format!("SIGINT failed: {}", io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
fn send_fixture_interrupt(pid: Option<u32>) -> std::future::Ready<Result<(), String>> {
    std::future::ready(send_sigint(pid))
}

#[cfg(windows)]
const CONSOLE_CTRL_TARGET_PID: &str = "HOIMIN_TEST_CONSOLE_CTRL_TARGET_PID";

#[cfg(windows)]
#[test]
#[ignore = "subprocess helper for Windows console-control delivery"]
fn console_ctrl_sender_helper() {
    use windows_sys::Win32::System::Console::{
        AttachConsole, CTRL_C_EVENT, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
    };

    let pid = std::env::var(CONSOLE_CTRL_TARGET_PID)
        .expect("missing console-control target PID")
        .parse::<u32>()
        .expect("invalid console-control target PID");
    // SAFETY: this isolated helper attaches only to the dedicated console
    // created for the test child and detaches before returning.
    unsafe {
        // The helper inherits the Cargo test process's console when one is
        // present. A Windows process must detach before attaching elsewhere.
        let _ = FreeConsole();

        assert_ne!(
            AttachConsole(pid),
            0,
            "AttachConsole({pid}) failed: {}",
            std::io::Error::last_os_error()
        );
        assert_ne!(
            SetConsoleCtrlHandler(None, 1),
            0,
            "SetConsoleCtrlHandler(ignore) failed: {}",
            std::io::Error::last_os_error()
        );
        let generated = GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0);
        let generated_error = std::io::Error::last_os_error();
        let detached = FreeConsole();
        let detached_error = std::io::Error::last_os_error();
        assert_ne!(
            generated, 0,
            "GenerateConsoleCtrlEvent failed: {generated_error}"
        );
        assert_ne!(detached, 0, "FreeConsole failed: {detached_error}");
    }
}

#[cfg(windows)]
async fn send_fixture_interrupt(pid: Option<u32>) -> Result<(), String> {
    let pid = pid.ok_or_else(|| "hoimin exited before console interrupt".to_owned())?;
    let mut command =
        tokio::process::Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
    command
        .arg("--ignored")
        .arg("--exact")
        .arg("console_ctrl_sender_helper")
        .env(CONSOLE_CTRL_TARGET_PID, pid.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .map_err(|_| "console-control sender timed out".to_owned())?
        .map_err(|error| format!("spawn console-control sender: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "console-control sender failed with {}: stdout={:?} stderr={:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

#[cfg(any(unix, windows))]
async fn reap_test_child(child: &mut tokio::process::Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(());
    }
    child.start_kill().map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| "timed out reaping hoimin test child".to_owned())?
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(unix)]
async fn kill_fixture_processes(active: &Path, descendant_marker: &Path) -> Result<(), String> {
    let mut pids = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(active) {
        for entry in entries.flatten() {
            if let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            {
                pids.insert(pid);
            }
        }
    }
    if let Some(pid) = std::fs::read_to_string(descendant_marker)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
    {
        pids.insert(pid);
    }
    for pid in pids {
        let Some(process) = FixtureProcess::open(pid) else {
            continue;
        };
        if !process.is_alive() {
            continue;
        }
        // SAFETY: the PID was written by this test's live fixture process tree.
        let termination_failed = unsafe { libc::kill(process.pid, libc::SIGKILL) } != 0;
        if termination_failed {
            return Err(format!(
                "failed to terminate fixture process {pid}: {}",
                io::Error::last_os_error()
            ));
        }
        if !process.wait_until_stops(Duration::from_secs(5)).await {
            return Err(format!("fixture process {pid} survived termination"));
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn kill_fixture_processes(processes: Option<&FixtureProcesses>) -> Result<(), String> {
    let Some(processes) = processes else {
        return Ok(());
    };
    for process in [&processes.active_mutant, &processes.descendant] {
        if !process.is_alive() {
            continue;
        }
        process.terminate()?;
        if !process.wait_until_stops(Duration::from_secs(5)).await {
            return Err(format!(
                "fixture process {} survived termination",
                process.pid()
            ));
        }
    }
    Ok(())
}

// Uses a real timeout to leave the session incomplete; never edits saved results.
#[tokio::test]
async fn selected_resource_policy_survives_fresh_and_natural_resume_reports() {
    let mut mismatches = Vec::new();
    for format in ["json", "jsonl"] {
        let project = tempfile::tempdir().unwrap();
        let sessions = tempfile::tempdir().unwrap();
        let session = sessions.path().join("session.sqlite");
        std::fs::write(
            project.path().join("calc.py"),
            "def first(a, b):\n    return a + b\ndef second(a, b):\n    return a + b\n",
        )
        .unwrap();

        let mut prior_ids = Vec::new();
        let mut history = Vec::new();
        for resume in [false, true] {
            let stdout = resource_policy_run(project.path(), &session, format, resume).await;
            let (run, baseline, mutants, summary) = if format == "json" {
                let doc: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
                (
                    doc["run"].clone(),
                    doc["baseline"].clone(),
                    doc["mutants"].as_array().unwrap().clone(),
                    doc["summary"].clone(),
                )
            } else {
                let events: Vec<serde_json::Value> = String::from_utf8_lossy(&stdout)
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                let event = |kind| events.iter().find(|v| v["kind"] == kind).unwrap().clone();
                (
                    event("run_started"),
                    event("baseline_finished"),
                    events
                        .iter()
                        .filter(|v| v["kind"] == "mutant_finished")
                        .cloned()
                        .collect(),
                    event("run_finished"),
                )
            };
            assert_eq!(summary["complete"], false);
            assert_eq!(mutants.len(), 2);
            let killed = mutants.iter().find(|v| v["status"] == "killed").unwrap();
            let timeout = mutants.iter().find(|v| v["status"] == "timeout").unwrap();
            assert_eq!(timeout["termination"], "Timeout");
            let ids: Vec<_> = mutants
                .iter()
                .map(|v| v["candidate"]["id"].clone())
                .collect();
            if resume {
                assert_eq!(ids, prior_ids);
                assert!(killed["termination"].is_null());
                assert!(killed["output"].is_null());
                assert_eq!(killed["elapsed_ms"], 0);
            } else {
                assert!(!killed["termination"].is_null());
                prior_ids = ids;
            }
            let mode = baseline["resource_mode"].clone();
            if cfg!(target_os = "macos") {
                assert_eq!(mode, "best_effort");
                assert_eq!(run["resource_control"]["mechanism"], "portable");
            }
            if run["resource_control"]["mode"] != mode
                || run["resource_control"]["mechanism"]
                    .as_str()
                    .unwrap()
                    .is_empty()
                || mutants.iter().any(|v| v["resource_mode"] != mode)
            {
                mismatches.push(format!(
                    "{format} resume={resume}: {}",
                    String::from_utf8_lossy(&stdout)
                ));
            }
            let connection = rusqlite::Connection::open(&session).unwrap();
            let rows = connection.prepare("SELECT * FROM results WHERE status = 'killed' AND run_id = (SELECT run_id FROM runs ORDER BY id LIMIT 1) ORDER BY mutant_id").unwrap()
                .query_map([], |row| Ok((0..row.as_ref().column_count()).map(|i| row.get::<_, rusqlite::types::Value>(i).unwrap()).collect::<Vec<_>>())).unwrap()
                .collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0][0],
                rusqlite::types::Value::Text(run["run_id"].as_str().unwrap().to_owned())
            );
            assert_eq!(
                rows[0][1],
                rusqlite::types::Value::Text(
                    killed["candidate"]["id"].as_str().unwrap().to_owned()
                )
            );
            if resume {
                assert_eq!(rows, history);
            } else {
                history = rows;
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

async fn resource_policy_run(root: &Path, session: &Path, format: &str, resume: bool) -> Vec<u8> {
    let command = "from pathlib import Path; import time; s=Path('calc.py').read_text(); first,second=s.split('def second'); time.sleep(6) if 'a - b' in second else None; assert 'a - b' not in first";
    let mut args: Vec<OsString> = ["hoimin", "run", "--root"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(root.into());
    args.extend(
        [
            "--file",
            "calc.py",
            "--operators",
            "binary_add_sub",
            "--jobs",
            "1",
            "--min-free-space",
            "1B",
            "--mutant-timeout",
            "2s",
            "--format",
            format,
            "--allow-best-effort-memory",
            "--session",
        ]
        .into_iter()
        .map(OsString::from),
    );
    args.push(session.as_os_str().to_owned());
    if resume {
        args.push("--resume".into());
    }
    args.extend([
        OsString::from("--"),
        python_executable().into(),
        "-c".into(),
        command.into(),
    ]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(exit, 4, "{}", String::from_utf8_lossy(&stderr));
    stdout
}

#[tokio::test]
async fn parenthesized_exception_to_bare_run_survives_value_error_test() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    std::fs::write(project.path().join("src/calc.py"),
        "def classify():\n    try:\n        raise ValueError('x')\n    except (\n        # grouping\n        (Exception)\n    ):\n        return 'caught'\n").unwrap();
    let run = run_project_options(
        project.path(),
        1,
        "from src.calc import classify; assert classify() == 'caught'",
        &["--operators", "exception_exception_to_bare"],
    )
    .await;
    assert_eq!(run.exit_code, 1, "{}", run.stderr);
    assert_eq!(run.document["summary"]["complete"], true);
    // pins: issue #451 — `except ()` incorrectly killed this mutant.
    assert_eq!(run.statuses, ["survived"]);
    assert_eq!(run.document["summary"]["counts"]["killed"], 0);
    assert_eq!(run.document["summary"]["counts"]["survived"], 1);
}

#[tokio::test]
async fn active_session_artifacts_allow_real_cli_in_root() {
    for inside in [false, true] {
        for relative in [false, true] {
            for existing in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let project = directory.path().join("project");
                std::fs::create_dir(&project).unwrap();
                write_parallel_project(&project);
                std::fs::write(project.join("fixture.db"), b"ordinary fixture").unwrap();
                let database = if inside {
                    project.join("session.sqlite3")
                } else {
                    directory.path().join("session.sqlite3")
                };
                if existing {
                    drop(hoimin_cli::session::SessionHandler::open(&database).unwrap());
                }
                let session = if relative {
                    database.strip_prefix(directory.path()).unwrap()
                } else {
                    database.as_path()
                };
                let command = "from pathlib import Path; assert Path('fixture.db').read_bytes() == b'ordinary fixture'; assert not Path('session.sqlite3').exists(); from src.calc import total; assert total(1,2,3,4,5) == 15";
                let mut first_id = None;
                let mut killed_id = None;
                for resume in [false, true] {
                    let mut args = real_cli_session_args(&project, session, resume, command);
                    let jobs = args.iter().position(|arg| arg == "--jobs").unwrap();
                    args[jobs + 1] = "2".into();
                    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
                        .current_dir(directory.path())
                        .args(args)
                        .kill_on_drop(true)
                        .output()
                        .await
                        .unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(4),
                        "inside={inside} relative={relative} existing={existing} resume={resume} stdout={} stderr={}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
                    let mutants = report["mutants"].as_array().unwrap();
                    let killed = mutants
                        .iter()
                        .find(|mutant| mutant["status"] == "killed")
                        .expect("an actual mutant must be killed");
                    if let Some(id) = &killed_id {
                        let reused = mutants
                            .iter()
                            .find(|mutant| &mutant["candidate"]["id"] == id)
                            .unwrap();
                        assert_eq!(reused["status"], "killed");
                        assert!(reused["termination"].is_null());
                    } else {
                        killed_id = Some(killed["candidate"]["id"].clone());
                    }
                    assert_eq!(report["summary"]["complete"], false);
                    let id = report["run"]["run_id"].clone();
                    if let Some(first) = &first_id {
                        assert_eq!(&id, first);
                    } else {
                        first_id = Some(id);
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn active_session_artifacts_follow_root_parent_and_database_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    write_parallel_project(&root);
    let alias = directory.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let database = root.join("active*?[1].db");
    for existing in [false, true] {
        let session = if existing {
            let leaf = directory.path().join("leaf.db");
            std::os::unix::fs::symlink(&database, &leaf).unwrap();
            leaf
        } else {
            alias.join("active*?[1].db")
        };
        let args = real_cli_session_args(
            &alias,
            &session,
            false,
            "from pathlib import Path; assert not list(Path('.').glob('*.db*')); from src.calc import total; assert total(1,2,3,4,5) == 15",
        );
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .args(args)
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(4),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["mutants"][0]["status"], "killed");
    }
}

#[tokio::test]
async fn active_session_artifacts_do_not_exempt_original_source_or_fixture_edits() {
    for path in ["fixture.db", "src/calc.py"] {
        for alias in [false, true] {
            let project = tempfile::tempdir().unwrap();
            write_parallel_project(project.path());
            std::fs::write(project.path().join("fixture.db"), b"original fixture").unwrap();
            let session = project.path().join("session.db");
            #[cfg(any(unix, windows))]
            if alias {
                let actual = project.path().join("actual-locks");
                std::fs::create_dir(&actual).unwrap();
                session_directory_alias(&actual, &project.path().join(".session.db.hoimin-locks"));
            }
            #[cfg(not(any(unix, windows)))]
            let _ = alias;
            let original = project.path().join(path);
            let command = format!(
                "from pathlib import Path; Path({:?}).write_text('changed')",
                original.to_str().unwrap()
            );
            let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
                .args(real_cli_session_args(
                    project.path(),
                    &session,
                    false,
                    &command,
                ))
                .kill_on_drop(true)
                .output()
                .await
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("workspace.original.changed"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[tokio::test]
async fn active_session_artifacts_missing_parent_fails_preflight_without_creation() {
    let project = tempfile::tempdir().unwrap();
    write_parallel_project(project.path());
    let missing_parent = project.path().join("missing");
    let session = missing_parent.join("session.db");
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(real_cli_session_args(
            project.path(),
            &session,
            false,
            "raise AssertionError('baseline must not run')",
        ))
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("session.path"));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["baseline"].is_null());
    assert_eq!(report["summary"]["complete"], false);
    assert!(!missing_parent.exists());
}

#[cfg(any(unix, windows))]
fn session_directory_alias(target: &Path, alias: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, alias).unwrap();
    #[cfg(windows)]
    {
        let result = match std::os::windows::fs::symlink_dir(target, alias) {
            Ok(()) => Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported
                ) || error.raw_os_error() == Some(1314) =>
            {
                // Directory aliases also work as junctions, without symlink privileges.
                let output = std::process::Command::new("cmd.exe")
                    .args(["/D", "/C", "mklink", "/J"])
                    .arg(alias)
                    .arg(target)
                    .output()
                    .expect("create session alias junction");
                if output.status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other(format!(
                        "failed to create session alias junction: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )))
                }
            }
            Err(error) => Err(error),
        };
        result.expect("session alias regression requires a directory alias");
    }
}

#[cfg(any(unix, windows))]
async fn assert_active_session_aliased_lock_tree_is_excluded(jobs: &str, mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let source = b"value = True\n";
    std::fs::write(root.join("calc.py"), source).unwrap();
    let fixture = rusqlite::Connection::open(root.join("fixture.db")).unwrap();
    fixture
        .execute_batch(
            "CREATE TABLE fixture(value TEXT); INSERT INTO fixture VALUES ('ordinary fixture');",
        )
        .unwrap();
    drop(fixture);
    let fixture_bytes = std::fs::read(root.join("fixture.db")).unwrap();
    std::fs::create_dir(root.join("actual-locks-backup")).unwrap();
    std::fs::write(root.join("actual-locks-backup/keep"), b"keep").unwrap();
    let session_parent = if mode == "outside-alias" {
        directory.path()
    } else {
        root.as_path()
    };
    let session = session_parent.join("session.db");
    let alias = session_parent.join(".session.db.hoimin-locks");
    let actual = match mode {
        "outside-target" => directory.path().join("actual-locks"),
        "native-case" => root.join(".SESSION.DB.HOIMIN-LOCKS"),
        _ => root.join("actual-locks"),
    };
    std::fs::create_dir(&actual).unwrap();
    if mode == "native-case" {
        if !alias.exists() {
            eprintln!("SKIP native case-alias test: filesystem is case-sensitive");
            return;
        }
        eprintln!("native case-alias capability confirmed");
    } else {
        session_directory_alias(&actual, &alias);
    }
    std::fs::write(actual.join("existing.lock"), b"preserved").unwrap();
    let script = "from pathlib import Path; import sqlite3; assert not Path('actual-locks').exists(); assert not Path('.SESSION.DB.HOIMIN-LOCKS').exists(); assert sqlite3.connect('file:fixture.db?mode=ro', uri=True).execute('select value from fixture').fetchone() == ('ordinary fixture',); assert Path('actual-locks-backup/keep').read_bytes() == b'keep'; import calc; assert calc.value";
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .current_dir(&root)
        .args("run --root . --file calc.py --operators boolean_literal --include ** --format json --allow-best-effort-memory --min-free-space 1B --total-timeout 20s".split_whitespace())
        .args(["--jobs", jobs])
        .arg("--session")
        .arg(&session)
        .arg("--")
        .arg(python_executable())
        .args(["-c", script])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("session alias CLI timed out")
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "jobs={jobs} stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
    assert_eq!(report["mutants"][0]["status"], "killed");
    assert_eq!(std::fs::read(root.join("calc.py")).unwrap(), source);
    assert_eq!(
        std::fs::read(root.join("fixture.db")).unwrap(),
        fixture_bytes
    );
    assert_eq!(
        std::fs::read(actual.join("existing.lock")).unwrap(),
        b"preserved"
    );
    let owned_locks = std::fs::read_dir(actual)
        .unwrap()
        .filter(|entry| {
            let path = entry.as_ref().unwrap().path();
            path.file_name().unwrap() != "existing.lock"
                && path
                    .extension()
                    .is_some_and(|extension| extension == "lock")
        })
        .count();
    assert_eq!(
        owned_locks, 1,
        "ownership acquisition must create its own lock"
    );
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn active_session_aliased_lock_tree_is_excluded_serial() {
    assert_active_session_aliased_lock_tree_is_excluded("1", "inside").await;
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn active_session_aliased_lock_tree_is_excluded_parallel() {
    assert_active_session_aliased_lock_tree_is_excluded("2", "inside").await;
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn active_session_lock_tree_aliases_across_root_boundary() {
    for mode in ["outside-alias", "outside-target"] {
        assert_active_session_aliased_lock_tree_is_excluded("1", mode).await;
    }
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn active_session_lock_tree_native_case_alias() {
    assert_active_session_aliased_lock_tree_is_excluded("1", "native-case").await;
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn active_session_lock_tree_alias_supports_natural_resume() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = "def first(a, b):\n    return a + b\ndef second(a, b):\n    return a + b\n";
    std::fs::write(root.join("calc.py"), source).unwrap();
    let actual = root.join("actual-locks");
    std::fs::create_dir(&actual).unwrap();
    session_directory_alias(&actual, &root.join(".session.db.hoimin-locks"));
    let mut first_id = None;
    let mut candidate_ids = Vec::new();
    for resume in [false, true] {
        let stdout = tokio::time::timeout(
            Duration::from_secs(30),
            resource_policy_run(root, &root.join("session.db"), "json", resume),
        )
        .await
        .expect("bounded session resume");
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["complete"], false);
        let mutants = report["mutants"].as_array().unwrap();
        assert_eq!(mutants.len(), 2);
        let killed = mutants.iter().find(|m| m["status"] == "killed").unwrap();
        let timeout = mutants.iter().find(|m| m["status"] == "timeout").unwrap();
        assert_eq!(timeout["termination"], "Timeout");
        let ids: Vec<_> = mutants
            .iter()
            .map(|m| m["candidate"]["id"].clone())
            .collect();
        if resume {
            assert_eq!(first_id.as_ref().unwrap(), &report["run"]["run_id"]);
            assert_eq!(ids, candidate_ids);
            assert!(killed["termination"].is_null());
            assert_eq!(killed["elapsed_ms"], 0);
        } else {
            first_id = Some(report["run"]["run_id"].clone());
            candidate_ids = ids;
            assert!(!killed["termination"].is_null());
        }
        assert_eq!(
            std::fs::read_to_string(root.join("calc.py")).unwrap(),
            source
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn active_session_lock_tree_invalid_aliases_fail_before_baseline() {
    for looping in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        write_parallel_project(root);
        let source = std::fs::read(root.join("src/calc.py")).unwrap();
        let alias = root.join(".session.db.hoimin-locks");
        session_directory_alias(if looping { &alias } else { root }, &alias);
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
        command
            .args(real_cli_session_args(
                root,
                &root.join("session.db"),
                false,
                "raise AssertionError('baseline must not run')",
            ))
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        let code = if looping {
            "session.path"
        } else {
            "workspace.path.invalid"
        };
        assert!(diagnostics.contains(code), "{diagnostics}");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(report["baseline"].is_null());
        assert!(!root.join("session.db").exists());
        assert_eq!(std::fs::read(root.join("src/calc.py")).unwrap(), source);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_baseline_log_consumer_respects_total_timeout_and_grace() {
    use std::process::Stdio;

    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), "value = 1 + 2\n").unwrap();
    let (consumer, producer, _) = full_report_transport();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(["run", "--root"])
        .arg(project.path())
        .args([
            "--file",
            "target.py",
            "--format",
            "json",
            "--allow-best-effort-memory",
            "--min-free-space",
            TEST_MIN_FREE_SPACE,
            "--total-timeout",
            "1s",
            "--",
        ])
        .arg(python_executable())
        .args(["-c", "print('FAILED_BASELINE_LOG'); raise SystemExit(7)"])
        .stdout(Stdio::null())
        .stderr(Stdio::from(producer))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(6), child.wait()).await;
    if status.is_err() {
        child.kill().await.unwrap();
        child.wait().await.unwrap();
    }
    drop(consumer);
    assert_eq!(
        status
            .expect("stalled baseline log exceeded timeout plus grace")
            .unwrap()
            .code(),
        Some(2)
    );
}
