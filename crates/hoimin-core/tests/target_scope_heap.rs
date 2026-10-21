//! Small production resolver allocation observations, with fixture bytes excluded.
use hoimin_core::{DiscoveredFile, Selection, resolve_explicit};

#[path = "support/requested_heap.rs"]
mod requested_heap;

#[global_allocator]
static ALLOCATOR: requested_heap::TrackingAllocator = requested_heap::TrackingAllocator;

#[test]
fn source_membership_keeps_resolver_heap_linear_in_distinct_roots() {
    requested_heap::self_check();
    for (roots, files) in [(1, 16), (32, 512), (128, 512), (512, 512)] {
        let selection = Selection {
            root: "/project".into(),
            sources: (0..roots).map(|i| format!("src{i:04}").into()).collect(),
            ..Selection::default()
        };
        let discovered: Vec<_> = (0..files)
            .map(|i| DiscoveredFile::python(format!("src{:04}/file{i:04}.py", i % roots)))
            .collect();
        let baseline = requested_heap::begin();
        let targets = resolve_explicit(&selection, &discovered).unwrap();
        let measured = requested_heap::finish(baseline);
        assert_eq!(targets.len(), files);
        // Includes all resolver indexes, normalized sources, and output, rather
        // than treating table capacity as actual allocated bytes. A Cartesian
        // retained root/file index would exceed this linear budget.
        assert!(measured.peak_delta < 1024 * (roots + files));
        assert!(measured.retained_delta > 0);
        eprintln!(
            "roots={roots} files={files} requested_peak={} output_retained={}",
            measured.peak_delta, measured.retained_delta
        );
        drop(targets);
    }
}
