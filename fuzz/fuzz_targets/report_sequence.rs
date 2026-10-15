#![no_main]

use hoimin_core::{
    BaselineFinished, ByteSpan, Diagnostic, DiskRunSummary, MutantFinished, MutantStarted,
    MutationCandidate, MutationOutcome, OutputEvent, OutputSpoolRef, ProcessOutputState,
    ProcessTermination, ReportSequence, ResourceControl, ResourceMode, ReusedMutationStatus,
    RunStarted, RunSummary, summarize,
};
use libfuzzer_sys::fuzz_target;

fn round_trip(event: &OutputEvent) {
    let encoded = serde_json::to_vec(event).unwrap();
    let decoded = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(*event, decoded);
}

fn candidate(id: String, sequence: u64, payload: &str) -> MutationCandidate {
    MutationCandidate {
        id,
        sequence,
        path: "input.py".into(),
        span: ByteSpan {
            start: 0,
            length: 0,
        },
        original: String::new(),
        replacement: payload.to_owned(),
        operator: "fuzz".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: "0".repeat(64),
    }
}

fn termination(selector: u8) -> ProcessTermination {
    match selector % 6 {
        0 => ProcessTermination::Exit(0),
        1 => ProcessTermination::Exit(1),
        2 => ProcessTermination::Timeout,
        3 => ProcessTermination::OutOfMemory,
        4 => ProcessTermination::ProcessLimit,
        _ => ProcessTermination::Cancelled,
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }

    // Exercise the public JSON event boundary before feeding accepted records
    // into the stateful sequence validator.
    let mut decoded_sequence = ReportSequence::new();
    for line in data
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let Ok(event) = serde_json::from_slice::<OutputEvent>(line) else {
            continue;
        };
        round_trip(&event);
        if decoded_sequence.validate(&event).is_ok() {
            decoded_sequence.observe(&event).unwrap();
            assert!(decoded_sequence.validate(&event).is_err());
        }
    }

    // Always construct a complete lifecycle so arbitrary non-JSON bytes still
    // reach every normal state transition and result classification.
    let payload = String::from_utf8_lossy(data);
    let run_id = format!("run-{payload}");
    let mutant_id = format!("mutant-{payload}");
    let resource_mode = if data.first().copied().unwrap_or(0) & 1 == 0 {
        ResourceMode::Hard
    } else {
        ResourceMode::BestEffort
    };
    let resource_control = ResourceControl {
        mode: resource_mode,
        mechanism: "fuzz".into(),
    };
    let termination = termination(data.get(1).copied().unwrap_or(0));
    let selector = data.get(2).copied().unwrap_or(0);
    let outcome = match selector % 4 {
        0 | 1 => MutationOutcome::Executed {
            termination,
            output_state: if selector & 1 == 0 {
                ProcessOutputState::Complete
            } else {
                ProcessOutputState::CloseTimedOut
            },
            output: OutputSpoolRef {
                token: "mutant".into(),
                retained: 0,
                observed: u64::try_from(data.len()).unwrap(),
            },
            elapsed: std::time::Duration::from_millis(u64::from(selector)),
        },
        2 => MutationOutcome::Reused {
            status: if data.first().copied().unwrap_or(0) & 1 == 0 {
                ReusedMutationStatus::Killed
            } else {
                ReusedMutationStatus::Survived
            },
        },
        _ => MutationOutcome::NotRun,
    };
    let status = outcome.status();
    let candidate = candidate(mutant_id.clone(), 1, &payload);
    let events = [
        OutputEvent::RunStarted(RunStarted::minimal(run_id.clone(), 1, resource_control)),
        OutputEvent::Diagnostic(Diagnostic::new(
            run_id.clone(),
            2,
            "info",
            "fuzz",
            payload.as_ref(),
        )),
        OutputEvent::BaselineFinished(BaselineFinished {
            schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
            sequence: 3,
            run_id: run_id.clone(),
            termination: ProcessTermination::Exit(0),
            elapsed_ms: u64::from(data.first().copied().unwrap_or(0)),
            resource_mode,
            output: OutputSpoolRef {
                token: "baseline".into(),
                retained: 0,
                observed: 0,
            },
        }),
        OutputEvent::MutantStarted(MutantStarted::new(run_id.clone(), 4, mutant_id, 1)),
        OutputEvent::MutantFinished(MutantFinished {
            schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
            sequence: 5,
            run_id: run_id.clone(),
            candidate,
            outcome,
            resource_mode,
            diagnostics: Vec::new(),
        }),
        OutputEvent::RunFinished(RunSummary {
            schema_version: hoimin_core::REPORT_SCHEMA_VERSION,
            sequence: 6,
            run_id: run_id.clone(),
            counts: summarize(&[status]),
            complete: true,
            exit_code: 0,
            disk: DiskRunSummary::unmeasured(0, 0),
            verification_selection: None,
        }),
    ];
    let mut valid_sequence = ReportSequence::new();
    for event in &events {
        round_trip(event);
        assert_eq!(valid_sequence.validate(event), Ok(()));
        valid_sequence.observe(event).unwrap();
    }
    assert!(
        valid_sequence
            .validate(&OutputEvent::Diagnostic(Diagnostic::new(
                run_id, 7, "info", "late", "late"
            )))
            .is_err()
    );

    // Malformed wire is rejected before it reaches lifecycle validation.
    let mut malformed = serde_json::to_value(&events[4]).unwrap();
    malformed["status"] = serde_json::json!("error");
    let encoded = serde_json::to_vec(&malformed).unwrap();
    assert!(serde_json::from_slice::<OutputEvent>(&encoded).is_err());
});
