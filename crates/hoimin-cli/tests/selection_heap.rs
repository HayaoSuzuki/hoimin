//! Isolated allocator binary: fixture construction and expected IDs are outside
//! measurement. Compile the real private selection source with production types.
use std::num::NonZeroUsize;

use camino::Utf8PathBuf;
use hoimin_cli::cli::{self, TopSelectionPolicy};
use hoimin_cli::plan::{RankedPlanCandidate, RankingReason, RankingReasonCode};
use hoimin_core::{ByteSpan, MutationCandidate};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[path = "../src/plan/selection.rs"]
mod selection;

#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

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
    for policy in [TopSelectionPolicy::Strict, TopSelectionPolicy::Diverse] {
        for offset in [0, 512, 1023] {
            // Eight equally sized files: round robin row is offset / 8 and
            // column is offset % 8. These positions are independent of selection.
            let expected_index = match policy {
                TopSelectionPolicy::Strict => offset,
                TopSelectionPolicy::Diverse => (offset % 8) * 128 + offset / 8,
            };
            heap_tracking::begin();
            let selected = selection::select_top_candidate_ids_at(&candidates, one, policy, offset);
            let peak = heap_tracking::finish();
            eprintln!(
                "{policy:?} offset={offset} peak={peak} capacity={}",
                selected.capacity()
            );
            assert_eq!(selected, [candidates[expected_index].id.clone()]);
            if peak >= MAX_PEAK || selected.capacity() != 1 {
                regressions.push(format!(
                    "{policy:?} offset={offset}: peak={peak} capacity={}",
                    selected.capacity()
                ));
            }
        }
        // Sensitivity: the old owned-prefix algorithm must exceed the same
        // bound even if someone later shrinks its returned vector.
        heap_tracking::begin();
        let prefix = selection::select_top_candidate_ids(
            &candidates,
            NonZeroUsize::new(candidates.len()).unwrap(),
            policy,
        );
        let mut broken = prefix.into_iter().skip(1023).collect::<Vec<_>>();
        broken.shrink_to_fit();
        let broken_peak = heap_tracking::finish();
        eprintln!("{policy:?} eager_prefix_peak={broken_peak}");
        assert_eq!(broken.len(), 1);
        assert!(
            broken_peak > MAX_PEAK,
            "insensitive allocator guard: {broken_peak}"
        );
    }
    assert!(regressions.is_empty(), "{regressions:?}");
}
