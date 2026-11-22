use super::{
    ReduceArgs,
    input::Input,
    runner::{Runner, Stop},
};
use hoimin_budget_math::MutantTimeout;
use hoimin_core::reduce::{
    Context, Event, Identity, Outcome, Proposal, ResultEntry, State, Triple, exact_results, median,
    pair_pass,
};
use hoimin_core::{MutationCandidate, ProcessTermination};
use serde_json::json;
use std::{collections::VecDeque, time::Duration};
use tokio::time::Instant;

use super::input::Files;

pub(super) async fn run(args: ReduceArgs) -> Result<i32, String> {
    let start = Instant::now();
    let budget = humantime::parse_duration(&args.total_timeout).map_err(|e| e.to_string())?;
    if budget.is_zero() {
        return Err("total-timeout must be positive".into());
    }
    let deadline = start
        .checked_add(budget)
        .ok_or("total-timeout is too large")?;
    let input = match tokio::time::timeout_at(deadline, Input::prepare(&args)).await {
        Ok(input) => input?,
        Err(_) => return Ok(4),
    };
    let artifacts =
        super::artifacts::Artifacts::create(&input.output).map_err(|e| e.to_string())?;
    let mut runner = match Runner::new(&input, artifacts.clone(), start, deadline).await {
        Ok(runner) => runner,
        Err(stop) => {
            artifacts
                .write(
                    "report.json",
                    &serde_json::to_vec_pretty(
                        &json!({"schema_version":1,"kind":"reduce","stop":stop,"best":null}),
                    )
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            return Ok(stop.exit_code);
        }
    };
    let mut state = State::new(budget.as_nanos());
    let mut trials = Vec::new();
    let mut frozen_timeout = None;
    let search = search(
        &input,
        &mut runner,
        &mut state,
        &mut trials,
        &mut frozen_timeout,
    )
    .await;
    let mut stop = search.err();
    // Validate the entire copied project and each original edited-file snapshot
    // while the workspace still owns its immutable manifest.
    if let Err(error) = runner
        .verify_originals()
        .and_then(|()| input.verify_sources().map_err(Stop::infrastructure))
    {
        state.best = None;
        stop = Some(error);
    }
    if let Err(error) = runner.finish_execution(stop.as_ref()).await {
        stop = Some(error);
    }
    if let Err(error) = runner.close().await {
        stop = Some(error);
    }
    let selected_ids: Vec<_> = input.candidates.iter().map(|m| &m.id).collect();
    let definitions:Vec<_>=input.sources.iter().flat_map(|s|s.definitions.definitions().iter().enumerate().map(|(i,d)|json!({"id":s.first_id+i as u64,"path":s.path,"name":d.name,"start":d.range.start,"end":d.range.end}))).collect();
    let source_hashes: Vec<_> = input
        .sources
        .iter()
        .map(|s| json!({"path":s.path,"blake3":blake3::hash(&s.bytes).to_hex().to_string()}))
        .collect();
    let mut diff = None;
    if let Some(best) = &state.best {
        let removed: Vec<_> = best.removed.iter().map(|s| s.id).collect();
        let (files, _) = input.variant(&removed)?;
        let bytes = make_diff(&input, &files);
        match runner.publish("proposal.diff", &bytes).await {
            Ok(()) => diff = Some("proposal.diff"),
            Err(error) => stop = Some(error),
        }
    }
    let code = stop.as_ref().map_or(0, |s| s.exit_code);
    let report = json!({"schema_version":1,"kind":"reduce","plan_hash":input.plan_hash,
        "snapshot_hash":runner.snapshot_hash,"run_id":runner.run_id,"source_hashes":source_hashes,"environment_hash":runner.environment_hash(),"normalized_config":input.config,"selected_mutants":selected_ids,
        "test_selectors":args.test,"keep_selectors":args.keep,"definitions":definitions,"selected_definitions":input.selected,"kept_definitions":input.kept,
        "minimum_reduction":{"numerator":input.rate.0,"denominator":input.rate.1},
        "frozen_mutant_timeout_ns":frozen_timeout.map(|d:Duration|d.as_nanos()),
        "termination_reason":stop.as_ref().map_or("search_exhausted",|s|s.reason.as_str()),
        "budget_ns":budget.as_nanos(),"elapsed_ns":runner.tick(),"stop":stop,"exit_code":code,
        "observations":runner.observations,"trials":trials,"best":state.best,"diff":diff,
        "limitations":["Nonzero mutant exits cannot generally distinguish assertion failures from import or collection errors; inspect retained execution output.",
            "Evidence is restricted to the selected mutants, sources, tests and execution conditions; future fault detection and global optimality are not guaranteed."]});
    let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    if let Err(error) = runner.publish("report.json", &bytes).await {
        let _ = runner.artifacts.remove("proposal.diff");
        return Ok(error.exit_code);
    }
    Ok(code)
}

async fn normal(runner: &mut Runner, id: u64, files: &Files) -> Result<Option<u128>, Stop> {
    let result = runner
        .execute(id, files, None, runner.normal_timeout())
        .await?;
    Ok(matches!(result.termination, ProcessTermination::Exit(0)).then_some(result.elapsed_ns))
}
async fn three(runner: &mut Runner, id: u64, files: &Files) -> Result<Option<[u128; 3]>, Stop> {
    let mut times = [0; 3];
    for slot in &mut times {
        let Some(time) = normal(runner, id, files).await? else {
            return Ok(None);
        };
        *slot = time;
    }
    Ok(Some(times))
}
async fn results(
    runner: &mut Runner,
    id: u64,
    files: &Files,
    candidates: &[MutationCandidate],
    timeout: Duration,
) -> Result<Vec<ResultEntry>, Stop> {
    let mut results = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        let observation = runner.execute(id, files, Some(candidate), timeout).await?;
        let outcome = match observation.termination {
            ProcessTermination::Exit(0) => Outcome::Survived,
            ProcessTermination::Exit(_) => Outcome::Killed,
            _ => unreachable!("runner rejects non-exit outcomes"),
        };
        results.push(ResultEntry {
            id: i as u64,
            outcome,
        });
    }
    Ok(results)
}
fn reserve(original: [u128; 3], reduced: [u128; 3]) -> Duration {
    let total = original
        .into_iter()
        .max()
        .unwrap_or(0)
        .saturating_add(reduced.into_iter().max().unwrap_or(0))
        .saturating_mul(3);
    Duration::from_nanos(u64::try_from(total.saturating_add(total / 5)).unwrap_or(u64::MAX))
}
#[allow(
    clippy::too_many_lines,
    reason = "keeps search, candidate rejection, and certification ordering explicit"
)]
async fn search(
    input: &Input,
    runner: &mut Runner,
    state: &mut State,
    trials: &mut Vec<serde_json::Value>,
    frozen_timeout: &mut Option<Duration>,
) -> Result<(), Stop> {
    let original_files: Files = input
        .sources
        .iter()
        .map(|s| (s.path.clone(), s.bytes.clone()))
        .collect();
    let originals = three(runner, 0, &original_files)
        .await?
        .ok_or_else(|| Stop {
            reason: "original_baseline_failed".into(),
            exit_code: 2,
        })?;
    let timeout = match input.config.limits.mutant_timeout {
        MutantTimeout::Auto => hoimin_budget_math::auto_mutant_timeout(Duration::from_nanos(
            u64::try_from(originals[0]).map_err(Stop::infrastructure)?,
        )),
        MutantTimeout::Fixed(value) => value.get(),
    };
    *frozen_timeout = Some(timeout);
    let before = results(runner, 0, &original_files, &input.candidates, timeout).await?;
    let context = Context {
        original: input.original.clone(),
        kept_spans: input.kept_spans.clone(),
        selected: (0..input.candidates.len() as u64).collect(),
        before,
        // These handles are scoped to this owned session; variant IDs are bound
        // to the actual immutable file bytes constructed immediately below.
        identity: Identity {
            source: 1,
            plan: 1,
            command: 1,
            environment: 1,
            timeout: timeout.as_nanos(),
            variant: 0,
        },
        baseline_pass: true,
        source_disjoint: true,
        selectors_unique: true,
        numerator: input.rate.0,
        denominator: input.rate.1,
    };
    if !context.valid() {
        return Err(Stop {
            reason: "original suite killed no selected mutant".into(),
            exit_code: 2,
        });
    }
    let mut groups = VecDeque::from([input.selected.clone()]);
    let mut removed = Vec::new();
    let mut id = 0u64;
    while let Some(group) = groups.pop_front() {
        if group.is_empty() {
            continue;
        }
        runner.check_deadline()?;
        if runner.remaining() <= reserve(originals, originals) {
            return Err(Stop {
                reason: "final_confirmation_reserve".into(),
                exit_code: 4,
            });
        }
        id += 1;
        let mut attempt = removed.clone();
        attempt.extend(&group);
        attempt.sort_unstable();
        attempt.dedup();
        let (files, spans) = match input.variant(&attempt) {
            Ok(v) => v,
            Err(e) => {
                trials.push(json!({"id":id,"removed":attempt,"rejected":e}));
                split(&mut groups, &group);
                continue;
            }
        };
        trials.push(json!({"id":id,"removed":attempt,"status":"evaluating","variant_hashes":files.iter().map(|(path,bytes)|json!({"path":path,"blake3":blake3::hash(bytes).to_hex().to_string()})).collect::<Vec<_>>()}));
        let Some(exploration) = three(runner, id, &files).await? else {
            trials.push(json!({"id":id,"removed":attempt,"rejected":"normal_test_failed"}));
            split(&mut groups, &group);
            continue;
        };
        let after = results(runner, id, &files, &input.candidates, timeout).await?;
        let faster = median(exploration)
            < state
                .best
                .as_ref()
                .map_or(median(originals), |p| median(p.exploration.values()));
        if !exact_results(&context.selected, &context.before, &after)
            || !faster
            || !pair_pass(
                input.rate.0,
                input.rate.1,
                median(originals),
                median(exploration),
            )
        {
            trials.push(json!({"id":id,"removed":attempt,"exploration_ns":exploration,"after":after,"rejected":"results_or_exploration"}));
            split(&mut groups, &group);
            continue;
        }
        if runner.remaining() <= reserve(originals, exploration) {
            trials.push(json!({"id":id,"removed":attempt,"exploration_ns":exploration,"after":after,"rejected":"insufficient_final_budget"}));
            return Err(Stop {
                reason: "final_confirmation_reserve".into(),
                exit_code: 4,
            });
        }
        let mut original_pairs = [0; 3];
        let mut reduced_pairs = [0; 3];
        let mut baseline_pass = true;
        let mut pair_count = 0;
        for i in 0..3 {
            let Some(o) = normal(runner, 0, &original_files).await? else {
                baseline_pass = false;
                break;
            };
            original_pairs[i] = o;
            let Some(r) = normal(runner, id, &files).await? else {
                baseline_pass = false;
                break;
            };
            reduced_pairs[i] = r;
            pair_count += 1;
        }
        let mut evidence = context.identity.clone();
        evidence.variant = id;
        let physical: Vec<_> = spans.iter().map(|s| s.id).collect();
        let proposal = Proposal {
            id,
            retained: input
                .original
                .iter()
                .filter(|n| !physical.contains(n))
                .copied()
                .collect(),
            removed: spans,
            after,
            evidence,
            baseline_pass,
            exploration: Triple::from(exploration),
            original_pairs: Triple::from(original_pairs),
            reduced_pairs: Triple::from(reduced_pairs),
            pair_count,
            completed_at: runner.tick(),
        };
        let certified = context.check(&proposal).certified;
        trials.push(json!({"proposal":proposal,"checks":context.check(&proposal),"variant_hashes":files.iter().map(|(path,bytes)|json!({"path":path,"blake3":blake3::hash(bytes).to_hex().to_string()})).collect::<Vec<_>>()}));
        let tick = proposal.completed_at;
        state.step(
            &context,
            Event {
                tick,
                proposal: Some(proposal),
                stop: false,
            },
        );
        if state.stopped {
            return Err(Stop::budget());
        }
        if certified {
            removed = physical;
            let remaining = input
                .selected
                .iter()
                .filter(|i| !removed.contains(i))
                .copied()
                .collect::<Vec<_>>();
            groups = VecDeque::from([remaining]);
        } else {
            split(&mut groups, &group);
        }
    }
    Ok(())
}
fn split(groups: &mut VecDeque<Vec<u64>>, group: &[u64]) {
    if group.len() > 1 {
        let middle = group.len() / 2;
        groups.push_front(group[middle..].to_vec());
        groups.push_front(group[..middle].to_vec());
    }
}
fn make_diff(input: &Input, files: &Files) -> Vec<u8> {
    let mut diff = Vec::new();
    for (path, bytes) in files {
        let source = input
            .sources
            .iter()
            .find(|s| s.path == *path)
            .expect("variant source");
        if source.bytes == *bytes {
            continue;
        }
        append_file_diff(&mut diff, path.as_str(), &source.bytes, bytes);
    }
    diff
}
fn append_file_diff(diff: &mut Vec<u8>, path: &str, original: &[u8], bytes: &[u8]) {
    let lines = |b: &[u8]| b.split_inclusive(|c| *c == b'\n').count();
    let old = lines(original);
    let new = lines(bytes);
    diff.extend_from_slice(
        format!(
            "--- a/{path}\n+++ b/{path}\n@@ -{},{old} +{},{new} @@\n",
            usize::from(old > 0),
            usize::from(new > 0)
        )
        .as_bytes(),
    );
    append_lines(diff, b'-', original);
    append_lines(diff, b'+', bytes);
}
fn append_lines(output: &mut Vec<u8>, prefix: u8, bytes: &[u8]) {
    for line in bytes.split_inclusive(|c| *c == b'\n') {
        output.push(prefix);
        output.extend_from_slice(line);
        if !line.ends_with(b"\n") {
            output.extend_from_slice(b"\n\\ No newline at end of file\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn apply_patch(original: &[u8], patch: &[u8], file_name: &str) -> Vec<u8> {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(file_name), original).unwrap();
        let mut child = std::process::Command::new("git")
            .arg("apply")
            .current_dir(directory.path())
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(patch).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        std::fs::read(directory.path().join(file_name)).unwrap()
    }
    #[rstest::rstest]
    #[case(b"def test_a(): pass\n", b"", "checks.py")]
    #[case(
        b"# coding: latin-1\r\nname='\xe9'\r\ndef test_a(): pass\r\n",
        b"# coding: latin-1\r\nname='\xe9'\r\n",
        "checks.py"
    )]
    #[case(b"def test_a(): pass", b"", "checks.py")]
    #[case(b"def test_a(): pass\n", b"", "check file.py")]
    fn diff_applies_to_the_exact_validated_bytes(
        #[case] original: &[u8],
        #[case] expected: &[u8],
        #[case] file_name: &str,
    ) {
        // Arrange
        let mut patch = Vec::new();
        append_file_diff(&mut patch, file_name, original, expected);
        // Act
        let actual = apply_patch(original, &patch, file_name);
        // Assert
        assert_eq!(actual, expected);
    }
}
