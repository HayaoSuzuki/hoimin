use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use hoimin_cli::{
    analyzer::discover_targets,
    cli::{OutputFormat, ParsedCommand, TopSelectionPolicy, VerifySelection, parse_from},
    plan::{
        PlanManifest, ResolvedVerifySelection, VerifySelectionScope, create, prepare_verify,
        prepare_verify_selection,
    },
    shell,
    target::TargetHandler,
};
use hoimin_core::{
    MAX_JOBS, MutationCandidate, OutputFormat as CoreOutputFormat, VerificationSelectionPolicy,
};

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
    assert_eq!(manifest["schema_version"], 2);
    assert_eq!(manifest["ranking_rule_version"], 1);
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
    assert!(manifest["normalized_config"].get("session").is_none());
    assert!(manifest["normalized_config"].get("resume").is_none());
    assert!(!workspace_marker.exists());
    assert!(!session_path.exists());
    assert!(stderr.is_empty());
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
    assert!(stdout.is_empty());
    assert!(!stderr.is_empty());
    assert!(!marker.exists());
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
            "invalid zero or overflowing limit: total_timeout",
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
            "invalid zero or overflowing limit: baseline_timeout",
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
            "invalid zero or overflowing limit: max_processes",
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
    assert!(tampered.normalized_config.test_argv.is_empty());
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

    for case in ["schema", "kind", "parent_path", "absolute_path"] {
        let mut value = original.clone();
        match case {
            "schema" => value["schema_version"] = serde_json::json!(1),
            "kind" => value["kind"] = serde_json::json!("report"),
            "parent_path" => value["sources"][0]["path"] = serde_json::json!("../outside.py"),
            "absolute_path" => value["sources"][0]["path"] = serde_json::json!("/outside.py"),
            _ => unreachable!(),
        }
        write_json(&path, &value);

        let error = prepare_verify(&path, &requested, OutputFormat::Json)
            .await
            .unwrap_err();
        assert_error_code(error, "plan.manifest.invalid");
    }
    assert!(!marker.exists());
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
            policy: hoimin_cli::cli::TopSelectionPolicy::Strict,
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
            policy: hoimin_cli::cli::TopSelectionPolicy::Strict,
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
    assert!(!lines.is_empty());
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
    let args = plan_args(project, options.iter().copied(), marker);
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
