use std::io::Write;

use hoimin_cli::progress::{InputReport, read_report};
use serde_json::{Value, json};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[test]
fn jsonl_peak_does_not_grow_with_diagnostic_history() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("events.jsonl");
    let mut peaks = Vec::new();
    for count in [32_u64, 2_000, 8_000] {
        write_events(&path, count);
        heap_tracking::begin();
        let report = read_report(&path).unwrap();
        let peak = heap_tracking::finish();
        assert!(matches!(report, InputReport::Usable(_)));
        peaks.push(peak);
    }
    assert!(peaks[1] <= peaks[0] + 128 * 1024, "{peaks:?}");
    assert!(peaks[2] <= peaks[0] + 128 * 1024, "{peaks:?}");
}

fn write_events(path: &std::path::Path, count: u64) {
    let mut document: Value =
        serde_json::from_str(include_str!("golden/reports/schema-v3-current.json")).unwrap();
    document["mutants"] = json!([]);
    document["summary"]["counts"] = serde_json::to_value(hoimin_core::summarize(&[])).unwrap();
    document["summary"]["exit_code"] = json!(0);
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut sequence = 1_u64;
    let run_id = document["run"]["run_id"].clone();
    for key in ["run", "baseline"] {
        document[key]["sequence"] = json!(sequence);
        writeln!(file, "{}", document[key]).unwrap();
        sequence += 1;
    }
    for _ in 0..count {
        writeln!(
            file,
            "{}",
            json!({
                "kind": "diagnostic", "schema_version": 3, "sequence": sequence,
                "run_id": run_id, "level": "info", "code": "test", "message": "x".repeat(1024)
            })
        )
        .unwrap();
        sequence += 1;
    }
    document["summary"]["sequence"] = json!(sequence);
    writeln!(file, "{}", document["summary"]).unwrap();
    file.flush().unwrap();
}
