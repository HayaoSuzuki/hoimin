#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use std::fs;

use camino::Utf8PathBuf;
use hoimin_cli::fingerprint_inputs::resolve;
use hoimin_core::FingerprintInputFile;

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
use heap_tracking::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

static ALLOCATION_WINDOW: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[rstest::rstest]
#[case::exact("exact", vec![], vec!["input.bin".into()])]
#[case::glob("glob", vec!["*.bin".into()], vec![])]
#[case::overlap("overlap", vec!["*.bin".into()], vec!["input.bin".into()])]
fn auxiliary_hashing_heap_is_independent_of_largest_input(
    #[case] mode: &str,
    #[case] patterns: Vec<String>,
    #[case] files: Vec<String>,
    #[values(1usize, 16usize, 64usize)] mib: usize,
) {
    let _window = ALLOCATION_WINDOW.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let bytes: Vec<u8> = (0_u8..=255).cycle().take(mib * 1024 * 1024).collect();
    fs::write(root.join("input.bin"), &bytes).unwrap();
    let expected = vec![FingerprintInputFile {
        path: "input.bin".into(),
        hash: blake3::hash(&bytes).to_hex().to_string(),
    }];
    drop(bytes);

    for repeat in 0..3 {
        heap_tracking::begin();

        let result = resolve(&root, &patterns, &files);
        let peak = heap_tracking::finish();

        assert_eq!(result.unwrap(), expected);
        eprintln!("fingerprint-peak mib={mib} mode={mode} repeat={repeat} bytes={peak}");

        assert!(
            peak < 256 * 1024,
            "whole-file allocation: {mib} MiB {mode}: {peak}"
        );
    }
}
