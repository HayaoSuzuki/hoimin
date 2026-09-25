use std::io;

use hoimin_cli::report::ReportHandler;
use hoimin_core::{Diagnostic, EffectId, EmitOutput, OutputEvent, OutputFormat};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;

#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

#[test]
fn diagnostic_serialization_memory_does_not_scale_with_escaped_message_size() {
    let spool = tempfile::tempdir().unwrap();
    for format in [OutputFormat::Json, OutputFormat::Jsonl] {
        let mut peaks = Vec::new();
        for length in [16 * 1024, 1024 * 1024] {
            // Input ownership exists before measurement; only serialization work
            // is counted. The output sink does not retain the serialized event.
            let event = OutputEvent::Diagnostic(Diagnostic::new(
                "run",
                1,
                "error",
                "baseline.output",
                "x\n".repeat(length / 2),
            ));
            let mut handler =
                ReportHandler::new(format, io::sink(), io::sink(), spool.path()).unwrap();
            heap_tracking::begin();
            let result = handler.handle(EmitOutput {
                id: EffectId(9),
                event,
            });
            let peak = heap_tracking::finish();
            assert_eq!(result.unwrap().id, EffectId(9));
            eprintln!("diagnostic heap format={format:?} message={length} peak={peak}");
            assert!(peak <= 32 * 1024, "whole diagnostic retained: {peak}");
            peaks.push(peak);
        }
        assert!(
            peaks[1] <= peaks[0] + 8 * 1024,
            "heap grew with message: {peaks:?}"
        );
    }
}
