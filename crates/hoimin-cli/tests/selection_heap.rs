//! Isolated allocator binary: fixture construction and expected IDs are outside
//! measurement. Compile the real private selection source with production types.
use std::alloc::{GlobalAlloc, Layout};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use camino::Utf8PathBuf;
use hoimin_cli::cli::{self, TopSelectionPolicy};
use hoimin_cli::plan::{RankedPlanCandidate, RankingReason, RankingReasonCode};
use hoimin_core::{ByteSpan, MutationCandidate};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[path = "../src/plan/selection.rs"]
mod selection;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);

fn count(pointer: *mut u8) {
    if !pointer.is_null() && COUNTING.load(Ordering::Relaxed) {
        CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { heap_tracking::TrackingAllocator.alloc(layout) };
        count(pointer);
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { heap_tracking::TrackingAllocator.alloc_zeroed(layout) };
        count(pointer);
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { heap_tracking::TrackingAllocator.realloc(pointer, layout, size) };
        count(pointer);
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { heap_tracking::TrackingAllocator.dealloc(pointer, layout) };
    }
}

fn begin() {
    heap_tracking::begin();
    CALLS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
}

fn finish() -> (usize, usize) {
    COUNTING.store(false, Ordering::Relaxed);
    (heap_tracking::finish(), CALLS.load(Ordering::Relaxed))
}

fn candidates() -> Vec<RankedPlanCandidate> {
    (0..1024)
        .map(|index| RankedPlanCandidate {
            candidate: MutationCandidate {
                id: format!("{index:04}{}", "x".repeat(4092)),
                sequence: index as u64 + 1,
                path: Utf8PathBuf::from(format!("file-{}.py", index / 128)),
                span: ByteSpan {
                    start: index as u64,
                    length: 1,
                },
                original: "x".into(),
                replacement: "y".into(),
                operator: "binary_add_sub".into(),
                line: u32::try_from(index).unwrap() + 1,
                column: 0,
                symbol: None,
                file_hash: "0".repeat(64),
            },
            rank: index + 1,
            score: 70,
            ranking_reasons: vec![RankingReason {
                code: RankingReasonCode::Arithmetic,
                score: 70,
            }],
        })
        .collect()
}

#[test]
fn offset_selection_does_not_clone_or_retain_the_discarded_prefix() {
    const MAX_PEAK: usize = 512 * 1024;
    let candidates = candidates();
    let one = NonZeroUsize::new(1).unwrap();
    let mut regressions = Vec::new();
    for policy in [
        TopSelectionPolicy::Strict,
        TopSelectionPolicy::Diverse,
        TopSelectionPolicy::LineDiverse,
    ] {
        let mut first_page_calls = None;
        for offset in [0, 512, 1023] {
            // Eight equally sized files: round robin row is offset / 8 and
            // column is offset % 8. These positions are independent of selection.
            let expected_index = match policy {
                TopSelectionPolicy::Strict | TopSelectionPolicy::LineDiverse => offset,
                TopSelectionPolicy::Diverse => (offset % 8) * 128 + offset / 8,
            };
            begin();
            let selected = selection::select_top_candidate_ids_at(&candidates, one, policy, offset);
            let (peak, calls) = finish();
            eprintln!(
                "{policy:?} offset={offset} peak={peak} calls={calls} capacity={}",
                selected.capacity()
            );
            assert_eq!(selected, [candidates[expected_index].id.clone()]);
            // Every page builds the same one score tier; discarding borrowed
            // IDs adds no allocator calls even when clones would be freed early.
            let expected_calls = *first_page_calls.get_or_insert(calls);
            if peak >= MAX_PEAK || selected.capacity() != 1 || calls != expected_calls {
                regressions.push(format!(
                    "{policy:?} offset={offset}: peak={peak} calls={calls} expected_calls={expected_calls} capacity={}",
                    selected.capacity()
                ));
            }
        }
        // Sensitivity: the old owned-prefix algorithm must exceed the same
        // bound even if someone later shrinks its returned vector.
        begin();
        let prefix = selection::select_top_candidate_ids(
            &candidates,
            NonZeroUsize::new(candidates.len()).unwrap(),
            policy,
        );
        let mut broken = prefix.into_iter().skip(1023).collect::<Vec<_>>();
        broken.shrink_to_fit();
        let (broken_peak, broken_calls) = finish();
        eprintln!("{policy:?} eager_prefix_peak={broken_peak}");
        assert_eq!(broken.len(), 1);
        assert!(broken_calls > first_page_calls.unwrap());
        assert!(
            broken_peak > MAX_PEAK,
            "insensitive allocator guard: {broken_peak}"
        );
    }
    // A clone-then-skip stream frees each discarded ID immediately: low peak
    // alone misses it, while allocation counts must reject it.
    begin();
    let streamed = candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .skip(1023)
        .collect::<Vec<_>>();
    let (streamed_peak, streamed_calls) = finish();
    assert_eq!(streamed.len(), 1);
    assert!(streamed_peak < MAX_PEAK);
    assert!(streamed_calls >= candidates.len());
    assert!(regressions.is_empty(), "{regressions:?}");
}
