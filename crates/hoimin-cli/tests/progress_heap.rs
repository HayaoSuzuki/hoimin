use std::{num::NonZeroUsize, path::Path};

use hoimin_cli::{
    cli::{ProgressArgs, ProgressOutputFormat},
    progress,
};
use serde_json::{Value, json};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;

use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[test]
fn progress_heap_peak_is_independent_of_history_length() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("large-report.json");
    let mut previous_bytes = 0;
    for mutants in [500, 1_000, 2_000] {
        let document = large_report(mutants);
        assert_eq!(document["mutants"].as_array().unwrap().len(), mutants);
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        drop(document);
        let bytes = std::fs::metadata(&path).unwrap().len();
        assert!(bytes > previous_bytes);
        previous_bytes = bytes;
        let (short_peak, _) = measure_history(&path, 2, mutants);
        for history in [2, 4, 8] {
            let (peak, document) = measure_history(&path, history, mutants);
            assert!(
                peak <= short_peak + 512 * 1024,
                "mutants={mutants}, bytes={bytes}, history={history}, baseline={short_peak}, peak={peak}"
            );
            eprintln!(
                "progress-peak mutants={mutants} report_bytes={bytes} history={history} peak={peak}"
            );
            if mutants == 2_000 && history == 8 {
                let eager_peak = measure_eager_history(&path, history, &document);
                assert!(
                    eager_peak > short_peak + 512 * 1024,
                    "retaining report history escaped the same bound: baseline={short_peak}, eager={eager_peak}"
                );
                eprintln!(
                    "progress-eager-control mutants={mutants} report_bytes={bytes} history={history} peak={eager_peak}"
                );
            }
        }
        if mutants == 2_000 {
            let (long_peak, _) = measure_history(&path, 16, mutants);
            let (details_peak, detailed) = measure_history_with_details(&path, 16, mutants, true);
            assert!(
                details_peak <= short_peak + 512 * 1024,
                "details retained history: short={short_peak} details={details_peak}"
            );
            assert_eq!(detailed["details"]["previous_input"], 14);
            assert_eq!(detailed["details"]["current_input"], 15);
            assert_eq!(detailed["details"]["transitions"], json!([]));
            eprintln!(
                "progress-peak mutants={mutants} report_bytes={bytes} history=16 peak={long_peak}"
            );
            assert!(
                long_peak <= short_peak + 512 * 1024,
                "short={short_peak}, long={long_peak}"
            );
        }
    }
}

fn measure_history(path: &Path, history: usize, mutants: usize) -> (usize, Value) {
    measure_history_with_details(path, history, mutants, false)
}

fn measure_history_with_details(
    path: &Path,
    history: usize,
    mutants: usize,
    details: bool,
) -> (usize, Value) {
    let reports = std::iter::repeat_n(path.to_path_buf(), history).collect();
    let args = ProgressArgs {
        details,
        details_limit: 100,
        reports,
        patience: NonZeroUsize::new(3).unwrap(),
        format: ProgressOutputFormat::Json,
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    heap_tracking::begin();
    let result = progress::run(args, &mut stdout, &mut stderr);
    let peak = heap_tracking::finish();
    assert_eq!(result.unwrap(), 0);
    let warnings = String::from_utf8(stderr).unwrap();
    assert_eq!(warnings.lines().count(), history - 1);
    assert!(warnings.lines().all(|line| line.contains("same path")));
    // Capture allocations are legitimate compact O(history) output, but parsing
    // and semantic assertions run after the measured interval.
    let document: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(document["inputs"].as_array().unwrap().len(), history);
    assert!(
        document["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|input| input["usable"] == true)
    );
    let comparisons = document["comparisons"].as_array().unwrap();
    assert_eq!(comparisons.len(), history - 1);
    assert_eq!(document["consecutive_stalls"], history - 1);
    assert_eq!(
        document["latest"]["state"],
        if history >= 4 { "saturated" } else { "stalled" }
    );
    for comparison in comparisons {
        assert_eq!(comparison["common"], mutants);
        assert_eq!(comparison["state"], "stalled");
        assert_eq!(comparison["previous_score"], 1.0);
        assert_eq!(comparison["current_score"], 1.0);
        assert_eq!(comparison["score_delta"], 0.0);
        for field in [
            "added",
            "removed",
            "ambiguous",
            "inconclusive",
            "improvements",
            "regressions",
            "carried_survivors",
        ] {
            assert_eq!(comparison[field], 0, "{field}");
        }
    }
    (peak, document)
}

fn measure_eager_history(path: &Path, history: usize, streamed: &Value) -> usize {
    heap_tracking::begin();
    // Deliberately retain actual parsed reports, reproducing the all-history
    // allocation strategy without injecting fabricated allocation counts.
    let reports = (0..history)
        .map(|_| progress::read_report(path).unwrap())
        .collect::<Vec<_>>();
    let result = progress::compare_reports(&reports, NonZeroUsize::new(3).unwrap());
    std::hint::black_box(&reports);
    let peak = heap_tracking::finish();
    assert_eq!(result.comparisons.len(), history - 1);
    assert_eq!(result.consecutive_stalls, history - 1);
    assert_eq!(result.latest, progress::ProgressState::Saturated);
    for (actual, expected) in result
        .comparisons
        .iter()
        .zip(streamed["comparisons"].as_array().unwrap())
    {
        assert_eq!(actual.state, progress::ProgressState::Stalled);
        for (field, value) in [
            ("common", actual.common),
            ("added", actual.added),
            ("removed", actual.removed),
            ("ambiguous", actual.ambiguous),
            ("inconclusive", actual.inconclusive),
            ("improvements", actual.improvements),
            ("regressions", actual.regressions),
            ("carried_survivors", actual.carried_survivors),
        ] {
            assert_eq!(json!(value), expected[field], "{field}");
        }
        assert_eq!(json!(actual.previous_score), expected["previous_score"]);
        assert_eq!(json!(actual.current_score), expected["current_score"]);
        assert_eq!(json!(actual.score_delta), expected["score_delta"]);
    }
    peak
}

fn large_report(count: usize) -> Value {
    let mut document: Value =
        serde_json::from_str(include_str!("golden/reports/schema-v3-current.json")).unwrap();
    let template = document["mutants"][0].clone();
    let mutants = (0..count)
        .map(|index| {
            let mut mutant = template.clone();
            mutant["sequence"] = json!(index + 4);
            mutant["candidate"]["id"] = json!(format!("mutant-{index}"));
            mutant["candidate"]["sequence"] = json!(index);
            mutant["candidate"]["span"]["start"] = json!(index);
            mutant["candidate"]["original"] = json!(format!("original-{index}"));
            mutant["candidate"]["replacement"] = json!(format!("replacement-{index}"));
            mutant["output"]["token"] = json!(format!("mutant-output-{index}"));
            mutant
        })
        .collect();
    document["mutants"] = Value::Array(mutants);
    document["summary"]["sequence"] = json!(count + 5);
    document["summary"]["counts"]["killed"] = json!(count);
    document["summary"]["counts"]["score"] = json!(1.0);
    document
}
