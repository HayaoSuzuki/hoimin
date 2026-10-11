use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::plan::PlanManifest;
use hoimin_cli::process::{ProcessHandler, ProcessRequest};
use hoimin_cli::resource::ResourceBackend;
use hoimin_cli::workspace::{
    CopyOptions, WorkerWorkspace, WorkspacePlan, build_command_environment,
};
use hoimin_core::{
    BudgetLedger, CandidateDescriptor, CommandArg, EffectId, MutationCandidate, ProcessFinished,
    ProcessLimits, ProcessTermination, RunBudgets, RunProcess, reserve_workspace_copy,
    validate_candidate,
};
use proptest::strategy::{Strategy, ValueTree};
use serde_json::{Value, json};

use super::generator::{self, SYMBOL, TARGET};

const COPY_LIMIT: u64 = 8 * 1024 * 1024 * 1024;
const FREE_RESERVE: u64 = 10 * 1024 * 1024 * 1024;
const MAX_OUTPUT: u64 = 1024 * 1024;

fn worker(root: &Utf8Path) -> Result<(WorkspacePlan, WorkerWorkspace), String> {
    let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default())
        .map_err(|e| e.to_string())?;
    if plan.aggregate_bytes() > COPY_LIMIT {
        return Err("sample exceeds 8GiB copy allowance".into());
    }
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1024 * 1024 * 1024,
        copy: COPY_LIMIT,
        processes: 16,
    });
    let grant =
        reserve_workspace_copy(&mut ledger, &plan.completed()).map_err(|e| e.to_string())?;
    let copy = plan
        .create_worker(
            &grant
                .create_worker(EffectId(2), 0)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    Ok((plan, copy))
}

fn mutation(source: &[u8], candidate: &CandidateDescriptor) -> Result<MutationCandidate, String> {
    Ok(MutationCandidate {
        id: validate_candidate(source, candidate)
            .map_err(|e| e.to_string())?
            .to_string(),
        sequence: 0,
        path: candidate.path.clone(),
        span: candidate.span,
        original: candidate.original.clone(),
        replacement: candidate.replacement.clone(),
        operator: candidate.operator.clone(),
        line: candidate.line,
        column: candidate.column,
        symbol: candidate.symbol.clone(),
        file_hash: candidate.file_hash.clone(),
    })
}

pub fn exercise_fixture(source: &str) {
    let root = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(root.path()).unwrap();
    fs::create_dir_all(root.join("src/proofreadtools/checks")).unwrap();
    fs::write(root.join(TARGET), source).unwrap();
    let (plan, mut copy) = worker(root).unwrap();
    let candidate = generator::generate_candidates(source, 790, 1)
        .unwrap()
        .remove(0);
    copy.apply_mutation(&mutation(source.as_bytes(), &candidate).unwrap())
        .unwrap();
    assert_ne!(copy.read(TARGET).unwrap(), source.as_bytes());
    copy.reset().unwrap();
    assert_eq!(copy.read(TARGET).unwrap(), source.as_bytes());
    copy.try_cleanup().unwrap();
    plan.verify_originals().unwrap();
}

pub async fn exercise_timeout(source: &str) {
    let root = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(root.path()).unwrap();
    fs::create_dir_all(root.join("src/proofreadtools/checks")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(root.join(TARGET), source).unwrap();
    fs::write(
        root.join("tests/test_variant_candidates.py"),
        "def test_wait():\n    import time\n    time.sleep(10)\n",
    )
    .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let scratch = Utf8Path::from_path(scratch.path()).unwrap().to_owned();
    let mut runtime = Runtime {
        handler: ProcessHandler::new(resource_backend().unwrap(), scratch.clone()),
        python: "python".into(),
        script: "unused.py".into(),
        scratch,
        processes: 0,
        plans: Vec::new(),
    };
    let (plan, mut copy) = worker(root).unwrap();
    let candidate = generator::generate_candidates(source, 790, 1)
        .unwrap()
        .remove(0);
    let result = evaluate(
        &mut runtime,
        &mut copy,
        source.as_bytes(),
        &candidate,
        790,
        0.001,
    )
    .await;
    copy.try_cleanup().unwrap();
    plan.verify_originals().unwrap();
    let drain = runtime
        .handler
        .drain_for_shutdown(Duration::from_secs(5))
        .await;
    runtime.handler.close().unwrap();
    assert!(drain.all_reaped && drain.output_drains_joined && drain.secondary_errors.is_empty());
    assert!(
        result.is_err(),
        "an incomplete process must abort the entire experiment"
    );
    assert_eq!(
        runtime.processes, 1,
        "no witness search may follow an incomplete test"
    );
}

fn resource_backend() -> Result<ResourceBackend, String> {
    #[cfg(windows)]
    {
        hoimin_cli::resource::WindowsBackend::new()
            .map(ResourceBackend::Windows)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        hoimin_cli::resource::PortableBackend::new(true)
            .map(ResourceBackend::Portable)
            .map_err(|e| e.to_string())
    }
}

fn arg(value: &OsStr) -> CommandArg {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        CommandArg::Windows(value.encode_wide().collect())
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CommandArg::Unix(value.as_bytes().to_vec())
    }
}

fn complete(result: &ProcessFinished) -> bool {
    result.output_state.is_complete() && result.output.observed == result.output.retained
}

fn eligible(row: &Value) -> bool {
    row["status"] == "survived_related_suite" && row["witness"]["status"] == "witness"
}

struct Runtime {
    handler: ProcessHandler,
    python: Utf8PathBuf,
    script: Utf8PathBuf,
    scratch: Utf8PathBuf,
    processes: u64,
    plans: Vec<Value>,
}

impl Runtime {
    async fn validate_sample(&mut self, root: &Utf8Path) -> Result<(), String> {
        for (args, expected) in [
            (vec!["rev-parse", "HEAD"], generator::SAMPLE_SHA),
            (vec!["status", "--porcelain", "--untracked-files=no"], ""),
        ] {
            let mut argv = vec![OsString::from("git"), "-C".into(), root.into()];
            argv.extend(args.into_iter().map(OsString::from));
            let (finished, output) = self.command(root, argv, 10.0).await?;
            if finished.termination != ProcessTermination::Exit(0) || output.trim() != expected {
                return Err("sample must be the clean pinned checkout".into());
            }
        }
        Ok(())
    }
    async fn command(
        &mut self,
        cwd: &Utf8Path,
        argv: Vec<OsString>,
        seconds: f64,
    ) -> Result<(ProcessFinished, String), String> {
        self.processes += 1;
        let process = RunProcess {
            id: EffectId(self.processes),
            worker: Some(0),
            run_id: None,
            mutant_id: None,
            argv: argv.iter().map(|v| arg(v)).collect(),
            cwd: cwd.to_owned(),
            limits: ProcessLimits {
                timeout: Duration::from_secs_f64(seconds.max(0.001)),
                max_output_bytes: MAX_OUTPUT,
                max_memory_bytes: 1024 * 1024 * 1024,
                max_processes: 16,
            },
        };
        // Rewrite import roots for every child; the editable original install
        // must never take precedence over the mutated worker.
        let mut inherited = std::env::vars_os().collect::<BTreeMap<_, _>>();
        inherited.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
        inherited.insert("PYTHONHASHSEED".into(), "0".into());
        inherited.insert("PYTHONUTF8".into(), "1".into());
        inherited.insert("PYTHONIOENCODING".into(), "utf-8".into());
        // NumPy/OpenBLAS workers otherwise reserve many thread stacks inside
        // the shared 1GiB job. Keep the same memory cap; bound native threads.
        for name in ["OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "MKL_NUM_THREADS"] {
            inherited.insert(name.into(), "1".into());
        }
        let environment = build_command_environment(cwd, cwd, &["src".into()], &inherited)
            .map_err(|e| e.to_string())?;
        let result = self
            .handler
            .run(ProcessRequest::from(process).with_environment(environment))
            .await
            .map_err(|e| format!("{e:?}"))?;
        let path = self
            .handler
            .spool_path(&result.output)
            .map_err(|e| e.to_string())?;
        let output = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        fs::remove_file(path).map_err(|e| e.to_string())?;
        if !complete(&result) {
            return Err("incomplete or truncated process output; stop experiment".into());
        }
        Ok((result, output))
    }

    async fn pytest(
        &mut self,
        copy: &WorkerWorkspace,
        test: &str,
        seconds: f64,
    ) -> Result<(ProcessFinished, String), String> {
        self.command(
            copy.root(),
            vec![
                self.python.clone().into(),
                "-m".into(),
                "pytest".into(),
                "-q".into(),
                "-p".into(),
                "no:cacheprovider".into(),
                test.into(),
            ],
            seconds,
        )
        .await
    }

    async fn search(
        &mut self,
        root: &Utf8Path,
        original: &Utf8Path,
        mutant: &Utf8Path,
        seed: u64,
        seconds: f64,
    ) -> Result<Value, String> {
        let (result, output) = self
            .command(
                root,
                vec![
                    self.python.clone().into(),
                    self.script.clone().into(),
                    "search".into(),
                    "--original".into(),
                    original.into(),
                    "--mutant".into(),
                    mutant.into(),
                    "--seed".into(),
                    seed.to_string().into(),
                    "--seconds".into(),
                    (seconds * 0.7).to_string().into(),
                ],
                seconds,
            )
            .await?;
        if result.termination == ProcessTermination::Timeout {
            return Ok(json!({"status":"no_witness_within_budget"}));
        }
        if result.termination != ProcessTermination::Exit(0) {
            return Err(format!("input search failed: {output}"));
        }
        serde_json::from_str(&output).map_err(|e| e.to_string())
    }

    async fn observe(&mut self, copy: &WorkerWorkspace, x: &str, y: &str) -> Result<Value, String> {
        let (finished, log) = self
            .command(
                copy.root(),
                vec![
                    self.python.clone().into(),
                    self.script.clone().into(),
                    "observe".into(),
                    "--x".into(),
                    x.into(),
                    "--y".into(),
                    y.into(),
                ],
                10.0,
            )
            .await?;
        if finished.termination != ProcessTermination::Exit(0) {
            return Err(format!("real observation failed: {log}"));
        }
        let value: Value = serde_json::from_str(&log).map_err(|e| e.to_string())?;
        let path = Utf8Path::new(value["path"].as_str().ok_or("missing module path")?)
            .canonicalize_utf8()
            .map_err(|e| e.to_string())?;
        let expected = copy
            .root()
            .join(TARGET)
            .canonicalize_utf8()
            .map_err(|e| e.to_string())?;
        if path != expected {
            return Err("observation imported another checkout".into());
        }
        Ok(value)
    }
}

async fn compare_crosshair(
    runtime: &mut Runtime,
    copy: &WorkerWorkspace,
    original: &Utf8Path,
    mutant: &Utf8Path,
) -> Result<(Value, Vec<String>), String> {
    let (finished, probe) = runtime
        .command(
            copy.root(),
            vec![
                runtime.python.clone().into(),
                runtime.script.clone().into(),
                "crosshair-probe".into(),
                "--original".into(),
                original.into(),
                "--mutant".into(),
                mutant.into(),
            ],
            10.0,
        )
        .await?;
    if finished.termination != ProcessTermination::Exit(0) {
        return Err(format!("probe generation failed: {probe}"));
    }
    fs::write(runtime.scratch.join("search_probe.py"), probe).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let (crosshair_result, crosshair_log) = runtime
        .command(
            &runtime.scratch.clone(),
            vec![
                runtime.python.clone().into(),
                "-m".into(),
                "crosshair".into(),
                "diffbehavior".into(),
                "--per_condition_timeout".into(),
                "45".into(),
                "--per_path_timeout".into(),
                "3".into(),
                "search_probe.original".into(),
                "search_probe.mutant".into(),
            ],
            60.0,
        )
        .await?;
    let crosshair_wall = start.elapsed().as_secs_f64();
    let crosshair_output = runtime.scratch.join("crosshair.txt");
    fs::write(&crosshair_output, &crosshair_log).map_err(|e| e.to_string())?;
    let (parsed, arguments) = runtime
        .command(
            copy.root(),
            vec![
                runtime.python.clone().into(),
                runtime.script.clone().into(),
                "parse-crosshair".into(),
                "--original".into(),
                crosshair_output.into(),
            ],
            10.0,
        )
        .await?;
    if parsed.termination != ProcessTermination::Exit(0) {
        return Err(format!("crosshair parsing failed: {arguments}"));
    }
    let crosshair_args: Vec<String> =
        serde_json::from_str(&arguments).map_err(|e| e.to_string())?;
    Ok((
        json!({"termination":format!("{:?}",crosshair_result.termination),"wall":crosshair_wall,"output":crosshair_log}),
        crosshair_args,
    ))
}

async fn verify_generated(
    runtime: &mut Runtime,
    copy: &WorkerWorkspace,
    candidate: &MutationCandidate,
    witnesses: &[Value],
) -> Result<Value, String> {
    let witness = witnesses
        .iter()
        .find(|w| w["status"] == "reproduced_real_difference")
        .ok_or("no real witness")?;
    let x = witness["args"][0].as_str().ok_or("missing x")?;
    let y = witness["args"][1].as_str().ok_or("missing y")?;
    let expected = witness["originals"][0]["value"]
        .as_f64()
        .ok_or("missing finite expected")?;
    let (finished, test) = runtime
        .command(
            copy.root(),
            vec![
                runtime.python.clone().into(),
                runtime.script.clone().into(),
                "regression".into(),
                "--x".into(),
                x.into(),
                "--y".into(),
                y.into(),
                "--expected".into(),
                expected.to_string().into(),
            ],
            10.0,
        )
        .await?;
    if finished.termination != ProcessTermination::Exit(0) {
        return Err(format!("regression generation failed: {test}"));
    }
    let test_path = "tests/test_hoimin_generated_length_ratio.py";
    fs::write(copy.root().join(test_path), &test).map_err(|e| e.to_string())?;
    // Re-plan and copy AFTER adding the characterization test: stale manifests
    // cannot establish that the new test was included in the verified workspace.
    let fresh_manifest = fixed_candidates(runtime, copy.root()).await?;
    let fresh_evidence = runtime.plans.last().ok_or("missing new plan evidence")?;
    if !fresh_evidence["fingerprint_inputs"]
        .as_array()
        .ok_or("missing fingerprints")?
        .iter()
        .any(|record| record["path"] == test_path)
    {
        return Err("new plan did not fingerprint the generated test".into());
    }
    let (fresh_plan, mut fresh) = worker(copy.root())?;
    let verified = async {
        let (pass, pass_log) = runtime.pytest(&fresh, test_path, 30.0).await?;
        if pass.termination != ProcessTermination::Exit(0)
            || !pass_log.contains("1 passed")
            || pass_log.contains("skipped")
            || pass_log.contains("xfailed")
        {
            return Err(format!("generated original test failed: {pass_log}"));
        }
        fresh.apply_mutation(candidate).map_err(|e| e.to_string())?;
        let (fail, fail_log) = runtime.pytest(&fresh, test_path, 30.0).await?;
        if verdict(&fail, &fail_log) != "detected_assertion" {
            return Err(format!(
                "generated test did not detect assertion: {fail_log}"
            ));
        }
        fresh.reset().map_err(|e| e.to_string())?;
        let started = Instant::now();
        let (suite, suite_log) = runtime.command(fresh.root(),vec![runtime.python.clone().into(),"-m".into(),"pytest".into(),"-q".into(),"-p".into(),"no:cacheprovider".into(),"-m".into(),"not slow".into()],600.0).await?;
        Ok(json!({"original":pass_log,"mutant":fail_log,"lightweight_suite":{"termination":format!("{:?}",suite.termination),"wall":started.elapsed().as_secs_f64(),"output":suite_log}}))
    }
    .await;
    fresh.try_cleanup().map_err(|e| e.to_string())?;
    fresh_plan.verify_originals().map_err(|e| e.to_string())?;
    Ok(
        json!({"generated_test":test,"test_hash":blake3::hash(test.as_bytes()).to_hex().to_string(),"fresh_plan_candidates":fresh_manifest.len(),"verification":verified?}),
    )
}

pub fn comparison_candidate(arms: &[Value]) -> Option<&Value> {
    arms.iter()
        .flat_map(|arm| {
            arm["shrink_rows"]
                .as_array()
                .into_iter()
                .flatten()
                .rev()
                .filter(|entry| entry["kept"] == true)
                .map(|entry| &entry["row"])
                .chain(arm["rows"].as_array().into_iter().flatten())
        })
        .find(|row| eligible(row))
}

async fn replay(
    runtime: &mut Runtime,
    copy: &mut WorkerWorkspace,
    source: &[u8],
    arms: &[Value],
) -> Result<Value, String> {
    let Some(row) = comparison_candidate(arms) else {
        return Ok(json!({"status":"no_eligible_finite_witness"}));
    };
    let descriptor: CandidateDescriptor =
        serde_json::from_value(row["candidate"].clone()).map_err(|e| e.to_string())?;
    let candidate = mutation(source, &descriptor)?;
    let original = runtime.scratch.join("comparison_original.py");
    let mutant = runtime.scratch.join("comparison_mutant.py");
    fs::write(&original, source).map_err(|e| e.to_string())?;
    copy.apply_mutation(&candidate).map_err(|e| e.to_string())?;
    fs::write(&mutant, copy.read(TARGET).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    copy.reset().map_err(|e| e.to_string())?;
    let start = Instant::now();
    let hypothesis = runtime
        .search(copy.root(), &original, &mutant, 790, 60.0)
        .await?;
    let hypothesis_wall = start.elapsed().as_secs_f64();
    let (crosshair, crosshair_args) = compare_crosshair(runtime, copy, &original, &mutant).await?;
    let mut witnesses = Vec::new();
    for (method, args) in [
        (
            "hypothesis",
            hypothesis["args"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        ),
        ("crosshair", crosshair_args),
    ] {
        if args.len() != 2 {
            witnesses.push(json!({"method":method,"status":"no_valid_witness"}));
            continue;
        }
        let mut originals = Vec::new();
        let mut mutants = Vec::new();
        for _ in 0..2 {
            originals.push(runtime.observe(copy, &args[0], &args[1]).await?);
        }
        copy.apply_mutation(&candidate).map_err(|e| e.to_string())?;
        for _ in 0..2 {
            mutants.push(runtime.observe(copy, &args[0], &args[1]).await?);
        }
        copy.reset().map_err(|e| e.to_string())?;
        if originals[0]["value"] != originals[1]["value"]
            || mutants[0]["value"] != mutants[1]["value"]
            || originals[0]["value"] == mutants[0]["value"]
        {
            return Err("witness failed independent real replay".into());
        }
        witnesses.push(json!({"method":method,"status":"reproduced_real_difference","args":args,"originals":originals,"mutants":mutants}));
    }
    let generated = verify_generated(runtime, copy, &candidate, &witnesses).await?;
    Ok(
        json!({"candidate":descriptor,"prior_status":row["status"],"hypothesis":hypothesis,"hypothesis_wall":hypothesis_wall,
        "crosshair":crosshair,"real_replay":witnesses,"generated":generated}),
    )
}

pub fn verdict(result: &ProcessFinished, output: &str) -> &'static str {
    if !complete(result) {
        return "incomplete";
    }
    match result.termination {
        ProcessTermination::Exit(0)
            if output.contains(" passed")
                && !output.contains("skipped")
                && !output.contains("xfailed") =>
        {
            "survived_related_suite"
        }
        ProcessTermination::Exit(1)
            if output.contains("AssertionError")
                && !output.contains("ImportError")
                && !output.contains("ERROR collecting") =>
        {
            "detected_assertion"
        }
        ProcessTermination::Exit(_) => "test_failure_other",
        _ => "incomplete",
    }
}

async fn fixed_candidates(
    runtime: &mut Runtime,
    root: &Utf8Path,
) -> Result<Vec<CandidateDescriptor>, String> {
    let mut argv = vec![OsString::from(env!("CARGO_BIN_EXE_hoimin"))];
    // Pinned target's expression line; --symbol requires MODULE:QUALNAME and
    // --source, whose whole-file selection would broaden this experiment.
    let line = format!("{TARGET}:41");
    for value in [
        "plan",
        "--root",
        root.as_str(),
        "--file",
        TARGET,
        "--line",
        &line,
        "--max-mutants",
        "32",
        "--jobs",
        "1",
        "--max-workspace-size",
        "8GiB",
        "--min-free-space",
        "10GiB",
        "--fingerprint-include",
        "tests/**/*.py",
        "--",
        runtime.python.as_str(),
        "-m",
        "pytest",
        "-q",
        "tests/test_variant_candidates.py",
    ] {
        argv.push(value.into());
    }
    let (result, output) = runtime.command(root, argv, 60.0).await?;
    if result.termination != ProcessTermination::Exit(0) {
        return Err(format!("plan failed: {output}"));
    }
    let manifest: PlanManifest = serde_json::from_str(&output).map_err(|e| e.to_string())?;
    runtime.plans.push(json!({"manifest_hash":blake3::hash(output.as_bytes()).to_hex().to_string(),"sources":manifest.sources,"fingerprint_inputs":manifest.fingerprint_inputs}));
    Ok(manifest
        .candidates
        .into_iter()
        .map(|ranked| {
            let c = ranked.candidate;
            CandidateDescriptor {
                schema_version: hoimin_core::CANDIDATE_SCHEMA_VERSION,
                path: c.path,
                span: c.span,
                original: c.original,
                replacement: c.replacement,
                operator: c.operator,
                line: c.line,
                column: c.column,
                symbol: c.symbol,
                file_hash: c.file_hash,
            }
        })
        .collect())
}

async fn evaluate(
    runtime: &mut Runtime,
    copy: &mut WorkerWorkspace,
    source: &[u8],
    candidate: &CandidateDescriptor,
    seed: u64,
    seconds: f64,
) -> Result<Value, String> {
    let started = Instant::now();
    copy.apply_mutation(&mutation(source, candidate)?)
        .map_err(|e| e.to_string())?;
    let (finished, output) = runtime
        .pytest(copy, "tests/test_variant_candidates.py", seconds)
        .await?;
    let status = verdict(&finished, &output);
    if status == "incomplete" {
        copy.reset().map_err(|e| e.to_string())?;
        return Err(format!(
            "incomplete test process; stop all comparisons: {:?}",
            finished.termination
        ));
    }
    let original = runtime.scratch.join("original.py");
    let mutated = runtime.scratch.join("mutant.py");
    fs::write(&original, source).map_err(|e| e.to_string())?;
    fs::write(&mutated, copy.read(TARGET).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let remaining = seconds - started.elapsed().as_secs_f64();
    let witness = if remaining > 0.5 {
        runtime
            .search(copy.root(), &original, &mutated, seed, remaining.min(2.0))
            .await?
    } else {
        json!({"status":"no_witness_within_budget"})
    };
    copy.reset().map_err(|e| e.to_string())?;
    Ok(
        json!({"candidate": candidate, "id": mutation(source, candidate)?.id, "status":status, "witness":witness, "elapsed":started.elapsed().as_secs_f64()}),
    )
}

fn choose_index(
    mode: &str,
    pool: &[CandidateDescriptor],
    weights: &BTreeMap<String, usize>,
    rng: &mut proptest::test_runner::TestRunner,
) -> Result<usize, String> {
    let index = if mode == "fixed" {
        0
    } else {
        let tickets = pool
            .iter()
            .enumerate()
            .flat_map(|(index, candidate)| {
                std::iter::repeat_n(
                    index,
                    if mode == "guided" {
                        *weights.get(&candidate.operator).unwrap_or(&1)
                    } else {
                        1
                    },
                )
            })
            .collect::<Vec<_>>();
        tickets[(0..tickets.len())
            .new_tree(rng)
            .map_err(|e| e.to_string())?
            .current()]
    };
    Ok(index)
}

async fn arms(
    runtime: &mut Runtime,
    copy: &mut WorkerWorkspace,
    source: &[u8],
    fixed: &[CandidateDescriptor],
) -> Result<Vec<Value>, String> {
    let text = std::str::from_utf8(source).map_err(|e| e.to_string())?;
    let mut reports = Vec::new();
    for seed in [790, 776, 777] {
        for mode in ["fixed", "random", "guided"] {
            let start = Instant::now();
            let generated = generator::candidate_space(text)?;
            let before = runtime.processes;
            let mut pool = if mode == "fixed" {
                fixed.to_vec()
            } else {
                generated.clone()
            };
            let mut rng = generator::seeded_runner(seed);
            let mut weights = BTreeMap::<String, usize>::new();
            let mut rows = Vec::new();
            let mut shrink_rows = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            let mut kinds = std::collections::BTreeSet::new();
            while !pool.is_empty()
                && rows.len() + shrink_rows.len() < 32
                && start.elapsed().as_secs_f64() < 59.0
            {
                if fs2::available_space(copy.root()).map_err(|e| e.to_string())? <= FREE_RESERVE {
                    return Err("disk reserve exhausted during experiment".into());
                }
                let index = choose_index(mode, &pool, &weights, &mut rng)?;
                let candidate = pool.remove(index);
                let id = mutation(source, &candidate)?.id;
                if !seen.insert(id) {
                    continue;
                }
                let row = evaluate(
                    runtime,
                    copy,
                    source,
                    &candidate,
                    seed,
                    60.0 - start.elapsed().as_secs_f64(),
                )
                .await?;
                if mode == "guided" {
                    let reward = usize::from(row["witness"]["status"] == "witness")
                        + usize::from(kinds.insert(candidate.operator.clone()));
                    *weights.entry(candidate.operator.clone()).or_insert(1) += reward;
                }
                eprintln!(
                    "{mode} seed={seed} n={} {} {}",
                    rows.len() + 1,
                    row["status"],
                    row["witness"]["status"]
                );
                let incomplete = row["status"] == "incomplete";
                if mode != "fixed" && eligible(&row) {
                    let (space, mut tree) = generator::shrink_tree(text, &candidate)?;
                    let mut next = tree.simplify();
                    while next
                        && rows.len() + shrink_rows.len() + 1 < 32
                        && start.elapsed().as_secs_f64() < 59.0
                    {
                        let smaller = &space[tree.current()];
                        if !seen.insert(mutation(source, smaller)?.id) {
                            next = tree.complicate();
                            continue;
                        }
                        let checked = evaluate(
                            runtime,
                            copy,
                            source,
                            smaller,
                            seed,
                            60.0 - start.elapsed().as_secs_f64(),
                        )
                        .await?;
                        let keep = eligible(&checked);
                        let stop = checked["status"] == "incomplete";
                        shrink_rows.push(json!({"kept":keep,"row":checked}));
                        if stop {
                            return Err("incomplete candidate shrink evaluation".into());
                        }
                        next = if keep {
                            tree.simplify()
                        } else {
                            tree.complicate()
                        };
                    }
                }
                rows.push(row);
                if incomplete {
                    break;
                }
            }
            reports.push(json!({"mode":mode,"seed":seed,"candidate_space":if mode=="fixed"{fixed.len()}else{generated.len()},"rows":rows,"shrink_rows":shrink_rows,"elapsed":start.elapsed().as_secs_f64(),"processes":runtime.processes-before,"remaining":pool.len(),"weights":weights}));
        }
    }
    Ok(reports)
}

pub async fn experiment() -> Result<(), String> {
    let started = Instant::now();
    let root =
        Utf8PathBuf::from(std::env::var("HOIMIN_PROOFREADTOOLS_ROOT").map_err(|e| e.to_string())?);
    let python =
        Utf8PathBuf::from(std::env::var("HOIMIN_RESEARCH_PYTHON").map_err(|e| e.to_string())?);
    let output =
        Utf8PathBuf::from(std::env::var("HOIMIN_RESEARCH_OUTPUT").map_err(|e| e.to_string())?);
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let scratch = Utf8Path::from_path(temp.path())
        .ok_or("non-UTF8 temp")?
        .to_owned();
    for path in [&root, &scratch] {
        if fs2::available_space(path).map_err(|e| e.to_string())? <= FREE_RESERVE {
            return Err("less than 10GiB free".into());
        }
    }
    let backend = resource_backend()?;
    let mut runtime = Runtime {
        handler: ProcessHandler::new(backend, scratch.clone()),
        python,
        script: Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/proofreadtools_search.py"),
        scratch,
        processes: 0,
        plans: Vec::new(),
    };
    runtime.validate_sample(&root).await?;
    let (plan, mut copy) = worker(&root)?;
    let result = async {
        let (baseline, log) = runtime.pytest(&copy, "tests/test_variant_candidates.py", 60.0).await?;
        if baseline.termination != ProcessTermination::Exit(0) { return Err(format!("baseline failed: {log}")); }
        let source = fs::read(root.join(TARGET)).map_err(|e| e.to_string())?;
        let (arms, comparison) = if let Ok(previous) = std::env::var("HOIMIN_RESEARCH_REPLAY") {
            let prior: Value = serde_json::from_slice(&fs::read(previous).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            if prior["sample_sha"] != generator::SAMPLE_SHA || prior["source_hash"] != blake3::hash(&source).to_hex().to_string() { return Err("previous evidence source mismatch".into()); }
            let arms = prior["arms"].as_array().ok_or("missing previous arms")?.clone();
            let comparison = replay(&mut runtime,&mut copy,&source,&arms).await?;
            (arms, comparison)
        } else {
            let fixed = fixed_candidates(&mut runtime, &root).await?;
            let arms = arms(&mut runtime, &mut copy, &source, &fixed).await?;
            let comparison = replay(&mut runtime,&mut copy,&source,&arms).await?;
            (arms, comparison)
        };
        Ok(json!({"sample_sha":generator::SAMPLE_SHA,"source_hash":blake3::hash(&source).to_hex().to_string(),"target":TARGET,"symbol":SYMBOL,"baseline":log,"arms":arms,"comparison":comparison,"processes":runtime.processes,"platform":std::env::consts::OS,"plans":runtime.plans}))
    }.await;
    let cleanup = copy.try_cleanup().map_err(|e| e.to_string());
    let originals = plan.verify_originals().map_err(|e| e.to_string());
    let drain = runtime
        .handler
        .drain_for_shutdown(Duration::from_secs(5))
        .await;
    runtime.handler.close().map_err(|e| e.to_string())?;
    cleanup?;
    originals?;
    if !drain.all_reaped || !drain.output_drains_joined || !drain.secondary_errors.is_empty() {
        return Err(format!("incomplete shutdown: {drain:?}"));
    }
    eprintln!("cleanup complete; shutdown={drain:?}");
    let mut report = result?;
    report["cleanup"] = json!({"workspace_removed":true,"originals_verified":true,"all_reaped":true,"output_drains_joined":true});
    report["total_wall"] = json!(started.elapsed().as_secs_f64());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| e.to_string())?;
    file.write_all(
        serde_json::to_string_pretty(&report)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
