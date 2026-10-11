use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::plan::PlanManifest;
use hoimin_cli::process::ProcessHandler;
use hoimin_cli::workspace::WorkerWorkspace;
use hoimin_core::{CandidateDescriptor, ProcessTermination};
use serde_json::{Value, json};

use super::{generator, runner};
use runner::{Runtime, mutation, worker};

const SHA: &str = "dab0a5c47f159eb58472110dcbc10bd4a9b01adb";
const ARM_SECONDS: f64 = 30.0;
const LIMIT: usize = 12;
const RESERVE: u64 = 10 * 1024 * 1024 * 1024;

struct Target {
    key: &'static str,
    path: &'static str,
    symbol: &'static str,
    lines: &'static str,
    suite: &'static str,
}

const TARGETS: [Target; 3] = [
    Target {
        key: "numberformat",
        path: "django/utils/numberformat.py",
        symbol: "format",
        lines: "7-108",
        suite: "utils_tests.test_numberformat",
    },
    Target {
        key: "timesince",
        path: "django/utils/timesince.py",
        symbol: "timesince",
        lines: "28-135",
        suite: "utils_tests.test_timesince",
    },
    Target {
        key: "duration",
        path: "django/utils/duration.py",
        symbol: "duration_microseconds",
        lines: "45-46",
        suite: "utils_tests.test_duration",
    },
];

fn target(path: &str) -> Result<&'static Target, String> {
    TARGETS
        .iter()
        .find(|t| t.path == path)
        .ok_or_else(|| "unknown research target".into())
}

fn suite_count(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        line.strip_prefix("Ran ")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn skip_count(output: &str) -> u64 {
    output
        .rsplit_once("skipped=")
        .and_then(|(_, rest)| {
            rest.split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

pub fn suite_status(success: bool, output: &str, count: u64, skips: u64) -> &'static str {
    if suite_count(output) != Some(count) || skip_count(output) != skips {
        return "invalid_suite";
    }
    if success && output.contains("\nOK") {
        "survived_related_suite"
    } else if !success
        && output.contains("AssertionError")
        && output.contains("FAILED (failures=")
        && !output.contains("errors=")
    {
        "detected_assertion"
    } else {
        "test_failure_other"
    }
}

fn suite_argv(runtime: &Runtime, label: &str) -> Vec<OsString> {
    vec![
        runtime.python.clone().into(),
        "tests/runtests.py".into(),
        label.into(),
        "--parallel".into(),
        "1".into(),
        "--verbosity".into(),
        "1".into(),
        "--noinput".into(),
    ]
}

async fn fixed(
    runtime: &mut Runtime,
    root: &Utf8Path,
    entry: &Target,
) -> Result<Vec<CandidateDescriptor>, String> {
    let mut command = vec![OsString::from(env!("CARGO_BIN_EXE_hoimin"))];
    for value in [
        "plan",
        "--root",
        root.as_str(),
        "--line",
        &format!("{}:{}", entry.path, entry.lines),
        "--max-mutants",
        "1000",
        "--jobs",
        "1",
        "--max-workspace-size",
        "8GiB",
        "--min-free-space",
        "10GiB",
        "--fingerprint-include",
        "tests/utils_tests/**/*.py",
        "--",
        runtime.python.as_str(),
        "tests/runtests.py",
        entry.suite,
        "--parallel",
        "1",
        "--noinput",
    ] {
        command.push(value.into());
    }
    let (finished, output) = runtime.command(root, command, 60.0).await?;
    if finished.termination != ProcessTermination::Exit(0) {
        return Err(format!("plan failed: {output}"));
    }
    let plan: PlanManifest = serde_json::from_str(&output).map_err(|e| e.to_string())?;
    runtime.plans.push(json!({"target":entry.key,"hash":blake3::hash(output.as_bytes()).to_hex().to_string(),"sources":plan.sources,"fingerprint_inputs":plan.fingerprint_inputs}));
    Ok(plan
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
        .filter(|c| c.symbol.as_deref() == Some(entry.symbol))
        .collect())
}

async fn search(
    runtime: &mut Runtime,
    copy: &WorkerWorkspace,
    entry: &Target,
    source: &[u8],
    seed: u64,
    seconds: f64,
) -> Result<Value, String> {
    let original = runtime.scratch.join("django_original.py");
    let mutant = runtime.scratch.join("django_mutant.py");
    fs::write(&original, source).map_err(|e| e.to_string())?;
    fs::write(&mutant, copy.read(entry.path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let command = vec![
        runtime.python.clone().into(),
        runtime.script.clone().into(),
        "search".into(),
        entry.key.into(),
        "--original".into(),
        original.into(),
        "--mutant".into(),
        mutant.into(),
        "--seed".into(),
        seed.to_string().into(),
        "--seconds".into(),
        (seconds * 0.5).to_string().into(),
    ];
    let (finished, output) = runtime.command(copy.root(), command, seconds).await?;
    if finished.termination != ProcessTermination::Exit(0) {
        return Err(format!(
            "input search incomplete: {:?}: {output}",
            finished.termination
        ));
    }
    serde_json::from_str(&output).map_err(|e| e.to_string())
}

async fn evaluate(
    runtime: &mut Runtime,
    copy: &mut WorkerWorkspace,
    candidate: &CandidateDescriptor,
    source: &[u8],
    baseline: &Value,
    seed: u64,
    seconds: f64,
) -> Result<Value, String> {
    let start = Instant::now();
    let entry = target(candidate.path.as_str())?;
    copy.apply_mutation(&mutation(source, candidate)?)
        .map_err(|e| e.to_string())?;
    let result = async {
        let command = suite_argv(runtime, entry.suite);
        let (finished, log) = runtime.command(copy.root(),command,seconds).await?;
        if !matches!(finished.termination, ProcessTermination::Exit(_)) { return Err(format!("incomplete suite: {:?}", finished.termination)); }
        let status = suite_status(finished.termination == ProcessTermination::Exit(0),&log,baseline["count"].as_u64().ok_or("missing count")?,baseline["skips"].as_u64().ok_or("missing skips")?);
        if status == "invalid_suite" { return Err(format!("suite contract changed: {log}")); }
        let remaining = seconds - start.elapsed().as_secs_f64();
        let witness = if status == "survived_related_suite" && remaining >= 1.5 { search(runtime,copy,entry,source,seed,remaining.min(3.0)).await? } else { json!({"status":"not_searched","args":[]}) };
        Ok(json!({"candidate":candidate,"id":mutation(source,candidate)?.id,"target":entry.key,"status":status,"suite_log":log,"witness":witness,"wall":start.elapsed().as_secs_f64()}))
    }.await;
    copy.reset().map_err(|e| e.to_string())?;
    result
}

async fn arms(
    runtime: &mut Runtime,
    copy: &mut WorkerWorkspace,
    sources: &BTreeMap<String, Vec<u8>>,
    baselines: &Value,
    fixed: &[CandidateDescriptor],
    generated: &[CandidateDescriptor],
) -> Result<Vec<Value>, String> {
    let mut arms = Vec::new();
    for seed in [790, 776, 777, 748, 750] {
        for mode in ["fixed", "random", "guided"] {
            let start = Instant::now();
            let before = runtime.processes;
            let mut pool = if mode == "fixed" {
                fixed.to_vec()
            } else {
                generated.to_vec()
            };
            let mut rng = generator::seeded_runner(seed);
            let mut weights = BTreeMap::new();
            let mut kinds = BTreeSet::new();
            let mut rows = Vec::new();
            while !pool.is_empty()
                && rows.len() < LIMIT
                && start.elapsed().as_secs_f64() < ARM_SECONDS - 1.5
            {
                if fs2::available_space(copy.root()).map_err(|e| e.to_string())? <= RESERVE {
                    return Err("disk reserve exhausted".into());
                }
                let index = runner::choose_index(mode, &pool, &weights, &mut rng)?;
                let candidate = pool.remove(index);
                let entry = target(candidate.path.as_str())?;
                let row = evaluate(
                    runtime,
                    copy,
                    &candidate,
                    &sources[entry.path],
                    &baselines[entry.key],
                    seed,
                    (ARM_SECONDS - start.elapsed().as_secs_f64()).min(10.0),
                )
                .await?;
                if mode == "guided" {
                    let reward = usize::from(row["witness"]["status"] == "witness")
                        + usize::from(kinds.insert(candidate.operator.clone()));
                    *weights.entry(candidate.operator.clone()).or_insert(1) += reward;
                }
                eprintln!(
                    "django {mode} {seed} {} {} {}",
                    rows.len() + 1,
                    row["status"],
                    row["witness"]["status"]
                );
                rows.push(row);
            }
            arms.push(json!({"mode":mode,"seed":seed,"rows":rows,"wall":start.elapsed().as_secs_f64(),"processes":runtime.processes-before,"remaining":pool.len(),"weights":weights,"stop":if pool.is_empty(){"exhausted"}else if rows.len()==LIMIT{"count_limit"}else{"time_limit"}}));
        }
    }
    Ok(arms)
}

async fn observe(
    runtime: &mut Runtime,
    copy: &WorkerWorkspace,
    entry: &Target,
    args: &Value,
) -> Result<Value, String> {
    let command = vec![
        runtime.python.clone().into(),
        runtime.script.clone().into(),
        "observe".into(),
        entry.key.into(),
        "--args".into(),
        args.to_string().into(),
    ];
    let (finished, output) = runtime.command(copy.root(), command, 10.0).await?;
    if finished.termination != ProcessTermination::Exit(0) {
        return Err(format!("real observation failed: {output}"));
    }
    let value: Value = serde_json::from_str(&output).map_err(|e| e.to_string())?;
    let actual = Utf8Path::new(value["path"].as_str().ok_or("missing source path")?)
        .canonicalize_utf8()
        .map_err(|e| e.to_string())?;
    if actual
        != copy
            .root()
            .join(entry.path)
            .canonicalize_utf8()
            .map_err(|e| e.to_string())?
    {
        return Err("imported another checkout".into());
    }
    Ok(value)
}

async fn verify_test(
    runtime: &mut Runtime,
    copy: &WorkerWorkspace,
    entry: &Target,
    candidate: &hoimin_core::MutationCandidate,
    test: &str,
) -> Result<Value, String> {
    let test_path = format!("tests/utils_tests/test_hoimin_generated_{}.py", entry.key);
    fs::write(copy.root().join(&test_path), test).map_err(|e| e.to_string())?;
    let fresh_candidates = fixed(runtime, copy.root(), entry).await?;
    if !runtime.plans.last().ok_or("missing fresh plan")?["fingerprint_inputs"]
        .as_array()
        .ok_or("missing fingerprints")?
        .iter()
        .any(|v| v["path"] == test_path)
    {
        return Err("generated test missing from plan fingerprints".into());
    }
    let (fresh_plan, mut fresh) = worker(copy.root())?;
    let verified = async {
        let label = format!("utils_tests.test_hoimin_generated_{}", entry.key);
        let command = suite_argv(runtime, &label);
        let (pass, pass_log) = runtime.command(fresh.root(), command, 30.0).await?;
        if suite_status(
            pass.termination == ProcessTermination::Exit(0),
            &pass_log,
            1,
            0,
        ) != "survived_related_suite"
        {
            return Err(format!("generated original failed: {pass_log}"));
        }
        fresh.apply_mutation(candidate).map_err(|e| e.to_string())?;
        let command = suite_argv(runtime, &label);
        let (fail, fail_log) = runtime.command(fresh.root(), command, 30.0).await?;
        if fail.termination != ProcessTermination::Exit(1)
            || suite_status(false, &fail_log, 1, 0) != "detected_assertion"
        {
            return Err(format!(
                "generated mutant did not fail assertion: {fail_log}"
            ));
        }
        Ok(json!({"original":pass_log,"mutant":fail_log}))
    }
    .await;
    fresh.try_cleanup().map_err(|e| e.to_string())?;
    fresh_plan.verify_originals().map_err(|e| e.to_string())?;
    fs::remove_file(copy.root().join(test_path)).map_err(|e| e.to_string())?;
    Ok(json!({"fresh_plan_candidates":fresh_candidates.len(),"verification":verified?}))
}

async fn replay(
    runtime: &mut Runtime,
    copy: &mut WorkerWorkspace,
    sources: &BTreeMap<String, Vec<u8>>,
    arms: &[Value],
) -> Result<Vec<Value>, String> {
    let mut results = Vec::new();
    for entry in &TARGETS {
        let row = arms
            .iter()
            .flat_map(|arm| arm["rows"].as_array().into_iter().flatten())
            .find(|row| {
                row["target"] == entry.key
                    && row["status"] == "survived_related_suite"
                    && row["witness"]["status"] == "witness"
            });
        let Some(row) = row else {
            results.push(json!({"target":entry.key,"status":"no_eligible_survivor"}));
            continue;
        };
        let candidate: CandidateDescriptor =
            serde_json::from_value(row["candidate"].clone()).map_err(|e| e.to_string())?;
        let candidate = mutation(&sources[entry.path], &candidate)?;
        let args = &row["witness"]["args"];
        let mut originals = Vec::new();
        let mut mutants = Vec::new();
        for _ in 0..2 {
            originals.push(observe(runtime, copy, entry, args).await?);
        }
        copy.apply_mutation(&candidate).map_err(|e| e.to_string())?;
        let observation = async {
            for _ in 0..2 {
                mutants.push(observe(runtime, copy, entry, args).await?);
            }
            Ok::<(), String>(())
        }
        .await;
        copy.reset().map_err(|e| e.to_string())?;
        observation?;
        if originals[0]["value"] != originals[1]["value"]
            || mutants[0]["value"] != mutants[1]["value"]
            || originals[0]["value"] == mutants[0]["value"]
        {
            return Err("non-reproducible witness".into());
        }
        let command = vec![
            runtime.python.clone().into(),
            runtime.script.clone().into(),
            "regression".into(),
            entry.key.into(),
            "--args".into(),
            args.to_string().into(),
            "--expected".into(),
            originals[0]["value"].to_string().into(),
        ];
        let (finished, test) = runtime.command(copy.root(), command, 10.0).await?;
        if finished.termination != ProcessTermination::Exit(0) {
            return Err(format!("render failed: {test}"));
        }
        let generated = verify_test(runtime, copy, entry, &candidate, &test).await?;
        results.push(json!({"target":entry.key,"status":"verified_generated_test","candidate":row["candidate"],"args":args,"originals":originals,"mutants":mutants,"generated_test":test,"test_hash":blake3::hash(test.as_bytes()).to_hex().to_string(),"generated_verification":generated}));
    }
    Ok(results)
}

pub async fn experiment() -> Result<(), String> {
    let started = Instant::now();
    let root = Utf8PathBuf::from(std::env::var("HOIMIN_DJANGO_ROOT").map_err(|e| e.to_string())?);
    let python =
        Utf8PathBuf::from(std::env::var("HOIMIN_RESEARCH_PYTHON").map_err(|e| e.to_string())?);
    let output =
        Utf8PathBuf::from(std::env::var("HOIMIN_RESEARCH_OUTPUT").map_err(|e| e.to_string())?);
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let scratch = Utf8Path::from_path(temp.path())
        .ok_or("non-UTF8 scratch")?
        .to_owned();
    for path in [&root, &scratch] {
        if fs2::available_space(path).map_err(|e| e.to_string())? <= RESERVE {
            return Err("less than 10GiB free".into());
        }
    }
    let mut runtime = Runtime {
        handler: ProcessHandler::new(runner::resource_backend()?, scratch.clone()),
        python,
        script: Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/django_search.py"),
        scratch,
        processes: 0,
        plans: Vec::new(),
    };
    for (args, expected) in [
        (vec!["rev-parse", "HEAD"], SHA),
        (vec!["status", "--porcelain", "--untracked-files=no"], ""),
    ] {
        let mut command = vec!["git".into(), "-C".into(), root.clone().into()];
        command.extend(args.into_iter().map(OsString::from));
        let (done, text) = runtime.command(&root, command, 10.0).await?;
        if done.termination != ProcessTermination::Exit(0) || text.trim() != expected {
            return Err("Django must be clean pinned checkout".into());
        }
    }
    let (plan, mut copy) = worker(&root)?;
    let result=async {
        let mut sources=BTreeMap::new();
        let mut baselines=json!({});
        let mut fixed_pool=Vec::new();
        let mut generated_pool=Vec::new();
        for entry in &TARGETS {
            let command=suite_argv(&runtime,entry.suite);
            let (done,log)=runtime.command(copy.root(),command,60.0).await?;
            let count=suite_count(&log).ok_or_else(||format!("baseline did not run: {log}"))?;
            if done.termination!=ProcessTermination::Exit(0) || count==0 { return Err(format!("baseline failed: {log}")); }
            baselines[entry.key]=json!({"count":count,"skips":skip_count(&log),"log":log});
            let source=fs::read(root.join(entry.path)).map_err(|e| e.to_string())?;
            generated_pool.extend(generator::candidate_space_for(std::str::from_utf8(&source).map_err(|e| e.to_string())?,entry.path,entry.symbol)?);
            fixed_pool.extend(fixed(&mut runtime,&root,entry).await?);
            sources.insert(entry.path.to_owned(),source);
        }
        let reports=arms(&mut runtime,&mut copy,&sources,&baselines,&fixed_pool,&generated_pool).await?;
        let comparisons=replay(&mut runtime,&mut copy,&sources,&reports).await?;
        let hashes=sources.iter().map(|(p,s)|(p.clone(),blake3::hash(s).to_hex().to_string())).collect::<BTreeMap<_,_>>();
        Ok(json!({"sample_sha":SHA,"source_hashes":hashes,"baselines":baselines,"fixed_candidates":fixed_pool,"generated_candidates":generated_pool,"arms":reports,"comparisons":comparisons,"processes":runtime.processes,"plans":runtime.plans,"platform":std::env::consts::OS,"arm_seconds":ARM_SECONDS,"arm_count_limit":LIMIT}))
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
        return Err(format!("incomplete cleanup: {drain:?}"));
    }
    eprintln!("Django cleanup complete: {drain:?}");
    let mut report = result?;
    report["cleanup"] = json!({"workspace_removed":true,"originals_verified":true,"all_reaped":true,"output_drains_joined":true});
    report["wall"] = json!(started.elapsed().as_secs_f64());
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?
        .write_all(
            serde_json::to_string_pretty(&report)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
