#![cfg_attr(coverage_nightly, feature(coverage_attribute, stmt_expr_attributes))]
use hoimin_core::{
    ByteSpan, CANDIDATE_SCHEMA_VERSION, CandidateDescriptor, CandidateValidationContext,
    decode_python_source, validate_candidate_with_context,
};

// These decoding and indexing operations are synchronous; count their thread,
// excluding parallel libtest bookkeeping outside the whole-case lock.
#[path = "../../hoimin-core/tests/support/requested_heap.rs"]
mod heap_tracking;
use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// Each case owns this process's allocation window. Sources and diagnostic
// storage are prepared outside them; each source has three lines and one True.
static ALLOCATION_WINDOW: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[rstest::rstest]
#[case::small(65_536)]
#[case::medium(262_144)]
#[case::large(524_256)]
fn latin1_positions_do_not_retain_duplicate_coordinates(
    #[case] length: usize,
    #[values(0usize, 64usize, 2usize, 1usize)] stride: usize,
) {
    let _window = ALLOCATION_WINDOW.lock().unwrap();
    heap_tracking::self_check();
    let (source, expansions, positions) = latin1_fixture(length, stride);

    let window = heap_tracking::begin();

    let decoded = decode_python_source(&source).unwrap();
    let decoded_heap = heap_tracking::finish(window);
    let decode_peak = decoded_heap.peak_delta;
    eprintln!("decode retained={}", decoded_heap.retained_delta);

    let decoded_len = decoded.text().len();
    drop(decoded);

    let candidate = CandidateDescriptor {
        schema_version: CANDIDATE_SCHEMA_VERSION,
        path: "source.py".into(),
        span: ByteSpan {
            start: u64::try_from(source.len() - 5).unwrap(),
            length: 4,
        },
        original: "True".into(),
        replacement: "False".into(),
        operator: "boolean_literal".into(),
        line: 3,
        column: 8,
        symbol: None,
        file_hash: blake3::hash(&source).to_hex().to_string(),
    };
    let window = heap_tracking::begin();
    let context = CandidateValidationContext::new(&source).unwrap();
    validate_candidate_with_context(&context, &candidate).unwrap();
    let context_heap = heap_tracking::finish(window);
    let context_peak = context_heap.peak_delta;
    eprintln!("context retained={}", context_heap.retained_delta);
    let context_len = context.decoded_source().unwrap().text().len();
    drop(context);

    // Allow geometric capacity rounding and small codec/hash metadata,
    // but only one machine-word position per non-ASCII character.
    let decode_bound = 2 * source.len() + positions * size_of::<usize>() + 1024;
    eprintln!(
        "latin1-heap length={length} stride={stride} decode={decode_peak} context={context_peak}"
    );
    assert_eq!(decoded_len, source.len() + expansions);
    assert_eq!(context_len, source.len() + expansions);
    assert!(
        decode_peak <= decode_bound,
        "redundant Latin-1 decoder indexes: length={length} stride={stride} decode={decode_peak} bound={decode_bound}"
    );
    assert!(
        context_peak <= decode_peak + 1024,
        "redundant Latin-1 validation indexes: length={length} stride={stride} decode={decode_peak} context={context_peak}"
    );
}

#[rstest::rstest]
#[case::explicit_ascii(b"# coding: ascii\n# plain\nvalue = True\n".as_slice())]
#[case::implicit_utf8(b"# plain\nvalue = True\n".as_slice())]
fn ascii_source_does_not_retain_an_index(#[case] source: &[u8]) {
    let _window = ALLOCATION_WINDOW.lock().unwrap();
    heap_tracking::self_check();

    let window = heap_tracking::begin();
    let context = CandidateValidationContext::new(source).unwrap();
    let measured = heap_tracking::finish(window);
    let peak = measured.peak_delta;
    eprintln!("ASCII retained={}", measured.retained_delta);

    assert!(peak < 1024, "ASCII unexpectedly retained an index: {peak}");
    drop(context);
}

fn latin1_fixture(length: usize, stride: usize) -> (Vec<u8>, usize, usize) {
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
    let positions = if expansions == 0 {
        0
    } else {
        expansions.next_power_of_two()
    };

    (source, expansions, positions)
}
