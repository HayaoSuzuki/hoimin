use std::num::NonZeroUsize;

use hoimin_cli::progress::{self, InputReport, ProgressState, UsableReport};
use hoimin_core::{MutantFinished, MutationStatus};
use serde_json::Value;

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[derive(Clone, Copy, Debug)]
enum Scenario {
    DifferentIds,
    DuplicateContent,
    Inconclusive,
    MatchingIds,
}

// One test owns the process-global measurement window. Input construction and
// output assertions deliberately occur outside that window.
#[test]
fn comparison_heap_does_not_scale_with_candidate_text() {
    let measurements = [
        Scenario::DifferentIds,
        Scenario::DuplicateContent,
        Scenario::Inconclusive,
        Scenario::MatchingIds,
    ]
    .map(|scenario| {
        let small = measure(scenario, 32);
        let large = measure(scenario, 256 * 1024);
        eprintln!("comparison-peak scenario={scenario:?} small={small} large={large}");
        (scenario, small, large)
    });
    for (scenario, small, large) in measurements {
        assert!(
            large <= small + 64 * 1024,
            "comparison cloned candidate text: scenario={scenario:?} small={small} large={large}"
        );
    }
}

fn measure(scenario: Scenario, text_bytes: usize) -> usize {
    let reports = [
        report(scenario, text_bytes, false),
        report(scenario, text_bytes, true),
    ];
    let patience = NonZeroUsize::new(3).unwrap();
    heap_tracking::begin();
    let result = progress::compare_reports(&reports, patience);
    let peak = heap_tracking::finish();

    let comparison = &result.comparisons[0];
    let duplicates = matches!(scenario, Scenario::DuplicateContent);
    let inconclusive = matches!(scenario, Scenario::Inconclusive);
    assert_eq!(comparison.common, if duplicates { 0 } else { 16 });
    assert_eq!(comparison.ambiguous, if duplicates { 8 } else { 0 });
    assert_eq!(comparison.inconclusive, if inconclusive { 16 } else { 0 });
    assert_eq!(comparison.added, 0);
    assert_eq!(comparison.removed, 0);
    assert_eq!(comparison.improvements, 0);
    assert_eq!(comparison.regressions, 0);
    assert_eq!(
        comparison.carried_survivors,
        if duplicates || inconclusive { 0 } else { 16 }
    );
    let score = if duplicates || inconclusive {
        None
    } else {
        Some(0.0)
    };
    assert_eq!(comparison.previous_score, score);
    assert_eq!(comparison.current_score, score);
    assert_eq!(comparison.score_delta, score);
    let matching = matches!(scenario, Scenario::MatchingIds);
    assert_eq!(
        result.latest,
        if matching {
            ProgressState::Stalled
        } else {
            ProgressState::Indeterminate
        }
    );
    assert_eq!(result.consecutive_stalls, usize::from(matching));
    peak
}

fn report(scenario: Scenario, text_bytes: usize, after: bool) -> InputReport {
    let document: Value =
        serde_json::from_str(include_str!("golden/reports/schema-v3-current.json")).unwrap();
    let template: MutantFinished = serde_json::from_value(document["mutants"][0].clone()).unwrap();
    let payload = "x".repeat(text_bytes);
    let mutants = (0..16)
        .map(|index| {
            let mut mutant = template.clone();
            let key = if matches!(scenario, Scenario::DuplicateContent) {
                index / 2
            } else {
                index
            };
            let phase = if matches!(scenario, Scenario::MatchingIds) {
                false
            } else {
                after
            };
            mutant.candidate.id = format!("{phase}-{index}-{payload}");
            mutant.candidate.original = format!("{key}-{payload}");
            mutant.candidate.replacement = format!("replacement-{payload}");
            mutant.candidate.path = format!("{payload}.py").into();
            mutant.candidate.operator.clone_from(&payload);
            mutant.candidate.symbol = Some(payload.clone());
            mutant.status = if matches!(scenario, Scenario::Inconclusive) {
                MutationStatus::Timeout
            } else {
                MutationStatus::Survived
            };
            mutant
        })
        .collect();
    InputReport::Usable(UsableReport {
        source: "report.json".into(),
        mutants,
    })
}
