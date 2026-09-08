use std::io;

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    ByteSpan, EffectId, EmitOutput, MutantFinished, MutationCandidate, MutationStatus,
    MutationSummary, OutputEvent, OutputFormat, ProcessTermination, ResourceMode, RunStarted,
    RunSummary,
};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;

use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[test]
fn json_heap_peak_is_independent_of_mutant_count() {
    let small_peak = measured_peak(32);
    let large_peak = measured_peak(10_000);

    assert!(
        large_peak <= small_peak + 64 * 1024,
        "small peak={small_peak} bytes, large peak={large_peak} bytes"
    );
    assert!(
        large_peak <= 512 * 1024,
        "large report retained {large_peak} heap bytes"
    );
}

fn measured_peak(mutants: u64) -> usize {
    let spool = tempfile::tempdir().unwrap();
    heap_tracking::begin();

    {
        let mut handler =
            ReportHandler::new(OutputFormat::Json, io::sink(), io::sink(), spool.path()).unwrap();
        emit(
            &mut handler,
            OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
        );
        for index in 0..mutants {
            emit(&mut handler, mutant_finished(index + 2, index));
        }
        emit(
            &mut handler,
            OutputEvent::RunFinished(RunSummary {
                schema_version: 1,
                sequence: mutants + 2,
                run_id: "run-1".to_owned(),
                counts: MutationSummary {
                    killed: mutants,
                    score: Some(1.0),
                    ..MutationSummary::default()
                },
                complete: true,
                exit_code: 0,
                disk: hoimin_core::DiskRunSummary::unmeasured(8, 10),
                verification_selection: None,
            }),
        );
    }

    heap_tracking::finish()
}

fn emit(handler: &mut ReportHandler<impl io::Write, impl io::Write>, event: OutputEvent) {
    handler
        .handle(EmitOutput {
            id: EffectId(event.sequence()),
            event,
        })
        .unwrap();
}

fn mutant_finished(event_sequence: u64, mutant_sequence: u64) -> OutputEvent {
    OutputEvent::MutantFinished(MutantFinished {
        schema_version: 1,
        sequence: event_sequence,
        run_id: "run-1".to_owned(),
        candidate: MutationCandidate {
            id: format!("m{mutant_sequence}"),
            sequence: mutant_sequence,
            path: "src/example.py".into(),
            span: ByteSpan {
                start: mutant_sequence,
                length: 1,
            },
            original: "+".to_owned(),
            replacement: "-".to_owned(),
            operator: "binary".to_owned(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: "hash".to_owned(),
        },
        status: MutationStatus::Killed,
        termination: Some(ProcessTermination::Exit(1)),
        output_state: hoimin_core::ProcessOutputState::Complete,
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: None,
        diagnostics: Vec::new(),
    })
}
