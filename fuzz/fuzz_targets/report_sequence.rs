#![no_main]

use hoimin_core::{
    BaselineFinished, ByteSpan, Diagnostic, DiskRunSummary, MutantFinished, MutantStarted,
    MutationCandidate, MutationStatus, OutputEvent, OutputSpoolRef, ProcessOutputState,
    ProcessTermination, ReportSequence, ResourceControl, ResourceMode, RunStarted, RunSummary,
    classify_mutant, summarize,
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
    let status = classify_mutant(termination);
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
            status,
            termination: Some(termination),
            output_state: ProcessOutputState::Complete,
            elapsed_ms: 0,
            resource_mode,
            output: None,
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

    // A rejected result must not consume its sequence number or active mutant.
    let mut recoverable = ReportSequence::new();
    recoverable.observe(&events[0]).unwrap();
    recoverable.observe(&events[3]).unwrap();
    let OutputEvent::MutantFinished(good) = &events[4] else {
        unreachable!();
    };
    let mut bad = good.clone();
    bad.status = MutationStatus::Error;
    assert!(
        recoverable
            .validate(&OutputEvent::MutantFinished(bad))
            .is_err()
    );
    recoverable.observe(&events[4]).unwrap();
});
