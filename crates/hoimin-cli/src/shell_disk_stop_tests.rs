mod disk_stop_tests {
    use super::*;

    struct FailDuringMaterialization(Arc<AtomicBool>);

    impl DiskMeasurement for FailDuringMaterialization {
        fn measure(&self) -> std::io::Result<MeterReading> {
            if self.0.load(Ordering::Acquire) {
                return Err(std::io::Error::other("synthetic disk scan failure"));
            }
            Ok(MeterReading {
                owned_bytes: 0,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: Duration::ZERO,
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disk_reason_is_visible_when_blocking_io_outlives_shutdown_grace() {
        // Arrange: retain one materialization operation until the run has returned.
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "30s");
        let (mut control, mut pause) = RunControl::with_materialization_pause_and_shutdown_grace(
            0,
            Duration::from_millis(200),
        );
        let paused = Arc::new(AtomicBool::new(false));
        control.disk_meter = Some(TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(
            FailDuringMaterialization(Arc::clone(&paused)),
        )))));
        control.disk_sample_interval = Duration::from_millis(1);
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let controller = tokio::task::spawn_blocking(move || {
            pause.wait_until_entered(Duration::from_secs(5)).unwrap();
            let guard = pause.release_guard();
            paused.store(true, Ordering::Release);
            let _ = release_rx.recv_timeout(Duration::from_secs(10));
            drop(guard);
        });

        // Act
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            run_loop_with_control(config, stdout.clone(), stderr.clone(), control),
        )
        .await;
        let _ = release_tx.send(());
        controller.await.unwrap();
        let error = result.unwrap().unwrap_err();
        let diagnostic = String::from_utf8(stderr.snapshot()).unwrap();
        let events = String::from_utf8(stdout.snapshot()).unwrap();

        // Assert: failure remains bounded and no final-success event is fabricated.
        assert!(error.contains("shutdown grace expired"), "{error}");
        assert!(
            diagnostic.contains("disk.measurement.failed"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("synthetic disk scan failure"),
            "{diagnostic}"
        );
        assert!(!events.contains("run_finished"), "{events}");
    }
}
