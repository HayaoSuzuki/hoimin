use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::future::Future;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use hoimin_cli::cli::{OutputFormat, TopSelectionPolicy, VerifySelection};
use hoimin_cli::plan::{PlanManifest, prepare_verify_selection};

struct ManifestAllocator;

#[global_allocator]
static ALLOCATOR: ManifestAllocator = ManifestAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static MANIFEST_BYTES: AtomicUsize = AtomicUsize::new(0);
static BUFFER: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static RELEASES: AtomicUsize = AtomicUsize::new(0);

fn allocated(pointer: *mut u8, size: usize) {
    if !pointer.is_null()
        && TRACKING.load(Ordering::SeqCst)
        && size == MANIFEST_BYTES.load(Ordering::SeqCst)
    {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        BUFFER.store(pointer as usize, Ordering::SeqCst);
    }
}

unsafe impl GlobalAlloc for ManifestAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        allocated(pointer, layout.size());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        allocated(pointer, layout.size());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if BUFFER
            .compare_exchange(pointer as usize, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            RELEASES.fetch_add(1, Ordering::SeqCst);
        }
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null()
            && BUFFER
                .compare_exchange(
                    pointer as usize,
                    replacement as usize,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_err()
        {
            allocated(replacement, size);
        }
        replacement
    }
}

struct BlockingReadGate(Option<mpsc::Sender<()>>);

impl Drop for BlockingReadGate {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}

fn hold_blocking_reader(runtime: &tokio::runtime::Runtime) -> BlockingReadGate {
    let (ready, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    runtime.spawn_blocking(move || {
        ready.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(30))
            .expect("manifest lifetime observation must release the reader");
    });
    let gate = BlockingReadGate(Some(release));
    started
        .recv_timeout(Duration::from_secs(10))
        .expect("blocking reader started");
    gate
}

fn write_public_manifest(
    runtime: &tokio::runtime::Runtime,
    directory: &Path,
    marker: &Path,
) -> (PathBuf, PlanManifest, usize) {
    let root = directory.join("project");
    std::fs::create_dir(&root).unwrap();
    let payload = "x".repeat(256 * 1024);
    let mut source = String::new();
    for index in 0..24 {
        writeln!(source, "record_{index} = ['{payload}']").unwrap();
    }
    std::fs::write(root.join("sample.py"), source).unwrap();
    let python = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        });
    let arguments = vec![
        OsString::from("hoimin"),
        "plan".into(),
        "--root".into(),
        root.into_os_string(),
        "--file".into(),
        "sample.py".into(),
        "--operators".into(),
        "collection_list_tuple".into(),
        "--allow-best-effort-memory".into(),
        "--min-free-space".into(),
        "1B".into(),
        "--".into(),
        python.into_os_string(),
        "-c".into(),
        format!(
            "from pathlib import Path; Path({:?}).write_text('executed')",
            marker.to_string_lossy()
        )
        .into(),
    ];
    let mut bytes = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        runtime.block_on(hoimin_cli::run_with_io(arguments, &mut bytes, &mut stderr)),
        0,
        "{stderr:?}"
    );
    let manifest: PlanManifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest.candidates.len(), 24);
    let manifest_size = bytes.len();
    assert!(manifest_size > 12 * 1024 * 1024);
    let path = directory.join("plan.json");
    std::fs::write(&path, bytes).unwrap();
    (path, manifest, manifest_size)
}

#[test]
fn public_prepare_releases_manifest_bytes_before_source_read_and_preserves_preview() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("test-command-ran");
    let (path, manifest, manifest_size) =
        write_public_manifest(&runtime, directory.path(), &marker);

    for count in [1, 5] {
        let requested = VerifySelection::Top {
            count: NonZeroUsize::new(count).unwrap(),
            policy: TopSelectionPolicy::Strict,
        };
        let gate = hold_blocking_reader(&runtime);
        let mut prepare = Box::pin(prepare_verify_selection(
            &path,
            &requested,
            OutputFormat::Json,
        ));
        let entered = runtime.enter();
        MANIFEST_BYTES.store(manifest_size, Ordering::SeqCst);
        ALLOCATIONS.store(0, Ordering::SeqCst);
        RELEASES.store(0, Ordering::SeqCst);
        BUFFER.store(0, Ordering::SeqCst);
        TRACKING.store(true, Ordering::SeqCst);
        let poll = prepare
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        TRACKING.store(false, Ordering::SeqCst);
        drop(entered);
        assert!(
            matches!(poll, Poll::Pending),
            "source read must remain queued"
        );
        assert_eq!(
            ALLOCATIONS.load(Ordering::SeqCst),
            1,
            "unique input allocation"
        );
        assert_eq!(
            RELEASES.load(Ordering::SeqCst),
            1,
            "raw JSON must be freed before source reading"
        );
        assert_eq!(BUFFER.load(Ordering::SeqCst), 0);
        drop(gate);
        let verified = runtime.block_on(prepare).unwrap();
        let mut api_bytes = Vec::new();
        verified
            .preview
            .write(OutputFormat::Json, &mut api_bytes)
            .unwrap();
        let api: serde_json::Value = serde_json::from_slice(&api_bytes).unwrap();
        let rows = api["candidates"].as_array().unwrap();
        assert_eq!(rows.len(), count);
        for (row, candidate) in rows.iter().zip(&manifest.candidates) {
            assert_eq!(row["id"], candidate.id);
            assert_eq!(row["rank"], candidate.rank);
            assert_eq!(row["original"], candidate.original);
            assert_eq!(row["replacement"], candidate.replacement);
        }
        let output = runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(30),
                tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
                    .arg("verify")
                    .arg(&path)
                    .args(["--top", &count.to_string(), "--dry-run"])
                    .env("TMPDIR", directory.path())
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .expect("public dry-run deadline")
            .unwrap()
        });
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            api
        );
        assert!(!marker.exists());
    }
}
