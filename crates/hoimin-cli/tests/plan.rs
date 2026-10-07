use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use hoimin_cli::target::git::{ResolveGitChanges, handle_git};
use hoimin_cli::{
    analyzer::discover_targets,
    cli::{OutputFormat, ParsedCommand, TopSelectionPolicy, VerifySelection, parse_from},
    plan::{
        PlanManifest, RankedPlanCandidate, ResolvedVerifySelection, VerifySelectionScope, create,
        prepare_verify, prepare_verify_selection,
    },
    shell,
    target::TargetHandler,
};
#[cfg(unix)]
use hoimin_core::EffectId;
use hoimin_core::{
    MAX_JOBS, MutationCandidate, OutputFormat as CoreOutputFormat, VerificationSelectionPolicy,
};

const TEST_MIN_FREE_SPACE: &str = "1B";

#[tokio::test]
async fn source_scoped_plan_preserves_candidates_as_unrelated_files_grow() {
    let mut reference = None;
    for count in [0, 1_000, 5_000] {
        let project = Project::new();
        std::fs::create_dir(project.path.join("unrelated")).unwrap();
        for index in 0..count {
            std::fs::write(
                project.path.join("unrelated").join(format!("{index}.txt")),
                b"x",
            )
            .unwrap();
        }
        let marker = project.path.join("baseline-ran");
        for exact in [false, true] {
            let mut args = plan_args(&project, ["--operators", "binary_add_sub"], &marker);
            if exact {
                let source = args
                    .iter()
                    .position(|argument| argument == "--source")
                    .unwrap();
                args[source] = "--file".into();
                args[source + 1] = "src/calc.py".into();
            }
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
            assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&stderr));
            assert_eq!(stderr, Vec::<u8>::new());
            let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            assert_eq!(value["diagnostics"], serde_json::json!([]));
            assert_eq!(value["candidates"].as_array().unwrap().len(), 1);
            if let Some(expected) = &reference {
                assert_eq!(&value["candidates"], expected);
            } else {
                reference = Some(value["candidates"].clone());
            }
            assert!(!marker.exists());
        }
    }
}

#[cfg(unix)]
struct TestChild(std::process::Child);

#[cfg(unix)]
impl Drop for TestChild {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn plan_rejects_unapproved_best_effort_memory_before_project_work() {
    let project = Project::new();
    let marker = project.path.join("test-command-ran");
    let args = plan_args_without_best_effort(&project, &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(exit, 2);
    assert!(stdout.is_empty(), "failed plan emitted stdout");
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("--allow-best-effort-memory"));
    assert!(stderr.contains("max-memory is not enforced"));
    assert!(!marker.exists(), "test command ran during plan validation");
}

#[tokio::test]
async fn create_plan_emits_versioned_manifest_without_runtime_side_effects() {
    let project = Project::new();
    let workspace_marker = project.path.join("test-command-ran");
    let session_path = project.path.join("session.sqlite3");
    let args = plan_args(
        &project,
        [
            "--file",
            "src/calc.py",
            "--fingerprint-include",
            "config.toml",
            "--fingerprint-file",
            "pyproject.toml",
        ],
        &workspace_marker,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    let stdout = String::from_utf8(stdout).unwrap();
    assert_eq!(stdout.matches('\n').count(), 1);
    let manifest: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(manifest["schema_version"], 5);
    assert_eq!(manifest["ranking_rule_version"], 4);
    assert_eq!(manifest["kind"], "plan");
    assert!(
        manifest["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["path"] == "src/calc.py")
    );
    assert!(
        manifest["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|candidate| candidate["id"].as_str().unwrap().starts_with("m1_"))
    );
    for (index, candidate) in manifest["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(candidate["rank"], index + 1);
        assert!(candidate["score"].is_u64());
        assert!(candidate["ranking_reasons"].is_array());
    }
    assert_eq!(manifest["fingerprint_inputs"].as_array().unwrap().len(), 2);
    assert_eq!(
        manifest["normalized_config"]["fingerprint_files"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        manifest["normalized_config"]["limits"]["max_workspace_size"],
        8 * 1024 * 1024 * 1024_u64
    );
    assert_eq!(
        manifest["normalized_config"]["limits"]["min_free_space"],
        10 * 1024 * 1024 * 1024_u64
    );
    assert!(manifest["normalized_config"].get("session").is_none());
    assert!(manifest["normalized_config"].get("resume").is_none());
    assert!(!workspace_marker.exists());
    assert!(!session_path.exists());
    assert_eq!(stderr, Vec::<u8>::new());
}

#[tokio::test]
async fn plans_for_new_operator_families_pass_verify() {
    for (selector, source) in [
        (
            "collection_ops",
            "def selected(items):\n    return any(items)\n",
        ),
        (
            "structure_ops",
            "def selected(items, value):\n    items.append(value)\n",
        ),
        (
            "bitwise_ops",
            "def selected(left, right):\n    return left | right\n",
        ),
        (
            "exception_ops",
            "def selected(message):\n    raise ValueError(message)\n",
        ),
    ] {
        let project = Project::new_with_source(source);
        let (path, manifest, marker) = write_plan_manifest(
            &project,
            &["--file", "src/calc.py", "--operators", selector],
        )
        .await;
        let candidate_id = manifest
            .candidates
            .first()
            .unwrap_or_else(|| panic!("{selector} did not produce a candidate"))
            .id
            .clone();
        if selector == "exception_ops" {
            let candidate = &manifest.candidates[0].candidate;
            assert_eq!(candidate.operator, "exception_type_pair");
            assert_eq!(candidate.original, "ValueError");
            assert_eq!(candidate.replacement, "TypeError");
        }

        let verified = prepare_verify(
            &path,
            std::slice::from_ref(&candidate_id),
            OutputFormat::Json,
        )
        .await
        .unwrap_or_else(|error| panic!("{selector} plan failed verification: {error}"));

        assert_eq!(
            verified.selection,
            ResolvedVerifySelection::ExplicitCandidates(BTreeSet::from([candidate_id]))
        );
        assert!(!marker.exists());
    }
}

#[tokio::test]
async fn explicit_class_symbol_ranks_and_verifies_its_method_before_an_unrelated_function() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("a.py"),
        "def other():\n    return True\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("calc.py"),
        "class Box:\n    def check(self):\n        return True\n",
    )
    .unwrap();
    let args = vec![
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("."),
        OsString::from("--symbol"),
        OsString::from("calc:Box"),
        OsString::from("--operators"),
        OsString::from("boolean_literal"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--min-free-space"),
        OsString::from(TEST_MIN_FREE_SPACE),
        OsString::from("--"),
        python_executable().into_os_string(),
        OsString::from("-c"),
        OsString::from("from calc import Box; assert Box().check()"),
    ];
    let mut plan_stdout = Vec::new();
    let mut plan_stderr = Vec::new();

    let plan_exit = hoimin_cli::run_with_io(args, &mut plan_stdout, &mut plan_stderr).await;

    assert_eq!(
        plan_exit,
        0,
        "stderr={}",
        String::from_utf8_lossy(&plan_stderr)
    );
    assert_eq!(plan_stderr, Vec::<u8>::new());
    let manifest: PlanManifest = serde_json::from_slice(&plan_stdout).unwrap();
    assert_eq!(manifest.candidates.len(), 2);
    assert_eq!(
        manifest
            .candidates
            .iter()
            .map(|candidate| (
                candidate.path.as_str(),
                candidate.symbol.as_deref(),
                candidate.rank,
                candidate.score,
            ))
            .collect::<Vec<_>>(),
        vec![
            ("calc.py", Some("Box.check"), 1, 350),
            ("a.py", Some("other"), 2, 100),
        ]
    );
    assert_eq!(manifest.schema_version, 5);
    assert_eq!(manifest.ranking_rule_version, 4);
    let planned = &manifest.candidates[0];
    let planned_id = planned.id.clone();
    let plan_path = project.path().join("plan.json");
    std::fs::write(&plan_path, &plan_stdout).unwrap();
    let source_before = std::fs::read(project.path().join("calc.py")).unwrap();
    let plan_before = std::fs::read(&plan_path).unwrap();
    let verify_args = [
        OsString::from("hoimin"),
        OsString::from("verify"),
        plan_path.as_os_str().to_owned(),
        OsString::from("--top"),
        OsString::from("1"),
    ];
    let mut verify_stdout = Vec::new();
    let mut verify_stderr = Vec::new();

    let verify_exit =
        hoimin_cli::run_with_io(verify_args, &mut verify_stdout, &mut verify_stderr).await;

    assert_eq!(
        verify_exit,
        0,
        "stderr={}",
        String::from_utf8_lossy(&verify_stderr)
    );
    assert_eq!(verify_stderr, Vec::<u8>::new());
    let report: serde_json::Value = serde_json::from_slice(&verify_stdout).unwrap();
    assert_eq!(
        report["baseline"]["termination"],
        serde_json::json!({"Exit": 0})
    );
    let mutants = report["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0]["candidate"]["id"], planned_id);
    assert_eq!(mutants[0]["candidate"]["path"], "calc.py");
    assert_eq!(mutants[0]["candidate"]["symbol"], "Box.check");
    assert_eq!(mutants[0]["status"], "killed");
    assert_eq!(std::fs::read(&plan_path).unwrap(), plan_before);
    assert_eq!(
        std::fs::read(project.path().join("calc.py")).unwrap(),
        source_before
    );
}

#[tokio::test]
async fn public_plans_preserve_bom_sources_and_python_columns() {
    for (name, source, expected_line, expected_column, expected_start) in [
        ("leading_bom.py", "\u{feff}enabled = True\n", 1, 10, 13),
        ("plain.py", "enabled = True\n", 1, 10, 10),
        (
            "bom_comment.py",
            "\u{feff}# heading\nenabled = True\n",
            2,
            10,
            23,
        ),
        (
            "multibyte.py",
            "\u{feff}日本 = \"x\"; enabled = True\n",
            1,
            20,
            27,
        ),
        (
            "interior_bom.py",
            "marker = \"\u{feff}\"; enabled = True\n",
            1,
            24,
            26,
        ),
    ] {
        let project = Project::new_with_sources(&[(name, source)]);
        let original = source.as_bytes();
        let marker = project.path.join("test-command-ran");
        let module = name.strip_suffix(".py").unwrap();
        let test_command = format!("from {module} import enabled; assert enabled is True");
        let options = [
            "--file",
            &format!("src/{name}"),
            "--operators",
            "boolean_literal",
        ];
        let mut args = plan_args(&project, options.iter().copied(), &marker);
        insert_test_min_free_space(&mut args);
        *args.last_mut().unwrap() = OsString::from(&test_command);
        let mut run_args = args.clone();
        run_args[1] = OsString::from("run");
        let separator = run_args
            .iter()
            .position(|argument| argument == "--")
            .unwrap();
        run_args.splice(
            separator..separator,
            [OsString::from("--format"), OsString::from("json")],
        );
        let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
            panic!("expected plan arguments");
        };

        let output = create(plan.into_run_config().unwrap()).await.unwrap();
        let candidate = output
            .manifest
            .candidates
            .iter()
            .find(|candidate| candidate.candidate.original == "True")
            .unwrap_or_else(|| panic!("missing boolean candidate for {name}"));

        assert_eq!(candidate.candidate.line, expected_line, "{name}");
        assert_eq!(candidate.candidate.column, expected_column, "{name}");
        assert_eq!(candidate.candidate.span.start, expected_start, "{name}");
        assert_eq!(candidate.candidate.span.length, 4, "{name}");
        assert_eq!(
            candidate.candidate.file_hash,
            blake3::hash(original).to_hex().to_string(),
            "{name}",
        );
        let planned_id = candidate.id.clone();
        let planned_candidate = serde_json::to_value(&candidate.candidate).unwrap();
        let path = project.path.join("plan.json");
        write_json(&path, &serde_json::to_value(&output.manifest).unwrap());
        let plan_bytes = std::fs::read(&path).unwrap();
        let verify_args = vec![
            OsString::from("hoimin"),
            OsString::from("verify"),
            path.as_os_str().to_owned(),
            OsString::from("--candidate"),
            OsString::from(&planned_id),
            OsString::from("--format"),
            OsString::from("json"),
        ];
        for (mode, execution_args) in [("verify", verify_args), ("run", run_args)] {
            assert_planned_boolean_is_killed(
                name,
                mode,
                execution_args,
                planned_id.as_str(),
                &planned_candidate,
            )
            .await;
        }
        assert_eq!(std::fs::read(&path).unwrap(), plan_bytes, "{name}");
        assert_eq!(
            std::fs::read(project.path.join("src").join(name)).unwrap(),
            original
        );
    }
}

async fn assert_planned_boolean_is_killed(
    name: &str,
    mode: &str,
    args: Vec<OsString>,
    planned_id: &str,
    planned_candidate: &serde_json::Value,
) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(
        code,
        0,
        "{name} {mode}: stderr={}",
        String::from_utf8_lossy(&stderr),
    );
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(
        report["baseline"]["termination"]["Exit"], 0,
        "{name} {mode}",
    );
    let mutant = report["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|mutant| mutant["candidate"]["id"] == planned_id)
        .unwrap_or_else(|| panic!("{name} {mode}: planned mutant was not executed"));
    assert_eq!(mutant["status"], "killed", "{name} {mode}");
    assert_eq!(&mutant["candidate"], planned_candidate, "{name} {mode}");
}

#[tokio::test]
async fn cli_verify_rejects_a_bom_counted_column_before_baseline() {
    let source = "\u{feff}enabled = True\n";
    let project = Project::new_with_source(source);
    let (path, manifest, marker) = write_plan_manifest(
        &project,
        &["--file", "src/calc.py", "--operators", "boolean_literal"],
    )
    .await;
    let index = manifest
        .candidates
        .iter()
        .position(|candidate| candidate.candidate.original == "True")
        .unwrap();
    let requested = manifest.candidates[index].id.clone();
    assert_eq!(manifest.candidates[index].candidate.column, 10);
    let mut tampered = serde_json::to_value(manifest).unwrap();
    tampered["candidates"][index]["column"] = serde_json::json!(11);
    write_json(&path, &tampered);
    let args = [
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--candidate"),
        OsString::from(requested),
        OsString::from("--format"),
        OsString::from("json"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        "plan.candidate.invalid: candidate line or column does not match its byte span\n",
    );
    assert!(!marker.exists(), "invalid coordinate ran the baseline");
    assert_eq!(
        std::fs::read(project.path.join("src/calc.py")).unwrap(),
        source.as_bytes(),
    );
}

#[tokio::test]
async fn plan_candidates_match_shared_discovery_for_normalized_selectors() {
    let project = Project::new();
    for options in [
        vec!["--file", "src/calc.py", "--profile", "focused"],
        vec!["--file", "src/calc.py", "--operators", "binary_add_sub"],
        vec!["--line", "src/calc.py:2"],
        vec!["--symbol", "calc:only_add"],
    ] {
        let marker = project.path.join("test-command-ran");
        let args = plan_args(&project, options.iter().copied(), &marker);
        let mut expected = discover_for_plan_args(args.clone()).await;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

        assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
        let manifest: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let mut actual: Vec<MutationCandidate> =
            serde_json::from_value(manifest["candidates"].clone()).unwrap();
        actual.sort_by(|left, right| left.id.cmp(&right.id));
        expected.sort_by(|left, right| left.id.cmp(&right.id));
        assert_eq!(actual, expected, "options={options:?}");
        assert!(!marker.exists());
    }
}

#[tokio::test]
async fn changed_selection_plan_matches_the_real_run_candidate_and_id() {
    let (project, base_revision) = Project::new_changed_git();
    let marker = project.path.join("test-command-ran");
    let expected = BTreeSet::from([(
        "src/calc.py",
        2_u64,
        13_u64,
        "binary_add_sub",
        "+",
        "-",
        "changed",
    )]);

    let mut run_args = plan_args(
        &project,
        ["--changed", "--operators", "binary_add_sub"],
        &marker,
    );
    insert_test_min_free_space(&mut run_args);
    run_args[1] = OsString::from("run");
    *run_args.last_mut().unwrap() = OsString::from("pass");
    let mut run_stdout = Vec::new();
    let mut run_stderr = Vec::new();
    let run_code = hoimin_cli::run_with_io(run_args, &mut run_stdout, &mut run_stderr).await;
    assert_eq!(
        run_code,
        1,
        "stderr={}",
        String::from_utf8_lossy(&run_stderr)
    );
    let run_report: serde_json::Value = serde_json::from_slice(&run_stdout).unwrap();
    let run_mutants = run_report["mutants"].as_array().unwrap();
    assert_eq!(run_mutants.len(), 1);
    assert_eq!(json_candidate_tuples(run_mutants), expected);

    let plan_args = plan_args(
        &project,
        [
            "--changed",
            "--diff-base",
            base_revision.as_str(),
            "--operators",
            "binary_add_sub",
        ],
        &marker,
    );
    let mut plan_stdout = Vec::new();
    let mut plan_stderr = Vec::new();
    let plan_code = hoimin_cli::run_with_io(plan_args, &mut plan_stdout, &mut plan_stderr).await;
    assert_eq!(
        plan_code,
        0,
        "stderr={}",
        String::from_utf8_lossy(&plan_stderr)
    );
    let manifest: PlanManifest = serde_json::from_slice(&plan_stdout).unwrap();
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(candidate_tuples(&manifest.candidates), expected);

    let run_ids = run_mutants
        .iter()
        .map(|mutant| mutant["candidate"]["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let plan_ids = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(run_ids, plan_ids);
}

#[cfg(unix)]
#[test]
fn changed_plan_skips_an_untracked_fifo_with_a_bounded_subprocess() {
    let (project, _) = Project::new_changed_git();
    let fifo = project.path.join("src/pipe");
    let output = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mkfifo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::os::unix::fs::symlink("pipe", project.path.join("src/stalled.py")).unwrap();
    assert!(
        run_git(
            &project.path,
            &["ls-files", "--others", "--exclude-standard"]
        )
        .lines()
        .any(|path| path == "src/stalled.py")
    );

    let mut child = TestChild(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "changed_plan_fifo_child"])
            .env("HOIMIN_FIFO_PLAN_ROOT", &project.path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.0.kill().unwrap();
            break child.0.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    assert!(status.success(), "plan subprocess status: {status}");
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "subprocess fixture for the changed-plan FIFO regression"]
async fn changed_plan_fifo_child() {
    let root = PathBuf::from(std::env::var_os("HOIMIN_FIFO_PLAN_ROOT").unwrap());
    let git_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();
    let changed = handle_git(ResolveGitChanges {
        id: EffectId(31),
        root: git_root,
        diff_base: None,
    })
    .await
    .unwrap();
    assert!(
        !changed
            .changed
            .contains_key(camino::Utf8Path::new("src/stalled.py"))
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = hoimin_cli::run_with_io(
        [
            OsString::from("hoimin"),
            OsString::from("plan"),
            OsString::from("--root"),
            root.into_os_string(),
            OsString::from("--source"),
            OsString::from("src"),
            OsString::from("--changed"),
            OsString::from("--operators"),
            OsString::from("binary_add_sub"),
            OsString::from("--min-free-space"),
            OsString::from(TEST_MIN_FREE_SPACE),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            python_executable().into_os_string(),
            OsString::from("-c"),
            OsString::from("pass"),
        ],
        &mut stdout,
        &mut stderr,
    )
    .await;

    assert_eq!(exit, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert_ne!(stdout, Vec::<u8>::new());
}

#[tokio::test]
async fn plan_candidate_limit_emits_partial_manifest_and_exit_four() {
    let project = Project::new();
    let marker = project.path.join("test-command-ran");
    let args = plan_args(
        &project,
        ["--file", "src/calc.py", "--max-candidates", "1"],
        &marker,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 4, "stderr={}", String::from_utf8_lossy(&stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(manifest["truncated"], true);
    assert!(
        manifest["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "candidate_limit")
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn plan_invalid_syntax_returns_two_without_a_manifest() {
    let project = Project::new_with_source("def broken(:\n");
    let marker = project.path.join("test-command-ran");
    let args = plan_args(&project, ["--file", "src/calc.py"], &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert_ne!(stderr, Vec::<u8>::new());
    assert!(!marker.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn plan_rejects_a_literal_backslash_source_without_rewriting_it() {
    let project = Project::new_with_sources(&[(
        r"literal\calc.py",
        "def add(left, right):\n    return left + right\n",
    )]);
    let marker = project.path.join("test-command-ran");
    let args = plan_args(&project, std::iter::empty(), &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty(), "failed plan emitted stdout");
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains(r"src/literal\calc.py"), "{stderr}");
    assert!(
        !stderr.contains("src/literal/calc.py: No such file"),
        "{stderr}"
    );
    assert!(!marker.exists(), "test command ran during plan validation");
}

#[tokio::test]
async fn plan_preparation_failures_return_two_without_a_manifest() {
    let project = Project::new();
    for (case, options) in [
        (
            "unmatched fingerprint include",
            vec![
                "--file",
                "src/calc.py",
                "--fingerprint-include",
                "missing.toml",
            ],
        ),
        ("unresolved target", vec!["--file", "src/missing.py"]),
    ] {
        let marker = project.path.join(format!("test-command-ran-{case}"));
        let args = plan_args(&project, options, &marker);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

        assert_eq!(
            code,
            2,
            "case={case}, stderr={}",
            String::from_utf8_lossy(&stderr)
        );
        assert!(stdout.is_empty(), "case={case}");
        assert!(!stderr.is_empty(), "case={case}");
        assert!(!marker.exists(), "case={case}");
    }
}

#[tokio::test]
async fn plan_analyzer_timeout_dispatch_returns_two_without_a_manifest() {
    let project = Project::new();
    let marker = project.path.join("test-command-ran");
    let args = plan_args(
        &project,
        ["--file", "src/calc.py", "--analyzer-timeout", "1ns"],
        &marker,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty(), "timed-out plan emitted a manifest");
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        "plan.discovery: analyzer.timeout: --analyzer-timeout expired after 1ns\n"
    );
    assert!(!marker.exists(), "plan launched the test command");
}

#[tokio::test]
async fn plan_oversized_analyzer_timeout_returns_two_without_panicking() {
    let project = Project::new();
    let marker = project.path.join("test-command-ran");
    let args = plan_args(
        &project,
        [
            "--file",
            "src/calc.py",
            "--analyzer-timeout",
            "18446744073709551615s",
        ],
        &marker,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty(), "invalid deadline emitted a manifest");
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        "invalid zero or overflowing limit: --analyzer-timeout\n"
    );
    assert!(!marker.exists(), "plan launched the test command");
}

#[tokio::test]
async fn verify_analyzer_timeout_dispatch_stops_before_the_test_command() {
    let project = Project::new();
    let (path, mut manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = manifest.candidates[0].id.clone();
    let ParsedCommand::Plan(plan) = parse_from(plan_args(
        &project,
        ["--file", "src/calc.py", "--analyzer-timeout", "1ns"],
        &marker,
    ))
    .unwrap() else {
        panic!("expected plan arguments")
    };
    manifest.normalized_config.limits.analyzer_timeout =
        plan.into_run_config().unwrap().limits.analyzer_timeout;
    write_json(&path, &serde_json::to_value(&manifest).unwrap());
    let args = [
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--candidate"),
        OsString::from(requested),
        OsString::from("--format"),
        OsString::from("jsonl"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 2);
    assert!(stdout.is_empty(), "timed-out verify emitted run output");
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        "plan.discovery: analyzer.timeout: --analyzer-timeout expired after 1ns\n"
    );
    assert!(!marker.exists(), "verify launched the test command");
}

#[tokio::test]
async fn verify_rejects_changed_source_before_baseline() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    std::fs::write(
        project.path.join("src/calc.py"),
        "def only_add(left, right):\n    return left - right\n",
    )
    .unwrap();

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.source.changed");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_reports_source_change_before_ranking_for_changed_symbol_plan() {
    let (project, _base_revision) = Project::new_changed_git();
    let (path, manifest, marker) =
        write_plan_manifest(&project, &["--changed", "--symbol", "calc:changed"]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    run_git(&project.path, &["add", "src/calc.py"]);
    run_git(
        &project.path,
        &["commit", "--quiet", "-m", "commit planned change"],
    );

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.source.changed");
    assert!(!marker.exists());
}

type ManifestMutation = fn(&mut serde_json::Value);
type InvalidManifestCase = (&'static str, ManifestMutation, &'static str);

fn invalid_normalized_config_cases() -> Vec<InvalidManifestCase> {
    let cases: Vec<InvalidManifestCase> = vec![
        (
            "empty argv",
            |value| {
                value["normalized_config"]["test_argv"] = serde_json::json!([]);
            },
            "at least one test argv element is required",
        ),
        (
            "jobs exceed processes",
            |value| {
                value["normalized_config"]["limits"]["jobs"] = serde_json::json!(2);
                value["normalized_config"]["limits"]["max_processes"] = serde_json::json!(1);
            },
            "--jobs 2 exceeds --max-processes 1",
        ),
        (
            "zero total timeout",
            |value| {
                value["normalized_config"]["limits"]["total_timeout"] =
                    serde_json::json!({"secs": 0, "nanos": 0});
            },
            "invalid zero or overflowing limit: --total-timeout",
        ),
        (
            "missing selector",
            |value| {
                let selection = &mut value["normalized_config"]["selection"];
                selection["sources"] = serde_json::json!([]);
                selection["files"] = serde_json::json!([]);
                selection["lines"] = serde_json::json!([]);
                selection["symbols"] = serde_json::json!([]);
                selection["changed"] = serde_json::json!(false);
            },
            "at least one target selector is required",
        ),
        (
            "diff base without changed",
            |value| {
                let selection = &mut value["normalized_config"]["selection"];
                selection["diff_base"] = serde_json::json!("HEAD");
                selection["changed"] = serde_json::json!(false);
            },
            "--diff-base requires --changed",
        ),
        (
            "changed without source",
            |value| {
                let selection = &mut value["normalized_config"]["selection"];
                selection["sources"] = serde_json::json!([]);
                selection["changed"] = serde_json::json!(true);
            },
            "--changed requires --source",
        ),
        (
            "symbol without source",
            |value| {
                let selection = &mut value["normalized_config"]["selection"];
                selection["sources"] = serde_json::json!([]);
                selection["files"] = serde_json::json!([]);
                selection["symbols"] = serde_json::json!([{
                    "module": "calc",
                    "qualname": "only_add",
                }]);
            },
            "--symbol requires --source",
        ),
        (
            "jobs exceed maximum",
            |value| {
                value["normalized_config"]["limits"]["jobs"] = serde_json::json!(MAX_JOBS + 1);
            },
            "--jobs 257 exceeds the supported maximum 256",
        ),
        (
            "overflowing baseline timeout",
            |value| {
                value["normalized_config"]["limits"]["baseline_timeout"] =
                    serde_json::json!({"secs": u64::MAX, "nanos": 0});
            },
            "invalid zero or overflowing limit: --baseline-timeout",
        ),
    ];
    #[cfg(target_pointer_width = "64")]
    let cases = {
        let mut cases = cases;
        cases.push((
            "processes exceed u32",
            |value| {
                value["normalized_config"]["limits"]["max_processes"] =
                    serde_json::json!(u64::from(u32::MAX) + 1);
            },
            "invalid zero or overflowing limit: --max-processes",
        ));
        cases
    };
    cases
}

#[tokio::test]
async fn verify_rejects_invalid_normalized_config_before_project_work() {
    for (name, mutate, expected_message) in invalid_normalized_config_cases() {
        let project = Project::new();
        let (path, manifest, baseline_marker) = write_plan_manifest(&project, &[]).await;
        let requested = vec![manifest.candidates[0].id.clone()];
        let mut value = serde_json::to_value(manifest).unwrap();
        mutate(&mut value);
        let tampered: PlanManifest = serde_json::from_value(value.clone())
            .unwrap_or_else(|error| panic!("{name} must remain structurally valid: {error}"));
        assert_eq!(
            tampered
                .normalized_config
                .validate()
                .unwrap_err()
                .to_string(),
            expected_message,
            "{name}"
        );
        write_json(&path, &value);

        std::fs::remove_file(project.path.join("src/calc.py")).unwrap();
        let error = prepare_verify(&path, &requested, OutputFormat::Json)
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("plan.manifest.invalid: {expected_message}"),
            "{name}"
        );
        assert!(!baseline_marker.exists(), "{name}");
    }
}

#[tokio::test]
async fn verify_prioritizes_invalid_normalized_config_over_requested_id_limits() {
    let project = Project::new();
    let (path, manifest, baseline_marker) = write_plan_manifest(&project, &[]).await;
    assert!(manifest.candidates.len() >= 2);
    let requested = manifest
        .candidates
        .iter()
        .take(2)
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let mut value = serde_json::to_value(manifest).unwrap();
    value["normalized_config"]["test_argv"] = serde_json::json!([]);
    value["normalized_config"]["limits"]["max_mutants"] = serde_json::json!(1);
    let tampered: PlanManifest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(tampered.normalized_config.limits.max_mutants.get(), 1);
    assert_eq!(tampered.normalized_config.test_argv, Vec::new());
    write_json(&path, &value);

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "plan.manifest.invalid: at least one test argv element is required"
    );
    assert!(!baseline_marker.exists());
}

#[tokio::test]
async fn verify_rejects_changed_fingerprint_input_before_baseline() {
    let project = Project::new();
    let (path, manifest, marker) =
        write_plan_manifest(&project, &["--fingerprint-include", "config.toml"]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    std::fs::write(project.path.join("config.toml"), "[changed]\n").unwrap();

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.fingerprint_input.changed");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_rejects_fingerprint_input_deleted_after_preparation_before_execution() {
    let project = Project::new();
    let (path, manifest, marker) =
        write_plan_manifest(&project, &["--fingerprint-include", "config.toml"]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    let verified = prepare_verify(&path, &requested, OutputFormat::Jsonl)
        .await
        .unwrap();
    let fingerprint_copy_inputs = verified.fingerprint_copy_inputs();
    std::fs::remove_file(project.path.join("config.toml")).unwrap();
    let ResolvedVerifySelection::ExplicitCandidates(candidate_ids) = verified.selection else {
        panic!("explicit candidate verification must retain an explicit selection");
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit = shell::run_selected_loop_with_fingerprint_inputs(
        verified.config,
        candidate_ids,
        verified.verification_selection,
        fingerprint_copy_inputs,
        &mut stdout,
        &mut stderr,
    )
    .await
    .unwrap();

    assert_eq!(exit, 2, "stderr={}", String::from_utf8_lossy(&stderr));
    let diagnostics = stderr
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "plan.fingerprint_input.changed"),
        "missing changed-input diagnostic: {}",
        String::from_utf8_lossy(&stderr)
    );
    let events = stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(events.iter().all(|event| !matches!(
        event["kind"].as_str(),
        Some("baseline_finished" | "mutant_started" | "mutant_finished")
    )));
    assert!(!marker.exists(), "test command ran after fingerprint drift");
}

#[tokio::test]
async fn verify_exact_file_ignores_nested_names_but_rejects_root_change() {
    let project = Project::new();
    let nested = project.path.join(".worktrees/a/pyproject.toml");
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    std::fs::write(&nested, "nested = 1\n").unwrap();
    let (path, manifest, marker) =
        write_plan_manifest(&project, &["--fingerprint-file", "pyproject.toml"]).await;
    let requested = vec![manifest.candidates[0].id.clone()];

    std::fs::write(&nested, "nested = 2\n").unwrap();
    let verified = prepare_verify(&path, &requested, OutputFormat::Json).await;
    assert!(verified.is_ok(), "{verified:?}");

    std::fs::write(project.path.join("pyproject.toml"), "value = 2\n").unwrap();
    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();
    assert_error_code(error, "plan.fingerprint_input.changed");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_accepts_legacy_manifest_without_fingerprint_files() {
    let project = Project::new();
    let (path, manifest, _marker) = write_plan_manifest(&project, &[]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    let mut value = serde_json::to_value(manifest).unwrap();
    value["normalized_config"]
        .as_object_mut()
        .unwrap()
        .remove("fingerprint_files");
    write_json(&path, &value);

    let verified = prepare_verify(&path, &requested, OutputFormat::Json).await;

    assert!(verified.is_ok(), "{verified:?}");
}

#[tokio::test]
async fn verify_rejects_incoherent_selection_root_before_baseline() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    let other_workspace = project.path.join("other-workspace");
    std::fs::create_dir_all(other_workspace.join("src")).unwrap();
    std::fs::copy(
        project.path.join("src/calc.py"),
        other_workspace.join("src/calc.py"),
    )
    .unwrap();
    let mut value = serde_json::to_value(manifest).unwrap();
    value["normalized_config"]["selection"]["root"] = serde_json::json!(other_workspace);
    write_json(&path, &value);

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.manifest.invalid");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_rejects_tampered_candidate_before_baseline() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    let mut value = serde_json::to_value(manifest).unwrap();
    value["candidates"][0]["original"] = serde_json::json!("wrong");
    write_json(&path, &value);

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.candidate.invalid");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_rejects_a_tampered_requested_symbol_after_scoped_rediscovery() {
    let project = Project::new_with_sources(&[
        ("a.py", "def first(left, right):\n    return left + right\n"),
        (
            "b.py",
            "def second(left, right):\n    return left == right\n",
        ),
    ]);
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested_index = manifest
        .candidates
        .iter()
        .position(|candidate| candidate.path == Path::new("src/b.py"))
        .unwrap();
    let requested = manifest.candidates[requested_index].id.clone();
    let mut value = serde_json::to_value(manifest).unwrap();
    value["candidates"][requested_index]["symbol"] = serde_json::json!("forged");
    write_json(&path, &value);

    let error = prepare_verify(&path, &[requested], OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.candidate.invalid");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_rejects_malformed_headers_and_source_paths() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    let original = serde_json::to_value(manifest).unwrap();

    std::fs::write(&path, b"{").unwrap();
    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();
    assert_error_code(error, "plan.manifest.invalid");

    for case in [
        "schema",
        "ranking_version",
        "kind",
        "parent_path",
        "absolute_path",
    ] {
        let mut value = original.clone();
        match case {
            "schema" => value["schema_version"] = serde_json::json!(1),
            "ranking_version" => value["ranking_rule_version"] = serde_json::json!(1),
            "kind" => value["kind"] = serde_json::json!("report"),
            "parent_path" => value["sources"][0]["path"] = serde_json::json!("../outside.py"),
            "absolute_path" => value["sources"][0]["path"] = serde_json::json!("/outside.py"),
            _ => unreachable!(),
        }
        write_json(&path, &value);

        let error = prepare_verify(&path, &requested, OutputFormat::Json)
            .await
            .unwrap_err();
        if case == "ranking_version" {
            assert!(
                error
                    .to_string()
                    .contains("unsupported ranking rule version 1"),
                "{error}"
            );
        }
        assert_error_code(error, "plan.manifest.invalid");
    }
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_rejects_a_ranking_version_three_plan_before_baseline_with_regeneration_guidance() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = vec![manifest.candidates[0].id.clone()];
    let mut value = serde_json::to_value(manifest).unwrap();
    assert_eq!(value["schema_version"], 5);
    value["ranking_rule_version"] = serde_json::json!(3);
    write_json(&path, &value);

    let error = prepare_verify(&path, &requested, OutputFormat::Json)
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "plan.manifest.invalid: unsupported ranking rule version 3; regenerate the plan with this hoimin version"
    );
    assert!(!marker.exists(), "ranking rejection must precede baseline");
}

#[tokio::test]
async fn verify_rejects_duplicate_manifest_candidates_and_missing_requested_ids() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let mut duplicate = serde_json::to_value(&manifest).unwrap();
    let candidate = duplicate["candidates"][0].clone();
    duplicate["candidates"]
        .as_array_mut()
        .unwrap()
        .push(candidate);
    write_json(&path, &duplicate);

    let error = prepare_verify(
        &path,
        &[manifest.candidates[0].id.clone()],
        OutputFormat::Json,
    )
    .await
    .unwrap_err();
    assert_error_code(error, "plan.manifest.invalid");

    write_json(&path, &serde_json::to_value(manifest).unwrap());
    let error = prepare_verify(
        &path,
        &["m1_0000000000000000000000000000000000000000000000000000000000000000".to_owned()],
        OutputFormat::Json,
    )
    .await
    .unwrap_err();
    assert_error_code(error, "plan.candidate.invalid");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_deduplicates_requested_ids_and_rejects_max_mutants_overflow() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    assert!(manifest.candidates.len() >= 2);
    let first = manifest.candidates[0].id.clone();
    let second = manifest.candidates[1].id.clone();

    let verified = prepare_verify(&path, &[first.clone(), first.clone()], OutputFormat::Human)
        .await
        .unwrap();
    assert_eq!(
        verified.selection,
        ResolvedVerifySelection::ExplicitCandidates(BTreeSet::from([first.clone()]))
    );
    assert_eq!(verified.config.output.format, CoreOutputFormat::Human);
    assert_eq!(verified.config.session, None);
    assert!(!verified.config.resume);

    let mut overflow = serde_json::to_value(manifest).unwrap();
    overflow["normalized_config"]["limits"]["max_mutants"] = serde_json::json!(1);
    write_json(&path, &overflow);
    let error = prepare_verify(&path, &[first, second], OutputFormat::Json)
        .await
        .unwrap_err();
    assert_error_code(error, "plan.candidate.invalid");
    assert!(!marker.exists());
}

#[tokio::test]
async fn truncated_plan_accepts_a_contained_candidate() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &["--max-candidates", "1"]).await;
    assert!(manifest.truncated);
    let requested = vec![manifest.candidates[0].id.clone()];

    let verified = prepare_verify(&path, &requested, OutputFormat::Jsonl)
        .await
        .unwrap();

    assert!(matches!(
        verified.selection,
        ResolvedVerifySelection::ExplicitCandidates(ref ids) if ids.len() == 1
    ));
    assert_eq!(verified.config.output.format, CoreOutputFormat::Jsonl);
    assert!(!marker.exists());
}

#[tokio::test]
async fn truncated_plan_accepts_a_retained_candidate_from_a_later_file() {
    let project = Project::new_with_sources(&[
        ("a.py", "def first(left, right):\n    return left + right\n"),
        (
            "b.py",
            "def second(left, right):\n    return left == right and left != right\n",
        ),
    ]);
    let (path, manifest, marker) = write_plan_manifest(&project, &["--max-candidates", "2"]).await;
    assert!(manifest.truncated);
    let candidate = manifest
        .candidates
        .iter()
        .find(|candidate| candidate.path == Path::new("src/b.py"))
        .unwrap();
    assert_eq!(candidate.sequence, 2);

    let verified = prepare_verify(
        &path,
        std::slice::from_ref(&candidate.id),
        OutputFormat::Jsonl,
    )
    .await
    .unwrap();

    assert!(verified.plan_truncated);
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_accepts_a_requested_candidate_from_a_later_file() {
    let project = Project::new_with_sources(&[
        ("a.py", "def first(left, right):\n    return left + right\n"),
        (
            "b.py",
            "def second(left, right):\n    return left == right\n",
        ),
    ]);
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let candidate = manifest
        .candidates
        .iter()
        .find(|candidate| candidate.path == Path::new("src/b.py"))
        .unwrap();
    assert!(candidate.sequence > 1);

    let verified = prepare_verify(
        &path,
        std::slice::from_ref(&candidate.id),
        OutputFormat::Json,
    )
    .await
    .unwrap();

    assert_eq!(
        verified.selection,
        ResolvedVerifySelection::ExplicitCandidates(BTreeSet::from([candidate.id.clone()]))
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_does_not_analyze_an_unrequested_file_after_source_record_validation() {
    let project = Project::new_with_sources(&[
        ("a.py", "# no mutation candidates\n"),
        (
            "b.py",
            "def second(left, right):\n    return left == right\n",
        ),
    ]);
    let (path, mut manifest, marker) = write_plan_manifest(&project, &[]).await;
    let requested = manifest
        .candidates
        .iter()
        .find(|candidate| candidate.path == Path::new("src/b.py"))
        .unwrap()
        .id
        .clone();
    let unrequested_bytes = [0xff];
    std::fs::write(project.path.join("src/a.py"), unrequested_bytes).unwrap();
    manifest
        .sources
        .iter_mut()
        .find(|source| source.path == Path::new("src/a.py"))
        .unwrap()
        .hash = blake3::hash(&unrequested_bytes).to_hex().to_string();
    write_json(&path, &serde_json::to_value(manifest).unwrap());

    let verified = prepare_verify(&path, &[requested], OutputFormat::Json).await;

    assert!(verified.is_ok(), "{verified:?}");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_accepts_generated_sequence_permutations_and_same_span_operators() {
    let project = Project::new_with_source(
        "def selected(items, value, left, right):\n    items.append(value)\n    return left == right\n",
    );
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    assert!(
        manifest
            .candidates
            .windows(2)
            .any(|pair| pair[0].sequence > pair[1].sequence),
        "ranking should differ from discovery order"
    );
    assert!(manifest.candidates.iter().enumerate().any(|(index, left)| {
        manifest.candidates[index + 1..]
            .iter()
            .any(|right| left.span == right.span && left.operator != right.operator)
    }));
    let requested = manifest
        .candidates
        .iter()
        .find(|candidate| candidate.operator == "structure_append_extend")
        .unwrap()
        .id
        .clone();

    let verified = prepare_verify(&path, &[requested], OutputFormat::Json).await;

    assert!(verified.is_ok(), "{verified:?}");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_rejects_candidate_sequences_that_are_not_a_complete_permutation() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    assert!(manifest.candidates.len() >= 2);
    let requested = manifest.candidates[0].id.clone();
    let mut value = serde_json::to_value(&manifest).unwrap();
    value["candidates"][0]["sequence"] = serde_json::json!(manifest.candidates[1].sequence);
    write_json(&path, &value);

    let error = prepare_verify(&path, &[requested], OutputFormat::Json)
        .await
        .unwrap_err();

    assert_error_code(error, "plan.candidate.invalid");
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_top_resolves_the_saved_rank_prefix_and_retained_scope() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    assert!(manifest.candidates.len() >= 2);
    let expected = manifest
        .candidates
        .iter()
        .take(1)
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();

    let verified = prepare_verify_selection(
        &path,
        &VerifySelection::Top {
            count: std::num::NonZeroUsize::new(1).unwrap(),
            policy: TopSelectionPolicy::Strict,
        },
        OutputFormat::Json,
    )
    .await
    .unwrap();

    assert_eq!(
        verified.selection,
        ResolvedVerifySelection::RankedCandidates(expected)
    );
    assert_eq!(
        verified.selection_scope,
        VerifySelectionScope::RetainedCandidates
    );
    assert!(!verified.plan_truncated);
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_top_above_a_truncated_plan_selects_every_retained_candidate() {
    let project = Project::new();
    let (path, manifest, marker) = write_plan_manifest(&project, &["--max-candidates", "1"]).await;
    assert!(manifest.truncated);
    let expected = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();

    let verified = prepare_verify_selection(
        &path,
        &VerifySelection::Top {
            count: std::num::NonZeroUsize::new(30).unwrap(),
            policy: TopSelectionPolicy::Strict,
        },
        OutputFormat::Json,
    )
    .await
    .unwrap();

    assert_eq!(
        verified.selection,
        ResolvedVerifySelection::RankedCandidates(expected)
    );
    assert_eq!(
        verified.selection_scope,
        VerifySelectionScope::RetainedCandidates
    );
    assert!(verified.plan_truncated);
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_top_diverse_round_robins_equal_score_candidates_without_mutating_the_manifest() {
    let project = Project::new_with_sources(&[
        (
            "a.py",
            "def a1(left, right):\n    return left == right\n\ndef a2(left, right):\n    return left == right\n",
        ),
        (
            "b.py",
            "def b1(left, right):\n    return left == right\n\ndef b2(left, right):\n    return left == right\n",
        ),
        ("c.py", "def c1(left, right):\n    return left == right\n"),
    ]);
    let (path, manifest, marker) =
        write_plan_manifest(&project, &["--operators", "compare_eq_ne"]).await;
    assert_eq!(manifest.candidates.len(), 5);
    assert!(
        manifest
            .candidates
            .windows(2)
            .all(|pair| pair[0].score == pair[1].score)
    );
    let ids = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let paths = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        ["src/a.py", "src/a.py", "src/b.py", "src/b.py", "src/c.py"]
    );
    let expected = vec![
        ids[0].clone(),
        ids[2].clone(),
        ids[4].clone(),
        ids[1].clone(),
        ids[3].clone(),
    ];
    let before = std::fs::read(&path).unwrap();

    let verified = prepare_verify_selection(
        &path,
        &VerifySelection::Top {
            count: std::num::NonZeroUsize::new(5).unwrap(),
            policy: TopSelectionPolicy::Diverse,
        },
        OutputFormat::Json,
    )
    .await
    .unwrap();

    assert_eq!(
        verified.selection,
        ResolvedVerifySelection::RankedCandidates(expected)
    );
    assert_eq!(
        verified.verification_selection.policy,
        VerificationSelectionPolicy::FileRoundRobinV1
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_top_diverse_does_not_cross_score_tiers() {
    let project = Project::new_with_sources(&[
        (
            "a.py",
            "def a1(left, right):\n    return left == right\n\ndef a2(left, right):\n    return left == right\n",
        ),
        ("b.py", "def b1(left, right):\n    return left + right\n"),
    ]);
    let (path, manifest, marker) = write_plan_manifest(&project, &[]).await;
    let a2 = manifest
        .candidates
        .iter()
        .filter(|candidate| candidate.path == Path::new("src/a.py"))
        .nth(1)
        .unwrap();
    let b1 = manifest
        .candidates
        .iter()
        .find(|candidate| candidate.path == Path::new("src/b.py"))
        .unwrap();
    assert!(a2.score > b1.score);
    let expected = vec![
        manifest.candidates[0].id.clone(),
        a2.id.clone(),
        b1.id.clone(),
    ];
    let before = std::fs::read(&path).unwrap();

    let verified = prepare_verify_selection(
        &path,
        &VerifySelection::Top {
            count: std::num::NonZeroUsize::new(3).unwrap(),
            policy: TopSelectionPolicy::Diverse,
        },
        OutputFormat::Json,
    )
    .await
    .unwrap();

    assert_eq!(
        verified.selection,
        ResolvedVerifySelection::RankedCandidates(expected)
    );
    assert_eq!(
        verified.verification_selection.policy,
        VerificationSelectionPolicy::FileRoundRobinV1
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!marker.exists());
}

#[tokio::test]
async fn verify_top_real_cli_reports_diverse_order_and_preserves_strict_rank_prefix() {
    let project = Project::new_with_sources(&[
        (
            "a.py",
            "def a1(left, right):\n    return left == right\n\ndef a2(left, right):\n    return left == right\n",
        ),
        (
            "b.py",
            "def b1(left, right):\n    return left == right\n\ndef b2(left, right):\n    return left == right\n",
        ),
        ("c.py", "def c1(left, right):\n    return left == right\n"),
        ("d.py", "def d1(left, right):\n    return left + right\n"),
    ]);
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) = write_plan_manifest_with_marker(&project, &[], &marker).await;
    assert_eq!(manifest.candidates.len(), 6);
    let ranked_ids = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let ranked_paths = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ranked_paths,
        [
            "src/a.py", "src/a.py", "src/b.py", "src/b.py", "src/c.py", "src/d.py",
        ]
    );
    assert!(
        manifest.candidates[..5]
            .iter()
            .all(|candidate| candidate.score > manifest.candidates[5].score)
    );
    let diverse_ids = vec![
        ranked_ids[0].clone(),
        ranked_ids[2].clone(),
        ranked_ids[4].clone(),
        ranked_ids[1].clone(),
        ranked_ids[3].clone(),
        ranked_ids[5].clone(),
    ];
    let plan_before = std::fs::read(&path).unwrap();

    for (policy, expected_policy, expected_ids) in [
        (
            Some("diverse"),
            "file_round_robin_v1",
            diverse_ids.as_slice(),
        ),
        (None, "strict", ranked_ids.as_slice()),
    ] {
        assert_real_cli_top_selection(
            &path,
            &manifest,
            &plan_before,
            &ranked_ids,
            policy,
            expected_policy,
            expected_ids,
        )
        .await;
    }
}

async fn assert_real_cli_top_selection(
    path: &Path,
    manifest: &PlanManifest,
    plan_before: &[u8],
    ranked_ids: &[String],
    policy: Option<&str>,
    expected_policy: &str,
    expected_ids: &[String],
) {
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--top"),
        OsString::from("6"),
    ];
    if let Some(policy) = policy {
        args.extend([OsString::from("--selection-policy"), OsString::from(policy)]);
    }
    args.extend([OsString::from("--format"), OsString::from("jsonl")]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    let events = stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let started = events
        .iter()
        .find(|event| event["kind"] == "run_started")
        .unwrap();
    assert_eq!(started["verification_selection"]["mode"], "top");
    assert_eq!(started["verification_selection"]["policy"], expected_policy);
    let actual_ids = events
        .iter()
        .filter(|event| event["kind"] == "mutant_started")
        .map(|event| event["mutant_id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, expected_ids);
    let lower_tier_position = actual_ids
        .iter()
        .position(|id| id == &ranked_ids[5])
        .unwrap();
    assert!(
        ranked_ids[..5]
            .iter()
            .all(|id| actual_ids[..lower_tier_position].contains(id)),
        "every high-tier candidate must be scheduled before the lower-tier candidate"
    );
    let actual_scores = actual_ids
        .iter()
        .map(|id| {
            manifest
                .candidates
                .iter()
                .find(|candidate| candidate.id == *id)
                .unwrap()
                .score
        })
        .collect::<Vec<_>>();
    assert!(
        actual_scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "a lower-score candidate preceded a remaining higher-score candidate"
    );
    assert!(
        stderr.is_empty(),
        "unexpected policy warning or parse error: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(std::fs::read(path).unwrap(), plan_before);
}

#[tokio::test]
async fn verify_runs_only_requested_candidates() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) = write_plan_manifest_with_marker(&project, &[], &marker).await;
    assert!(manifest.candidates.len() >= 2);
    let requested = vec![
        manifest.candidates[0].id.clone(),
        manifest.candidates[1].id.clone(),
    ];
    let session_path = project.path.join("session.sqlite3");
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("jsonl"),
    ];
    for candidate_id in &requested {
        args.extend([OsString::from("--candidate"), OsString::from(candidate_id)]);
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(marker.exists(), "verify must execute a fresh baseline");
    assert!(
        !session_path.exists(),
        "verify must not create a session database"
    );
    let events = stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        events
            .iter()
            .filter(|event| event["kind"] == "baseline_finished")
            .count(),
        1,
        "verify must execute exactly one fresh baseline"
    );
    let started = events
        .iter()
        .find(|event| event["kind"] == "run_started")
        .unwrap();
    let baseline = events
        .iter()
        .find(|event| event["kind"] == "baseline_finished")
        .unwrap();
    assert_selected_resource_policy(started, baseline);
    assert!(
        events
            .iter()
            .filter(|event| event["kind"] == "mutant_finished")
            .all(|event| event["resource_mode"] == baseline["resource_mode"])
    );
    assert_eq!(
        started["verification_selection"],
        serde_json::json!({
            "mode": "candidate_ids",
            "policy": "explicit_candidates",
            "requested": 2,
            "selected": 2,
            "scope": "explicit_candidates",
            "plan_truncated": false,
        })
    );
    let mut expected_config = serde_json::to_value(&manifest.normalized_config).unwrap();
    expected_config["output"]["format"] = serde_json::json!("jsonl");
    expected_config["session"] = serde_json::Value::Null;
    expected_config["resume"] = serde_json::json!(false);
    assert_eq!(started["normalized_config"], expected_config);
    let actual = events
        .iter()
        .filter(|event| event["kind"] == "mutant_finished")
        .map(|event| event["candidate"]["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), requested.len());
    assert_eq!(
        actual.into_iter().collect::<BTreeSet<_>>(),
        requested.into_iter().collect()
    );
}

#[tokio::test]
async fn verify_top_strict_rejects_empty_plan_before_baseline() {
    assert_empty_top_selection_is_rejected("strict").await;
}

#[tokio::test]
async fn verify_top_diverse_rejects_empty_plan_before_baseline() {
    assert_empty_top_selection_is_rejected("diverse").await;
}

async fn assert_empty_top_selection_is_rejected(policy: &str) {
    let project = Project::new_with_source("# No mutation candidates.\n");
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) = write_plan_manifest_with_marker(&project, &[], &marker).await;
    assert_eq!(manifest.candidates, Vec::new());
    assert!(!manifest.truncated);
    let original_plan = std::fs::read(&path).unwrap();

    for format in ["json", "jsonl"] {
        let args = [
            OsString::from("hoimin"),
            OsString::from("verify"),
            path.as_os_str().to_owned(),
            OsString::from("--top"),
            OsString::from("5"),
            OsString::from("--selection-policy"),
            OsString::from(policy),
            OsString::from("--format"),
            OsString::from(format),
        ];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

        assert_eq!(code, 2, "policy={policy}, format={format}");
        assert!(stdout.is_empty(), "empty verification emitted run output");
        let stderr = String::from_utf8(stderr).unwrap();
        for expected in ["plan.candidate.invalid", "--top", "no retained candidates"] {
            assert!(stderr.contains(expected), "stderr={stderr}");
        }
        assert!(!marker.exists(), "empty verification ran the baseline");
        assert_eq!(std::fs::read(&path).unwrap(), original_plan);
    }
}

#[tokio::test]
async fn verify_top_executes_the_highest_ranked_retained_candidate() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) =
        write_plan_manifest_with_marker(&project, &["--max-candidates", "1"], &marker).await;
    assert!(manifest.truncated);
    let candidate_id = manifest.candidates[0].id.clone();
    let args = [
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--top"),
        OsString::from("1"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 4, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(
        marker.exists(),
        "verify must execute the baseline and selected mutant"
    );
    let document: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert!(!document["baseline"].is_null());
    assert_selected_resource_policy(&document["run"], &document["baseline"]);
    let expected_selection = serde_json::json!({
        "mode": "top",
        "policy": "strict",
        "requested": 1,
        "selected": 1,
        "scope": "retained_candidates",
        "plan_truncated": true,
    });
    assert_eq!(
        document["run"]["verification_selection"],
        expected_selection
    );
    assert_eq!(
        document["summary"]["verification_selection"],
        expected_selection
    );
    assert_eq!(document["summary"]["complete"], false);
    let mutants = document["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), 1);
    assert_eq!(
        mutants[0]["resource_mode"],
        document["baseline"]["resource_mode"]
    );
    assert_eq!(mutants[0]["candidate"]["id"], candidate_id);
}

#[tokio::test]
async fn verify_top_budget_shortfall_warns_on_stderr_and_preserves_json_and_plan() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) = write_budget_plan_manifest(&project, &marker).await;
    assert!(manifest.candidates.len() >= 2);
    let plan_before = std::fs::read(&path).unwrap();
    let limits_before =
        serde_json::to_vec(&serde_json::to_value(&manifest.normalized_config.limits).unwrap())
            .unwrap();
    let args = [
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--top"),
        OsString::from("2"),
        OsString::from("--format"),
        OsString::from("json"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    let _: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert!(
        !String::from_utf8_lossy(&stdout).contains("budget.projected_shortfall"),
        "budget warning leaked to JSON stdout"
    );
    assert_budget_shortfall_warning(&stderr);
    assert_eq!(std::fs::read(&path).unwrap(), plan_before);
    let reloaded: PlanManifest = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let limits_after =
        serde_json::to_vec(&serde_json::to_value(&reloaded.normalized_config.limits).unwrap())
            .unwrap();
    assert_eq!(limits_after, limits_before);
}

#[tokio::test]
async fn verify_top_budget_shortfall_preserves_jsonl_stdout() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) = write_budget_plan_manifest(&project, &marker).await;
    assert!(manifest.candidates.len() >= 2);
    let args = [
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--top"),
        OsString::from("2"),
        OsString::from("--format"),
        OsString::from("jsonl"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    let lines = stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert_ne!(lines, Vec::<&[u8]>::new());
    for line in lines {
        let _: serde_json::Value = serde_json::from_slice(line).unwrap();
        assert!(
            !String::from_utf8_lossy(line).contains("budget.projected_shortfall"),
            "budget warning leaked to JSONL stdout"
        );
    }
    assert_budget_shortfall_warning(&stderr);
}

#[tokio::test]
async fn verify_explicit_candidates_do_not_emit_budget_shortfall_warning() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, manifest) = write_budget_plan_manifest(&project, &marker).await;
    assert!(manifest.candidates.len() >= 2);
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("verify"),
        path.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("json"),
    ];
    for candidate in manifest.candidates.iter().take(2) {
        args.extend([OsString::from("--candidate"), OsString::from(&candidate.id)]);
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    let _: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert!(
        !String::from_utf8_lossy(&stderr).contains("budget.projected_shortfall"),
        "explicit candidate IDs must not emit the top-ranked budget warning"
    );
}

fn assert_budget_shortfall_warning(stderr: &[u8]) {
    let diagnostics = stderr
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .filter(|diagnostic| {
            diagnostic["level"] == "warning" && diagnostic["code"] == "budget.projected_shortfall"
        })
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    let message = diagnostics[0]["message"].as_str().unwrap();
    for expected in [
        "selected=2",
        "jobs=1",
        "planned_total_timeout=10s",
        "baseline=",
        "effective_mutant_timeout=6s",
        "remaining=",
        "projected_capacity=12s",
        "not a guaranteed failure",
        "--jobs",
        "--total-timeout",
        "new plan",
    ] {
        assert!(
            message.contains(expected),
            "missing {expected:?} in diagnostic message: {message}"
        );
    }
}

fn assert_error_code(error: impl std::fmt::Display, code: &str) {
    assert!(
        error.to_string().starts_with(code),
        "expected {code}, got {error}"
    );
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

fn candidate_tuples(
    candidates: &[RankedPlanCandidate],
) -> BTreeSet<(&str, u64, u64, &str, &str, &str, &str)> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate.path.as_str(),
                u64::from(candidate.line),
                u64::from(candidate.column),
                candidate.operator.as_str(),
                candidate.original.as_str(),
                candidate.replacement.as_str(),
                candidate.symbol.as_deref().unwrap(),
            )
        })
        .collect()
}

async fn write_plan_manifest(
    project: &Project,
    options: &[&str],
) -> (PathBuf, PlanManifest, PathBuf) {
    let marker = project.path.join("test-command-ran");
    let (path, manifest) = write_plan_manifest_with_marker(project, options, &marker).await;
    (path, manifest, marker)
}

async fn write_plan_manifest_with_marker(
    project: &Project,
    options: &[&str],
    marker: &Path,
) -> (PathBuf, PlanManifest) {
    let mut args = plan_args(project, options.iter().copied(), marker);
    insert_test_min_free_space(&mut args);
    let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
        panic!("expected plan arguments");
    };
    let output = create(plan.into_run_config().unwrap()).await.unwrap();
    let path = project.path.join("plan.json");
    write_json(&path, &serde_json::to_value(&output.manifest).unwrap());
    (path, output.manifest)
}

async fn write_budget_plan_manifest(project: &Project, marker: &Path) -> (PathBuf, PlanManifest) {
    let mut args = plan_args(
        project,
        [
            "--jobs",
            "1",
            "--mutant-timeout",
            "6s",
            "--total-timeout",
            "10s",
        ],
        marker,
    );
    insert_test_min_free_space(&mut args);
    *args.last_mut().unwrap() = OsString::from(format!(
        "import time; from pathlib import Path; time.sleep(0.2); \
         Path({:?}).write_text('executed')",
        marker.to_string_lossy()
    ));
    let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
        panic!("expected plan arguments");
    };
    let output = create(plan.into_run_config().unwrap()).await.unwrap();
    let path = project.path.join("plan.json");
    write_json(&path, &serde_json::to_value(&output.manifest).unwrap());
    (path, output.manifest)
}

fn write_json(path: &Path, value: &serde_json::Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

async fn discover_for_plan_args(args: Vec<OsString>) -> Vec<MutationCandidate> {
    let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
        panic!("expected plan arguments");
    };
    let config = shell::prepare_run_config(plan.into_run_config().unwrap()).unwrap();
    let targets = TargetHandler::resolve(&config.selection).await.unwrap();
    discover_targets(
        &config.root,
        &targets,
        &config.operators,
        config.profile,
        config.limits.max_candidates.get(),
    )
    .await
    .unwrap()
    .candidates
}

fn plan_args<'a>(
    project: &Project,
    options: impl IntoIterator<Item = &'a str>,
    marker: &Path,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        project.path.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--allow-best-effort-memory"),
    ];
    args.extend(options.into_iter().map(OsString::from));
    args.extend([
        OsString::from("--"),
        python_executable().into_os_string(),
        OsString::from("-c"),
        OsString::from(format!(
            "from pathlib import Path; Path({:?}).write_text('executed')",
            marker.to_string_lossy()
        )),
    ]);
    args
}

fn insert_test_min_free_space(args: &mut Vec<OsString>) {
    assert!(
        !args.iter().any(|argument| argument == "--min-free-space"),
        "test fixture must not override an explicit filesystem reserve"
    );
    let separator = args
        .iter()
        .position(|argument| argument == "--")
        .expect("test command separator");
    args.splice(
        separator..separator,
        [
            OsString::from("--min-free-space"),
            OsString::from(TEST_MIN_FREE_SPACE),
        ],
    );
}

#[cfg(target_os = "macos")]
fn plan_args_without_best_effort(project: &Project, marker: &Path) -> Vec<OsString> {
    vec![
        OsString::from("hoimin"),
        OsString::from("plan"),
        OsString::from("--root"),
        project.path.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--"),
        python_executable().into_os_string(),
        OsString::from("-c"),
        OsString::from(format!(
            "from pathlib import Path; Path({:?}).write_text('executed')",
            marker.to_string_lossy()
        )),
    ]
}

struct Project {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl Project {
    fn new() -> Self {
        Self::new_with_source(
            "def only_add(left, right):\n    return left + right\n\ndef equal(left, right):\n    return left == right\n",
        )
    }

    fn new_with_source(source: &str) -> Self {
        Self::new_with_sources(&[("calc.py", source)])
    }

    fn new_with_sources(sources: &[(&str, &str)]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_owned();
        std::fs::create_dir(path.join("src")).unwrap();
        for (name, source) in sources {
            std::fs::write(path.join("src").join(name), source).unwrap();
        }
        std::fs::write(path.join("config.toml"), "[tool.hoimin]\n").unwrap();
        std::fs::write(path.join("pyproject.toml"), "value = 1\n").unwrap();
        Self {
            _directory: directory,
            path,
        }
    }

    fn new_changed_git() -> (Self, String) {
        let project = Self::new_with_source(
            "def changed(a, b):\n    return a + b\n\ndef untouched(a, b):\n    return a + b\n",
        );
        run_git(&project.path, &["init", "--quiet"]);
        run_git(&project.path, &["config", "user.name", "Hoimin Test"]);
        run_git(
            &project.path,
            &["config", "user.email", "hoimin-test@example.invalid"],
        );
        run_git(&project.path, &["add", "src/calc.py"]);
        run_git(&project.path, &["commit", "--quiet", "-m", "fixture base"]);
        let base_revision = run_git(&project.path, &["rev-parse", "HEAD"]);
        std::fs::write(
            project.path.join("src/calc.py"),
            "def changed(a, b):\n    return a + b  # changed\n\ndef untouched(a, b):\n    return a + b\n",
        )
        .unwrap();
        (project, base_revision)
    }
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

fn python_executable() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = if cfg!(windows) {
        root.join(".venv/Scripts/python.exe")
    } else {
        root.join(".venv/bin/python")
    };
    assert!(
        executable.is_file(),
        "missing controlled test Python interpreter: {}",
        executable.display()
    );
    executable
}

fn assert_selected_resource_policy(run: &serde_json::Value, baseline: &serde_json::Value) {
    assert_eq!(run["resource_control"]["mode"], baseline["resource_mode"]);
    assert_ne!(run["resource_control"]["mechanism"].as_str().unwrap(), "");
    if cfg!(target_os = "macos") {
        assert_eq!(run["resource_control"]["mode"], "best_effort");
        assert_eq!(run["resource_control"]["mechanism"], "portable");
    }
}

#[tokio::test]
async fn root_selection_excludes_uncopyable_sources_in_plan_run_and_verify() {
    let project = Project::new_with_source("value = 1 + 2\n");
    std::fs::create_dir(project.path.join("venv")).unwrap();
    std::fs::write(project.path.join("venv/dep.py"), "value = 3 + 4\n").unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let mut args = plan_args(
        &project,
        [
            "--source",
            ".",
            "--include",
            "venv/**",
            "--operators",
            "binary_add_sub",
        ],
        &marker,
    );
    insert_test_min_free_space(&mut args);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        hoimin_cli::run_with_io(args.clone(), &mut stdout, &mut stderr).await,
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(manifest.candidates.len(), 1);
    assert_eq!(manifest.candidates[0].path, "src/calc.py");
    assert!(!marker.exists());
    let path = coordinator.path().join("plan.json");
    std::fs::write(&path, &stdout).unwrap();
    let verify_args = vec![
        "hoimin".into(),
        "verify".into(),
        path.into_os_string(),
        "--top".into(),
        "1".into(),
    ];
    args[1] = "run".into();
    for command in [verify_args, args] {
        stdout.clear();
        stderr.clear();
        let code = hoimin_cli::run_with_io(command, &mut stdout, &mut stderr).await;
        assert_eq!(code, 1, "{}", String::from_utf8_lossy(&stderr));
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        // pins: issue #452 — source discovery previously admitted files omitted from copying.
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["survived"], 1);
        assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
        assert!(marker.exists());
        std::fs::remove_file(&marker).unwrap();
    }
}

#[tokio::test]
async fn plan_rejects_an_explicit_uncopyable_file_before_baseline() {
    let project = Project::new_with_source("value = 1 + 2\n");
    std::fs::create_dir(project.path.join("src/venv")).unwrap();
    std::fs::write(project.path.join("src/venv/dep.py"), "value = 3 + 4\n").unwrap();
    let marker = project.path.join("test-command-ran");
    let args = plan_args(&project, ["--file", "src/venv/dep.py"], &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    let error = String::from_utf8(stderr).unwrap();
    assert!(error.contains("src/venv/dep.py"), "{error}");
    assert!(error.contains("outside"), "{error}");
    assert!(!marker.exists());
}

#[tokio::test]
async fn python_physical_lines_match_plan_run_and_verify_selection() {
    for (newline, ending) in [("\r", ""), ("\r\n", "\r\n"), ("\n", "\n"), ("\r", "\r\n")] {
        let source = format!("def f():{newline}    return 1 + 2{ending}");
        let project = Project::new_with_source(&source);
        let coordinator = tempfile::tempdir().unwrap();
        let marker = coordinator.path().join("test-command-ran");
        let mut args = plan_args(
            &project,
            ["--line", "src/calc.py:2-2", "--operators", "binary_add_sub"],
            &marker,
        );
        *args.last_mut().unwrap() = OsString::from(format!(
            "from pathlib import Path; Path({:?}).write_text('executed'); from src.calc import f; assert f() == 3",
            marker.to_string_lossy()
        ));
        insert_test_min_free_space(&mut args);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            hoimin_cli::run_with_io(args.clone(), &mut stdout, &mut stderr).await,
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let mut manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(manifest.candidates.len(), 1, "{source:?}");
        let candidate = &manifest.candidates[0];
        assert_eq!((candidate.line, candidate.column), (2, 13));
        assert_eq!(
            usize::try_from(candidate.span.start).unwrap(),
            source.find('+').unwrap()
        );
        assert_eq!(
            candidate.file_hash,
            blake3::hash(source.as_bytes()).to_hex().to_string()
        );
        let path = coordinator.path().join("plan.json");
        std::fs::write(&path, &stdout).unwrap();
        let verify_args = vec![
            "hoimin".into(),
            "verify".into(),
            path.clone().into_os_string(),
            "--top".into(),
            "1".into(),
        ];
        args[1] = "run".into();
        for command in [verify_args.clone(), args] {
            stdout.clear();
            stderr.clear();
            let code = hoimin_cli::run_with_io(command, &mut stdout, &mut stderr).await;
            assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
            let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            assert_eq!(report["summary"]["complete"], true);
            // pins: issue #455 — lone CR previously produced an empty successful run.
            assert_eq!(report["summary"]["counts"]["killed"], 1);
            assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
            assert!(marker.exists());
            std::fs::remove_file(&marker).unwrap();
        }
        if newline == "\r" {
            manifest.candidates[0].candidate.line = 1;
            manifest.candidates[0].candidate.column = 22;
            write_json(&path, &serde_json::to_value(&manifest).unwrap());
            stdout.clear();
            stderr.clear();
            let code = hoimin_cli::run_with_io(verify_args, &mut stdout, &mut stderr).await;
            assert_eq!(code, 2);
            assert!(!marker.exists(), "stale CR plan must fail before baseline");
        }
        assert_eq!(
            std::fs::read(project.path.join("src/calc.py")).unwrap(),
            source.as_bytes()
        );
    }
}

#[tokio::test]
async fn oversized_candidate_plan_fails_with_context_before_baseline() {
    let source = format!("value = [\"{}\"]\n", "a".repeat(1100 * 1024));
    let project = Project::new_with_source(&source);
    let marker = project.path.join("test-command-ran");
    let args = plan_args(&project, ["--operators", "collection_list_tuple"], &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    // pins: issue #459 — plan used to succeed for a candidate the spool cannot encode.
    assert_eq!(code, 2);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(!marker.exists());
    let error = String::from_utf8(stderr).unwrap();
    for detail in ["src/calc.py:1", "collection_list_tuple", "2097152"] {
        assert!(error.contains(detail), "missing {detail}: {error}");
    }
}

#[tokio::test]
async fn oversized_legacy_candidate_is_rejected_before_verify_baseline() {
    let project = Project::new_with_source("value = [\"a\"]\n");
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("test-command-ran");
    let (path, mut manifest) = write_plan_manifest_with_marker(
        &project,
        &["--operators", "collection_list_tuple"],
        &marker,
    )
    .await;
    let contents = format!("\"{}\"", "a".repeat(1100 * 1024));
    let source = format!("value = [{contents}]\n");
    std::fs::write(project.path.join("src/calc.py"), &source).unwrap();
    let candidate = &mut manifest.candidates[0].candidate;
    candidate.original = format!("[{contents}]");
    candidate.replacement = format!("({contents},)");
    candidate.span.length = u64::try_from(candidate.original.len()).unwrap();
    candidate.file_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    candidate.id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity {
        schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
        file_hash: candidate.file_hash.clone(),
        path: candidate.path.clone(),
        span: candidate.span,
        operator: candidate.operator.clone(),
        replacement: candidate.replacement.clone(),
    })
    .to_string();
    manifest.sources[0].hash.clone_from(&candidate.file_hash);
    write_json(&path, &serde_json::to_value(&manifest).unwrap());
    let args = vec![
        "hoimin".into(),
        "verify".into(),
        path.into_os_string(),
        "--top".into(),
        "1".into(),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 2);
    // pins: issue #459 — an old valid oversized plan used to start baseline first.
    assert!(!marker.exists());
    assert_eq!(stdout, Vec::<u8>::new());
    let error = String::from_utf8(stderr).unwrap();
    for detail in ["src/calc.py:1", "collection_list_tuple", "2097152"] {
        assert!(error.contains(detail), "missing {detail}: {error}");
    }
}

#[tokio::test]
async fn candidate_record_limits_agree_for_plan_verify_and_direct_run() {
    for (size, fits) in [(900 * 1024, true), (1100 * 1024, false)] {
        let source = format!("value = [\"{}\"]\n", "a".repeat(size));
        let project = Project::new_with_source(&source);
        let coordinator = tempfile::tempdir().unwrap();
        let marker = coordinator.path().join("test-command-ran");
        let mut args = plan_args(&project, ["--operators", "collection_list_tuple"], &marker);
        insert_test_min_free_space(&mut args);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if fits {
            assert_eq!(
                hoimin_cli::run_with_io(args.clone(), &mut stdout, &mut stderr).await,
                0
            );
            let path = coordinator.path().join("plan.json");
            std::fs::write(&path, &stdout).unwrap();
            stdout.clear();
            stderr.clear();
            let verify = vec![
                "hoimin".into(),
                "verify".into(),
                path.into_os_string(),
                "--top".into(),
                "1".into(),
            ];
            assert_eq!(
                hoimin_cli::run_with_io(verify, &mut stdout, &mut stderr).await,
                1,
                "{}",
                String::from_utf8_lossy(&stderr)
            );
            let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            assert_eq!(report["summary"]["complete"], true);
            assert_eq!(report["summary"]["counts"]["survived"], 1);
        }
        args[1] = "run".into();
        stdout.clear();
        stderr.clear();
        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
        assert_eq!(
            code,
            if fits { 1 } else { 2 },
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(report["summary"]["complete"], fits);
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        if fits {
            assert_eq!(report["summary"]["counts"]["survived"], 1);
        } else {
            assert!(report["mutants"].as_array().unwrap().is_empty(), "{report}");
            let error = String::from_utf8(stderr).unwrap();
            for detail in ["src/calc.py:1", "collection_list_tuple", "2097152"] {
                assert!(error.contains(detail), "missing {detail}: {error}");
            }
        }
    }
}

#[tokio::test]
async fn fixed_batch_partition_covers_120_candidates_without_overriding_limits() {
    use std::fmt::Write as _;
    let mut source = String::new();
    for index in 0..120 {
        writeln!(source, "x{index} = 1 + 2").unwrap();
    }
    let project = Project::new_with_source(&source);
    let (path, manifest, marker) =
        write_plan_manifest(&project, &["--operators", "binary_add_sub"]).await;
    assert_eq!(manifest.candidates.len(), 120);
    let expected: BTreeSet<_> = manifest.candidates.iter().map(|c| c.id.clone()).collect();
    for policy in ["strict", "diverse"] {
        let mut batches = Vec::new();
        for (count, offset) in [("100", "0"), ("20", "100"), ("20", "100")] {
            let ParsedCommand::Verify(args) = parse_from([
                "hoimin",
                "verify",
                path.to_str().unwrap(),
                "--top",
                count,
                "--offset",
                offset,
                "--selection-policy",
                policy,
            ])
            .unwrap() else {
                panic!("verify args");
            };
            let verified = prepare_verify_selection(&args.manifest, &args.selection, args.format)
                .await
                .unwrap();
            assert_eq!(verified.config.limits, manifest.normalized_config.limits);
            let ResolvedVerifySelection::RankedCandidates(ids) = verified.selection else {
                panic!("ranked selection");
            };
            assert_eq!(ids.len(), count.parse::<usize>().unwrap());
            batches.push(ids);
        }
        assert_eq!(batches[1], batches[2]);
        let first: BTreeSet<_> = batches[0].iter().cloned().collect();
        let second: BTreeSet<_> = batches[1].iter().cloned().collect();
        assert!(first.is_disjoint(&second));
        assert_eq!(
            first.union(&second).cloned().collect::<BTreeSet<_>>(),
            expected
        );
    }
    assert!(!marker.exists());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap(),
        serde_json::to_value(manifest).unwrap()
    );
}

#[tokio::test]
async fn fixed_batch_bounds_preserve_truncation_and_reject_invalid_ranges() {
    let project = Project::new_with_source("x = 1 + 2 + 3 + 4\n");
    let (path, manifest, marker) = write_plan_manifest(
        &project,
        &[
            "--operators",
            "binary_add_sub",
            "--max-candidates",
            "2",
            "--max-mutants",
            "1",
        ],
    )
    .await;
    assert!(manifest.truncated);
    let selection = |count, offset| VerifySelection::TopRange {
        count: std::num::NonZeroUsize::new(count).unwrap(),
        policy: TopSelectionPolicy::Strict,
        offset,
    };
    let suffix = prepare_verify_selection(&path, &selection(usize::MAX, 1), OutputFormat::Json)
        .await
        .unwrap();
    assert!(suffix.plan_truncated);
    assert_eq!(suffix.verification_selection.requested, usize::MAX);
    assert_eq!(suffix.verification_selection.selected, 1);
    assert_eq!(suffix.config.limits, manifest.normalized_config.limits);
    for offset in [2, 3, usize::MAX] {
        let error = prepare_verify_selection(&path, &selection(1, offset), OutputFormat::Json)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("--offset"), "{error}");
    }
    let error = prepare_verify_selection(&path, &selection(2, 0), OutputFormat::Json)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("exceeds max_mutants 1"));
    assert!(!marker.exists());
    for args in [
        vec!["hoimin", "verify", "plan.json", "--offset", "1"],
        vec![
            "hoimin",
            "verify",
            "plan.json",
            "--candidate",
            "ID",
            "--offset",
            "1",
        ],
        vec![
            "hoimin",
            "verify",
            "plan.json",
            "--top",
            "0",
            "--offset",
            "1",
        ],
        vec![
            "hoimin",
            "verify",
            "plan.json",
            "--top",
            "1",
            "--offset",
            "-1",
        ],
    ] {
        assert!(parse_from(args).is_err());
    }
}

#[tokio::test]
async fn fixed_batch_real_cli_reports_disjoint_ids_and_repeated_progress() {
    let project = Project::new_with_source("x = 1 + 2 + 3 + 4\n");
    let coordinator = tempfile::tempdir().unwrap();
    let (path, manifest) = write_plan_manifest_with_marker(
        &project,
        &["--operators", "binary_add_sub"],
        &coordinator.path().join("marker"),
    )
    .await;
    let binary = env!("CARGO_BIN_EXE_hoimin");
    let mut ids = Vec::new();
    for (offset, filename) in [("0", "a.json"), ("1", "b-1.json"), ("1", "b-2.json")] {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new(binary)
                .args([
                    "verify",
                    path.to_str().unwrap(),
                    "--top",
                    "1",
                    "--offset",
                    offset,
                ])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .expect("verify subprocess deadline")
        .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["run"]["verification_selection"]["selected"], 1);
        assert_eq!(report["mutants"].as_array().unwrap().len(), 1);
        ids.push(
            report["mutants"][0]["candidate"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        std::fs::write(coordinator.path().join(filename), output.stdout).unwrap();
    }
    assert_eq!(
        ids,
        [
            manifest.candidates[0].id.clone(),
            manifest.candidates[1].id.clone(),
            manifest.candidates[1].id.clone()
        ]
    );
    let progress = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(binary)
            .args(["progress", "--format", "json"])
            .arg(coordinator.path().join("b-1.json"))
            .arg(coordinator.path().join("b-2.json"))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("progress subprocess deadline")
    .unwrap();
    assert!(
        progress.status.success(),
        "{}",
        String::from_utf8_lossy(&progress.stderr)
    );
    let comparison: serde_json::Value = serde_json::from_slice(&progress.stdout).unwrap();
    assert_eq!(comparison["comparisons"][0]["common"], 1);
}

#[tokio::test]
async fn verify_metrics_exports_existing_sidecar_for_each_selection() {
    let project = Project::new_with_source("def total():\n    return 1 + 2 + 3\n");
    let coordinator = tempfile::tempdir().unwrap();
    let (path, manifest) = write_plan_manifest_with_marker(
        &project,
        &[
            "--file",
            "src/calc.py",
            "--operators",
            "binary_add_sub",
            "--jobs",
            "2",
        ],
        &coordinator.path().join("marker"),
    )
    .await;
    assert_eq!(manifest.candidates.len(), 2);
    for selection in [
        vec!["--top", "2"],
        vec!["--top", "2", "--selection-policy", "diverse"],
        vec![
            "--candidate",
            manifest.candidates[0].candidate.id.as_str(),
            "--candidate",
            manifest.candidates[1].candidate.id.as_str(),
        ],
    ] {
        let directory = tempfile::tempdir().unwrap();
        let metrics_path = directory.path().join("metrics.json");
        let mut args = vec![
            OsString::from("hoimin"),
            OsString::from("verify"),
            path.as_os_str().to_owned(),
        ];
        args.extend(selection.iter().map(OsString::from));
        args.extend([
            OsString::from("--metrics"),
            metrics_path.as_os_str().to_owned(),
        ]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
        assert_eq!(exit, 1, "{}", String::from_utf8_lossy(&stderr));
        let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let metrics: hoimin_core::RunMetrics =
            serde_json::from_slice(&std::fs::read(&metrics_path).unwrap()).unwrap();
        metrics.validate().unwrap();
        assert_eq!(metrics.discovered, 2);
        assert_eq!(metrics.executed, 2);
        assert_eq!(metrics.run_id, report["run"]["run_id"]);
        let expected: BTreeSet<_> = manifest
            .candidates
            .iter()
            .map(|c| c.candidate.id.as_str())
            .collect();
        let actual: BTreeSet<_> = report["mutants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["candidate"]["id"].as_str().unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn verify_metrics_preserves_write_warning_and_source_collision_contracts() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, _) = write_plan_manifest_with_marker(&project, &[], &marker).await;
    let source = project.path.join("src/calc.py");
    let original = std::fs::read(&source).unwrap();
    for (destination, expected_exit, diagnostic) in [
        (
            coordinator.path().join("missing/metrics.json"),
            1,
            "metrics.write",
        ),
        (source.clone(), 2, "metrics.destination"),
    ] {
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        let args = [
            OsString::from("hoimin"),
            OsString::from("verify"),
            path.as_os_str().to_owned(),
            OsString::from("--top"),
            OsString::from("1"),
            OsString::from("--metrics"),
            destination.into_os_string(),
        ];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
        assert_eq!(exit, expected_exit, "{}", String::from_utf8_lossy(&stderr));
        assert!(
            String::from_utf8_lossy(&stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(marker.exists(), expected_exit == 1);
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
}

#[tokio::test]
async fn verify_metrics_is_not_written_when_plan_validation_fails() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, _) = write_plan_manifest_with_marker(&project, &[], &marker).await;
    std::fs::write(project.path.join("src/calc.py"), "changed = True\n").unwrap();
    let metrics_path = coordinator.path().join("metrics.json");
    std::fs::write(&metrics_path, "previous sidecar").unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = hoimin_cli::run_with_io(
        [
            OsString::from("hoimin"),
            OsString::from("verify"),
            path.into_os_string(),
            OsString::from("--top"),
            OsString::from("1"),
            OsString::from("--metrics"),
            metrics_path.as_os_str().to_owned(),
        ],
        &mut stdout,
        &mut stderr,
    )
    .await;
    assert_eq!(exit, 2);
    assert!(String::from_utf8_lossy(&stderr).contains("plan.source.changed"));
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read_to_string(metrics_path).unwrap(),
        "previous sidecar"
    );
}

#[tokio::test]
async fn verify_metrics_real_cli_uses_invocation_directory_and_preserves_results() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, _) = write_plan_manifest_with_marker(&project, &[], &marker).await;
    let binary = env!("CARGO_BIN_EXE_hoimin");
    let control = std::process::Command::new(binary)
        .current_dir(coordinator.path())
        .args(["verify", path.to_str().unwrap(), "--top", "2"])
        .output()
        .unwrap();
    let actual = std::process::Command::new(binary)
        .current_dir(coordinator.path())
        .args([
            "verify",
            path.to_str().unwrap(),
            "--top",
            "2",
            "--metrics",
            "metrics.json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        actual.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(actual.status.code(), control.status.code());
    let expected: serde_json::Value = serde_json::from_slice(&control.stdout).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&actual.stdout).unwrap();
    for field in ["candidate", "status"] {
        let values = |v: &serde_json::Value| {
            v["mutants"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m[field].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(values(&report), values(&expected));
    }
    assert!(report["run"]["normalized_config"]["limits"].is_object());
    assert_eq!(
        report["run"]["normalized_config"]["limits"],
        expected["run"]["normalized_config"]["limits"]
    );
    let mut observed_config = report["run"]["normalized_config"].clone();
    let mut expected_config = expected["run"]["normalized_config"].clone();
    assert!(
        observed_config
            .as_object_mut()
            .unwrap()
            .remove("output")
            .is_some()
    );
    assert!(
        expected_config
            .as_object_mut()
            .unwrap()
            .remove("output")
            .is_some()
    );
    assert_eq!(observed_config, expected_config);
    let metrics: hoimin_core::RunMetrics =
        serde_json::from_slice(&std::fs::read(coordinator.path().join("metrics.json")).unwrap())
            .unwrap();
    metrics.validate().unwrap();
    assert_eq!(metrics.executed, 2);
    assert!(!project.path.join("metrics.json").exists());
    let help = std::process::Command::new(binary)
        .args(["verify", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--metrics <PATH>"));
}

#[tokio::test]
async fn verify_metrics_exports_baseline_failure_and_total_timeout_with_jsonl_output() {
    for (command, options, expected_exit) in [
        ("raise SystemExit(1)", vec![], 3),
        (
            "import time; time.sleep(20)",
            vec!["--total-timeout", "100ms"],
            4,
        ),
    ] {
        let project = Project::new();
        let coordinator = tempfile::tempdir().unwrap();
        let mut args = plan_args(&project, options, &coordinator.path().join("marker"));
        insert_test_min_free_space(&mut args);
        *args.last_mut().unwrap() = OsString::from(command);
        let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
            panic!("expected plan");
        };
        let planned = create(plan.into_run_config().unwrap()).await.unwrap();
        let path = project.path.join("plan.json");
        write_json(&path, &serde_json::to_value(&planned.manifest).unwrap());
        let metrics_path = coordinator.path().join("metrics.json");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = hoimin_cli::run_with_io(
            [
                OsString::from("hoimin"),
                OsString::from("verify"),
                path.into_os_string(),
                OsString::from("--top"),
                OsString::from("1"),
                OsString::from("--format"),
                OsString::from("jsonl"),
                OsString::from("--metrics"),
                metrics_path.as_os_str().to_owned(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;
        assert_eq!(exit, expected_exit, "{}", String::from_utf8_lossy(&stderr));
        let events: Vec<serde_json::Value> = stdout
            .split(|b| *b == b'\n')
            .filter(|s| !s.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        if expected_exit == 3 {
            assert!(events.iter().any(|e| e["kind"] == "baseline_finished"));
        }
        assert!(events.iter().any(|e| e["kind"] == "run_finished"));
        let metrics: hoimin_core::RunMetrics =
            serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
        metrics.validate().unwrap();
        assert_eq!(metrics.executed, 0);
        if expected_exit == 3 {
            assert!(metrics.stages.iter().any(|s| s.name == "baseline"));
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn preparation_errors_precede_the_analyzer_deadline_without_baseline() {
    use std::os::unix::fs::PermissionsExt;
    for stage in ["manifest", "source", "fingerprint", "copy"] {
        let project = Project::new();
        let (path, mut manifest, marker) =
            write_plan_manifest(&project, &["--fingerprint-include", "config.toml"]).await;
        let requested = manifest.candidates[0].id.clone();
        let ParsedCommand::Plan(plan) = parse_from(plan_args(
            &project,
            ["--file", "src/calc.py", "--analyzer-timeout", "1ns"],
            &marker,
        ))
        .unwrap() else {
            panic!("plan args")
        };
        manifest.normalized_config.limits.analyzer_timeout =
            plan.into_run_config().unwrap().limits.analyzer_timeout;
        match stage {
            "manifest" => manifest.schema_version = 0,
            "source" => {
                std::fs::write(project.path.join("src/calc.py"), "changed = 1 + 2\n").unwrap();
            }
            "fingerprint" => {
                std::fs::write(project.path.join("config.toml"), "changed = true\n").unwrap();
            }
            "copy" => {
                let unreadable = project.path.join("unreadable.txt");
                std::fs::write(&unreadable, "unselected").unwrap();
                std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o0))
                    .unwrap();
                assert!(
                    std::fs::read(&unreadable).is_err(),
                    "BOUNDARY_INFRASTRUCTURE: fixture requires an unprivileged reader"
                );
            }
            _ => unreachable!(),
        }
        write_json(&path, &serde_json::to_value(manifest).unwrap());
        let error = prepare_verify(&path, &[requested], OutputFormat::Json)
            .await
            .unwrap_err();
        let message = error.to_string();
        let expected = match stage {
            "manifest" => "plan.manifest",
            "source" => "plan.source.changed",
            "fingerprint" => "plan.fingerprint_input.changed",
            "copy" => "plan.workspace",
            _ => unreachable!(),
        };
        assert!(message.contains(expected), "{stage}: {message}");
        assert!(
            !message.contains("analyzer.timeout"),
            "preparation is outside analyzer rediscovery deadline"
        );
        assert!(!marker.exists());
    }
}
#[tokio::test]
async fn symbol_definition_missing_is_rejected_before_baseline() {
    for command in ["plan", "run"] {
        let project = Project::new();
        let marker = project.path.join("baseline-marker");
        let mut args = plan_args(
            &project,
            ["--symbol", "calc:only_add", "--symbol", "calc:missing"],
            &marker,
        );
        args[1] = command.into();
        insert_test_min_free_space(&mut args);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
        // pins: issue #476
        assert_eq!(code, 2, "{command}: {}", String::from_utf8_lossy(&stderr));
        assert!(String::from_utf8_lossy(&stderr).contains("calc:missing"));
        assert!(!marker.exists(), "{command} ran baseline");
    }
}

#[tokio::test]
async fn symbol_definition_missing_is_rejected_with_clean_git() {
    let (project, _) = Project::new_changed_git();
    run_git(&project.path, &["checkout", "--", "src/calc.py"]);
    let marker = project.path.join("baseline-marker");
    let args = plan_args(&project, ["--symbol", "calc:missing", "--changed"], &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert_eq!(code, 2, "{}", String::from_utf8_lossy(&stderr));
    assert!(String::from_utf8_lossy(&stderr).contains("calc:missing"));
    assert!(!marker.exists());
}

#[tokio::test]
async fn symbol_definition_verify_checks_unrequested_symbol_files() {
    let project = Project::new_with_sources(&[
        ("calc.py", "value = 1 + 2\n"),
        ("other.py", "def empty():\n    pass\n"),
    ]);
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("baseline-marker");
    let args = plan_args(
        &project,
        ["--symbol", "other:empty", "--operators", "binary_add_sub"],
        &marker,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
        0
    );
    let mut manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(manifest.candidates.len(), 1);
    manifest.normalized_config.selection.symbols[0].qualname = "missing".into();
    let path = coordinator.path().join("plan.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    stdout.clear();
    stderr.clear();
    let code = hoimin_cli::run_with_io(
        vec![
            "hoimin".into(),
            "verify".into(),
            path.into_os_string(),
            "--top".into(),
            "1".into(),
        ],
        &mut stdout,
        &mut stderr,
    )
    .await;
    assert_eq!(code, 2, "{}", String::from_utf8_lossy(&stderr));
    assert!(String::from_utf8_lossy(&stderr).contains("other:missing"));
    assert!(!marker.exists());
}

#[tokio::test]
async fn symbol_definition_existing_empty_scopes_are_valid() {
    let project = Project::new_with_source(
        "class Box:\n    def method(self):\n        pass\n    class Inner:\n        pass\nasync def outer():\n    def inner():\n        pass\n",
    );
    let marker = project.path.join("baseline-marker");
    for name in ["Box", "Box.method", "Box.Inner", "outer", "outer.inner"] {
        let selector = format!("calc:{name}");
        let args = plan_args(
            &project,
            [
                "--symbol",
                &selector,
                "--operators",
                "boolean_literal",
                "--line",
                "src/calc.py:1",
            ],
            &marker,
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
            0,
            "{name}: {}",
            String::from_utf8_lossy(&stderr)
        );
        let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(manifest.candidates, Vec::new());
        assert!(manifest.diagnostics.is_empty());
    }
}

#[tokio::test]
async fn symbol_definition_existing_unchanged_scope_is_valid() {
    let (project, _) = Project::new_changed_git();
    let marker = project.path.join("baseline-marker");
    for clean in [false, true] {
        if clean {
            run_git(&project.path, &["checkout", "--", "src/calc.py"]);
        }
        let args = plan_args(
            &project,
            ["--symbol", "calc:untouched", "--changed"],
            &marker,
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(manifest.candidates, Vec::new());
        assert!(manifest.diagnostics.is_empty());
    }
}

#[tokio::test]
async fn symbol_definition_package_init_is_valid() {
    let project = Project::new_with_source("value = True\n");
    std::fs::create_dir(project.path.join("src/pkg")).unwrap();
    std::fs::write(
        project.path.join("src/pkg/__init__.py"),
        "def empty():\n    pass\n",
    )
    .unwrap();
    let marker = project.path.join("baseline-marker");
    let args = plan_args(&project, ["--symbol", "pkg:empty"], &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
    assert!(
        manifest
            .candidates
            .iter()
            .all(|candidate| candidate.path == "src/calc.py")
    );
}

#[tokio::test]
async fn symbol_definition_requires_exact_definition_not_assignment_or_prefix() {
    let project = Project::new_with_source(
        "alias = True\nclass BoxOther:\n    def method(self):\n        pass\n",
    );
    let marker = project.path.join("baseline-marker");
    for selector in ["calc:alias", "calc:Box", "calc:BoxOther.missing"] {
        let args = plan_args(
            &project,
            ["--symbol", selector, "--max-candidates", "1"],
            &marker,
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
        assert_eq!(code, 2, "{}", String::from_utf8_lossy(&stderr));
        assert!(String::from_utf8_lossy(&stderr).contains(selector));
    }
}

#[tokio::test]
async fn symbol_definition_invalid_syntax_is_not_diagnosed_as_missing() {
    let project = Project::new_with_source("def broken(:\n    pass\n");
    let marker = project.path.join("baseline-marker");
    let args = plan_args(&project, ["--symbol", "calc:broken"], &marker);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
        2
    );
    let stderr = String::from_utf8_lossy(&stderr);
    assert!(stderr.contains("invalid Python syntax"));
    assert!(stderr.contains("src/calc.py"));
    assert!(!stderr.contains("symbol definition not found"));
    assert!(!marker.exists());
}

fn preview_started_ids(stdout: &[u8]) -> Vec<String> {
    stdout
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .filter(|event| event["kind"] == "mutant_started")
        .map(|event| event["mutant_id"].as_str().unwrap().to_owned())
        .collect()
}

async fn preview_cli(path: &Path, args: &[&str], temporary: &Path) -> std::process::Output {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
            .arg("verify")
            .arg(path)
            .args(args)
            .env("TMPDIR", temporary)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("verify subprocess deadline")
    .unwrap()
}

#[tokio::test]
async fn verify_ranking_large_bodies_preserves_public_preview_and_checks_unselected_entries() {
    let payload = "x".repeat(4096);
    let mut source = String::new();
    for index in 0..64 {
        writeln!(source, "record_{index} = ['{payload}']").unwrap();
    }
    let project = Project::new_with_source(&source);
    let temporary = tempfile::tempdir().unwrap();
    let marker = temporary.path().join("test-command-ran");
    let mut args = plan_args(&project, ["--operators", "collection_list_tuple"], &marker);
    insert_test_min_free_space(&mut args);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
        0,
        "{stderr:?}"
    );
    let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(manifest.candidates.len(), 64);
    let path = project.path.join("plan.json");
    std::fs::write(&path, stdout).unwrap();
    for count in [1, 7] {
        let output = preview_cli(
            &path,
            &["--top", &count.to_string(), "--dry-run"],
            temporary.path(),
        )
        .await;
        assert!(output.status.success(), "{output:?}");
        let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let rows = preview["candidates"].as_array().unwrap();
        assert_eq!(rows.len(), count);
        for (row, candidate) in rows.iter().zip(&manifest.candidates) {
            assert_eq!(row["id"], candidate.id);
            assert_eq!(row["rank"], candidate.rank);
            assert_eq!(row["original"], candidate.original);
            assert_eq!(row["replacement"], candidate.replacement);
        }
        assert!(!marker.exists());
    }
    let mut tampered = serde_json::to_value(&manifest).unwrap();
    tampered["candidates"][63]["score"] = serde_json::json!(70);
    tampered["candidates"][63]["ranking_reasons"] =
        serde_json::json!([{"code": "arithmetic", "score": 70}]);
    write_json(&path, &tampered);
    let output = preview_cli(&path, &["--top", "1", "--dry-run"], temporary.path()).await;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "plan.manifest.invalid: candidate ranking differs from the deterministic ranking rules\n"
    );
    assert!(!marker.exists());
}

async fn stale_cli_diagnostic(path: &Path, temporary: &Path, marker: &Path) -> String {
    let normal = preview_cli(path, &["--top", "1"], temporary).await;
    let preview = preview_cli(path, &["--top", "1", "--dry-run"], temporary).await;
    for output in [&normal, &preview] {
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
    }
    assert_eq!(normal.stderr, preview.stderr);
    assert!(!marker.exists());
    String::from_utf8(normal.stderr).unwrap()
}

#[tokio::test]
async fn verify_stale_details_report_all_six_changes_before_tests_in_both_modes() {
    for fingerprint in [false, true] {
        for change in ["modified", "added", "removed"] {
            let project =
                Project::new_with_sources(&[("a.py", "x = 1 + 2\n"), ("b.py", "x = 3 + 4\n")]);
            std::fs::create_dir(project.path.join("config")).unwrap();
            std::fs::write(project.path.join("config/a.toml"), "value = 1\n").unwrap();
            std::fs::write(project.path.join("config/b.toml"), "value = 2\n").unwrap();
            let temporary = tempfile::tempdir().unwrap();
            let marker = temporary.path().join("marker");
            let (path, _) = write_plan_manifest_with_marker(
                &project,
                &["--fingerprint-include", "config/*.toml"],
                &marker,
            )
            .await;
            let (directory, extension, code, records) = if fingerprint {
                ("config", "toml", "fingerprint_input", "fingerprint input")
            } else {
                ("src", "py", "source", "target source")
            };
            let name = if change == "added" { "c" } else { "b" };
            let relative = format!("{directory}/{name}.{extension}");
            if change == "removed" {
                std::fs::remove_file(project.path.join(&relative)).unwrap();
            } else {
                std::fs::write(project.path.join(&relative), "value = 9 + 10\n").unwrap();
            }
            let diagnostic = stale_cli_diagnostic(&path, temporary.path(), &marker).await;
            assert_eq!(
                diagnostic,
                format!(
                    "plan.{code}.changed: planned {records} records do not match the current workspace: {change} \"{relative}\"\n"
                )
            );
        }
    }
}

#[tokio::test]
async fn verify_stale_details_preserve_resolution_failures_and_source_priority() {
    for failure in ["missing", "unreadable", "symbol", "both"] {
        let project = Project::new();
        let temporary = tempfile::tempdir().unwrap();
        let marker = temporary.path().join("marker");
        let mut options = vec!["--fingerprint-file", "config.toml"];
        if failure == "symbol" {
            options.extend(["--symbol", "calc:only_add"]);
        }
        let (path, _) = write_plan_manifest_with_marker(&project, &options, &marker).await;
        if matches!(failure, "missing" | "unreadable") {
            std::fs::remove_file(project.path.join("config.toml")).unwrap();
            if failure == "unreadable" {
                std::fs::create_dir(project.path.join("config.toml")).unwrap();
            }
        } else {
            std::fs::write(project.path.join("src/calc.py"), "x = 1 + 2\n").unwrap();
            std::fs::write(project.path.join("config.toml"), "changed = true\n").unwrap();
        }
        let diagnostic = stale_cli_diagnostic(&path, temporary.path(), &marker).await;
        match failure {
            "missing" => assert!(
                diagnostic
                    .starts_with("plan.fingerprint_input.changed: fingerprint.file.not_found:"),
                "{diagnostic}"
            ),
            "unreadable" => assert!(
                diagnostic.starts_with(
                    "plan.fingerprint_input.changed: fingerprint.file.unsupported_file:"
                ),
                "{diagnostic}"
            ),
            "symbol" => assert!(
                diagnostic.contains("symbol definition not found: only_add"),
                "{diagnostic}"
            ),
            "both" => assert_eq!(
                diagnostic,
                "plan.source.changed: planned target source records do not match the current workspace: modified \"src/calc.py\"\n"
            ),
            _ => unreachable!(),
        }
        if failure != "both" {
            assert!(!diagnostic.contains("records do not match"), "{diagnostic}");
        }
    }
}

fn preview_ids(value: &serde_json::Value) -> Vec<&str> {
    value["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn verify_preview_public_cli_preserves_policy_range_and_execution_order_without_side_effects()
{
    let project = Project::new_with_sources(&[
        ("a.py", "a = 1 == 2; b = 3 == 4\n"),
        ("b.py", "a = 1 == 2; b = 3 == 4\n"),
        ("c.py", "a = 1 == 2\n"),
        ("d.py", "a = 1 + 2\n"),
    ]);
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let options = ["--jobs", "1", "--operators", "compare_eq_ne,binary_add_sub"];
    let (path, manifest) = write_plan_manifest_with_marker(&project, &options, &marker).await;
    assert_eq!(manifest.candidates.len(), 6);
    assert!(
        manifest.candidates[..5]
            .iter()
            .all(|candidate| candidate.score == manifest.candidates[0].score
                && candidate.score > manifest.candidates[5].score)
    );
    let before = std::fs::read(&path).unwrap();
    for (policy, offset, top, positions) in [
        ("strict", "0", "6", vec![0, 1, 2, 3, 4, 5]),
        ("diverse", "0", "6", vec![0, 2, 4, 1, 3, 5]),
        ("diverse", "2", "3", vec![4, 1, 3]),
        ("line-diverse", "0", "6", vec![0, 2, 4, 1, 3, 5]),
        ("line-diverse", "2", "3", vec![4, 1, 3]),
        ("strict", "4", "99", vec![4, 5]),
        ("diverse", "4", "99", vec![3, 5]),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let args = [
            "--top",
            top,
            "--offset",
            offset,
            "--selection-policy",
            policy,
            "--format",
            "jsonl",
        ];
        let mut dry_args = args.to_vec();
        dry_args.push("--dry-run");
        let output = preview_cli(&path, &dry_args, temporary.path()).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::str::from_utf8(&output.stdout).unwrap().lines().count(),
            1
        );
        assert!(output.stdout.ends_with(b"\n"));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["kind"], "verify_preview");
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["plan_schema_version"], 5);
        assert_eq!(value["ranking_rule_version"], 4);
        assert_eq!(value["offset"], offset.parse::<usize>().unwrap());
        assert_eq!(value["retained_candidates"], 6);
        assert_eq!(
            value["verification_selection"],
            serde_json::json!({
                "mode": "top", "policy": match policy { "strict" => "strict", "line-diverse" => "line_round_robin_v1", _ => "file_round_robin_v1" },
                "requested": top.parse::<usize>().unwrap(), "selected": positions.len(),
                "scope": "retained_candidates", "plan_truncated": false,
            })
        );
        let expected = positions
            .iter()
            .map(|&p| manifest.candidates[p].id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(preview_ids(&value), expected);
        for (index, &position) in positions.iter().enumerate() {
            let row = &value["candidates"][index];
            assert_eq!(row["selection_order"], index + 1);
            assert_eq!(row["rank"], manifest.candidates[position].rank);
            assert_eq!(row["path"], manifest.candidates[position].path.as_str());
            assert_eq!(row["line"], manifest.candidates[position].line);
            assert_eq!(
                row,
                &preview_candidate_value(&manifest.candidates[position], index + 1)
            );
        }
        assert!(!marker.exists());
        assert!(!project.path.join("session.sqlite3").exists());
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let run = preview_cli(&path, &args, temporary.path()).await;
        assert_eq!(
            run.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(preview_started_ids(&run.stdout), expected);
        assert!(
            marker.exists(),
            "normal verify must exercise the marker negative control"
        );
        std::fs::remove_file(&marker).unwrap();
    }
}

#[tokio::test]
async fn verify_preview_explicit_ids_follow_discovery_not_rank_argument_or_saved_sequence() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, mut manifest) =
        write_plan_manifest_with_marker(&project, &["--jobs", "1"], &marker).await;
    assert_eq!(manifest.candidates.len(), 2);
    assert_eq!(manifest.candidates[0].line, 5);
    assert_eq!(manifest.candidates[1].line, 2);
    manifest.candidates[0].candidate.sequence = 1;
    manifest.candidates[1].candidate.sequence = 2;
    write_json(&path, &serde_json::to_value(&manifest).unwrap());
    let args = [
        "--candidate",
        &manifest.candidates[0].id,
        "--candidate",
        &manifest.candidates[1].id,
        "--candidate",
        &manifest.candidates[0].id,
        "--format",
        "jsonl",
    ];
    let mut dry_args = args.to_vec();
    dry_args.push("--dry-run");
    let temporary = tempfile::tempdir().unwrap();
    let preview = preview_cli(&path, &dry_args, temporary.path()).await;
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let expected = vec![
        manifest.candidates[1].id.as_str(),
        manifest.candidates[0].id.as_str(),
    ];
    assert_eq!(preview_ids(&value), expected);
    for (index, position) in [1, 0].into_iter().enumerate() {
        assert_eq!(
            value["candidates"][index],
            preview_candidate_value(&manifest.candidates[position], index + 1)
        );
    }
    assert!(value["offset"].is_null());
    assert_eq!(
        value["verification_selection"],
        serde_json::json!({
            "mode": "candidate_ids", "policy": "explicit_candidates", "requested": 2, "selected": 2,
            "scope": "explicit_candidates", "plan_truncated": false,
        })
    );
    assert!(!marker.exists());
    assert!(!project.path.join("session.sqlite3").exists());
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
    let run = preview_cli(&path, &args, temporary.path()).await;
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(preview_started_ids(&run.stdout), expected);
}

#[tokio::test]
async fn verify_preview_truncated_formats_borrowed_dispatch_and_runtime_boundary() {
    let project = Project::new();
    let (path, mut manifest, marker) =
        write_plan_manifest(&project, &["--max-candidates", "1"]).await;
    assert!(manifest.truncated);
    let execution_tmp = tempfile::tempdir().unwrap();
    let preview = preview_cli(
        &path,
        &[
            "--top",
            "99",
            "--selection-policy",
            "line-diverse",
            "--dry-run",
        ],
        execution_tmp.path(),
    )
    .await;
    assert!(preview.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert!(!marker.exists());
    let run = preview_cli(
        &path,
        &[
            "--top",
            "99",
            "--selection-policy",
            "line-diverse",
            "--format",
            "jsonl",
        ],
        execution_tmp.path(),
    )
    .await;
    assert_eq!(
        run.status.code(),
        Some(2),
        "truncated execution remains incomplete"
    );
    assert_eq!(preview_started_ids(&run.stdout), preview_ids(&preview));
    assert!(marker.exists());
    std::fs::remove_file(&marker).unwrap();
    // Runtime disk checks must not run, while plan/config validation still does.
    manifest.normalized_config.limits.min_free_space = std::num::NonZeroU64::new(u64::MAX).unwrap();
    write_json(&path, &serde_json::to_value(&manifest).unwrap());
    let temporary = tempfile::tempdir().unwrap();
    for format in ["json", "jsonl", "human"] {
        let args = [
            "--top",
            "99",
            "--selection-policy",
            "line-diverse",
            "--dry-run",
            "--format",
            format,
        ];
        let output = preview_cli(&path, &args, temporary.path()).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = hoimin_cli::run_with_io(
            ["hoimin", "verify", path.to_str().unwrap()]
                .into_iter()
                .chain(args),
            &mut stdout,
            &mut stderr,
        )
        .await;
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, output.stdout);
        if format == "human" {
            let text = String::from_utf8(stdout).unwrap();
            assert!(text.contains(&format!(
                "{}:{}",
                manifest.candidates[0].path, manifest.candidates[0].line
            )));
            for required in [
                "verify preview",
                "requested=99",
                "selected=1",
                "offset=0",
                "plan_truncated=true",
                "retained_candidates=1",
                "rank=1",
                manifest.candidates[0].id.as_str(),
            ] {
                assert!(text.contains(required), "missing {required}: {text}");
            }
        } else {
            let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            assert_eq!(value["verification_selection"]["plan_truncated"], true);
            assert_eq!(value["verification_selection"]["requested"], 99);
            assert_eq!(value["verification_selection"]["selected"], 1);
            assert_eq!(value["retained_candidates"], 1);
        }
    }
    assert!(!marker.exists());
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn verify_preview_rejects_metrics_without_touching_existing_destination() {
    let project = Project::new();
    let (path, _, marker) = write_plan_manifest(&project, &[]).await;
    let destination = project.path.join("metrics.json");
    std::fs::write(&destination, "preserve existing metrics").unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let output = preview_cli(
        &path,
        &[
            "--top",
            "1",
            "--dry-run",
            "--metrics",
            destination.to_str().unwrap(),
        ],
        temporary.path(),
    )
    .await;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "preserve existing metrics"
    );
    assert!(!marker.exists());
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn verify_preview_invalid_inputs_match_normal_validation_before_runtime() {
    for invalid in [
        "empty",
        "empty_diverse",
        "malformed",
        "missing",
        "schema",
        "rank",
        "descriptor",
        "source",
        "fingerprint",
        "offset",
        "limit",
        "unknown",
    ] {
        let project = if invalid.starts_with("empty") {
            Project::new_with_source("pass\n")
        } else {
            Project::new()
        };
        let (path, manifest, marker) =
            write_plan_manifest(&project, &["--fingerprint-file", "config.toml"]).await;
        let mut value = serde_json::to_value(&manifest).unwrap();
        match invalid {
            "schema" => value["schema_version"] = 0.into(),
            "rank" => value["candidates"][0]["rank"] = 999.into(),
            "descriptor" => value["candidates"][0]["replacement"] = "invalid".into(),
            "source" => std::fs::write(project.path.join("src/calc.py"), "pass\n").unwrap(),
            "fingerprint" => {
                std::fs::write(project.path.join("config.toml"), "changed = true\n").unwrap();
            }
            "limit" => value["normalized_config"]["limits"]["max_mutants"] = 1.into(),
            _ => (),
        }
        write_json(&path, &value);
        if invalid == "malformed" {
            std::fs::write(&path, "{invalid JSON").unwrap();
        }
        if invalid == "missing" {
            std::fs::remove_file(&path).unwrap();
        }
        let mut args = if invalid == "unknown" {
            vec!["--candidate", "m1_missing"]
        } else {
            vec!["--top", "2"]
        };
        if invalid == "offset" {
            args.extend(["--offset", "2"]);
        }
        if invalid == "empty_diverse" {
            args.extend(["--selection-policy", "diverse"]);
        }
        let temporary = tempfile::tempdir().unwrap();
        let normal = preview_cli(&path, &args, temporary.path()).await;
        args.push("--dry-run");
        let preview = preview_cli(&path, &args, temporary.path()).await;
        assert_eq!(
            normal.status.code(),
            Some(2),
            "{invalid}: {}",
            String::from_utf8_lossy(&normal.stderr)
        );
        assert_eq!(preview.status.code(), Some(2), "{invalid}");
        assert_eq!(preview.stderr, normal.stderr, "{invalid}");
        assert!(preview.stdout.is_empty(), "{invalid}");
        assert!(!marker.exists(), "{invalid}");
        assert!(!project.path.join("session.sqlite3").exists());
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn verify_preview_output_failure_returns_error_without_execution() {
    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("preview destination failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let project = Project::new();
    let (path, _, marker) = write_plan_manifest(&project, &[]).await;
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(
        [
            "hoimin",
            "verify",
            path.to_str().unwrap(),
            "--top",
            "1",
            "--dry-run",
        ],
        &mut FailingWriter,
        &mut stderr,
    )
    .await;
    assert_eq!(code, 2);
    assert!(String::from_utf8_lossy(&stderr).contains("preview destination failed"));
    assert!(!marker.exists());
}

#[tokio::test]
async fn changed_context_discovers_neighbor_operators_and_survives_verify() {
    for (before, after, expected_line) in [
        (
            "def add(left, right):\n    return (\n        left +\n        right\n    )\n",
            "def add(left, right):\n    return (\n        left +\n        abs(right)\n    )\n",
            3,
        ),
        (
            "def add(left, right):\n    assert right >= 0\n    return left + right\n",
            "def add(left, right):\n    return left + right\n",
            2,
        ),
    ] {
        let sibling = "\ndef other(left, right):\n    return left + right\n";
        let project = Project::new_with_source(&format!("{before}{sibling}"));
        run_git(&project.path, &["init", "--quiet"]);
        run_git(&project.path, &["add", "."]);
        run_git(
            &project.path,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "base",
            ],
        );
        std::fs::write(
            project.path.join("src/calc.py"),
            format!("{after}{sibling}"),
        )
        .unwrap();
        for context in ["0", "1", "1073741823"] {
            let marker = project.path.join("test-command-ran");
            let args = plan_args(
                &project,
                [
                    "--changed",
                    "--changed-context",
                    context,
                    "--symbol",
                    "calc:add",
                    "--operators",
                    "binary_add_sub",
                    "--diff-base",
                    "HEAD",
                ],
                &marker,
            );
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            assert_eq!(
                hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await,
                0,
                "{}",
                String::from_utf8_lossy(&stderr)
            );
            let manifest: PlanManifest = serde_json::from_slice(&stdout).unwrap();
            assert_eq!(manifest.candidates.len(), usize::from(context != "0"));
            if context != "0" {
                let candidate = &manifest.candidates[0];
                assert_eq!(candidate.line, expected_line);
                let value = serde_json::to_value(candidate).unwrap();
                assert!(
                    value["ranking_reasons"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|reason| reason["code"] == "changed_line" && reason["score"] == 200)
                );
                let plan_path = project.path.join("saved-plan.json");
                std::fs::write(&plan_path, stdout).unwrap();
                let verified = prepare_verify(
                    &plan_path,
                    std::slice::from_ref(&candidate.id),
                    OutputFormat::Json,
                )
                .await
                .unwrap();
                assert_eq!(
                    verified.config.selection.changed_context,
                    context.parse::<u32>().unwrap()
                );
                assert_eq!(verified.config.selection.symbols.len(), 1);
            }
            assert!(!marker.exists());
        }
    }
}

async fn metrics_manifest_fixture(
    project: &Project,
    marker: &Path,
    baseline_fails: bool,
) -> (PathBuf, PlanManifest) {
    let mut args = plan_args(project, ["--jobs", "1"], marker);
    insert_test_min_free_space(&mut args);
    if baseline_fails {
        args.last_mut().unwrap().push("; raise SystemExit(1)");
    }
    let ParsedCommand::Plan(plan) = parse_from(args).unwrap() else {
        panic!("expected plan arguments");
    };
    let output = create(plan.into_run_config().unwrap()).await.unwrap();
    let path = project.path.join("plan.json");
    write_json(&path, &serde_json::to_value(&output.manifest).unwrap());
    (path, output.manifest)
}

#[tokio::test]
async fn verify_metrics_manifest_collisions_reject_before_baseline_in_public_cli() {
    for baseline_fails in [false, true] {
        let project = Project::new();
        let coordinator = tempfile::tempdir().unwrap();
        let marker = coordinator.path().join("marker");
        let (path, manifest) = metrics_manifest_fixture(&project, &marker, baseline_fails).await;
        let original = std::fs::read(&path).unwrap();
        std::fs::create_dir(project.path.join("child")).unwrap();
        let mut cases = vec![
            (path.clone(), path.clone()),
            (PathBuf::from("plan.json"), PathBuf::from("plan.json")),
            (PathBuf::from("./plan.json"), path.clone()),
            (path.clone(), PathBuf::from("./plan.json")),
            (PathBuf::from("child/../plan.json"), path.clone()),
            (path.clone(), PathBuf::from("child/../plan.json")),
        ];
        let case_alias = project.path.join("PLAN.JSON");
        if case_alias.exists() {
            cases.push((path.clone(), case_alias.clone()));
            cases.push((case_alias, path.clone()));
        }
        #[cfg(unix)]
        {
            let parent_alias = coordinator.path().join("project-alias");
            std::os::unix::fs::symlink(&project.path, &parent_alias).unwrap();
            cases.push((path.clone(), parent_alias.join("plan.json")));
            cases.push((parent_alias.join("plan.json"), path.clone()));
            let input_alias = project.path.join("input-alias.json");
            std::os::unix::fs::symlink(&path, &input_alias).unwrap();
            cases.push((input_alias.clone(), input_alias.clone()));
            cases.push((input_alias, path.clone()));
        }
        for selection in [
            ["--top", "1"],
            ["--candidate", manifest.candidates[0].candidate.id.as_str()],
        ] {
            for (input, destination) in &cases {
                let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
                    .current_dir(&project.path)
                    .arg("verify")
                    .arg(input)
                    .args(selection)
                    .arg("--metrics")
                    .arg(destination)
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(2),
                    "input={input:?}, destination={destination:?}, selection={selection:?}, failing={baseline_fails}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("metrics.destination.collision")
                );
                assert_eq!(std::fs::read(&path).unwrap(), original);
                assert!(
                    !marker.exists(),
                    "baseline executed before collision rejection"
                );
            }
        }
    }
}

#[tokio::test]
async fn verify_metrics_manifest_collision_is_also_rejected_with_borrowed_writers() {
    let project = Project::new();
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, manifest) = metrics_manifest_fixture(&project, &marker, false).await;
    let original = std::fs::read(&path).unwrap();
    for selection in [
        ["--top", "1"],
        ["--candidate", manifest.candidates[0].candidate.id.as_str()],
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = hoimin_cli::run_with_io(
            [
                "hoimin",
                "verify",
                path.to_str().unwrap(),
                selection[0],
                selection[1],
                "--metrics",
                path.to_str().unwrap(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;
        assert_eq!(exit, 2, "{}", String::from_utf8_lossy(&stderr));
        assert!(String::from_utf8_lossy(&stderr).contains("metrics.destination.collision"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!marker.exists());
    }
}

#[tokio::test]
async fn verify_metrics_manifest_distinct_entries_remain_replaceable() {
    for baseline_fails in [false, true] {
        let project = Project::new();
        let coordinator = tempfile::tempdir().unwrap();
        let marker = coordinator.path().join("marker");
        let (path, manifest) = metrics_manifest_fixture(&project, &marker, baseline_fails).await;
        let original = std::fs::read(&path).unwrap();
        for selection in [
            ["--top", "1"],
            ["--candidate", manifest.candidates[0].candidate.id.as_str()],
        ] {
            let outputs = tempfile::tempdir().unwrap();
            let new = outputs.path().join("new.json");
            let existing = outputs.path().join("existing.json");
            std::fs::write(&existing, "old metrics").unwrap();
            let hardlink = outputs.path().join("hardlink.json");
            std::fs::hard_link(&path, &hardlink).unwrap();
            let destinations = vec![new, existing, hardlink];
            #[cfg(unix)]
            let destinations = {
                let mut destinations = destinations;
                let symlink = outputs.path().join("symlink.json");
                std::os::unix::fs::symlink(&path, &symlink).unwrap();
                destinations.push(symlink);
                destinations
            };
            for destination in destinations {
                if marker.exists() {
                    std::fs::remove_file(&marker).unwrap();
                }
                let output = std::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
                    .current_dir(outputs.path())
                    .arg("verify")
                    .arg(&path)
                    .args(selection)
                    .arg("--metrics")
                    .arg(&destination)
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(if baseline_fails { 3 } else { 1 }),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(marker.exists());
                assert_eq!(std::fs::read(&path).unwrap(), original);
                assert!(
                    !std::fs::symlink_metadata(&destination)
                        .unwrap()
                        .file_type()
                        .is_symlink()
                );
                let metrics: hoimin_core::RunMetrics =
                    serde_json::from_slice(&std::fs::read(destination).unwrap()).unwrap();
                metrics.validate().unwrap();
                assert_eq!(metrics.executed, u64::from(!baseline_fails));
            }
        }
    }
}

fn preview_candidate_value(candidate: &RankedPlanCandidate, order: usize) -> serde_json::Value {
    serde_json::json!({
        "id": candidate.id,
        "rank": candidate.rank,
        "selection_order": order,
        "path": candidate.path,
        "line": candidate.line,
        "column": candidate.column,
        "operator": candidate.operator,
        "original": candidate.original,
        "replacement": candidate.replacement,
    })
}

fn assert_same_line_preview_output(
    text: &str,
    format: &str,
    expected: &[&RankedPlanCandidate],
    schema: &serde_json::Value,
) {
    if format == "human" {
        let rows: Vec<_> = text.lines().skip(1).collect();
        assert_eq!(rows.len(), expected.len());
        for (index, candidate) in expected.iter().enumerate() {
            assert_eq!(
                rows[index],
                format!(
                    "{}: {} rank={} {}:2:{} operator={} \"{}\" -> \"{}\"",
                    index + 1,
                    candidate.id,
                    candidate.rank,
                    candidate.path,
                    candidate.column,
                    candidate.operator,
                    candidate.original,
                    candidate.replacement
                )
            );
        }
    } else {
        assert_eq!(text.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["schema_version"], 2);
        assert_eq!(
            value["schema_version"],
            schema["properties"]["schema_version"]["const"]
        );
        assert_eq!(
            value["candidates"].as_array().unwrap().len(),
            expected.len()
        );
        for (index, candidate) in expected.iter().enumerate() {
            let row = &value["candidates"][index];
            assert_eq!(row, &preview_candidate_value(candidate, index + 1));
            let row_schema = &schema["properties"]["candidates"]["items"];
            let fields: BTreeSet<_> = row
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                fields,
                row_schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect()
            );
            assert_eq!(
                fields,
                row_schema["properties"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect()
            );
        }
    }
}

#[tokio::test]
async fn verify_preview_details_distinguish_same_line_mutations_in_all_formats() {
    let project =
        Project::new_with_source("def acceptable(value):\n    return value > 0 and value < 10\n");
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, manifest) = write_plan_manifest_with_marker(
        &project,
        &["--jobs", "1", "--operators", "compare_order,boolean_and_or"],
        &marker,
    )
    .await;
    let before = std::fs::read(&path).unwrap();
    assert_eq!(manifest.candidates.len(), 3);
    let at_column = |column| {
        manifest
            .candidates
            .iter()
            .find(|c| c.column == column)
            .unwrap()
    };
    let left = at_column(17);
    let boolean = at_column(21);
    let right = at_column(31);
    for (candidate, operator, original, replacement) in [
        (left, "compare_order", ">", ">="),
        (boolean, "boolean_and_or", "and", "or"),
        (right, "compare_order", "<", "<="),
    ] {
        assert_eq!(candidate.line, 2);
        assert_eq!(candidate.operator, operator);
        assert_eq!(candidate.original, original);
        assert_eq!(candidate.replacement, replacement);
    }
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/json-schema/verify-preview.schema.json"
    ))
    .unwrap();
    for (selection, expected) in [
        (
            vec!["--top", "3"],
            manifest.candidates.iter().collect::<Vec<_>>(),
        ),
        (
            vec![
                "--top",
                "2",
                "--offset",
                "1",
                "--selection-policy",
                "diverse",
            ],
            manifest.candidates[1..].iter().collect(),
        ),
        (
            vec![
                "--candidate",
                right.id.as_str(),
                "--candidate",
                boolean.id.as_str(),
                "--candidate",
                left.id.as_str(),
                "--candidate",
                left.id.as_str(),
            ],
            vec![left, boolean, right],
        ),
    ] {
        for format in ["json", "jsonl", "human"] {
            let temporary = tempfile::tempdir().unwrap();
            let mut args = selection.clone();
            args.extend(["--dry-run", "--format", format]);
            let output = preview_cli(&path, &args, temporary.path()).await;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert_same_line_preview_output(&text, format, &expected, &schema);
            assert!(!marker.exists());
            assert!(!project.path.join("session.sqlite3").exists());
            assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
}

#[tokio::test]
async fn verify_preview_details_escape_multiline_quoted_text_without_extra_rows() {
    let project = Project::new_with_source("[\n\t\"a\\\\b\", 'say \"hi\"',\n]\n");
    let coordinator = tempfile::tempdir().unwrap();
    let marker = coordinator.path().join("marker");
    let (path, manifest) = write_plan_manifest_with_marker(
        &project,
        &["--operators", "collection_list_tuple", "--jobs", "1"],
        &marker,
    )
    .await;
    assert_eq!(manifest.candidates.len(), 1);
    let candidate = &manifest.candidates[0];
    assert_eq!(candidate.column, 0);
    assert_eq!(candidate.original, "[\n\t\"a\\\\b\", 'say \"hi\"',\n]");
    assert_eq!(candidate.replacement, "(\n\t\"a\\\\b\", 'say \"hi\"',\n)");
    for format in ["human", "json", "jsonl"] {
        let temporary = tempfile::tempdir().unwrap();
        let output = preview_cli(
            &path,
            &["--top", "1", "--dry-run", "--format", format],
            temporary.path(),
        )
        .await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        if format == "human" {
            assert_eq!(text.lines().count(), 2);
            assert!(!text.contains('\t'));
            assert!(text.lines().nth(1).unwrap().ends_with(
                r#"operator=collection_list_tuple "[\n\t\"a\\\\b\", 'say \"hi\"',\n]" -> "(\n\t\"a\\\\b\", 'say \"hi\"',\n)""#
            ), "{text}");
        } else {
            assert_eq!(text.lines().count(), 1);
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(
                value["candidates"][0],
                preview_candidate_value(candidate, 1)
            );
        }
        assert!(!marker.exists());
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}
