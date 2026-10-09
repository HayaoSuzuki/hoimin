use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/report-sequence.jsonl");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    premise: String,
    prefix: Vec<OracleEvent>,
    target: OracleEvent,
    expected: Expected,
    probe: Option<OracleEvent>,
    probe_expected: Option<Expected>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleEvent {
    kind: String,
    run_id: String,
    sequence: u64,
    mutant_id: Option<String>,
    mutant_sequence: Option<u64>,
    status: Option<String>,
    termination: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Expected {
    accepted: bool,
    error_code: Option<String>,
    error_fields: BTreeMap<String, String>,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&item)?;
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        cases.push(item);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1 || item.id.is_empty() || item.premise.is_empty() {
        return Err(format!("{} has invalid schema or identity", item.id));
    }
    if !matches!(item.mode.as_str(), "strict" | "model-only") {
        return Err(format!("{} has unknown mode {}", item.id, item.mode));
    }
    if item.probe.is_some() != item.probe_expected.is_some() {
        return Err(format!("{} has unpaired probe fields", item.id));
    }
    for event in item
        .prefix
        .iter()
        .chain([&item.target])
        .chain(item.probe.iter())
    {
        validate_event(&item.id, event)?;
    }
    validate_expected(&item.id, &item.expected)?;
    if let Some(expected) = &item.probe_expected {
        validate_expected(&item.id, expected)?;
    }
    Ok(())
}

fn validate_event(id: &str, event: &OracleEvent) -> Result<(), String> {
    if !matches!(event.run_id.as_str(), "first" | "second") || event.sequence > 4 {
        return Err(format!("{id} has invalid run identity or sequence"));
    }
    let mutant_shape = event.mutant_id.is_some() && event.mutant_sequence.is_some();
    let finish_shape = mutant_shape && event.status.is_some();
    let valid = match event.kind.as_str() {
        "run_started" | "diagnostic" | "run_finished" => {
            !mutant_shape && event.status.is_none() && event.termination.is_none()
        }
        "mutant_started" => mutant_shape && event.status.is_none() && event.termination.is_none(),
        "mutant_finished" => finish_shape,
        _ => false,
    };
    if !valid
        || event
            .mutant_id
            .as_deref()
            .is_some_and(|value| !matches!(value, "alpha" | "beta"))
        || event.mutant_sequence.is_some_and(|value| value > 1)
        || event.status.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "survived"
                    | "killed"
                    | "timeout"
                    | "out_of_memory"
                    | "process_limit"
                    | "error"
                    | "not_run"
            )
        })
        || event.termination.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "exit_zero"
                    | "exit_nonzero"
                    | "timeout"
                    | "out_of_memory"
                    | "process_limit"
                    | "cancelled"
            )
        })
    {
        return Err(format!("{id} has invalid event {event:?}"));
    }
    Ok(())
}

fn validate_expected(id: &str, expected: &Expected) -> Result<(), String> {
    let required: &[&str] = match expected.error_code.as_deref() {
        None if expected.accepted => &[],
        Some("report.sequence.run_not_started") if !expected.accepted => &[],
        Some("report.sequence.run_already_started" | "report.sequence.run_already_finished")
            if !expected.accepted =>
        {
            &["run_id"]
        }
        Some("report.sequence.run_finished_with_active_mutants") if !expected.accepted => {
            &["count"]
        }
        Some("report.sequence.run_id_mismatch") if !expected.accepted => &["expected", "received"],
        Some("report.sequence.not_monotonic") if !expected.accepted => &["previous", "received"],
        Some(
            "report.sequence.mutant_already_started"
            | "report.sequence.duplicate_mutant_identity"
            | "report.sequence.mutant_not_started",
        ) if !expected.accepted => &["mutant_id", "mutant_sequence"],
        Some("report.sequence.mutant_identity_sequence_mismatch") if !expected.accepted => {
            &["mutant_id", "expected_sequence", "received_sequence"]
        }
        Some("report.sequence.mutant_status_termination_mismatch") if !expected.accepted => &[
            "mutant_id",
            "mutant_sequence",
            "status",
            "expected_status",
            "termination",
        ],
        _ => {
            return Err(format!(
                "{id} has inconsistent expected result {expected:?}"
            ));
        }
    };
    let actual = expected
        .error_fields
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let required = required.iter().copied().collect::<BTreeSet<_>>();
    if actual != required {
        return Err(format!("{id} has invalid error fields {actual:?}"));
    }
    Ok(())
}

#[test]
fn corpus_is_closed_and_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean corpus must satisfy the adapter schema");
    assert_eq!(cases.len(), 23);
}

#[test]
fn corpus_rejects_unknown_schema_mode_and_fields() {
    let mut row: serde_json::Value = serde_json::from_str(CORPUS.lines().next().unwrap()).unwrap();
    row["schema"] = json!(2);
    assert!(parse_corpus(&format!("{row}\n")).is_err());
    row["schema"] = json!(1);
    row["mode"] = json!("report");
    assert!(parse_corpus(&format!("{row}\n")).is_err());
    row["mode"] = json!("model-only");
    row["unexpected"] = json!(true);
    assert!(parse_corpus(&format!("{row}\n")).is_err());
}

#[test]
fn corpus_rejects_duplicate_ids_and_unpaired_probes() {
    let row = CORPUS.lines().next().unwrap();
    assert!(parse_corpus(&format!("{row}\n{row}\n")).is_err());
    let mut value: serde_json::Value = serde_json::from_str(row).unwrap();
    value["probe_expected"] = serde_json::Value::Null;
    assert!(parse_corpus(&format!("{value}\n")).is_err());
}

#[test]
fn corpus_rejects_unknown_or_missing_error_fields() {
    let row = CORPUS
        .lines()
        .find(|line| line.contains("rejects_cross_run"))
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(row).unwrap();
    value["expected"]["error_fields"]["extra"] = json!("x");
    assert!(parse_corpus(&format!("{value}\n")).is_err());
    value["expected"]["error_fields"]
        .as_object_mut()
        .unwrap()
        .remove("extra");
    value["expected"]["error_fields"]
        .as_object_mut()
        .unwrap()
        .remove("expected");
    assert!(parse_corpus(&format!("{value}\n")).is_err());
}

#[cfg(not(feature = "contracts"))]
mod correspondence {
    use super::*;
    use hoimin_core::{
        ByteSpan, Diagnostic, MutantFinished, MutantStarted, MutationCandidate, MutationStatus,
        MutationSummary, OutputEvent, ProcessTermination, ReportSequence, ReportSequenceError,
        ResourceMode, RunStarted, RunSummary,
    };
    use std::panic::{AssertUnwindSafe, catch_unwind};

    fn run_id(value: &str) -> Result<&'static str, String> {
        match value {
            "first" => Ok("run-first"),
            "second" => Ok("run-second"),
            _ => Err(format!("unknown run role {value}")),
        }
    }

    fn mutant_id(value: &str) -> Result<&'static str, String> {
        match value {
            "alpha" => Ok("mutant-alpha"),
            "beta" => Ok("mutant-beta"),
            _ => Err(format!("unknown mutant role {value}")),
        }
    }

    fn status(value: &str) -> Result<MutationStatus, String> {
        match value {
            "survived" => Ok(MutationStatus::Survived),
            "killed" => Ok(MutationStatus::Killed),
            "timeout" => Ok(MutationStatus::Timeout),
            "out_of_memory" => Ok(MutationStatus::OutOfMemory),
            "process_limit" => Ok(MutationStatus::ProcessLimit),
            "error" => Ok(MutationStatus::Error),
            "not_run" => Ok(MutationStatus::NotRun),
            _ => Err(format!("unknown status {value}")),
        }
    }

    fn termination(value: &str) -> Result<ProcessTermination, String> {
        match value {
            "exit_zero" => Ok(ProcessTermination::Exit(0)),
            "exit_nonzero" => Ok(ProcessTermination::Exit(7)),
            "timeout" => Ok(ProcessTermination::Timeout),
            "out_of_memory" => Ok(ProcessTermination::OutOfMemory),
            "process_limit" => Ok(ProcessTermination::ProcessLimit),
            "cancelled" => Ok(ProcessTermination::Cancelled),
            _ => Err(format!("unknown termination {value}")),
        }
    }

    fn candidate(id: &str, sequence: u64) -> MutationCandidate {
        MutationCandidate {
            id: id.to_owned(),
            sequence,
            path: "src/example.py".into(),
            span: ByteSpan {
                start: 0,
                length: 4,
            },
            original: "True".to_owned(),
            replacement: "False".to_owned(),
            operator: "boolean_literal".to_owned(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: "oracle-hash".to_owned(),
        }
    }

    fn to_output_event(event: &OracleEvent) -> Result<OutputEvent, String> {
        let run_id = run_id(&event.run_id)?;
        Ok(match event.kind.as_str() {
            "run_started" => OutputEvent::RunStarted(RunStarted::minimal(
                run_id,
                event.sequence,
                test_resource_control(),
            )),
            "diagnostic" => OutputEvent::Diagnostic(Diagnostic::new(
                run_id,
                event.sequence,
                "warning",
                "oracle",
                "oracle diagnostic",
            )),
            "mutant_started" => {
                let id = mutant_id(event.mutant_id.as_deref().ok_or("missing mutant_id")?)?;
                OutputEvent::MutantStarted(MutantStarted::new(
                    run_id,
                    event.sequence,
                    id,
                    event.mutant_sequence.ok_or("missing mutant_sequence")?,
                ))
            }
            "mutant_finished" => {
                let id = mutant_id(event.mutant_id.as_deref().ok_or("missing mutant_id")?)?;
                let mutant_sequence = event.mutant_sequence.ok_or("missing mutant_sequence")?;
                OutputEvent::MutantFinished(MutantFinished {
                    schema_version: 2,
                    sequence: event.sequence,
                    run_id: run_id.to_owned(),
                    candidate: candidate(id, mutant_sequence),
                    status: status(event.status.as_deref().ok_or("missing status")?)?,
                    termination: event.termination.as_deref().map(termination).transpose()?,
                    output_state: hoimin_core::ProcessOutputState::Complete,
                    elapsed_ms: 1,
                    resource_mode: ResourceMode::Hard,
                    output: None,
                    diagnostics: Vec::new(),
                })
            }
            "run_finished" => OutputEvent::RunFinished(RunSummary {
                schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
                sequence: event.sequence,
                run_id: run_id.to_owned(),
                counts: MutationSummary::default(),
                complete: true,
                exit_code: 0,
                disk: hoimin_core::DiskRunSummary::unmeasured(8, 10),
                verification_selection: None,
            }),
            value => return Err(format!("unknown event kind {value}")),
        })
    }

    fn normalize_run_id(value: &str) -> Result<String, String> {
        match value {
            "run-first" => Ok("first".to_owned()),
            "run-second" => Ok("second".to_owned()),
            _ => Err(format!("unknown observed run id {value}")),
        }
    }

    fn normalize_mutant_id(value: &str) -> Result<String, String> {
        match value {
            "mutant-alpha" => Ok("alpha".to_owned()),
            "mutant-beta" => Ok("beta".to_owned()),
            _ => Err(format!("unknown observed mutant id {value}")),
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive protocol-error mapping stays in one oracle adapter"
    )]
    fn observed_error(error: &ReportSequenceError) -> Result<Expected, String> {
        let (code, fields) = match error {
            ReportSequenceError::RunNotStarted => ("report.sequence.run_not_started", vec![]),
            ReportSequenceError::RunAlreadyStarted { run_id } => (
                "report.sequence.run_already_started",
                vec![("run_id", normalize_run_id(run_id)?)],
            ),
            ReportSequenceError::RunAlreadyFinished { run_id } => (
                "report.sequence.run_already_finished",
                vec![("run_id", normalize_run_id(run_id)?)],
            ),
            ReportSequenceError::RunFinishedWithActiveMutants { count } => (
                "report.sequence.run_finished_with_active_mutants",
                vec![("count", count.to_string())],
            ),
            ReportSequenceError::RunIdMismatch { expected, received } => (
                "report.sequence.run_id_mismatch",
                vec![
                    ("expected", normalize_run_id(expected)?),
                    ("received", normalize_run_id(received)?),
                ],
            ),
            ReportSequenceError::NotMonotonic { previous, received } => (
                "report.sequence.not_monotonic",
                vec![
                    ("previous", previous.to_string()),
                    ("received", received.to_string()),
                ],
            ),
            ReportSequenceError::MutantAlreadyStarted {
                mutant_id,
                mutant_sequence,
            } => (
                "report.sequence.mutant_already_started",
                mutant_fields(mutant_id, *mutant_sequence)?,
            ),
            ReportSequenceError::DuplicateMutantIdentity {
                mutant_id,
                mutant_sequence,
            } => (
                "report.sequence.duplicate_mutant_identity",
                mutant_fields(mutant_id, *mutant_sequence)?,
            ),
            ReportSequenceError::MutantIdentitySequenceMismatch {
                mutant_id,
                expected_sequence,
                received_sequence,
            } => (
                "report.sequence.mutant_identity_sequence_mismatch",
                vec![
                    ("mutant_id", normalize_mutant_id(mutant_id)?),
                    ("expected_sequence", expected_sequence.to_string()),
                    ("received_sequence", received_sequence.to_string()),
                ],
            ),
            ReportSequenceError::MutantNotStarted {
                mutant_id,
                mutant_sequence,
            } => (
                "report.sequence.mutant_not_started",
                mutant_fields(mutant_id, *mutant_sequence)?,
            ),
            ReportSequenceError::MutantStatusTerminationMismatch {
                mutant_id,
                mutant_sequence,
                status: actual,
                termination: actual_termination,
                expected_status,
            } => (
                "report.sequence.mutant_status_termination_mismatch",
                vec![
                    ("mutant_id", normalize_mutant_id(mutant_id)?),
                    ("mutant_sequence", mutant_sequence.to_string()),
                    ("status", status_name(*actual).to_owned()),
                    ("expected_status", status_name(*expected_status).to_owned()),
                    (
                        "termination",
                        termination_name(*actual_termination).to_owned(),
                    ),
                ],
            ),
            ReportSequenceError::MutantOutputDiagnosticMismatch {
                mutant_id,
                mutant_sequence,
                output_state,
            } => (
                "report.sequence.mutant_output_diagnostic_mismatch",
                vec![
                    ("mutant_id", normalize_mutant_id(mutant_id)?),
                    ("mutant_sequence", mutant_sequence.to_string()),
                    (
                        "output_state",
                        match output_state {
                            hoimin_core::ProcessOutputState::Complete => "complete",
                            hoimin_core::ProcessOutputState::CloseTimedOut => "close_timed_out",
                        }
                        .to_owned(),
                    ),
                ],
            ),
            ReportSequenceError::MutantOutputStateMismatch {
                mutant_id,
                mutant_sequence,
                output_state,
            } => (
                "report.sequence.mutant_output_state_mismatch",
                vec![
                    ("mutant_id", normalize_mutant_id(mutant_id)?),
                    ("mutant_sequence", mutant_sequence.to_string()),
                    (
                        "output_state",
                        match output_state {
                            hoimin_core::ProcessOutputState::Complete => "complete",
                            hoimin_core::ProcessOutputState::CloseTimedOut => "close_timed_out",
                        }
                        .to_owned(),
                    ),
                ],
            ),
        };
        Ok(Expected {
            accepted: false,
            error_code: Some(code.to_owned()),
            error_fields: fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        })
    }

    fn mutant_fields(id: &str, sequence: u64) -> Result<Vec<(&'static str, String)>, String> {
        Ok(vec![
            ("mutant_id", normalize_mutant_id(id)?),
            ("mutant_sequence", sequence.to_string()),
        ])
    }

    fn status_name(value: MutationStatus) -> &'static str {
        match value {
            MutationStatus::Survived => "survived",
            MutationStatus::Killed => "killed",
            MutationStatus::Timeout => "timeout",
            MutationStatus::OutOfMemory => "out_of_memory",
            MutationStatus::ProcessLimit => "process_limit",
            MutationStatus::Error => "error",
            MutationStatus::NotRun => "not_run",
        }
    }

    fn termination_name(value: ProcessTermination) -> &'static str {
        match value {
            ProcessTermination::Exit(0) => "exit_zero",
            ProcessTermination::Exit(_) => "exit_nonzero",
            ProcessTermination::Timeout => "timeout",
            ProcessTermination::OutOfMemory => "out_of_memory",
            ProcessTermination::ProcessLimit => "process_limit",
            ProcessTermination::Cancelled => "cancelled",
        }
    }

    fn observe(sequence: &mut ReportSequence, event: &OracleEvent) -> Result<Expected, String> {
        let output = to_output_event(event)?;
        match sequence.observe(&output) {
            Ok(()) => Ok(Expected {
                accepted: true,
                error_code: None,
                error_fields: BTreeMap::new(),
            }),
            Err(error) => observed_error(&error),
        }
    }

    fn replay(item: &OracleCase) -> Result<(), String> {
        let mut sequence = ReportSequence::new();
        for event in &item.prefix {
            let observed = observe(&mut sequence, event)?;
            if !observed.accepted {
                return Err(format!("{} prefix rejected: {observed:?}", item.id));
            }
        }
        let actual = observe(&mut sequence, &item.target)?;
        if actual != item.expected {
            return Err(format!(
                "{} target mismatch: expected {:?}, actual {actual:?}",
                item.id, item.expected
            ));
        }
        if let (Some(probe), Some(expected)) = (&item.probe, &item.probe_expected) {
            let actual = observe(&mut sequence, probe)?;
            if &actual != expected {
                return Err(format!(
                    "{} probe mismatch: expected {expected:?}, actual {actual:?}",
                    item.id
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn strict_public_report_sequence_observations_match_lean() {
        let cases = parse_corpus(CORPUS).expect("valid Lean corpus");
        let selected = std::env::var("HOIMIN_REPORT_SEQUENCE_CASE").ok();
        let mut exercised = 0;
        for item in cases
            .iter()
            .filter(|item| selected.as_ref().is_none_or(|id| id == &item.id))
        {
            exercised += 1;
            let result = catch_unwind(AssertUnwindSafe(|| replay(item)))
                .map_err(|_| format!("{} panicked: infrastructure-error", item.id))
                .and_then(|result| result);
            if let Err(error) = result {
                panic!("mode={}; {error}", item.mode);
            }
        }
        assert!(exercised > 0, "case filter selected no corpus row");
    }
}

#[cfg(not(feature = "contracts"))]
fn test_resource_control() -> hoimin_core::ResourceControl {
    hoimin_core::ResourceControl {
        mode: hoimin_core::ResourceMode::Hard,
        mechanism: "test_supplied_hard".into(),
    }
}
