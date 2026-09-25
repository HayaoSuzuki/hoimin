use hoimin_core::{CandidateValidationContext, decode_python_source};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// One test owns this process's allocation windows. Sources and diagnostic
// storage are prepared outside them; each source has three lines and one True.
#[test]
fn latin1_positions_do_not_retain_duplicate_coordinates() {
    let mut failures = Vec::new();
    for length in [65_536, 262_144, 524_256] {
        for stride in [0, 64, 2, 1] {
            let mut source = b"# coding: latin-1\n# ".to_vec();
            let mut expansions = 0usize;
            source.extend((0..length).map(|position| {
                if stride != 0 && position % stride == 0 {
                    expansions += 1;
                    0xe9
                } else {
                    b'a'
                }
            }));
            source.extend_from_slice(b"\nvalue = True\n");

            heap_tracking::begin();
            let decoded = decode_python_source(&source).unwrap();
            let decode_peak = heap_tracking::finish();
            assert_eq!(decoded.text().len(), source.len() + expansions);
            drop(decoded);

            heap_tracking::begin();
            let context = CandidateValidationContext::new(&source).unwrap();
            let context_peak = heap_tracking::finish();
            assert_eq!(
                context.decoded_source().unwrap().text().len(),
                source.len() + expansions
            );
            drop(context);

            // Allow geometric capacity rounding and small codec/hash metadata,
            // but only one machine-word position per non-ASCII character.
            let positions = if expansions == 0 {
                0
            } else {
                expansions.next_power_of_two()
            };
            let decode_bound = 2 * source.len() + positions * size_of::<usize>() + 1024;
            eprintln!(
                "latin1-heap length={length} stride={stride} decode={decode_peak} context={context_peak}"
            );
            if decode_peak > decode_bound || context_peak > decode_peak + 1024 {
                failures.push((length, stride, decode_peak, decode_bound, context_peak));
            }
        }
    }
    // The first bound detects duplicate decoder coordinates; the second
    // detects a separate per-character validation correction table.
    assert!(
        failures.is_empty(),
        "redundant Latin-1 indexes: {failures:?}"
    );

    for source in [
        b"# coding: ascii\n# plain\nvalue = True\n".as_slice(),
        b"# plain\nvalue = True\n",
    ] {
        heap_tracking::begin();
        let context = CandidateValidationContext::new(source).unwrap();
        let peak = heap_tracking::finish();
        assert!(peak < 1024, "ASCII unexpectedly retained an index: {peak}");
        drop(context);
    }
}
