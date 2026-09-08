use std::{io, num::NonZeroUsize, path::Path};

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
    std::fs::write(&path, serde_json::to_vec(&large_report()).unwrap()).unwrap();

    let short_peak = measure_history(&path, 2);
    let long_peak = measure_history(&path, 16);

    assert!(
        long_peak <= short_peak + 512 * 1024,
        "short={short_peak}, long={long_peak}"
    );
}

fn measure_history(path: &Path, history: usize) -> usize {
    let reports = std::iter::repeat_n(path.to_path_buf(), history).collect();
    let args = ProgressArgs {
        reports,
        patience: NonZeroUsize::new(3).unwrap(),
        format: ProgressOutputFormat::Json,
    };
    let mut stdout = io::sink();
    let mut stderr = io::sink();

    heap_tracking::begin();
    progress::run(args, &mut stdout, &mut stderr).unwrap();
    heap_tracking::finish()
}

fn large_report() -> Value {
    let mut document: Value =
        serde_json::from_str(include_str!("golden/reports/schema-v3-current.json")).unwrap();
    let template = document["mutants"][0].clone();
    let mutants = (0_u64..2_000)
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
    document["summary"]["sequence"] = json!(2_005);
    document["summary"]["counts"]["killed"] = json!(2_000);
    document["summary"]["counts"]["score"] = json!(1.0);
    document
}
