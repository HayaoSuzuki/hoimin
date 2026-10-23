#[path = "support/candidate_identity.rs"]
mod candidate_identity;

use hoimin_cli::{cli::ProgressArgs, progress};
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct CountingAllocator;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// This single-test executable compares total allocations of identical public
// input reads with details disabled/enabled, avoiding parser peak masking.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && ACTIVE.load(Ordering::Relaxed) {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() && ACTIVE.load(Ordering::Relaxed) {
            ALLOCATED.fetch_add(size, Ordering::Relaxed);
        }
        pointer
    }
}

#[test]
fn detail_allocations_do_not_scale_with_candidate_body_size() {
    let fixture = tempfile::tempdir().unwrap();
    let paths = [
        fixture.path().join("before.json"),
        fixture.path().join("after.json"),
    ];
    for size in [32, 256 * 1024] {
        for (path, (status, killed, survived, score, exit_code)) in paths
            .iter()
            .zip([("killed", 12, 0, 1.0, 1), ("survived", 0, 12, 0.0, 0)])
        {
            let mut doc: Value =
                serde_json::from_str(include_str!("golden/reports/schema-v10-current.json"))
                    .unwrap();
            let template = doc["mutants"][0].clone();
            doc["mutants"] = Value::Array(
                (0..12)
                    .map(|index| {
                        let mut event = template.clone();
                        event["sequence"] = json!(index + 4);
                        event["candidate"]["sequence"] = json!(index + 1);
                        event["candidate"]["span"]["start"] = json!(index);
                        event["candidate"]["original"] = json!("a".repeat(size));
                        event["candidate"]["replacement"] = json!("b".repeat(size));
                        candidate_identity::assign_id(&mut event["candidate"]);
                        event["outcome"]["executed"]["termination"] = json!({"Exit": exit_code});
                        event["status"] = json!(status);
                        event
                    })
                    .collect(),
            );
            doc["summary"]["sequence"] = json!(17);
            doc["summary"]["counts"]["killed"] = json!(killed);
            doc["summary"]["counts"]["survived"] = json!(survived);
            doc["summary"]["counts"]["score"] = json!(score);
            doc["summary"]["exit_code"] = json!(i32::from(survived > 0));
            std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
        }
        let measure = |details, expected_transitions| {
            let args = ProgressArgs {
                fail_on_regression: false,
                reports: paths.to_vec(),
                patience: NonZeroUsize::new(3).unwrap(),
                details,
                details_limit: 2,
            };
            let mut out = Vec::new();
            let mut err = Vec::new();
            ALLOCATED.store(0, Ordering::Relaxed);
            ACTIVE.store(true, Ordering::Relaxed);
            let result = progress::run(args, &mut out, &mut err);
            ACTIVE.store(false, Ordering::Relaxed);
            let bytes = ALLOCATED.load(Ordering::Relaxed);
            assert_eq!(result.unwrap(), 0);
            let doc: Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(doc["comparisons"][0]["regressions"], 12);
            assert_eq!(
                doc["details"]
                    .as_object()
                    .map(|_| doc["details"]["transitions"].as_array().unwrap().len()),
                expected_transitions
            );
            assert_eq!(
                doc["details"]
                    .as_object()
                    .map(|_| doc["details"]["omitted"].as_u64().unwrap()),
                expected_transitions.map(|_| 10)
            );
            bytes
        };
        let baseline = measure(false, None);
        let detailed = measure(true, Some(2));
        let overhead = detailed.saturating_sub(baseline);
        eprintln!(
            "details body={size}, baseline={baseline}, detailed={detailed}, overhead={overhead}"
        );
        assert!(
            overhead < 64 * 1024,
            "candidate body leaked into detail allocations: {overhead}"
        );
    }
}
