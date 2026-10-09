use hoimin_cli::cli::{ParsedCommand, parse_from};

#[test]
fn sample_cli_accepts_explicit_count_and_full_width_seed() {
    for seed in ["0", "42", "18446744073709551615"] {
        let parsed = parse_from([
            "hoimin",
            "verify",
            "plan.json",
            "--sample",
            "3",
            "--seed",
            seed,
        ]);
        assert!(matches!(parsed, Ok(ParsedCommand::Verify(_))), "{parsed:?}");
    }
}

#[test]
fn sample_cli_rejects_missing_or_conflicting_selection_parameters() {
    for options in [
        vec!["--sample", "3"],
        vec!["--seed", "42"],
        vec!["--sample", "0", "--seed", "42"],
        vec!["--sample", "3", "--seed", "-1"],
        vec!["--sample", "3", "--seed", "18446744073709551616"],
        vec!["--sample", "3", "--seed", "42", "--top", "1"],
        vec!["--sample", "3", "--seed", "42", "--candidate", "id"],
        vec!["--sample", "3", "--seed", "42", "--offset", "0"],
        vec![
            "--sample",
            "3",
            "--seed",
            "42",
            "--selection-policy",
            "strict",
        ],
        vec!["--top", "3", "--seed", "42"],
    ] {
        let mut args = vec!["hoimin", "verify", "plan.json"];
        args.extend(options);
        assert!(parse_from(args.clone()).is_err(), "{args:?}");
    }
}

use std::ffi::OsString;
use std::path::{Path, PathBuf};

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut out, &mut err).await;
    (
        code,
        serde_json::from_slice(&out).unwrap_or(serde_json::Value::Null),
        String::from_utf8(err).unwrap(),
    )
}

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.venv")
                .join(if cfg!(windows) {
                    "Scripts/python.exe"
                } else {
                    "bin/python"
                })
        },
        PathBuf::from,
    )
}

async fn fixture() -> (tempfile::TempDir, PathBuf, serde_json::Value) {
    fixture_with_budget(20).await
}

async fn fixture_with_budget(
    max_mutants: usize,
) -> (tempfile::TempDir, PathBuf, serde_json::Value) {
    fixture_population(max_mutants, 8).await
}

async fn fixture_population(
    max_mutants: usize,
    population: usize,
) -> (tempfile::TempDir, PathBuf, serde_json::Value) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("a.py"),
        match population {
            0 => "# empty\n",
            1 => "a = True\n",
            _ => "a = True; b = False; c = True\nd = False\nx = 1 + 2; y = 3 + 4\n",
        },
    )
    .unwrap();
    std::fs::write(
        root.join("b.py"),
        if population == 8 {
            "a = True\nx = 1 + 2\n"
        } else {
            "# empty\n"
        },
    )
    .unwrap();
    let (code, document, err) = cli(vec![
        "hoimin".into(),
        "plan".into(),
        "--root".into(),
        root.into_os_string(),
        "--source".into(),
        ".".into(),
        "--operators".into(),
        "boolean_literal,binary_add_sub".into(),
        "--max-mutants".into(),
        max_mutants.to_string().into(),
        "--jobs".into(),
        "1".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        "import a, b".into(),
    ])
    .await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(document["candidates"].as_array().unwrap().len(), population);
    let path = directory.path().join("plan.json");
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    (directory, path, document)
}

fn verify_args(path: &Path, count: usize, seed: u64) -> Vec<OsString> {
    vec![
        "hoimin".into(),
        "verify".into(),
        path.as_os_str().to_owned(),
        "--sample".into(),
        count.to_string().into(),
        "--seed".into(),
        seed.to_string().into(),
    ]
}

async fn sample_preview(path: &Path, count: usize, seed: u64) -> serde_json::Value {
    let mut args = verify_args(path, count, seed);
    args.push("--dry-run".into());
    let (code, output, err) = cli(args).await;
    assert_eq!(code, 0, "{err}");
    output
}

#[tokio::test]
async fn sample_execution_matches_preview_and_preserves_sample_scope() {
    let (_dir, path, _plan) = fixture().await;
    let preview = sample_preview(&path, 3, 42).await;
    assert_eq!(preview, sample_preview(&path, 3, 42).await);
    let selection = &preview["verification_selection"];
    assert_eq!(selection["mode"], "sample");
    assert_eq!(selection["scope"], "sampled_candidates");
    assert_eq!(selection["policy"], "splitmix64_fisher_yates_v1");
    assert_eq!(selection["sampling"]["population"], 8);
    assert_eq!(selection["sampling"]["seed"], 42);
    assert_eq!(selection["selected"], 3);
    assert_eq!(selection["requested"], 3);
    assert!(preview["offset"].is_null());
    let ids: Vec<_> = preview["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect();
    assert_eq!(
        selection["sampling"]["selected_ids"],
        serde_json::json!(ids)
    );
    let (code, report, err) = cli(verify_args(&path, 3, 42)).await;
    assert_eq!(code, 1, "{err}");
    assert_eq!(&report["run"]["verification_selection"], selection);
    assert_eq!(&report["summary"]["verification_selection"], selection);
    assert_eq!(report["summary"]["complete"], true);
    assert_eq!(report["summary"]["counts"]["survived"], 3);
    assert_eq!(report["summary"]["counts"]["killed"], 0);
    let executed: Vec<_> = report["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["candidate"]["id"].clone())
        .collect();
    assert_eq!(executed, ids);
    let full = sample_preview(&path, usize::MAX, 42).await;
    assert_eq!(full["verification_selection"]["selected"], 8);
    assert_eq!(full["verification_selection"]["requested"], usize::MAX);
    for (first, second) in preview["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .zip(full["candidates"].as_array().unwrap())
    {
        assert_eq!(first, second);
    }
    let report_path = path.with_file_name("report.json");
    std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let (code, progress, err) = cli(vec![
        "hoimin".into(),
        "progress".into(),
        report_path.clone().into_os_string(),
        report_path.into_os_string(),
        "--format".into(),
        "json".into(),
    ])
    .await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(progress["comparisons"][0]["common"], 3);
}

#[tokio::test]
async fn sample_rejects_truncation_empty_budget_and_tampering_before_tests() {
    let (dir, path, plan) = fixture_with_budget(2).await;
    for (change, count, message) in [
        ("budget", 3, "max_mutants"),
        ("truncated", 1, "truncated"),
        ("empty", 1, "empty"),
        ("duplicate", 1, "duplicate"),
        ("rank", 1, "rank"),
    ] {
        let mut modified = plan.clone();
        match change {
            "truncated" => modified["truncated"] = true.into(),
            "empty" => modified["candidates"] = serde_json::json!([]),
            "duplicate" => {
                modified["candidates"][1]["id"] = modified["candidates"][0]["id"].clone();
            }
            "rank" => modified["candidates"][0]["rank"] = 999.into(),
            _ => (),
        }
        std::fs::write(&path, serde_json::to_vec(&modified).unwrap()).unwrap();
        let (code, output, err) = cli(verify_args(&path, count, 0)).await;
        assert_eq!(code, 2, "{change}: {err}");
        assert!(output.is_null());
        assert!(err.contains(message), "{change}: {err}");
    }
    std::fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    std::fs::write(dir.path().join("project/a.py"), "changed = 1\n").unwrap();
    let (code, _, err) = cli(verify_args(&path, 1, 0)).await;
    assert_eq!(code, 2);
    assert!(err.contains("source.changed"), "{err}");
}

#[derive(serde::Deserialize)]
struct OracleCase {
    population: usize,
    count: usize,
    budget: usize,
    truncated: bool,
    seed: u64,
    accepted: bool,
    indices: Vec<usize>,
    mode: String,
}

#[tokio::test]
async fn sample_public_cli_matches_every_lean_oracle_case() {
    let cases: Vec<OracleCase> = include_str!("../../../formal/HoiminOracle/corpus/sampling.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 360);
    for population in [0, 1, 8] {
        let (_dir, path, plan) = fixture_population(8, population).await;
        for case in cases.iter().filter(|case| case.population == population) {
            assert_eq!(case.mode, "strict");
            let mut modified = plan.clone();
            modified["normalized_config"]["limits"]["max_mutants"] = case.budget.into();
            modified["truncated"] = case.truncated.into();
            std::fs::write(&path, serde_json::to_vec(&modified).unwrap()).unwrap();
            let mut args = verify_args(&path, case.count, case.seed);
            args.push("--dry-run".into());
            let (code, preview, err) = cli(args).await;
            assert_eq!(
                code,
                if case.accepted { 0 } else { 2 },
                "p={population} k={} b={} t={} seed={}: {err}",
                case.count,
                case.budget,
                case.truncated,
                case.seed
            );
            if case.accepted {
                let ids: Vec<_> = case
                    .indices
                    .iter()
                    .map(|&i| plan["candidates"][i]["id"].clone())
                    .collect();
                assert_eq!(
                    preview["verification_selection"]["sampling"]["selected_ids"],
                    serde_json::json!(ids)
                );
                let actual: Vec<_> = preview["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c["id"].clone())
                    .collect();
                assert_eq!(actual, ids);
            } else {
                assert!(preview.is_null());
            }
        }
    }
}

fn set_test_program(plan: &mut serde_json::Value, script: &str) {
    let arg = if cfg!(windows) {
        hoimin_core::CommandArg::Windows(script.encode_utf16().collect())
    } else {
        hoimin_core::CommandArg::Unix(script.as_bytes().to_vec())
    };
    *plan["normalized_config"]["test_argv"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap() = serde_json::to_value(arg).unwrap();
}

#[tokio::test]
async fn sample_dry_run_never_executes_and_failed_baseline_retains_all_selected_ids() {
    let (dir, path, mut plan) = fixture().await;
    let marker = dir.path().join("baseline-ran");
    let script = format!(
        "from pathlib import Path; Path({:?}).write_text('ran'); raise SystemExit(1)",
        marker.to_str().unwrap()
    );
    set_test_program(&mut plan, &script);
    std::fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let preview = sample_preview(&path, 3, 42).await;
    assert!(!marker.exists());
    let (code, report, err) = cli(verify_args(&path, 3, 42)).await;
    assert_ne!(code, 0, "{err}");
    assert!(marker.exists());
    assert_eq!(report["summary"]["complete"], false);
    assert_eq!(report["summary"]["counts"]["killed"], 0);
    assert_eq!(
        report["summary"]["verification_selection"],
        preview["verification_selection"]
    );
}

#[tokio::test]
async fn sample_timeout_is_incomplete_and_never_counts_unsampled_mutants_as_killed() {
    let (_dir, path, mut plan) = fixture_population(8, 1).await;
    set_test_program(
        &mut plan,
        "import a, time; time.sleep(2) if not a.a else None",
    );
    plan["normalized_config"]["limits"]["mutant_timeout"] =
        serde_json::json!({"fixed":{"secs":0,"nanos":100_000_000}});
    std::fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let (code, report, err) = cli(verify_args(&path, 1, 0)).await;
    assert_ne!(code, 0, "{err}");
    assert_eq!(report["summary"]["complete"], false, "{err}");
    assert_eq!(report["summary"]["counts"]["timeout"], 1);
    assert_eq!(report["summary"]["counts"]["killed"], 0);
    assert!(report["summary"]["counts"]["score"].is_null());
}

#[tokio::test]
async fn different_samples_keep_progress_candidate_set_checks() {
    let (_dir, path, _plan) = fixture().await;
    let mut paths = Vec::new();
    for seed in [1, 42] {
        let (code, report, err) = cli(verify_args(&path, 3, seed)).await;
        assert_eq!(code, 1, "{err}");
        let report_path = path.with_file_name(format!("report-{seed}.json"));
        std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        paths.push(report_path.into_os_string());
    }
    let mut args = vec![
        "hoimin".into(),
        "progress".into(),
        "--format".into(),
        "json".into(),
    ];
    args.extend(paths);
    let (code, progress, err) = cli(args).await;
    assert_eq!(code, 0, "{err}");
    let comparison = &progress["comparisons"][0];
    assert!(
        comparison["added"].as_u64().unwrap()
            + comparison["removed"].as_u64().unwrap()
            + comparison["ambiguous"].as_u64().unwrap()
            > 0
    );
    assert_eq!(comparison["state"], "indeterminate");
}

#[tokio::test]
#[ignore = "writes reproducible authored-project measurements; run explicitly with --ignored"]
async fn sample_authored_project_evaluation() {
    let mut projects = Vec::new();
    for name in ["constants", "functions"] {
        let (dir, path, mut document) = fixture().await;
        let script = if name == "constants" {
            "import a,b; assert a.a is True; assert a.b is False; assert a.x == 3; assert b.x == 3"
        } else {
            std::fs::write(
                dir.path().join("project/a.py"),
                "def add(x,y):\n    return x + y\ndef unused(x):\n    return x + 1\nflag = True\n",
            )
            .unwrap();
            std::fs::write(dir.path().join("project/b.py"), "def adjust(x):\n    return x + 1\ndef ignored(x):\n    return x + 2\nflag = True\n").unwrap();
            "import a,b; assert a.add(3,1) == 4; assert b.adjust(2) == 3; assert a.flag; assert b.flag"
        };
        set_test_program(&mut document, script);
        let manifest: hoimin_cli::plan::PlanManifest = serde_json::from_value(document).unwrap();
        let config = manifest
            .normalized_config
            .into_run_config(hoimin_core::OutputConfig {
                format: hoimin_core::OutputFormat::Json,
                metrics: None,
            });
        let plan = hoimin_cli::plan::create(config).await.unwrap().manifest;
        std::fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        let mut sources = serde_json::Map::new();
        for file in ["a.py", "b.py"] {
            sources.insert(
                file.to_owned(),
                std::fs::read_to_string(dir.path().join("project").join(file))
                    .unwrap()
                    .into(),
            );
        }
        let started = std::time::Instant::now();
        let (code, reference, err) = cli(vec![
            "hoimin".into(),
            "verify".into(),
            path.clone().into_os_string(),
            "--top".into(),
            "20".into(),
        ])
        .await;
        let reference_seconds = started.elapsed().as_secs_f64();
        assert!(matches!(code, 0 | 1), "{err}");
        assert_eq!(reference["summary"]["complete"], true);
        let reference_score = reference["summary"]["counts"]["score"].as_f64().unwrap();
        let population_results: Vec<_> = reference["mutants"].as_array().unwrap().iter().map(|m| serde_json::json!({"id":m["candidate"]["id"], "operator":m["candidate"]["operator"], "status":m["status"]})).collect();
        let mut trials = Vec::new();
        for count in [2, 4] {
            for seed in [0, 1, 42, u64::MAX] {
                let started = std::time::Instant::now();
                let (code, report, err) = cli(verify_args(&path, count, seed)).await;
                let seconds = started.elapsed().as_secs_f64();
                assert!(matches!(code, 0 | 1), "{err}");
                assert_eq!(report["summary"]["complete"], true);
                let score = report["summary"]["counts"]["score"].as_f64().unwrap();
                let mutants: Vec<_> = report["mutants"].as_array().unwrap().iter().map(|m| serde_json::json!({"id":m["candidate"]["id"], "operator":m["candidate"]["operator"], "status":m["status"]})).collect();
                for mutant in &mutants {
                    assert!(population_results.contains(mutant));
                }
                trials.push(serde_json::json!({"selection":report["run"]["verification_selection"], "counts":report["summary"]["counts"], "score_error":score-reference_score, "elapsed_seconds":seconds, "mutants":mutants}));
            }
        }
        projects.push(serde_json::json!({"name":name,"sources":sources,"source_records":plan.sources,"test_program":script,"operators":plan.normalized_config.operators,"population":plan.candidates.len(),"reference_counts":reference["summary"]["counts"],"reference_elapsed_seconds":reference_seconds,"population_results":population_results,"trials":trials}));
    }
    let report = serde_json::json!({"scope":"two authored Python fixtures; no representative-project or real-defect claim", "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"projects":projects});
    let destination = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/superpowers/reports/issue-695/observations.json");
    std::fs::write(destination, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
