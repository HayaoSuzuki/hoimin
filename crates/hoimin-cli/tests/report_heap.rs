use std::alloc::{GlobalAlloc, Layout, System};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    ByteSpan, EffectId, EmitOutput, MutantFinished, MutationCandidate, MutationStatus,
    MutationSummary, OutputEvent, OutputFormat, ProcessTermination, ResourceMode, RunStarted,
    RunSummary,
};

struct TrackingAllocator;

static MEASURING: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static BASELINE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        deallocated(layout.size());
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            if new_size >= layout.size() {
                allocated(new_size - layout.size());
            } else {
                deallocated(layout.size() - new_size);
            }
        }
        new_pointer
    }
}

fn allocated(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    if MEASURING.load(Ordering::Relaxed) {
        let measured = live.saturating_sub(BASELINE.load(Ordering::Relaxed));
        PEAK.fetch_max(measured, Ordering::Relaxed);
    }
}

fn deallocated(bytes: usize) {
    LIVE.fetch_sub(bytes, Ordering::Relaxed);
}

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
    PEAK.store(0, Ordering::Relaxed);
    BASELINE.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    MEASURING.store(true, Ordering::Relaxed);

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

    MEASURING.store(false, Ordering::Relaxed);
    PEAK.load(Ordering::Relaxed)
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
