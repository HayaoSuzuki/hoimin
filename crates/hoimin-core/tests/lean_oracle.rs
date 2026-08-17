use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use hoimin_core::{
    AnalysisFinished, CandidateSpoolRef, CleanupFinished, CommandArg, EffectId, OriginalsVerified,
    OutputConfig, OutputEmitted, OutputEvent, OutputSpoolRef, PreflightCompleted, ProcessFinished,
    ProcessTermination, RawRunConfig, RawRunLimits, ResourceMode, RunConfig, RunEffect, RunEvent,
    RunPhase, RunState, StartRequested, TargetSlice, TargetsResolved, WorkerCreated, transition,
};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct OracleStep {
    event: String,
    verdict: String,
    error_code: Option<String>,
    phase: String,
    emitted: Vec<String>,
    pending: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    schedule: Vec<String>,
    expected: Vec<OracleStep>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaseClass {
    Match,
    Mismatch,
    InfrastructureError,
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    verdict: String,
    error_code: Option<String>,
    phase: String,
    emitted: Vec<String>,
    pending: usize,
}

impl From<&OracleStep> for Observation {
    fn from(value: &OracleStep) -> Self {
        Self {
            verdict: value.verdict.clone(),
            error_code: value.error_code.clone(),
            phase: value.phase.clone(),
            emitted: value.emitted.clone(),
            pending: value.pending,
        }
    }
}

#[derive(Debug)]
struct CaseResult {
    id: String,
    mode: String,
    class: CaseClass,
    expected: Vec<Observation>,
    actual: Vec<Observation>,
    detail: Option<String>,
}

struct Driver {
    state: Option<RunState>,
    outstanding: Vec<RunEffect>,
    ordinary_id: EffectId,
}

impl Driver {
    fn for_scenario(scenario: &str) -> Result<Self, String> {
        let config = RunConfig::try_from(RawRunConfig {
            root: ".".into(),
            sources: vec!["src".into()],
            test_argv: vec![CommandArg::Unix(b"python".to_vec())],
            limits: RawRunLimits {
                baseline_timeout: Duration::from_secs(1),
                ..RawRunLimits::default()
            },
            output: OutputConfig::default(),
            ..RawRunConfig::default()
        })
        .map_err(|error| format!("fixture config is invalid: {error}"))?;
        let (state, outstanding) = transition(
            RunState::new("lean-oracle", config),
            RunEvent::StartRequested(StartRequested),
        )
        .map_err(|error| format!("cannot start fixture machine: {error}"))?;
        let ordinary_id = outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::ResolveTargets(value) => Some(value.id),
                _ => None,
            })
            .ok_or_else(|| "fixture did not emit ResolveTargets".to_owned())?;
        let mut driver = Self {
            state: Some(state),
            outstanding,
            ordinary_id,
        };
        match scenario {
            "pending_resolve" => {}
            "normal_cleaning_with_copy" => driver.advance_to_normal_cleanup()?,
            "final_pending_without_copy" => {
                driver.apply("cancel")?;
                driver.apply("complete_cleanup")?;
            }
            "finished_without_copy" => {
                driver.apply("cancel")?;
                driver.apply("complete_cleanup")?;
                driver.complete_final_output()?;
            }
            other => return Err(format!("unknown scenario {other}")),
        }
        Ok(driver)
    }

    fn apply_setup_completion(
        &mut self,
        event: RunEvent,
        completed_id: EffectId,
    ) -> Result<(), String> {
        let state = self
            .state
            .take()
            .ok_or_else(|| "setup state is unavailable".to_owned())?;
        let (state, mut emitted) = transition(state, event)
            .map_err(|error| format!("setup transition failed: {error}"))?;
        self.outstanding
            .retain(|effect| effect.id() != completed_id);
        self.outstanding.append(&mut emitted);
        self.state = Some(state);
        Ok(())
    }

    fn advance_to_normal_cleanup(&mut self) -> Result<(), String> {
        self.advance_to_baseline()?;
        self.advance_to_empty_analysis()?;
        self.advance_to_pre_final_cleanup()?;

        let state = self
            .state
            .as_ref()
            .ok_or_else(|| "setup state is unavailable".to_owned())?;
        if state.phase() != RunPhase::Cleaning
            || !self
                .outstanding
                .iter()
                .any(|effect| matches!(effect, RunEffect::Cleanup(_)))
        {
            return Err("setup did not reach normal Cleaning".to_owned());
        }
        Ok(())
    }

    fn advance_to_baseline(&mut self) -> Result<(), String> {
        self.apply_setup_completion(
            RunEvent::TargetsResolved(TargetsResolved {
                id: self.ordinary_id,
                targets: vec![TargetSlice {
                    path: "src/calc.py".into(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                }],
            }),
            self.ordinary_id,
        )?;

        let preflight_id = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::Preflight(value) => Some(value.id),
                _ => None,
            })
            .ok_or_else(|| "setup did not emit Preflight".to_owned())?;
        self.apply_setup_completion(
            RunEvent::PreflightCompleted(PreflightCompleted {
                id: preflight_id,
                per_worker_logical_bytes: 10,
                requested_workers: 1,
                aggregate_logical_bytes: 10,
                fingerprint: None,
            }),
            preflight_id,
        )?;

        let run_started_id = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::EmitOutput(value)
                    if matches!(&value.event, OutputEvent::RunStarted(_)) =>
                {
                    Some(value.id)
                }
                _ => None,
            })
            .ok_or_else(|| "setup did not emit RunStarted".to_owned())?;
        self.apply_setup_completion(
            RunEvent::OutputEmitted(OutputEmitted { id: run_started_id }),
            run_started_id,
        )?;

        let (create_id, worker, reservation_id) = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::CreateWorker(value) => {
                    Some((value.id(), value.worker(), value.reservation_id()))
                }
                _ => None,
            })
            .ok_or_else(|| "setup did not emit CreateWorker".to_owned())?;
        self.apply_setup_completion(
            RunEvent::WorkerCreated(WorkerCreated {
                id: create_id,
                worker,
                reservation_id,
            }),
            create_id,
        )?;

        let (verify_id, checkpoint) = pending_verification(&self.outstanding)?;
        self.apply_setup_completion(
            RunEvent::OriginalsVerified(OriginalsVerified {
                id: verify_id,
                checkpoint,
            }),
            verify_id,
        )?;
        Ok(())
    }

    fn advance_to_empty_analysis(&mut self) -> Result<(), String> {
        let baseline_id = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::RunBaseline(value) => Some(value.id),
                _ => None,
            })
            .ok_or_else(|| "setup did not emit RunBaseline".to_owned())?;
        self.apply_setup_completion(
            RunEvent::BaselineFinished(successful_process(baseline_id)),
            baseline_id,
        )?;

        let baseline_output_id = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::EmitOutput(value) => Some(value.id),
                _ => None,
            })
            .ok_or_else(|| "setup did not emit the baseline result".to_owned())?;
        self.apply_setup_completion(
            RunEvent::OutputEmitted(OutputEmitted {
                id: baseline_output_id,
            }),
            baseline_output_id,
        )?;
        Ok(())
    }

    fn advance_to_pre_final_cleanup(&mut self) -> Result<(), String> {
        let analysis_id = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::AnalyzeFile(value) => Some(value.id),
                _ => None,
            })
            .ok_or_else(|| "setup did not emit AnalyzeFile".to_owned())?;
        self.apply_setup_completion(
            RunEvent::AnalysisFinished(AnalysisFinished {
                id: analysis_id,
                spool: Some(CandidateSpoolRef {
                    token: "empty".to_owned(),
                    records: 0,
                }),
                truncated: false,
                diagnostics: Vec::new(),
            }),
            analysis_id,
        )?;

        let (verify_id, checkpoint) = pending_verification(&self.outstanding)?;
        self.apply_setup_completion(
            RunEvent::OriginalsVerified(OriginalsVerified {
                id: verify_id,
                checkpoint,
            }),
            verify_id,
        )?;
        Ok(())
    }

    fn abstract_phase(&self, state: &RunState) -> String {
        if state.phase() == RunPhase::Finished {
            "finished".to_owned()
        } else if self.outstanding.iter().any(|effect| {
            matches!(
                effect,
                RunEffect::EmitOutput(value)
                    if matches!(&value.event, OutputEvent::RunFinished(_))
            )
        }) {
            "final_pending".to_owned()
        } else if state.phase() == RunPhase::Cleaning {
            "cleaning".to_owned()
        } else {
            "running".to_owned()
        }
    }

    fn completion_event(&self, name: &str) -> Result<(RunEvent, EffectId), String> {
        match name {
            "complete_unknown" => {
                let id = EffectId(u64::MAX);
                Ok((
                    RunEvent::TargetsResolved(TargetsResolved {
                        id,
                        targets: Vec::new(),
                    }),
                    id,
                ))
            }
            "complete_wrong_cleanup" => Ok((
                RunEvent::CleanupFinished(CleanupFinished {
                    id: self.ordinary_id,
                    released_reservations: Vec::new(),
                }),
                self.ordinary_id,
            )),
            "complete_ordinary" | "complete_ordinary_again" | "complete_retired_ordinary" => Ok((
                RunEvent::TargetsResolved(TargetsResolved {
                    id: self.ordinary_id,
                    targets: Vec::new(),
                }),
                self.ordinary_id,
            )),
            "complete_cleanup" => {
                let cleanup = self
                    .outstanding
                    .iter()
                    .find_map(|effect| match effect {
                        RunEffect::Cleanup(value) => Some(value),
                        _ => None,
                    })
                    .ok_or_else(|| "no cleanup effect is pending".to_owned())?;
                Ok((
                    RunEvent::CleanupFinished(CleanupFinished {
                        id: cleanup.id,
                        released_reservations: cleanup.reservations.clone(),
                    }),
                    cleanup.id,
                ))
            }
            other => Err(format!("event {other} is not a completion")),
        }
    }

    fn apply(&mut self, name: &str) -> Result<Observation, String> {
        let state = self
            .state
            .take()
            .ok_or_else(|| format!("cannot execute {name} after a rejected transition"))?;
        let pre_phase = self.abstract_phase(&state);
        let pre_pending = state.pending_count();
        let (event, completion_id) = match name {
            "cancel" => (RunEvent::CancellationRequested, None),
            "deadline" => (RunEvent::DeadlineReached, None),
            _ => {
                let (event, id) = self.completion_event(name)?;
                (event, Some(id))
            }
        };
        match transition(state, event) {
            Err(error) => Ok(Observation {
                verdict: "rejected".to_owned(),
                error_code: Some(error.code().to_owned()),
                phase: pre_phase,
                emitted: Vec::new(),
                pending: pre_pending,
            }),
            Ok((mut state, mut emitted)) => {
                let mut visible: Vec<_> = emitted.iter().filter_map(normalize_effect).collect();
                self.outstanding
                    .retain(|effect| state.is_effect_pending(effect.id()));
                self.outstanding.append(&mut emitted);
                if completion_id.is_none()
                    && let Some(run_started_id) =
                        self.outstanding.iter().find_map(|effect| match effect {
                            RunEffect::EmitOutput(value)
                                if matches!(&value.event, OutputEvent::RunStarted(_)) =>
                            {
                                Some(value.id)
                            }
                            _ => None,
                        })
                {
                    let (next, mut after_started) = transition(
                        state,
                        RunEvent::OutputEmitted(OutputEmitted { id: run_started_id }),
                    )
                    .map_err(|error| format!("cannot acknowledge RunStarted: {error}"))?;
                    state = next;
                    visible = after_started.iter().filter_map(normalize_effect).collect();
                    self.outstanding
                        .retain(|effect| effect.id() != run_started_id);
                    self.outstanding.append(&mut after_started);
                }
                let observation = Observation {
                    verdict: "accepted".to_owned(),
                    error_code: None,
                    phase: self.abstract_phase(&state),
                    emitted: visible,
                    pending: state.pending_count(),
                };
                self.state = Some(state);
                Ok(observation)
            }
        }
    }

    fn complete_final_output(&mut self) -> Result<(), String> {
        let final_id = self
            .outstanding
            .iter()
            .find_map(|effect| match effect {
                RunEffect::EmitOutput(value)
                    if matches!(&value.event, OutputEvent::RunFinished(_)) =>
                {
                    Some(value.id)
                }
                _ => None,
            })
            .ok_or_else(|| "no final output effect is pending".to_owned())?;
        let state = self
            .state
            .take()
            .ok_or_else(|| "final output state is unavailable".to_owned())?;
        let (state, emitted) = transition(
            state,
            RunEvent::OutputEmitted(OutputEmitted { id: final_id }),
        )
        .map_err(|error| format!("cannot acknowledge RunFinished: {error}"))?;
        if !emitted.is_empty() {
            return Err(format!(
                "RunFinished acknowledgement emitted unexpected effects: {emitted:?}"
            ));
        }
        self.outstanding.retain(|effect| effect.id() != final_id);
        self.state = Some(state);
        Ok(())
    }
}

fn normalize_effect(effect: &RunEffect) -> Option<String> {
    match effect {
        RunEffect::Cleanup(_) => Some("cleanup".to_owned()),
        RunEffect::EmitOutput(value) if matches!(&value.event, OutputEvent::RunFinished(_)) => {
            Some("final_output".to_owned())
        }
        RunEffect::EmitOutput(value) if matches!(&value.event, OutputEvent::RunStarted(_)) => None,
        _ => Some("ordinary".to_owned()),
    }
}

fn pending_verification(
    effects: &[RunEffect],
) -> Result<(EffectId, hoimin_core::IntegrityCheckpoint), String> {
    effects
        .iter()
        .find_map(|effect| match effect {
            RunEffect::VerifyOriginals(value) => Some((value.id, value.checkpoint)),
            _ => None,
        })
        .ok_or_else(|| "setup did not emit VerifyOriginals".to_owned())
}

fn successful_process(id: EffectId) -> ProcessFinished {
    ProcessFinished {
        id,
        worker: Some(0),
        termination: ProcessTermination::Exit(0),
        output: OutputSpoolRef {
            token: "output".to_owned(),
            retained: 0,
            observed: 0,
        },
        elapsed: Duration::from_millis(5),
        resource_mode: ResourceMode::Hard,
    }
}

fn run_case(case: &OracleCase) -> CaseResult {
    let mut driver = match Driver::for_scenario(&case.scenario) {
        Ok(driver) => driver,
        Err(error) => {
            return CaseResult {
                id: case.id.clone(),
                mode: case.mode.clone(),
                class: CaseClass::InfrastructureError,
                expected: case.expected.iter().map(Observation::from).collect(),
                actual: Vec::new(),
                detail: Some(error),
            };
        }
    };
    let expected: Vec<_> = case.expected.iter().map(Observation::from).collect();
    let mut actual = Vec::new();
    for event in &case.schedule {
        match driver.apply(event) {
            Ok(observation) => actual.push(observation),
            Err(error) => {
                return CaseResult {
                    id: case.id.clone(),
                    mode: case.mode.clone(),
                    class: CaseClass::InfrastructureError,
                    expected,
                    actual,
                    detail: Some(error),
                };
            }
        }
    }
    let class = if actual == expected {
        CaseClass::Match
    } else {
        CaseClass::Mismatch
    };
    let detail = (class == CaseClass::Mismatch).then(|| observation_diff(&expected, &actual));
    CaseResult {
        id: case.id.clone(),
        mode: case.mode.clone(),
        class,
        expected,
        actual,
        detail,
    }
}

fn observation_diff(expected: &[Observation], actual: &[Observation]) -> String {
    let mut differences = Vec::new();
    if expected.len() != actual.len() {
        differences.push(format!(
            "length: expected {}, actual {}",
            expected.len(),
            actual.len()
        ));
    }
    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        if expected.verdict != actual.verdict {
            differences.push(format!(
                "step {index} verdict: expected {:?}, actual {:?}",
                expected.verdict, actual.verdict
            ));
        }
        if expected.error_code != actual.error_code {
            differences.push(format!(
                "step {index} error_code: expected {:?}, actual {:?}",
                expected.error_code, actual.error_code
            ));
        }
        if expected.phase != actual.phase {
            differences.push(format!(
                "step {index} phase: expected {:?}, actual {:?}",
                expected.phase, actual.phase
            ));
        }
        if expected.emitted != actual.emitted {
            differences.push(format!(
                "step {index} emitted: expected {:?}, actual {:?}",
                expected.emitted, actual.emitted
            ));
        }
        if expected.pending != actual.pending {
            differences.push(format!(
                "step {index} pending: expected {}, actual {}",
                expected.pending, actual.pending
            ));
        }
    }
    differences.join("; ")
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn isolate_case(case: &OracleCase, execute: impl FnOnce() -> CaseResult) -> CaseResult {
    match catch_unwind(AssertUnwindSafe(execute)) {
        Ok(result) => result,
        Err(payload) => CaseResult {
            id: case.id.clone(),
            mode: case.mode.clone(),
            class: CaseClass::InfrastructureError,
            expected: case.expected.iter().map(Observation::from).collect(),
            actual: Vec::new(),
            detail: Some(format!(
                "case panicked: {}",
                panic_message(payload.as_ref())
            )),
        },
    }
}

fn run_case_isolated(case: &OracleCase) -> CaseResult {
    isolate_case(case, || run_case(case))
}

fn corpus_text() -> &'static str {
    include_str!("../../../formal/HoiminOracle/corpus/state-machine.jsonl")
}

fn known_event(value: &str) -> bool {
    matches!(
        value,
        "complete_unknown"
            | "complete_wrong_cleanup"
            | "complete_ordinary"
            | "complete_ordinary_again"
            | "complete_retired_ordinary"
            | "complete_cleanup"
            | "cancel"
            | "deadline"
    )
}

fn validate_case(case: &OracleCase) -> Result<(), String> {
    if case.schema != 1 {
        return Err(format!("unsupported schema {}", case.schema));
    }
    if !matches!(case.mode.as_str(), "report" | "strict") {
        return Err(format!("unknown mode {}", case.mode));
    }
    if !matches!(
        case.scenario.as_str(),
        "pending_resolve"
            | "normal_cleaning_with_copy"
            | "final_pending_without_copy"
            | "finished_without_copy"
    ) {
        return Err(format!("unknown scenario {}", case.scenario));
    }
    if case.schedule.is_empty() || case.expected.is_empty() {
        return Err(format!("case {} has no executable steps", case.id));
    }
    if case.schedule.len() != case.expected.len() {
        return Err(format!(
            "case {} has {} scheduled events but {} observations",
            case.id,
            case.schedule.len(),
            case.expected.len()
        ));
    }
    for (event, expected) in case.schedule.iter().zip(&case.expected) {
        if !known_event(event) {
            return Err(format!("unknown event {event}"));
        }
        if expected.event != *event {
            return Err(format!(
                "case {} observation event {} does not match schedule {event}",
                case.id, expected.event
            ));
        }
        if !matches!(expected.verdict.as_str(), "accepted" | "rejected") {
            return Err(format!("unknown verdict {}", expected.verdict));
        }
        if !matches!(
            expected.phase.as_str(),
            "running" | "cleaning" | "final_pending" | "finished"
        ) {
            return Err(format!("unknown phase {}", expected.phase));
        }
        for effect in &expected.emitted {
            if !matches!(effect.as_str(), "ordinary" | "cleanup" | "final_output") {
                return Err(format!("unknown emitted effect {effect}"));
            }
        }
        if let Some(code) = expected.error_code.as_deref()
            && !matches!(
                code,
                "machine.effect.unknown"
                    | "machine.effect.duplicate"
                    | "machine.effect.retired"
                    | "machine.effect.wrong_completion"
            )
        {
            return Err(format!("unknown error code {code}"));
        }
    }
    Ok(())
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let case: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&case)?;
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id {}", case.id));
        }
        cases.push(case);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn load_corpus() -> Result<Vec<OracleCase>, String> {
    parse_corpus(corpus_text())
}

#[test]
fn corpus_is_well_formed() {
    let cases = load_corpus().expect("the committed Lean corpus must parse");

    assert_eq!(cases.len(), 16);
    assert!(cases.iter().all(|case| !case.expected.is_empty()));
    assert!(cases.iter().all(|case| case.mode == "strict"));
}

#[test]
fn case_panics_are_classified_as_infrastructure_errors() {
    let case = load_corpus().unwrap().into_iter().next().unwrap();

    let result = isolate_case(&case, || panic!("deliberate oracle panic"));

    assert_eq!(result.class, CaseClass::InfrastructureError);
    assert_eq!(
        result.detail.as_deref(),
        Some("case panicked: deliberate oracle panic")
    );
}

#[test]
fn corpus_rejects_an_unknown_schema() {
    let invalid = corpus_text().replacen("\"schema\":1", "\"schema\":2", 1);
    let error = parse_corpus(&invalid).unwrap_err();

    assert!(error.contains("unsupported schema 2"), "{error}");
}

#[test]
fn corpus_rejects_duplicate_case_ids() {
    let first = corpus_text().lines().next().unwrap();
    let invalid = format!("{first}\n{first}\n");
    let error = parse_corpus(&invalid).unwrap_err();

    assert!(
        error.contains("duplicate case id unknown_completion_is_rejected"),
        "{error}"
    );
}

#[test]
fn corpus_rejects_unknown_modes_scenarios_events_and_observations() {
    for (from, to, expected) in [
        (
            "\"mode\":\"strict\"",
            "\"mode\":\"future\"",
            "unknown mode future",
        ),
        (
            "\"scenario\":\"pending_resolve\"",
            "\"scenario\":\"future\"",
            "unknown scenario future",
        ),
        (
            "\"complete_unknown\"",
            "\"future_event\"",
            "unknown event future_event",
        ),
        (
            "\"phase\":\"running\"",
            "\"phase\":\"future\"",
            "unknown phase future",
        ),
        (
            "\"emitted\":[]",
            "\"emitted\":[\"future\"]",
            "unknown emitted effect future",
        ),
    ] {
        let invalid = if expected == "unknown event future_event" {
            corpus_text().replace(from, to)
        } else {
            corpus_text().replacen(from, to, 1)
        };
        let error = parse_corpus(&invalid).unwrap_err();
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error:?}"
        );
    }
}

#[test]
fn oracle_correspondence() {
    let cases = load_corpus().unwrap();
    let selected = std::env::var("HOIMIN_ORACLE_CASE").ok();
    let results: Vec<_> = cases
        .iter()
        .filter(|case| selected.as_ref().is_none_or(|id| id == &case.id))
        .map(run_case_isolated)
        .collect();

    assert!(
        !results.is_empty(),
        "the case filter selected no corpus case"
    );
    for result in &results {
        println!(
            "case={} class={:?} expected={:?} actual={:?} detail={:?}",
            result.id, result.class, result.expected, result.actual, result.detail
        );
    }

    assert!(
        results
            .iter()
            .all(|result| result.class != CaseClass::InfrastructureError),
        "{results:#?}"
    );
    assert!(
        results
            .iter()
            .all(|result| result.mode != "strict" || result.class == CaseClass::Match),
        "strict Lean oracle mismatch: {results:#?}"
    );
}
