use hoimin_cli::{
    cli::{ProgressArgs, ProgressOutputFormat},
    progress,
};
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
        for (path, survived) in paths.iter().zip([false, true]) {
            let mut doc: Value =
                serde_json::from_str(include_str!("golden/reports/schema-v3-current.json"))
                    .unwrap();
            let template = doc["mutants"][0].clone();
            doc["mutants"] = Value::Array(
                (0..12)
                    .map(|index| {
                        let mut event = template.clone();
                        event["sequence"] = json!(index + 4);
                        event["candidate"]["id"] = json!(format!("candidate-{index:02}"));
                        event["candidate"]["sequence"] = json!(index + 1);
                        event["candidate"]["original"] = json!("a".repeat(size));
                        event["candidate"]["replacement"] = json!("b".repeat(size));
                        event["status"] = json!(if survived { "survived" } else { "killed" });
                        event["termination"] = json!({"Exit": i32::from(!survived)});
                        event
                    })
                    .collect(),
            );
            doc["summary"]["sequence"] = json!(17);
            doc["summary"]["counts"]["killed"] = json!(if survived { 0 } else { 12 });
            doc["summary"]["counts"]["survived"] = json!(if survived { 12 } else { 0 });
            doc["summary"]["counts"]["score"] = json!(if survived { 0.0 } else { 1.0 });
            doc["summary"]["exit_code"] = json!(i32::from(survived));
            std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
        }
        let measure = |details| {
            let args = ProgressArgs {
                reports: paths.to_vec(),
                patience: NonZeroUsize::new(3).unwrap(),
                format: ProgressOutputFormat::Json,
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
            if details {
                assert_eq!(doc["details"]["transitions"].as_array().unwrap().len(), 2);
                assert_eq!(doc["details"]["omitted"], 10);
            }
            bytes
        };
        let baseline = measure(false);
        let detailed = measure(true);
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
