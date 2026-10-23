use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    ByteSpan, EffectId, EmitOutput, MutantFinished, MutationCandidate, MutationSummary,
    OutputEvent, OutputFormat, ProcessTermination, ResourceMode, RunStarted, RunSummary,
};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;

use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

static MEASUREMENT: Mutex<()> = Mutex::new(());

#[test]
fn json_heap_peak_is_independent_of_mutant_count() {
    let _guard = MEASUREMENT.lock().unwrap();
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
            OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
        );
        for index in 0..mutants {
            emit(&mut handler, mutant_finished(index + 2, index));
        }
        emit(
            &mut handler,
            OutputEvent::RunFinished(RunSummary {
                resolved_mutant_timeout: None,
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
        outcome: hoimin_core::MutationOutcome::Executed {
            termination: ProcessTermination::Exit(1),
            output_state: hoimin_core::ProcessOutputState::Complete,
            output: hoimin_core::OutputSpoolRef {
                token: "fixture-output".into(),
                retained: 0,
                observed: 0,
            },
            elapsed: std::time::Duration::from_millis(2),
        },
        resource_mode: ResourceMode::Hard,
        diagnostics: Vec::new(),
    })
}

fn test_resource_control() -> hoimin_core::ResourceControl {
    hoimin_core::ResourceControl {
        mode: hoimin_core::ResourceMode::Hard,
        mechanism: "test_supplied_hard".into(),
    }
}

#[derive(Default)]
struct SpoolCounts {
    writes: AtomicUsize,
    bytes: AtomicUsize,
    flushes: AtomicUsize,
}

struct DiscardSpool(Arc<SpoolCounts>);

impl Write for DiscardSpool {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.writes.fetch_add(1, Ordering::Relaxed);
        self.0.bytes.fetch_add(bytes.len(), Ordering::Relaxed);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flushes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

impl Read for DiscardSpool {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("measurement must not finalize the discarded report")
    }
}

impl Seek for DiscardSpool {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        panic!("measurement must not finalize the discarded report")
    }
}

#[test]
fn json_record_heap_is_bounded_for_large_candidate_payloads() {
    let _guard = MEASUREMENT.lock().unwrap();
    let mut measurements = Vec::new();
    for count in [16, 64] {
        for payload_bytes in [1, 4 * 1024, 16 * 1024, 64 * 1024] {
            let counts = Arc::new(SpoolCounts::default());
            let mut handler = ReportHandler::with_mutant_spool(
                OutputFormat::Json,
                io::sink(),
                io::sink(),
                DiscardSpool(Arc::clone(&counts)),
            );
            emit(
                &mut handler,
                OutputEvent::RunStarted(RunStarted::minimal("run-1", 1, test_resource_control())),
            );
            let mut peak = 0;
            let mut expected_bytes = count - 1; // commas between records
            for index in 0..count {
                let mut event = mutant_finished(index as u64 + 2, index as u64);
                let OutputEvent::MutantFinished(mutant) = &mut event else {
                    unreachable!()
                };
                mutant.candidate.original = "a".repeat(payload_bytes);
                mutant.candidate.replacement = "b".repeat(payload_bytes);
                expected_bytes += serde_json::to_vec(&event).unwrap().len();
                // Payload, expected JSON and handler setup are outside the window.
                // Reset per event so consumed payloads do not hide later allocations.
                let request = EmitOutput {
                    id: EffectId(event.sequence()),
                    event,
                };
                heap_tracking::begin();
                let result = handler.handle(request);
                let event_peak = heap_tracking::finish();
                result.unwrap();
                peak = peak.max(event_peak);
            }
            let writes = counts.writes.load(Ordering::Relaxed);
            let bytes = counts.bytes.load(Ordering::Relaxed);
            let flushes = counts.flushes.load(Ordering::Relaxed);
            assert_eq!(bytes, expected_bytes);
            if payload_bytes == 1 {
                assert_eq!(writes, count);
            }
            eprintln!(
                "record measurement: count={count} payload_bytes={payload_bytes} peak={peak} writes={writes} bytes={bytes} flushes={flushes}"
            );
            measurements.push((count, payload_bytes, peak));
        }
    }
    for (count, payload, peak) in measurements {
        assert!(
            peak <= 16 * 1024,
            "count={count}, payload={payload} bytes per string: record allocated {peak} bytes"
        );
    }
}
