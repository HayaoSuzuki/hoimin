use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use hoimin_cli::{
    analyzer::discover_targets,
    cli::{OutputFormat, ParsedCommand, parse_from},
    plan::{PlanManifest, create, prepare_verify},
    shell,
    target::TargetHandler,
};
use hoimin_core::{MutationCandidate, OutputFormat as CoreOutputFormat};

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
    assert_eq!(manifest["schema_version"], 1);
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
        let expected = discover_for_plan_args(args.clone()).await;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

        assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
        let manifest: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        let actual: Vec<MutationCandidate> =
            serde_json::from_value(manifest["candidates"].clone()).unwrap();
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
            "schema" => value["schema_version"] = serde_json::json!(2),
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
    assert_error_code(error, "plan.candidate.invalid");

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
    assert_eq!(verified.candidate_ids.len(), 1);
    assert!(verified.candidate_ids.contains(&first));
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

    assert_eq!(verified.candidate_ids.len(), 1);
    assert_eq!(verified.config.output.format, CoreOutputFormat::Jsonl);
    assert!(!marker.exists());
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
async fn verify_executes_retained_candidate_from_truncated_plan() {
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
        OsString::from("--candidate"),
        OsString::from(&candidate_id),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(code, 1, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(
        marker.exists(),
        "verify must execute the baseline and selected mutant"
    );
    let document: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert!(!document["baseline"].is_null());
    let mutants = document["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0]["candidate"]["id"], candidate_id);
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
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_owned();
        std::fs::create_dir(path.join("src")).unwrap();
        std::fs::write(path.join("src/calc.py"), source).unwrap();
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
