//! Measure the production plan source phase through its public stage events.
//! The analyzer runs after the allocation window has closed.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hoimin_cli::plan::create;
use hoimin_core::{CommandArg, DiscoveryConfig, RawRunConfig, RunConfig};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, Registry};

#[path = "support/heap_tracking.rs"]
mod heap_tracking;
#[global_allocator]
static ALLOCATOR: heap_tracking::TrackingAllocator = heap_tracking::TrackingAllocator;

struct SourceWindow(Arc<Mutex<Option<usize>>>);

impl<S: tracing::Subscriber> Layer<S> for SourceWindow {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        struct Stage(Option<&'static str>);
        impl tracing::field::Visit for Stage {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "stage" {
                    self.0 = match value {
                        "reading sources" => Some("start"),
                        "analyzing sources" => Some("finish"),
                        _ => None,
                    };
                }
            }
        }
        let mut stage = Stage(None);
        event.record(&mut stage);
        match stage.0 {
            Some("start") => heap_tracking::begin(),
            Some("finish") => *self.0.lock().unwrap() = Some(heap_tracking::finish()),
            _ => {}
        }
    }
}

#[test]
fn plan_source_hashing_heap_is_independent_of_largest_source() {
    let directory = tempfile::tempdir().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for mib in [1, 16, 64] {
        let mut bytes = vec![b' '; mib * 1024 * 1024];
        bytes[0] = b'#';
        bytes.extend_from_slice(b"\nx = 0\n");
        std::fs::write(root.join("source.py"), &bytes).unwrap();
        let expected_hash = blake3::hash(&bytes).to_hex().to_string();
        drop(bytes);
        #[cfg(unix)]
        let argv = CommandArg::Unix(b"unused-test-command".to_vec());
        #[cfg(windows)]
        let argv = CommandArg::Windows("unused-test-command".encode_utf16().collect());
        let config = RunConfig::try_from(RawRunConfig {
            root: root.clone(),
            files: vec!["source.py".into()],
            operators: vec!["boolean_literal".into()],
            allow_best_effort_memory: true,
            test_argv: vec![argv],
            ..RawRunConfig::default()
        })
        .unwrap();
        let peak = Arc::new(Mutex::new(None));
        let subscriber = Registry::default().with(SourceWindow(Arc::clone(&peak)));

        let output = runtime
            .block_on(
                create(
                    config,
                    DiscoveryConfig::new(Duration::from_secs(30)).unwrap(),
                )
                .with_subscriber(subscriber),
            )
            .unwrap();

        assert_eq!(output.manifest.sources.len(), 1);
        assert_eq!(output.manifest.sources[0].hash, expected_hash);
        let peak = peak
            .lock()
            .unwrap()
            .expect("source stage window was observed");
        eprintln!("plan-source-peak mib={mib} bytes={peak}");
        assert!(
            peak < 256 * 1024,
            "whole-source allocation: {mib} MiB: {peak}"
        );
    }
}
