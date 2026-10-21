use std::io::Write;

#[path = "support/heap_tracking.rs"]
mod heap_tracking;

#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

#[test]
fn progress_decode_does_not_retain_raw_document_or_jsonl_line() {
    let directory = tempfile::tempdir().unwrap();
    let document: serde_json::Value =
        serde_json::from_str(include_str!("golden/reports/schema-v10-current.json")).unwrap();
    for jsonl in [false, true] {
        let small = directory.path().join("small");
        let large = directory.path().join("large");
        write_report(&small, &document, jsonl, 32);
        write_report(&large, &document, jsonl, 4 * 1024 * 1024);
        let small_peak = measured_peak(&small);
        let large_peak = measured_peak(&large);
        eprintln!(
            "progress decode jsonl={jsonl}: small_peak={small_peak}, large_peak={large_peak}"
        );
        assert!(
            large_peak <= small_peak + 128 * 1024,
            "jsonl={jsonl}, small peak={small_peak}, large peak={large_peak}"
        );
    }
}

fn measured_peak(path: &std::path::Path) -> usize {
    heap_tracking::begin();
    let result = hoimin_cli::progress::read_report(path);
    let peak = heap_tracking::finish();
    assert!(result.is_ok(), "{result:?}");
    peak
}

fn write_report(path: &std::path::Path, document: &serde_json::Value, jsonl: bool, padding: usize) {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    // Padding is deliberately inside the first physical line. Progress ignores
    // unknown normalized_config descendants, so none belongs in the result.
    if jsonl {
        let mut run = document["run"].clone();
        run["normalized_config"]["future_padding"] = serde_json::json!("PLACEHOLDER");
        write_padded(&mut file, &run, padding);
        writeln!(file).unwrap();
        serde_json::to_writer(&mut file, &document["baseline"]).unwrap();
        writeln!(file).unwrap();
        let mut sequence = document["baseline"]["sequence"].as_u64().unwrap();
        for mutant in document["mutants"].as_array().unwrap() {
            sequence += 1;
            let started = serde_json::json!({
                "kind": "mutant_started",
                "schema_version": mutant["schema_version"],
                "sequence": sequence,
                "run_id": mutant["run_id"],
                "mutant_id": mutant["candidate"]["id"],
                "mutant_sequence": mutant["candidate"]["sequence"],
            });
            serde_json::to_writer(&mut file, &started).unwrap();
            writeln!(file).unwrap();
            sequence += 1;
            let mut finished = mutant.clone();
            finished["sequence"] = sequence.into();
            serde_json::to_writer(&mut file, &finished).unwrap();
            writeln!(file).unwrap();
        }
        let mut summary = document["summary"].clone();
        summary["sequence"] = (sequence + 1).into();
        serde_json::to_writer(&mut file, &summary).unwrap();
    } else {
        let mut report = document.clone();
        report["run"]["normalized_config"]["future_padding"] = serde_json::json!("PLACEHOLDER");
        write_padded(&mut file, &report, padding);
    }
    file.flush().unwrap();
}

fn write_padded(output: &mut impl Write, value: &serde_json::Value, padding: usize) {
    let serialized = serde_json::to_string(value).unwrap();
    let (before, after) = serialized.split_once("PLACEHOLDER").unwrap();
    output.write_all(before.as_bytes()).unwrap();
    let chunk = [b'x'; 4096];
    let mut remaining = padding;
    while remaining > 0 {
        let count = remaining.min(chunk.len());
        output.write_all(&chunk[..count]).unwrap();
        remaining -= count;
    }
    output.write_all(after.as_bytes()).unwrap();
    // Long whitespace suffixes must also stream, including when they share an
    // event's physical line; none is part of a retained event payload.
    let chunk = [b' '; 4096];
    let mut remaining = padding;
    while remaining > 0 {
        let count = remaining.min(chunk.len());
        output.write_all(&chunk[..count]).unwrap();
        remaining -= count;
    }
}
