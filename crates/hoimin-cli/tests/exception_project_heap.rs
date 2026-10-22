//! One test owns the allocator windows, including discovery's blocking worker.
use camino::Utf8PathBuf;
use hoimin_core::{MutationOperatorSelection, MutationProfile, TargetSlice};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

#[test]
fn exception_project_heap_tracks_largest_source_instead_of_total_source_bytes() {
    const FILE_BYTES: usize = 64 * 1024;
    const GROWTH_ALLOWANCE: usize = 256 * 1024;
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let targets = [TargetSlice {
        path: "m00.py".into(),
        lines: vec![],
        symbols: vec![],
    }];
    let operators: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!(["exception_hierarchy"])).unwrap();

    // A single comment line enlarges each input without enlarging its summary.
    // Exactly sixteen 64 KiB files keep the entire fixture at 1 MiB.
    let suffix = "\nclass Root(Exception): pass\nclass Child(Root): pass\ntry: action()\nexcept Child: pass\n";
    let source = format!("#{}{suffix}", "x".repeat(FILE_BYTES - 1 - suffix.len()));
    std::fs::write(root.join("m00.py"), &source).unwrap();

    // Start the runtime's blocking worker and initialize parser/discovery state
    // before measuring. Each later call still loads its own exception project.
    let warmup = runtime
        .block_on(hoimin_cli::analyzer::discover_targets(
            &root,
            &targets,
            &operators,
            MutationProfile::Full,
            16,
        ))
        .unwrap();
    drop(warmup);

    let mut observations = Vec::with_capacity(3);
    let mut written = 1;
    for file_count in [1, 4, 16] {
        for index in written..file_count {
            std::fs::write(root.join(format!("m{index:02}.py")), &source).unwrap();
        }
        written = file_count;

        heap_tracking::begin();
        let result = runtime.block_on(hoimin_cli::analyzer::discover_targets(
            &root,
            &targets,
            &operators,
            MutationProfile::Full,
            16,
        ));
        let peak = heap_tracking::finish();
        eprintln!("exception-project-peak files={file_count} bytes={peak}");
        observations.push((file_count, peak));

        // The project index must actually be loaded: the sole target has one
        // supported hierarchy mutation, broadening its handler to the parent.
        let discovery = result.unwrap();
        assert_eq!(discovery.candidates.len(), 1, "files={file_count}");
        let candidate = &discovery.candidates[0];
        assert_eq!(candidate.path.as_str(), "m00.py");
        assert_eq!((candidate.line, candidate.column), (5, 7));
        assert_eq!(candidate.original, "Child");
        assert_eq!(candidate.replacement, "Root");
        assert!(
            discovery.diagnostics.is_empty(),
            "{:?}",
            discovery.diagnostics
        );
        assert!(!discovery.truncated);
    }

    // Allow ample path/summary/index growth above the measured one-file
    // baseline, while rejecting retention of all sixteen source bodies.
    // Record every fixture size before this assertion so RED logs show scaling.
    let bound = observations[0].1 + GROWTH_ALLOWANCE;
    for (file_count, peak) in observations {
        assert!(
            peak <= bound,
            "exception project retained source bodies: files={file_count} peak={peak} bound={bound}"
        );
    }
}
