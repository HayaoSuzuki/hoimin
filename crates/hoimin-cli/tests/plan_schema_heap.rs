//! Exercise the private production decoder without changing its visibility.
use std::sync::Mutex;

use hoimin_cli::plan::{
    PLAN_SCHEMA_VERSION, PlanError, PlanManifest, RankedPlanCandidate, RankingReason,
    RankingReasonCode,
};

#[path = "support/plan_schema_fixture.rs"]
mod fixture;
#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[path = "../src/plan/schema.rs"]
mod schema;

#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

static OBSERVATION: Mutex<()> = Mutex::new(());

fn assert_no_dom_overhead(payload_bytes: usize, schema_last: bool) {
    let _observation = OBSERVATION.lock().unwrap();
    let manifest = fixture::manifest(512, payload_bytes);
    let serialized = serde_json::to_string(&manifest).unwrap();
    let bytes = if schema_last {
        let body = serialized.strip_prefix("{\"schema_version\":10,").unwrap();
        format!(
            "{{{},\"schema_version\":10}}",
            body.strip_suffix('}').unwrap()
        )
        .into_bytes()
    } else {
        serialized.into_bytes()
    };
    heap_tracking::begin();
    let direct: PlanManifest = serde_json::from_slice(&bytes).unwrap();
    let direct_peak = heap_tracking::finish();
    drop(direct);

    heap_tracking::begin();
    let decoded = schema::decode_manifest(&bytes);
    let peak = heap_tracking::finish();

    let decoded = decoded.unwrap();
    assert_eq!(decoded.candidates, manifest.candidates);
    assert!(
        peak <= direct_peak + 128 * 1024,
        "DOM overhead: direct={direct_peak}, routed={peak}"
    );
    eprintln!(
        "plan-schema payload={payload_bytes} last={schema_last} input={} direct={direct_peak} routed={peak}",
        bytes.len()
    );
}

macro_rules! heap_cases {
    ($($name:ident: $payload:expr, $last:expr,)*) => {
        $(#[test]
        fn $name() {
            assert_no_dom_overhead($payload, $last);
        })*
    };
}

heap_cases! {
    schema_first_small_candidates_avoid_dom_overhead: 32, false,
    schema_first_large_candidates_avoid_dom_overhead: 4096, false,
    schema_last_small_candidates_avoid_dom_overhead: 32, true,
    schema_last_large_candidates_avoid_dom_overhead: 4096, true,
}

#[test]
fn dom_control_exceeds_decoder_overhead_budget() {
    let _observation = OBSERVATION.lock().unwrap();
    let bytes = serde_json::to_vec(&fixture::manifest(512, 32)).unwrap();
    heap_tracking::begin();
    let direct: PlanManifest = serde_json::from_slice(&bytes).unwrap();
    let direct_peak = heap_tracking::finish();
    drop(direct);

    heap_tracking::begin();
    let dom: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let peak = heap_tracking::finish();

    assert_eq!(dom["candidates"].as_array().unwrap().len(), 512);
    assert!(
        peak > direct_peak + 128 * 1024,
        "insensitive control: direct={direct_peak}, DOM={peak}"
    );
    eprintln!("plan-schema control direct={direct_peak} DOM={peak}");
}

#[test]
fn streaming_plan_keeps_decoded_payload_without_whole_input_copy() {
    use std::io::{BufReader, Read, Write};
    let _observation = OBSERVATION.lock().unwrap();
    let manifest = fixture::manifest(512, 4096);
    let mut file = tempfile::tempfile().unwrap();
    serde_json::to_writer(&mut file, &manifest).unwrap();
    // Large valid trailing whitespace must not become retained parser storage.
    file.write_all(&vec![b' '; 2 * 1024 * 1024]).unwrap();
    let input_bytes = usize::try_from(file.metadata().unwrap().len()).unwrap();
    std::io::Seek::rewind(&mut file).unwrap();
    heap_tracking::begin();
    let streamed = schema::decode_reader(BufReader::new(&mut file)).unwrap();
    let streaming_peak = heap_tracking::finish();
    assert_eq!(streamed.candidates, manifest.candidates);
    drop(streamed);

    std::io::Seek::rewind(&mut file).unwrap();
    heap_tracking::begin();
    let mut raw = Vec::new();
    file.read_to_end(&mut raw).unwrap();
    let buffered = schema::decode_manifest(&raw).unwrap();
    let buffered_peak = heap_tracking::finish();
    assert_eq!(buffered.candidates, manifest.candidates);
    assert!(
        buffered_peak >= streaming_peak + input_bytes - 64 * 1024,
        "input={input_bytes}, streaming={streaming_peak}, buffered={buffered_peak}"
    );
    eprintln!(
        "plan-stream input={input_bytes} streaming_peak={streaming_peak} buffered_peak={buffered_peak}"
    );
}
