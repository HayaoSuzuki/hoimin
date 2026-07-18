use std::alloc::{GlobalAlloc, Layout, System};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    ByteSpan, EffectId, EmitOutput, MutantFinished, MutationCandidate, MutationStatus, OutputEvent,
    OutputFormat, ProcessTermination, ResourceMode, RunStarted,
};

struct TrackingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && TRACKING.load(Ordering::Relaxed) {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() && TRACKING.load(Ordering::Relaxed) {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if TRACKING.load(Ordering::Relaxed) {
            deallocated(layout.size());
        }
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() && TRACKING.load(Ordering::Relaxed) {
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
    let current = CURRENT.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(current, Ordering::Relaxed);
}

fn deallocated(bytes: usize) {
    let _ = CURRENT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(bytes))
    });
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
    CURRENT.store(0, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    TRACKING.store(true, Ordering::Relaxed);

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
    }

    TRACKING.store(false, Ordering::Relaxed);
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
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: None,
    })
}
