use std::{alloc::{GlobalAlloc, Layout, System}, sync::atomic::{AtomicU64, Ordering}, time::Instant};
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile, TargetSlice};

struct CountAlloc;
static CALLS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
fn record(size: usize) {
    if size >= 500_000 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
    }
}
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() { record(layout.size()); }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() { record(layout.size()); }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() { record(size); }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) { unsafe { System.dealloc(p, layout) }; }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

fn main() {
    let root = std::env::args().nth(1).expect("temporary project root");
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let mut operators = MutationOperatorSelection::default();
    for name in operators.names() {
        let operator = MutationOperatorSelection::parse_selector(&name).unwrap()[0];
        if operator != MutationOperator::BinaryAddSub { operators.exclude(operator); }
    }
    let targets = vec![TargetSlice { path: "subject.py".into(), lines: vec![], symbols: vec![] }];
    for depth in [1, 8, 16, 32] {
        for method in ["ignore", "append"] {
            let source = format!("{}('{}', 1+2){}\n",
                format!("obj.{method}(").repeat(depth), "a".repeat(500_000), ")".repeat(depth));
            std::fs::write(std::path::Path::new(&root).join("subject.py"), &source).unwrap();
            CALLS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            let start = Instant::now();
            let output = runtime.block_on(hoimin_cli::analyzer::discover_targets(
                camino::Utf8Path::new(&root), &targets, &operators, MutationProfile::Full, 1)).unwrap();
            let elapsed = start.elapsed().as_micros();
            let calls = CALLS.load(Ordering::Relaxed);
            let bytes = BYTES.load(Ordering::Relaxed);
            assert_eq!(output.candidates.len(), 1);
            assert_eq!(output.candidates[0].operator, "binary_add_sub");
            assert_eq!(output.candidates[0].original, "+");
            assert_eq!(output.candidates[0].replacement, "-");
            println!("{{\"method\":\"{method}\",\"depth\":{depth},\"source_bytes\":{},\"large_calls\":{calls},\"cumulative_large_bytes\":{bytes},\"elapsed_us\":{elapsed}}}", source.len());
        }
    }
}
