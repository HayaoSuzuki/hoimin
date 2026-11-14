mod runtime_coverage {
    use super::super::preparation::{PreparationError, PreparationToken};
    use super::*;
    use std::io::Write;

    type TestPreparationOwner = Box<dyn std::fmt::Debug + Send>;

    async fn run_preparation(
        timing: &PreparationToken,
        interrupts: &mut crate::interrupt::InterruptMonitor,
        operation: impl FnOnce() -> Result<TestPreparationOwner, PreparationError> + Send + 'static,
    ) -> Result<TestPreparationOwner, PreparationError> {
        let operation: Box<dyn FnOnce() -> Result<TestPreparationOwner, PreparationError> + Send> =
            Box::new(operation);
        timing.run(interrupts, operation).await
    }

    fn preparation_monitor() -> (
        tokio::sync::mpsc::UnboundedSender<Result<(), String>>,
        crate::interrupt::InterruptMonitor,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        (
            sender,
            crate::interrupt::spawn_test_monitor(receiver, std::mem::drop::<i32>),
        )
    }

    #[derive(Clone, Copy)]
    enum DiagnosticFault {
        Write,
        Flush,
        PanicWrite,
        PanicFlush,
        None,
    }

    struct DiagnosticWriter(DiagnosticFault);

    impl Write for DiagnosticWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            match self.0 {
                DiagnosticFault::Write => {
                    Err(std::io::Error::other("owned diagnostic write failure"))
                }
                DiagnosticFault::PanicWrite => panic!("owned diagnostic writer panic"),
                _ => Ok(bytes.len()),
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            match self.0 {
                DiagnosticFault::Flush => {
                    Err(std::io::Error::other("owned diagnostic flush failure"))
                }
                DiagnosticFault::PanicFlush => panic!("owned diagnostic flush panic"),
                _ => Ok(()),
            }
        }
    }

    #[derive(Debug)]
    struct PanicDuringDisposal;

    impl Drop for PanicDuringDisposal {
        fn drop(&mut self) {
            panic!("owned preparation rollback panic");
        }
    }

    #[rstest::rstest]
    #[case(
        DiagnosticFault::Write,
        "preparation diagnostic output failed: owned diagnostic write failure"
    )]
    #[case(
        DiagnosticFault::Flush,
        "preparation diagnostic output failed: owned diagnostic flush failure"
    )]
    #[case(DiagnosticFault::PanicWrite, "preparation diagnostic task failed:")]
    #[case(DiagnosticFault::PanicFlush, "preparation diagnostic task failed:")]
    #[tokio::test]
    async fn preparation_report_preserves_primary_failure_when_diagnostic_fails(
        #[case] fault: DiagnosticFault,
        #[case] expected: &str,
    ) {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let error = timing
            .report(
                PreparationError::Failed("primary fixture".into()),
                Box::new(DiagnosticWriter(fault)) as Box<dyn std::io::Write + Send>,
            )
            .await
            .unwrap_err();

        let actual = error.starts_with("preparation failed: primary fixture; ");
        let actual_2 = error.contains(expected);

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
    }

    #[tokio::test]
    async fn preparation_report_flushes_orderly_cancellation_diagnostic() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();
        let result = timing
            .report(
                timing.check().unwrap_err(),
                Box::new(DiagnosticWriter(DiagnosticFault::None)) as Box<dyn std::io::Write + Send>,
            )
            .await;

        assert_eq!(result, Ok(130));
    }

    #[tokio::test]
    async fn preparation_dispose_reports_owned_destructor_panic() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let error = timing.dispose(PanicDuringDisposal).await.unwrap_err();

        let actual = matches!(error, PreparationError::Failed(message) if message.contains("preparation rollback task failed:") && message.contains("owned preparation rollback panic"));

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_dispose_finishes_owned_drop() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());

        let actual = timing
            .dispose(String::from("owned rollback resource"))
            .await
            .is_ok();

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_admit_rechecks_cancellation_after_future_completion() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let error = timing
            .admit(
                &mut interrupts,
                Box::pin(async move {
                    control.cancel();
                })
                    as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            )
            .await
            .unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_admit_stops_pending_future_at_original_deadline() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (_sender, mut interrupts) = preparation_monitor();
        let error = timing
            .admit(
                &mut interrupts,
                Box::pin(std::future::pending::<()>())
                    as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            )
            .await
            .unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::TotalTimeout)
        );

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_admit_maps_first_interrupt_to_cancellation() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (sender, mut interrupts) = preparation_monitor();
        sender.send(Ok(())).unwrap();
        let error = timing
            .admit(
                &mut interrupts,
                Box::pin(std::future::pending::<()>())
                    as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            )
            .await
            .unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_admit_preserves_interrupt_installation_failure() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (sender, mut interrupts) = preparation_monitor();
        sender.send(Err("owned interrupt failure".into())).unwrap();
        let error = timing
            .admit(
                &mut interrupts,
                Box::pin(std::future::pending::<()>())
                    as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            )
            .await
            .unwrap_err();

        let actual = matches!(error, PreparationError::Failed(message) if message == "owned interrupt failure");

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_run_reports_worker_panic_as_closed_result_channel() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (_sender, mut interrupts) = preparation_monitor();
        let error = run_preparation(&timing, &mut interrupts, || {
            panic!("owned preparation worker panic")
        })
        .await
        .unwrap_err();

        let actual = matches!(error, PreparationError::Failed(message) if message.contains("preparation task failed:"));

        assert!(actual);
    }
    #[tokio::test]
    async fn owned_blocking_absent_result_is_expired_without_retained_owner() {
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(30),
        );
        let mut task = tokio::task::spawn_blocking(|| None::<()>);
        let error = await_owned_blocking_until(&budget, &mut task)
            .await
            .unwrap_err();

        let actual = matches!(error, OwnedBlockingError::Expired(message) if message == budget.expiry_error(0, 0));

        assert!(actual);
    }

    #[tokio::test]
    async fn blocking_io_worker_panic_preserves_effect_identity() {
        let id = EffectId(42);
        let error = run_blocking_io::<()>(id, || panic!("owned blocking fixture panic"))
            .await
            .unwrap_err();

        let actual = error
            .failure
            .message()
            .contains("owned blocking fixture panic");

        assert_eq!(error.id, id);
        assert!(actual, "{}", error.failure.message());
    }
    #[test]
    fn cancelling_an_unqueued_worker_emits_metrics_state_warning() {
        let mut collector = Some(MetricsCollector::new("run-warning"));
        let mut warnings = Vec::new();
        cancel_queued_worker(7, &mut collector, &mut warnings);

        assert_eq!(
            warnings,
            vec![("metrics.state", "worker 7 has no queued process".to_owned())]
        );
    }
    fn blocked_preparation_operation(
        started: tokio::sync::oneshot::Sender<()>,
        release: &std::sync::mpsc::Receiver<()>,
    ) {
        started.send(()).unwrap();
        release.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    #[tokio::test]
    async fn preparation_run_cancellation_waits_for_owned_worker_shutdown() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                Ok(Box::new(()) as TestPreparationOwner)
            })
            .await
        });
        started_rx.await.unwrap();
        sender.send(Ok(())).unwrap();
        tokio::task::yield_now().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_run_interrupt_failure_survives_owned_worker_shutdown() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                Ok(Box::new(()) as TestPreparationOwner)
            })
            .await
        });
        started_rx.await.unwrap();
        sender
            .send(Err("owned run interrupt failure".into()))
            .unwrap();
        tokio::task::yield_now().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();

        let actual =
            matches!(error, PreparationError::Failed(message) if message == "preparation failed");

        assert!(actual);
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_run_timeout_waits_for_owned_worker_shutdown() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (_sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                Ok(Box::new(()) as TestPreparationOwner)
            })
            .await
        });
        started_rx.await.unwrap();
        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::TotalTimeout)
        );

        assert!(actual);
    }
    #[test]
    fn preparation_string_error_conversion_retains_message() {
        let error = PreparationError::from(String::from("owned preparation error"));

        let actual = matches!(error, PreparationError::Failed(message) if message == "owned preparation error");

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_admit_rejects_already_cancelled_work() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        control.cancel();
        let error = timing
            .admit(
                &mut interrupts,
                Box::pin(std::future::ready(()))
                    as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            )
            .await
            .unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }
    #[tokio::test]
    async fn preparation_cancellation_overrides_an_already_stopped_result() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();
        let result = timing
            .accept_test_result::<()>(Err(PreparationError::Stopped(ShutdownCause::TotalTimeout)))
            .await;

        let actual = matches!(
            result,
            Err(PreparationError::Stopped(ShutdownCause::Cancellation))
        );

        assert!(actual);
    }
    #[derive(Debug)]
    struct DisposalWitness(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for DisposalWitness {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[tokio::test]
    async fn preparation_disposes_successful_result_delivered_after_interrupt() {
        let control = RunControl::new();
        let ready = Arc::new(tokio::sync::Notify::new());
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone())
            .with_delivery_gate(Arc::clone(&ready), release_rx);
        let (sender, mut interrupts) = preparation_monitor();
        let disposed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let owner = DisposalWitness(Arc::clone(&disposed));
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                Ok(Box::new(owner) as TestPreparationOwner)
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), ready.notified())
            .await
            .unwrap();
        let retained_before_delivery = !disposed.load(Ordering::Acquire);
        sender.send(Ok(())).unwrap();
        control.cancelled().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();
        let disposed_after_delivery = disposed.load(Ordering::Acquire);
        let cancelled = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(retained_before_delivery);
        assert!(disposed_after_delivery);
        assert!(cancelled);
    }
    #[tokio::test]
    async fn preparation_run_reports_worker_panic_during_interrupt_shutdown() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                panic!("owned worker panicked during shutdown");
            })
            .await
        });
        started_rx.await.unwrap();
        sender.send(Ok(())).unwrap();
        control.cancelled().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();

        let actual = matches!(error, PreparationError::Failed(message) if message.contains("preparation task failed:"));

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_run_preserves_operation_failure_after_interrupt() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                Err(PreparationError::Failed("owned operation failed".into()))
            })
            .await
        });
        started_rx.await.unwrap();
        sender.send(Ok(())).unwrap();
        control.cancelled().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();

        let actual = matches!(error, PreparationError::Failed(message) if message == "owned operation failed");

        assert!(actual);
    }
    #[tokio::test]
    async fn preparation_reports_disposal_failure_after_checked_owner_delivery() {
        let control = RunControl::new();
        let ready = Arc::new(tokio::sync::Notify::new());
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone())
            .with_delivery_gate(Arc::clone(&ready), release_rx);
        let (sender, mut interrupts) = preparation_monitor();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, || {
                Ok(Box::new(PanicDuringDisposal) as TestPreparationOwner)
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), ready.notified())
            .await
            .unwrap();
        sender.send(Ok(())).unwrap();
        control.cancelled().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().err().unwrap();

        let actual = matches!(error, PreparationError::Failed(message) if message.contains("preparation rollback task failed:") && message.contains("owned preparation rollback panic"));

        assert!(actual);
    }
    #[rstest::rstest]
    #[case(Ok(7), Ok(()), Ok(()), Ok(7))]
    #[case(Err("run"), Ok(()), Ok(()), Err("run"))]
    #[case(Ok(7), Err("workspace"), Ok(()), Err("workspace cleanup failed: workspace"))]
    #[case(Ok(7), Ok(()), Err("process"), Err("process backend close failed: process"))]
    #[case(
        Ok(7),
        Err("workspace"),
        Err("process"),
        Err("workspace cleanup failed: workspace; process backend close failed: process")
    )]
    #[case(
        Err("run"),
        Err("workspace"),
        Err("process"),
        Err("run; workspace cleanup failed: workspace; process backend close failed: process")
    )]
    #[test]
    fn closing_resources_retains_primary_and_secondary_failure_order(
        #[case] run: Result<i32, &str>,
        #[case] workspace: Result<(), &str>,
        #[case] process: Result<(), &str>,
        #[case] expected: Result<i32, &str>,
    ) {
        let actual = combine_close_results(
            run.map_err(str::to_owned),
            workspace.map_err(str::to_owned),
            process.map_err(str::to_owned),
        );

        assert_eq!(actual, expected.map_err(str::to_owned));
    }

    fn initialized_metrics(stages: &[&str]) -> MetricsCollector {
        let mut collector = MetricsCollector::new("invalid-transition");
        for stage in stages {
            collector.begin_stage(*stage).unwrap();
        }
        collector
    }

    #[rstest::rstest]
    #[case(&[], RunPhase::Validate, RunPhase::Validate, true, false, false, "stage targets has not started")]
    #[case(&["targets", "preflight"], RunPhase::Validate, RunPhase::Validate, true, false, false, "stage preflight has already started")]
    #[case(&[], RunPhase::Validate, RunPhase::Validate, false, true, false, "stage preflight has not started")]
    #[case(&[], RunPhase::Copy, RunPhase::Analyze, false, false, false, "stage copy has not started")]
    #[case(&["copy", "analysis"], RunPhase::Copy, RunPhase::Analyze, false, false, false, "stage analysis has already started")]
    #[case(&[], RunPhase::Validate, RunPhase::Validate, false, false, true, "stage cleanup has not started")]
    #[test]
    fn invalid_transition_metrics_emit_the_failed_stage_warning(
        #[case] stages: &[&str],
        #[case] previous: RunPhase,
        #[case] next: RunPhase,
        #[case] targets_resolved: bool,
        #[case] preflight_completed: bool,
        #[case] cleanup_finished: bool,
        #[case] expected: &str,
    ) {
        let mut collector = Some(initialized_metrics(stages));
        let mut warnings = Vec::new();
        observe_accepted_transition(
            &mut collector,
            &mut warnings,
            previous,
            next,
            targets_resolved,
            preflight_completed,
            cleanup_finished,
        );

        assert_eq!(warnings, vec![("metrics.state", expected.to_owned())]);
    }
    #[allow(
        clippy::unnecessary_wraps,
        reason = "successful fixture implements the fallible preparation operation protocol"
    )]
    fn successful_preparation_operation() -> Result<TestPreparationOwner, PreparationError> {
        Ok(Box::new(()))
    }

    #[tokio::test]
    async fn preparation_run_returns_a_successful_owner() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let (_sender, mut interrupts) = preparation_monitor();
        let owner = run_preparation(&timing, &mut interrupts, successful_preparation_operation)
            .await
            .unwrap();

        assert_eq!(format!("{owner:?}"), "()");
    }

    #[tokio::test]
    async fn preparation_run_rejects_already_cancelled_operation() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        control.cancel();
        let error = run_preparation(&timing, &mut interrupts, successful_preparation_operation)
            .await
            .unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }

    #[derive(Debug)]
    struct BlockingDisposal {
        started: Arc<tokio::sync::Notify>,
        release: std::sync::mpsc::Receiver<()>,
        finished: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl Drop for BlockingDisposal {
        fn drop(&mut self) {
            self.started.notify_one();
            self.release.recv_timeout(Duration::from_secs(10)).unwrap();
            let _ = self.finished.take().unwrap().send(());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_disposal_timeout_retains_owner_until_drop_finishes() {
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let started = Arc::new(tokio::sync::Notify::new());
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
        let owner: TestPreparationOwner = Box::new(BlockingDisposal {
            started: Arc::clone(&started),
            release: release_rx,
            finished: Some(finished_tx),
        });
        let task = tokio::spawn(async move { timing.dispose(owner).await });
        started.notified().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        let error = task.await.unwrap().unwrap_err();
        let rollback_timed_out = matches!(error, PreparationError::Failed(message) if message == "preparation rollback exceeded shutdown grace");
        release_tx.send(()).unwrap();
        finished_rx.await.unwrap();

        assert!(rollback_timed_out);
    }
    #[derive(Clone, Copy)]
    enum CheckedPreparationResult {
        Owner,
        Stopped,
        Failed,
    }

    impl CheckedPreparationResult {
        fn into_result(self) -> Result<TestPreparationOwner, PreparationError> {
            match self {
                Self::Owner => successful_preparation_operation(),
                Self::Stopped => Err(PreparationError::Stopped(ShutdownCause::TotalTimeout)),
                Self::Failed => Err(PreparationError::Failed("owned checked failure".into())),
            }
        }
    }

    fn checked_preparation_token(cancelled: bool) -> PreparationToken {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        if cancelled {
            control.cancel();
        }
        timing
    }

    #[rstest::rstest]
    #[case(false, CheckedPreparationResult::Owner, "Ok(())")]
    #[case(false, CheckedPreparationResult::Stopped, "Err(Stopped(TotalTimeout))")]
    #[case(true, CheckedPreparationResult::Owner, "Err(Stopped(Cancellation))")]
    #[case(true, CheckedPreparationResult::Stopped, "Err(Stopped(Cancellation))")]
    #[case(
        true,
        CheckedPreparationResult::Failed,
        "Err(Failed(\"owned checked failure\"))"
    )]
    #[tokio::test]
    async fn checked_preparation_result_respects_cancellation_and_error_precedence(
        #[case] cancelled: bool,
        #[case] input: CheckedPreparationResult,
        #[case] expected: &str,
    ) {
        let timing = checked_preparation_token(cancelled);
        let result = timing.accept_test_result(input.into_result()).await;

        assert_eq!(format!("{result:?}"), expected);
    }

    #[tokio::test]
    async fn checked_preparation_owner_disposal_failure_survives_cancellation() {
        let timing = checked_preparation_token(true);
        let owner: TestPreparationOwner = Box::new(PanicDuringDisposal);
        let error = timing.accept_test_result(Ok(owner)).await.unwrap_err();

        let actual = matches!(error, PreparationError::Failed(message) if message.contains("owned preparation rollback panic"));

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_run_direct_cancellation_waits_for_checked_worker_stop() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                successful_preparation_operation()
            })
            .await
        });
        started_rx.await.unwrap();
        control.cancel();
        tokio::task::yield_now().await;
        release_tx.send(()).unwrap();
        let error = task.await.unwrap().unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }

    #[derive(Debug)]
    struct DropCompletion(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for DropCompletion {
        fn drop(&mut self) {
            let _ = self.0.take().unwrap().send(());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_worker_retains_owner_beyond_shutdown_grace() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (finished_tx, mut finished_rx) = tokio::sync::oneshot::channel();
        let owner: TestPreparationOwner = Box::new(DropCompletion(Some(finished_tx)));
        let task = tokio::spawn(async move {
            run_preparation(&timing, &mut interrupts, move || {
                blocked_preparation_operation(started_tx, &release_rx);
                Ok(owner)
            })
            .await
        });
        started_rx.await.unwrap();
        control.cancel();
        tokio::time::advance(Duration::from_secs(2)).await;
        let error = task.await.unwrap().unwrap_err();
        let shutdown_grace_expired = matches!(error, PreparationError::Failed(message) if message.contains("preparation shutdown grace expired"));
        let worker_still_pending = matches!(
            finished_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        );
        release_tx.send(()).unwrap();
        finished_rx.await.unwrap();

        assert!(shutdown_grace_expired);
        assert!(worker_still_pending);
    }
    #[tokio::test]
    async fn preparation_admit_returns_a_completed_future() {
        let timing = checked_preparation_token(false);
        let (_sender, mut interrupts) = preparation_monitor();
        let future = Box::pin(std::future::ready(()))
            as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

        let actual = timing.admit(&mut interrupts, future).await.is_ok();

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_admit_observes_direct_cancellation_while_future_is_pending() {
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let ready = Arc::new(tokio::sync::Notify::new());
        let observed = Arc::clone(&ready);
        let (_sender, mut interrupts) = preparation_monitor();
        let future = Box::pin(std::future::poll_fn(move |_| {
            observed.notify_one();
            std::task::Poll::Pending::<()>
        })) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
        let task = tokio::spawn(async move { timing.admit(&mut interrupts, future).await });
        ready.notified().await;
        control.cancel();
        let error = task.await.unwrap().unwrap_err();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );

        assert!(actual);
    }

    #[tokio::test]
    async fn preparation_report_preserves_cleanup_failure_after_cancellation() {
        let timing = checked_preparation_token(true);
        timing.record_failure("owned cleanup failure".into());
        let error = timing
            .report(
                timing.check().unwrap_err(),
                Box::new(DiagnosticWriter(DiagnosticFault::None)) as Box<dyn Write + Send>,
            )
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "cancellation during preparation; owned cleanup failure"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_report_does_not_write_after_shutdown_grace_expiry() {
        let timing = checked_preparation_token(false);
        timing.failure_budget();
        tokio::time::advance(Duration::from_secs(2)).await;
        let error = timing
            .report(
                PreparationError::Failed("owned primary failure".into()),
                Box::new(DiagnosticWriter(DiagnosticFault::PanicWrite)) as Box<dyn Write + Send>,
            )
            .await
            .unwrap_err();

        assert_eq!(error, "preparation failed: owned primary failure");
    }
    fn direct_process_request<C>(context: C) -> RunProcess<C> {
        RunProcess {
            id: EffectId(880),
            context,
            argv: successful_test_command()
                .iter()
                .map(|argument| dispatch_test_arg(argument))
                .collect(),
            cwd: Utf8PathBuf::from("."),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(5),
                max_output_bytes: 4096,
                max_memory_bytes: 512 * 1024 * 1024,
                max_processes: 16,
            },
        }
    }

    #[tokio::test]
    async fn direct_baseline_effect_runs_in_the_owned_worker_workspace() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let effect = RunEffect::RunBaseline(direct_process_request(hoimin_core::BaselineContext {
            worker: 0,
            run_id: "direct-baseline".into(),
        }));
        let event = execute_effect(&mut context, effect).await;

        let actual = matches!(event, RunEvent::BaselineFinished(value) if value.id == EffectId(880) && value.worker == Some(0) && value.termination == hoimin_core::ProcessTermination::Exit(0));

        assert!(actual);
    }

    #[tokio::test]
    async fn direct_mutant_effect_runs_in_the_owned_worker_workspace() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let effect = RunEffect::RunMutant(direct_process_request(hoimin_core::MutantContext {
            worker: 0,
            run_id: "direct-mutant".into(),
            mutant_id: "owned-mutant".into(),
        }));
        let event = execute_effect(&mut context, effect).await;

        let actual = matches!(event, RunEvent::MutantFinished(value) if value.id == EffectId(880) && value.worker == Some(0) && value.termination == hoimin_core::ProcessTermination::Exit(0));

        assert!(actual);
    }

    #[tokio::test]
    async fn direct_blocking_effect_preserves_missing_target_failure() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let event = execute_effect(
            &mut context,
            RunEffect::Preflight(Preflight { id: EffectId(881) }),
        )
        .await;

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.id == EffectId(881) && error.failure.code() == "shell.targets.missing");

        assert!(actual);
    }

    #[tokio::test]
    async fn direct_target_resolution_preserves_missing_file_failure() {
        let (project, _parent, mut context, _, _) = context_with_worker().await;
        let mut selection = shell_setup_test_config(&project).selection.clone();
        selection.files = vec![Utf8PathBuf::from("missing-selected.py")];
        let event = execute_effect(
            &mut context,
            RunEffect::ResolveTargets(hoimin_core::ResolveTargets {
                id: EffectId(882),
                selection,
            }),
        )
        .await;

        let actual = matches!(event, RunEvent::EffectFailed(error) if error.id == EffectId(882) && error.failure.code() == "target.resolve");

        assert!(actual);
    }
    type RootSetupResult = Result<
        (
            SetupRollback,
            Arc<ManagedRunRoot>,
            Arc<ManagedRunRoot>,
            Arc<ManagedChild>,
            Arc<ManagedChild>,
            ReclaimReport,
        ),
        PreparationError,
    >;

    fn create_roots_with_cancellation(
        parent: &Utf8Path,
        control: &RunControl,
        selected: Option<ManagedRootSetupBoundary>,
        timing: &PreparationToken,
    ) -> RootSetupResult {
        let boundary: Box<dyn Fn(ManagedRootSetupBoundary) + '_> = Box::new(move |observed| {
            if selected == Some(observed) {
                control.cancel();
            }
        });
        create_managed_shell_roots_controlled(parent, &boundary, Some(timing))
    }

    fn root_setup_residual(parent: &Utf8Path) -> Vec<OsString> {
        std::fs::read_dir(parent.join("hoimin-workspaces-v2"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != ".hoimin-coordinator")
            .collect()
    }

    #[rstest::rstest]
    #[case(ManagedRootSetupBoundary::CoordinatorOpened)]
    #[case(ManagedRootSetupBoundary::ReclaimFinished)]
    #[case(ManagedRootSetupBoundary::ExecutionPublished)]
    #[case(ManagedRootSetupBoundary::DeliveryPublished)]
    #[case(ManagedRootSetupBoundary::ExecutionSpoolCreated)]
    #[case(ManagedRootSetupBoundary::DeliverySpoolCreated)]
    #[test]
    fn managed_root_preparation_cancellation_removes_published_owners(
        #[case] boundary: ManagedRootSetupBoundary,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(directory.path()).unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let error = create_roots_with_cancellation(parent, &control, Some(boundary), &timing)
            .err()
            .unwrap();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );
        let actual_2 = root_setup_residual(parent);

        assert!(actual);
        assert_eq!(actual_2, Vec::<OsString>::new());
    }

    #[tokio::test]
    async fn managed_root_preparation_rejects_cancelled_work_before_publication() {
        let directory = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(directory.path()).unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();
        let error = create_roots_with_cancellation(parent, &control, None, &timing)
            .err()
            .unwrap();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );
        let actual_2 = !parent.join("hoimin-workspaces-v2").exists();

        assert!(actual);
        assert!(actual_2);
    }

    #[tokio::test]
    async fn managed_root_preparation_rejects_a_file_as_the_temporary_parent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file-parent");
        std::fs::write(&path, b"owned parent blocker").unwrap();
        let parent = Utf8Path::from_path(&path).unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let error = create_roots_with_cancellation(parent, &control, None, &timing)
            .err()
            .unwrap();

        let actual =
            matches!(error, PreparationError::Failed(message) if message.contains("file-parent"));
        let actual_2 = std::fs::read(path).unwrap();

        assert!(actual);
        assert_eq!(actual_2, b"owned parent blocker");
    }

    #[tokio::test]
    async fn managed_root_preparation_uncommitted_success_rolls_back_both_roots() {
        let directory = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(directory.path()).unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (rollback, execution, delivery, execution_spool, delivery_spool, _) =
            create_roots_with_cancellation(parent, &control, None, &timing).unwrap();
        let execution_present_before_rollback = execution.path().exists();
        let delivery_present_before_rollback = delivery.path().exists();
        drop((
            execution_spool,
            delivery_spool,
            execution,
            delivery,
            rollback,
        ));
        let remaining_roots = root_setup_residual(parent);

        assert!(execution_present_before_rollback);
        assert!(delivery_present_before_rollback);
        assert_eq!(remaining_roots, Vec::<OsString>::new());
    }
    #[rstest::rstest]
    #[case(owner_io_faults::Stage::ExecutionRoot)]
    #[case(owner_io_faults::Stage::DeliveryRoot)]
    #[case(owner_io_faults::Stage::ExecutionSpool)]
    #[case(owner_io_faults::Stage::DeliverySpool)]
    #[test]
    fn managed_owner_creation_failure_rolls_back_and_allows_retry(
        #[case] stage: owner_io_faults::Stage,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(directory.path()).unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let _fault = owner_io_faults::Guard::arm(stage);
        let error = create_roots_with_cancellation(parent, &control, None, &timing)
            .err()
            .unwrap();
        let creation_failed = matches!(error, PreparationError::Failed(message) if message.contains(&format!("injected shell owner operation: {stage:?}")));
        let roots_after_failure = root_setup_residual(parent);
        let retry_timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (rollback, execution, delivery, execution_spool, delivery_spool, _) =
            create_roots_with_cancellation(parent, &control, None, &retry_timing).unwrap();
        drop((
            execution_spool,
            delivery_spool,
            execution,
            delivery,
            rollback,
        ));
        let roots_after_retry = root_setup_residual(parent);

        assert!(creation_failed);
        assert_eq!(roots_after_failure, Vec::<OsString>::new());
        assert_eq!(roots_after_retry, Vec::<OsString>::new());
    }
    fn runtime_setup_config(project: &TempDir) -> RunConfig {
        shell_setup_test_config(project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap()
    }

    #[tokio::test]
    async fn shell_preparation_rejects_cancellation_before_creating_roots() {
        let project = tempfile::tempdir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(directory.path()).unwrap();
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();
        let error = prepare_shell_setup_controlled(
            runtime_setup_config(&project),
            parent,
            &std::mem::drop::<ShellSetupBoundary>,
            Some(&timing),
        )
        .err()
        .unwrap();

        let actual = matches!(
            error,
            PreparationError::Stopped(ShutdownCause::Cancellation)
        );
        let actual_2 = !parent.join("hoimin-workspaces-v2").exists();

        assert!(actual);
        assert!(actual_2);
    }

    #[tokio::test]
    async fn synchronous_shell_preparation_preserves_parent_creation_failure() {
        let project = tempfile::tempdir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owned-file-parent");
        std::fs::write(&path, b"owned shell parent").unwrap();
        let error = prepare_shell_setup_sync_in(
            runtime_setup_config(&project),
            Utf8Path::from_path(&path).unwrap(),
            &std::mem::drop::<ShellSetupBoundary>,
        )
        .err()
        .unwrap();

        let actual = error.contains("owned-file-parent");
        let actual_2 = std::fs::read(path).unwrap();

        assert!(actual, "{error}");
        assert_eq!(actual_2, b"owned shell parent");
    }

    #[tokio::test]
    async fn synchronous_root_preparation_preserves_parent_creation_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owned-root-parent");
        std::fs::write(&path, b"owned root parent").unwrap();
        let error = create_managed_shell_roots_in(
            Utf8Path::from_path(&path).unwrap(),
            &std::mem::drop::<ManagedRootSetupBoundary>,
        )
        .err()
        .unwrap();

        let actual = error.contains("owned-root-parent");
        let actual_2 = std::fs::read(path).unwrap();

        assert!(actual, "{error}");
        assert_eq!(actual_2, b"owned root parent");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn shell_preparation_rejects_unapproved_portable_memory_and_rolls_back_roots() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), b"pass\n").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let parent = Utf8Path::from_path(directory.path()).unwrap();
        let config = parse_host_independent_shell_test_config(vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--"),
            OsString::from("unused-command"),
        ])
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
        let error =
            prepare_shell_setup_sync_in(config, parent, &std::mem::drop::<ShellSetupBoundary>)
                .err()
                .unwrap();

        let actual = error.contains("require --allow-best-effort-memory");
        let actual_2 = root_setup_residual(parent);

        assert!(actual, "{error}");
        assert_eq!(actual_2, Vec::<OsString>::new());
    }
    #[test]
    fn default_run_control_has_no_completions_in_flight() {
        let actual = RunControl::new().max_completion_in_flight();

        assert_eq!(actual, 0);
    }

    #[test]
    fn shell_disk_lifecycle_rejects_duplicate_logical_roots() {
        let error =
            ShellDiskLifecycle::new([DiskRootId::Execution, DiskRootId::Execution]).unwrap_err();

        assert_eq!(
            error,
            hoimin_core::DiskLifecycleError::DuplicateRoot(DiskRootId::Execution)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn resource_cleanup_preserves_portable_backend_close_failure() {
        let (_project, parent, mut context, _, _) = context_with_worker().await;
        context.process = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests_with_close_failure()),
            Utf8PathBuf::from_path_buf(parent.path().join("owned-close-output")).unwrap(),
        ));
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let result =
            close_context_resources(&mut context, &budget, false, true, Box::new(|| {})).await;

        let actual = result
            .process
            .unwrap_err()
            .contains("injected portable close failure");

        assert!(actual);
        assert_eq!(result.workspace, Ok(()));
        assert_eq!(result.expiry, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn resource_cleanup_rejects_replaced_worker_and_preserves_original_owner() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let root = context
            .workspace()
            .worker(0)
            .unwrap()
            .root()
            .parent()
            .unwrap()
            .to_owned();
        let moved = root.with_extension("owned-original");
        std::fs::rename(&root, &moved).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("replacement-sentinel"), b"owned replacement").unwrap();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let result =
            close_context_resources(&mut context, &budget, false, true, Box::new(|| {})).await;
        let error = result.workspace.unwrap_err();

        let actual = error.contains(root.as_str());
        let actual_2 = std::fs::read(root.join("replacement-sentinel")).unwrap();
        let actual_3 = std::fs::read(moved.join("workspace/pkg/a.py")).unwrap();

        assert!(actual, "{error}");
        assert_eq!(result.process, Ok(()));
        assert_eq!(actual_2, b"owned replacement");
        assert_eq!(actual_3, b"original\n");
    }
    #[tokio::test]
    async fn final_measurement_pause_returns_when_its_controller_has_gone() {
        let (pause, controller) = FinalMeasurementPause::with_timeout(Duration::from_secs(5));
        drop(controller);
        let task = tokio::task::spawn_blocking(move || pause.wait());

        let actual = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .is_ok();

        assert!(actual);
    }

    #[tokio::test]
    async fn no_process_completion_does_not_request_a_failing_disk_sample() {
        let (_project, _parent, context, _, _) = context_with_worker().await;
        let monitor = DiskMonitor::start_unmeasured_with_meter(
            FailingDiskMeter,
            DiskPolicy {
                max_owned_bytes: context.config.limits.max_workspace_size,
                min_free_bytes: context.config.limits.min_free_space,
            },
            context.spool_dir.monitor_roots(),
            Duration::from_secs(60),
        )
        .unwrap();
        let sample = sample_after_process_drain(&monitor, false).await;

        let actual = sample.is_none();
        let actual_2 = monitor.stop_and_join(Duration::from_secs(5)).await;

        assert!(actual);
        assert!(actual_2);
    }
    #[tokio::test]
    async fn draining_owned_blocking_event_delivers_it_once_and_releases_its_slot() {
        let (_project, _parent, mut context, _, _) = context_with_worker().await;
        let (sender, mut receiver) = mpsc::channel(1);
        let mut process_tasks = JoinSet::new();
        let mut io_tasks = JoinSet::new();
        spawn_blocking_effect(
            BlockingEffect::TestOperation {
                id: EffectId(883),
                operation: Box::new(|| RunEvent::CancellationRequested),
            },
            sender,
            &mut io_tasks,
            false,
        );
        let mut in_flight = 1;
        let mut expired = false;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let mut events = Vec::new();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let result = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expired,
            &mut metrics,
            &mut warnings,
            |completion| events.push(accept_blocking_completion(&mut context, completion)),
        )
        .await;

        let actual = io_tasks.is_empty();

        assert_eq!(result, Ok(()));
        assert_eq!(events, vec![RunEvent::CancellationRequested]);
        assert_eq!(in_flight, 0);
        assert!(!expired);
        assert!(actual);
    }
    #[cfg(unix)]
    #[derive(Clone, Copy)]
    enum ObservedDiskFault {
        Panic,
        Io,
    }

    #[cfg(unix)]
    struct PanicAtOwnedBoundaryMeter {
        reached: Box<dyn Fn() -> bool + Send + Sync>,
        fault: ObservedDiskFault,
    }

    #[cfg(unix)]
    impl DiskMeasurement for PanicAtOwnedBoundaryMeter {
        fn measure(&self) -> std::io::Result<MeterReading> {
            if (self.reached)() {
                return match self.fault {
                    ObservedDiskFault::Panic => {
                        panic!("owned disk monitor panic at an observed boundary")
                    }
                    ObservedDiskFault::Io => Err(std::io::Error::other(
                        "owned disk monitor measurement failure",
                    )),
                };
            }
            Ok(MeterReading {
                owned_bytes: 0,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: Duration::from_millis(1),
            })
        }
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panicking_disk_monitor_interrupts_an_owned_process_task() {
        let project = tempfile::tempdir().unwrap();
        let mut data = output_failure_test_config(&project).into_data();
        data.test_argv = long_running_test_command()
            .iter()
            .map(|argument| dispatch_test_arg(argument))
            .collect();
        let config = RunConfig::try_from(data)
            .unwrap()
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut control = RunControl::new();
        control.disk_sample_interval = Duration::from_millis(1);
        let measuring = control.clone();
        control.disk_meter = Some(TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(
            PanicAtOwnedBoundaryMeter {
                reached: Box::new(move || measuring.max_process_tasks() > 0),
                fault: ObservedDiskFault::Panic,
            },
        )))));
        let observed = control.clone();
        let error = tokio::time::timeout(
            Duration::from_secs(10),
            run_loop_prepared(
                config,
                Vec::new(),
                Vec::new(),
                control,
                hoimin_core::RunSelection::all(),
                None,
            ),
        )
        .await
        .unwrap()
        .unwrap_err();

        let actual = error.contains("disk monitor stop channel closed");
        let actual_2 = observed.max_process_tasks() > 0;
        let actual_3 = !observed
            .finalization_events()
            .contains(&"output_acknowledged");

        assert!(actual, "{error}");
        assert!(actual_2);
        assert!(actual_3);
    }
    #[cfg(unix)]
    struct MonitorPanicSerialWriter {
        record: Vec<u8>,
        reached: Arc<std::sync::atomic::AtomicBool>,
        release: Option<std::sync::mpsc::Receiver<()>>,
    }

    #[cfg(unix)]
    impl Write for MonitorPanicSerialWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.record.extend_from_slice(bytes);
            if self
                .record
                .windows(b"baseline_finished".len())
                .any(|part| part == b"baseline_finished")
                && let Some(release) = self.release.take()
            {
                self.reached.store(true, Ordering::Release);
                release
                    .recv_timeout(Duration::from_secs(10))
                    .expect("owned serial writer must be released");
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.record.clear();
            Ok(())
        }
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panicking_disk_monitor_interrupts_serial_output_without_acknowledgement() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let measuring = Arc::clone(&reached);
        let mut control = RunControl::new();
        control.disk_sample_interval = Duration::from_millis(1);
        control.disk_meter = Some(TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(
            PanicAtOwnedBoundaryMeter {
                reached: Box::new(move || measuring.load(Ordering::Acquire)),
                fault: ObservedDiskFault::Panic,
            },
        )))));
        let observed = control.clone();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = MonitorPanicSerialWriter {
            record: Vec::new(),
            reached,
            release: Some(release_rx),
        };
        let mut run = Box::pin(run_loop_prepared(
            config,
            writer,
            Vec::new(),
            control,
            hoimin_core::RunSelection::all(),
            None,
        ));
        let result = tokio::time::timeout(Duration::from_secs(10), &mut run).await;
        let retained = observed.managed_root_paths()[1].exists();
        release_tx.send(()).unwrap();
        drop(run);
        let error = result.unwrap().unwrap_err();

        let actual = error.contains("disk monitor stop channel closed");
        let actual_2 = !observed
            .finalization_events()
            .contains(&"output_acknowledged");

        assert!(actual, "{error}");
        assert!(retained);
        assert!(actual_2);
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn measurement_failure_during_serial_output_finishes_owned_delivery() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let measuring = Arc::clone(&reached);
        let mut control = RunControl::new();
        control.disk_sample_interval = Duration::from_millis(1);
        control.disk_meter = Some(TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(
            PanicAtOwnedBoundaryMeter {
                reached: Box::new(move || measuring.load(Ordering::Acquire)),
                fault: ObservedDiskFault::Io,
            },
        )))));
        let observed = control.clone();
        let releasing = control.clone();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = MonitorPanicSerialWriter {
            record: Vec::new(),
            reached,
            release: Some(release_rx),
        };
        let mut run = Box::pin(run_loop_prepared(
            config,
            writer,
            Vec::new(),
            control,
            hoimin_core::RunSelection::all(),
            None,
        ));
        let release = async move {
            super::coverage_tests::wait_for_finalization_event(
                &releasing,
                "serial_disk_stop_observed",
            )
            .await;
            release_tx.send(()).unwrap();
        };
        let completed = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(&mut run, release)
        })
        .await;
        drop(run);
        let (result, ()) = completed.unwrap();

        let actual = result.unwrap();
        let actual_2 = observed.finalization_events().contains(&"disk_stop");
        let actual_3 = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        let actual_4 = observed
            .managed_root_paths()
            .iter()
            .all(|path| !path.exists());

        assert_eq!(actual, 2);
        assert!(actual_2);
        assert!(actual_3);
        assert!(actual_4);
    }
    #[tokio::test]
    async fn elapsed_deadline_after_preparation_stops_before_first_effect_dispatch() {
        let project = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let config = runtime_setup_config(&project);
        let control = RunControl::new();
        let observed = control.clone();
        let context = ShellContext::new_in(
            &config,
            Vec::new(),
            Vec::new(),
            Utf8PathBuf::from_path_buf(parent.path().to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let monitor = DiskMonitor::start_unmeasured(
            DiskPolicy {
                max_owned_bytes: config.limits.max_workspace_size,
                min_free_bytes: config.limits.min_free_space,
            },
            context.spool_dir.monitor_roots(),
        )
        .unwrap();
        let mut timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        timing.deadline = tokio::time::Instant::now();
        let (_sender, interrupts) = preparation_monitor();
        let result = run_loop_context(
            config,
            context,
            control,
            hoimin_core::RunSelection::all(),
            None,
            timing,
            interrupts,
            monitor,
        )
        .await;

        let actual = result.unwrap();
        let actual_2 = observed.max_process_tasks();

        assert_eq!(actual, 4);
        assert_eq!(actual_2, 0);
    }
    // macOS native filesystems reject this byte name with EILSEQ; Linux permits it.
    #[cfg(target_os = "linux")]
    #[test]
    fn session_artifact_preparation_rejects_a_non_utf8_resolved_file_name() {
        use std::os::unix::ffi::OsStringExt as _;
        let project = tempfile::tempdir().unwrap();
        let mut data = runtime_setup_config(&project).into_data();
        let physical = project
            .path()
            .join(OsString::from_vec(b"session-\xff.sqlite3".to_vec()));
        std::fs::write(&physical, b"owned session artifact").unwrap();
        let logical = project.path().join("session-link.sqlite3");
        std::os::unix::fs::symlink(&physical, &logical).unwrap();
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(logical).unwrap(),
        });
        let config = RunConfig::try_from(data).unwrap();

        let actual = prepare_session_artifacts(&config).unwrap_err();
        let actual_2 = std::fs::read(physical).unwrap();

        assert_eq!(actual, "session artifact path is not UTF-8");
        assert_eq!(actual_2, b"owned session artifact");
    }

    // macOS native filesystems reject this byte name with EILSEQ; Linux permits it.
    #[cfg(target_os = "linux")]
    #[test]
    fn session_artifact_preparation_rejects_a_non_utf8_resolved_database_parent() {
        use std::os::unix::ffi::OsStringExt as _;
        let project = tempfile::tempdir().unwrap();
        let mut data = runtime_setup_config(&project).into_data();
        let physical = project
            .path()
            .join(OsString::from_vec(b"project-\xff".to_vec()));
        std::fs::create_dir(&physical).unwrap();
        let logical = project.path().join("logical-project");
        std::os::unix::fs::symlink(&physical, &logical).unwrap();
        data.selection.root = Utf8PathBuf::from_path_buf(logical.clone()).unwrap();
        data.session = hoimin_core::SessionMode::Fresh(hoimin_core::SessionConfig {
            path: Utf8PathBuf::from_path_buf(logical.join("session.sqlite3")).unwrap(),
        });
        let config = RunConfig::try_from(data).unwrap();

        let actual = prepare_session_artifacts(&config).unwrap_err();
        let actual_2 = !physical.join("session.sqlite3").exists();

        assert_eq!(actual, "session database path is not UTF-8");
        assert!(actual_2);
    }
    #[tokio::test]
    async fn verified_plan_elapsed_preparation_deadline_reports_incomplete_without_baseline() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("target.py"),
            "def compute():\n    return 1 + 2\n",
        )
        .unwrap();
        let config = parse_host_independent_shell_test_config(vec![
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--total-timeout"),
            OsString::from("1ns"),
            OsString::from("--"),
            OsString::from("unused-baseline-command"),
        ]);
        let output = crate::plan::create(
            config,
            hoimin_core::DiscoveryConfig::new(Duration::from_secs(10)).unwrap(),
        )
        .await
        .unwrap();
        let selected = output.manifest.candidates[0].id.clone();
        let artifact = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            artifact.path(),
            serde_json::to_vec(&output.manifest).unwrap(),
        )
        .unwrap();
        let verified = crate::plan::prepare_verify(artifact.path(), &[selected])
            .await
            .unwrap();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let result = verified.execute(stdout.clone(), stderr.clone()).await;

        let actual = result.unwrap();
        let actual_2 = stdout.snapshot();
        let actual_3 = String::from_utf8(stderr.snapshot())
            .unwrap()
            .contains("during preparation; baseline was not started");

        assert_eq!(actual, 4);
        assert_eq!(actual_2, Vec::<u8>::new());
        assert!(actual_3);
    }
    #[rstest::rstest]
    #[case::cleanup_ready_thread(0, false, true)]
    #[case::root_deletion_thread(1, true, false)]
    #[tokio::test]
    async fn managed_cleanup_thread_start_failure_retains_root_and_reports_its_stage(
        #[case] preceding_spawns: usize,
        #[case] marker_exists: bool,
        #[case] marker_failure_detail: bool,
    ) {
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let fault = crate::io_faults::arm_after("cleanup.thread.spawn", preceding_spawns);
        let outcome = cleanup_managed_root(Arc::clone(&root), &budget).await;
        let fault_triggered = fault.triggered();
        let failed_disposition = outcome.record.disposition;
        let spawn_failure_reported = outcome.error.contains(
            "cleanup thread could not start: injected owned I/O failure: cleanup.thread.spawn",
        );
        let root_retained = root.path().exists();
        let cleanup_marker_present = root.path().join(".hoimin-cleanup-ready.json").exists();
        let cleanup_evidence_present = outcome
            .record
            .evidence
            .details
            .iter()
            .any(|detail| detail.starts_with("cleanup-ready: "));
        drop(fault);
        let retried_disposition = root.cleanup(Duration::from_secs(5)).disposition;

        assert!(fault_triggered);
        assert_eq!(failed_disposition, CleanupDisposition::Failed);
        assert!(spawn_failure_reported, "{}", outcome.error);
        assert!(root_retained);
        assert_eq!(cleanup_marker_present, marker_exists);
        assert_eq!(cleanup_evidence_present, marker_failure_detail);
        assert_eq!(retried_disposition, CleanupDisposition::Clean);
    }
    #[tokio::test]
    async fn process_release_wait_remains_pending_until_the_last_owner_drops() {
        let output = tempfile::tempdir().unwrap();
        let process = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap(),
        ));
        let weak = Arc::downgrade(&process);
        let mut waiting = Box::pin(wait_until_process_handler_is_released(&weak));
        let first_poll = std::future::poll_fn(|context| {
            std::task::Poll::Ready(std::future::Future::poll(waiting.as_mut(), context))
        })
        .await;
        let pending_before_release = first_poll.is_pending();
        drop(process);
        waiting.await;
        let owner_gone_after_release = weak.upgrade().is_none();

        assert!(pending_before_release);
        assert!(owner_gone_after_release);
    }
    #[rstest::rstest]
    #[case::native(RunControl::new)]
    #[case::configured_meter(|| RunControl::with_disk_meter(FailingDiskMeter))]
    #[test]
    fn disk_monitor_start_failure_preserves_its_diagnostic_and_rolls_back_roots(
        #[case] make_control: fn() -> RunControl,
    ) {
        let project = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let prepared = prepare_shell_setup_sync_in(
            runtime_setup_config(&project),
            Utf8Path::from_path(parent.path()).unwrap(),
            &std::mem::drop::<ShellSetupBoundary>,
        )
        .unwrap();
        let roots = prepared.spool_dir.paths();
        let control = make_control();
        let fault = crate::io_faults::arm("shell.disk_monitor.start");
        let error = begin_disk_monitor(&prepared, &control).err().unwrap();
        let fault_triggered = fault.triggered();
        let monitor_error = (error).clone();
        drop(fault);
        drop(prepared);
        let roots_removed = roots.iter().all(|root| !root.exists());

        assert!(fault_triggered);
        assert_eq!(
            monitor_error,
            "disk.measurement.failed: injected owned I/O failure: shell.disk_monitor.start"
        );
        assert!(roots_removed);
    }
    #[tokio::test]
    async fn asynchronous_preparation_monitor_failure_preserves_error_and_removes_owned_roots() {
        let project = tempfile::tempdir().unwrap();
        let config = runtime_setup_config(&project);
        let mut control = RunControl::new();
        control.preparation_io_fault = Some("shell.disk_monitor.start");
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let error = prepare_loop_context(&config, &control, &timing, &mut interrupts)
            .await
            .err()
            .unwrap();
        let monitor_start_failed = matches!(error, PreparationError::Failed(message) if message == "disk.measurement.failed: injected owned I/O failure: shell.disk_monitor.start");
        let parent = Utf8Path::from_path(control.managed_parent.path()).unwrap();
        let remaining_roots = root_setup_residual(parent);

        assert!(monitor_start_failed);
        assert_eq!(remaining_roots, Vec::<OsString>::new());
    }
    struct HeldInitialMeasurement {
        entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
        closed: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl DiskMeasurement for HeldInitialMeasurement {
        fn measure(&self) -> std::io::Result<MeterReading> {
            self.entered
                .lock()
                .unwrap()
                .take()
                .expect("only the initial sample is requested")
                .send(())
                .unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("owned initial sample must be released");
            Ok(MeterReading {
                owned_bytes: 0,
                available_by_filesystem: BTreeMap::from([(FilesystemKey(7), u64::MAX)]),
                conservative_entries: false,
                elapsed: Duration::from_millis(1),
            })
        }
    }

    impl Drop for HeldInitialMeasurement {
        fn drop(&mut self) {
            let _ = self.closed.take().unwrap().send(());
        }
    }

    #[tokio::test]
    async fn cancelled_initial_measurement_joins_before_rolling_back_prepared_roots() {
        let project = tempfile::tempdir().unwrap();
        let config = runtime_setup_config(&project);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (closed_tx, _closed_rx) = tokio::sync::oneshot::channel();
        let mut control = RunControl::with_disk_meter(HeldInitialMeasurement {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
            closed: Some(closed_tx),
        });
        control.disk_sample_interval = Duration::from_secs(60);
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let preparation = Box::pin(prepare_loop_context(
            &config,
            &control,
            &timing,
            &mut interrupts,
        ));
        let cancel = async {
            entered_rx.await.unwrap();
            control.cancel();
            release_tx.send(()).unwrap();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(preparation, cancel)
        })
        .await
        .unwrap();
        let cancelled = matches!(
            result,
            Err(PreparationError::Stopped(ShutdownCause::Cancellation))
        );
        let parent = Utf8Path::from_path(control.managed_parent.path()).unwrap();
        let remaining_roots = root_setup_residual(parent);
        let peak_process_tasks = control.max_process_tasks();

        assert!(cancelled);
        assert_eq!(remaining_roots, Vec::<OsString>::new());
        assert_eq!(peak_process_tasks, 0);
    }

    #[tokio::test]
    async fn unjoined_initial_measurement_retains_roots_until_its_owned_worker_exits() {
        let project = tempfile::tempdir().unwrap();
        let config = runtime_setup_config(&project);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
        let mut control = RunControl::with_disk_meter(HeldInitialMeasurement {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
            closed: Some(closed_tx),
        });
        control.shutdown_grace = Duration::from_millis(100);
        control.disk_sample_interval = Duration::from_secs(60);
        let parent_owner = Arc::clone(&control.managed_parent);
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let preparation = Box::pin(prepare_loop_context(
            &config,
            &control,
            &timing,
            &mut interrupts,
        ));
        let cancel = async {
            entered_rx.await.unwrap();
            control.cancel();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(preparation, cancel)
        })
        .await
        .unwrap();
        let error = result.err().unwrap();
        let join_grace_expired = matches!(error, PreparationError::Failed(message) if message.contains("preparation disk monitor did not join before shutdown grace; live measurement retains its roots"));
        let parent = Utf8Path::from_path(parent_owner.path()).unwrap();
        let roots_before_worker_exit = root_setup_residual(parent).len();
        release_tx.send(()).unwrap();
        drop(timing);
        drop(control);
        tokio::time::timeout(Duration::from_secs(5), closed_rx)
            .await
            .unwrap()
            .unwrap();
        let roots_after_worker_exit = root_setup_residual(parent).len();

        assert!(join_grace_expired);
        assert_eq!(roots_before_worker_exit, 2);
        assert_eq!(roots_after_worker_exit, 2);
    }
    #[rstest::rstest]
    #[case::before_environment(RunPreparationCheckpoint::BeforeEnvironment)]
    #[case::after_environment(RunPreparationCheckpoint::AfterEnvironment)]
    #[case::after_snapshot(RunPreparationCheckpoint::AfterSnapshot)]
    #[case::after_inputs(RunPreparationCheckpoint::AfterInputs)]
    #[tokio::test]
    async fn configuration_preparation_cancellation_stops_before_baseline(
        #[case] checkpoint: RunPreparationCheckpoint,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let mut control = RunControl::new();
        control.cancel_preparation_at = Some(checkpoint);
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let result = Box::pin(run_loop_with_control(
            config,
            stdout.clone(),
            stderr.clone(),
            control,
        ))
        .await;

        let actual = result.unwrap();
        let actual_2 = stdout.snapshot();
        let actual_3 = String::from_utf8(stderr.snapshot())
            .unwrap()
            .contains("cancellation during preparation; baseline was not started");
        let actual_4 = observed.max_process_tasks();

        assert_eq!(actual, 130);
        assert_eq!(actual_2, Vec::<u8>::new());
        assert!(actual_3);
        assert_eq!(actual_4, 0);
    }

    #[tokio::test]
    async fn configuration_preparation_missing_fingerprint_input_reports_its_path() {
        let project = tempfile::tempdir().unwrap();
        let mut data = output_failure_test_config(&project).into_data();
        data.fingerprint_files = vec!["absent-fingerprint.lock".into()];
        let config = RunConfig::try_from(data).unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let error = Box::pin(run_loop_with_control(
            config,
            stdout.clone(),
            stderr.clone(),
            control,
        ))
        .await
        .unwrap_err();

        let actual = error.contains("preparation failed");
        let actual_2 = error.contains("absent-fingerprint.lock");
        let actual_3 = stdout.snapshot();
        let actual_4 = String::from_utf8(stderr.snapshot())
            .unwrap()
            .contains("absent-fingerprint.lock");
        let actual_5 = observed.max_process_tasks();

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
        assert_eq!(actual_3, Vec::<u8>::new());
        assert!(actual_4);
        assert_eq!(actual_5, 0);
    }
    #[tokio::test]
    async fn cancellation_after_owned_root_setup_rolls_back_before_monitor_start() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let mut control = RunControl::new();
        control.cancel_preparation_at = Some(RunPreparationCheckpoint::AfterRoots);
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let result = Box::pin(run_loop_with_control(
            config,
            stdout.clone(),
            stderr.clone(),
            control,
        ))
        .await;
        let exit_code = result.unwrap();
        let output = stdout.snapshot();
        let cancelled_before_baseline = String::from_utf8(stderr.snapshot())
            .unwrap()
            .contains("cancellation during preparation; baseline was not started");
        let parent = Utf8Path::from_path(observed.managed_parent.path()).unwrap();
        let remaining_roots = root_setup_residual(parent);
        let peak_process_tasks = observed.max_process_tasks();

        assert_eq!(exit_code, 130);
        assert_eq!(output, Vec::<u8>::new());
        assert!(cancelled_before_baseline);
        assert_eq!(remaining_roots, Vec::<OsString>::new());
        assert_eq!(peak_process_tasks, 0);
    }

    #[tokio::test]
    async fn asynchronous_preparation_root_setup_failure_preserves_bootstrap_diagnostic() {
        let project = tempfile::tempdir().unwrap();
        let config = runtime_setup_config(&project);
        let mut control = RunControl::new();
        control.preparation_io_fault = Some("bootstrap.canonicalize");
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (_sender, mut interrupts) = preparation_monitor();
        let error = prepare_loop_context(&config, &control, &timing, &mut interrupts)
            .await
            .err()
            .unwrap();

        let actual = matches!(error, PreparationError::Failed(message) if message.contains("canonicalize managed-root parent") && message.contains("injected owned I/O failure: bootstrap.canonicalize"));
        let actual_2 = !control
            .managed_parent
            .path()
            .join("hoimin-workspaces-v2")
            .exists();

        assert!(actual);
        assert!(actual_2);
    }
    #[tokio::test]
    async fn rollback_timeout_after_joined_initial_measurement_preserves_owned_roots() {
        let project = tempfile::tempdir().unwrap();
        let config = runtime_setup_config(&project);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
        let mut control = RunControl::with_disk_meter(HeldInitialMeasurement {
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(release_rx),
            closed: Some(closed_tx),
        });
        control.shutdown_grace = Duration::from_secs(1);
        control.disk_sample_interval = Duration::from_secs(60);
        let parent_owner = Arc::clone(&control.managed_parent);
        let dispose_ready = Arc::new(tokio::sync::Notify::new());
        let (dispose_release_tx, dispose_release_rx) = std::sync::mpsc::channel();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone())
            .with_disposal_gate(Arc::clone(&dispose_ready), dispose_release_rx);
        let (_sender, mut interrupts) = preparation_monitor();
        let preparation = Box::pin(prepare_loop_context(
            &config,
            &control,
            &timing,
            &mut interrupts,
        ));
        let cancel = async {
            entered_rx.await.unwrap();
            control.cancel();
            release_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), dispose_ready.notified())
                .await
                .unwrap();
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(preparation, cancel)
        })
        .await
        .unwrap();
        let error = result.err().unwrap();
        let rollback_timed_out = matches!(error, PreparationError::Failed(message) if message == "preparation rollback exceeded shutdown grace");
        let parent = Utf8Path::from_path(parent_owner.path()).unwrap();
        let roots_before_drop_finishes = root_setup_residual(parent).len();
        dispose_release_tx.send(()).unwrap();
        drop(timing);
        drop(control);
        tokio::time::timeout(Duration::from_secs(5), closed_rx)
            .await
            .unwrap()
            .unwrap();
        let roots_after_drop_finishes = root_setup_residual(parent).len();

        assert!(rollback_timed_out);
        assert_eq!(roots_before_drop_finishes, 2);
        assert_eq!(roots_after_drop_finishes, 2);
    }
    #[derive(Clone, Copy)]
    enum FinalRecordFault {
        Write,
        RecordFlush,
        ExplicitFlush,
    }

    struct FailingFinalRecordWriter {
        fault: FinalRecordFault,
        record: Vec<u8>,
        final_seen: bool,
        final_flushes: usize,
        failed: Arc<AtomicBool>,
    }

    impl Write for FailingFinalRecordWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.record.extend_from_slice(bytes);
            self.final_seen |= self
                .record
                .windows(b"run_finished".len())
                .any(|part| part == b"run_finished");
            if self.final_seen
                && matches!(self.fault, FinalRecordFault::Write)
                && !self.failed.swap(true, Ordering::AcqRel)
            {
                return Err(std::io::Error::other("owned final record failure"));
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.record.clear();
            if self.final_seen {
                self.final_flushes += 1;
                let selected = match self.fault {
                    FinalRecordFault::Write => false,
                    FinalRecordFault::RecordFlush => self.final_flushes == 1,
                    FinalRecordFault::ExplicitFlush => self.final_flushes == 2,
                };
                if selected && !self.failed.swap(true, Ordering::AcqRel) {
                    return Err(std::io::Error::other("owned final record failure"));
                }
            }
            Ok(())
        }
    }

    #[rstest::rstest]
    #[case::write(FinalRecordFault::Write)]
    #[case::record_flush(FinalRecordFault::RecordFlush)]
    #[case::explicit_flush(FinalRecordFault::ExplicitFlush)]
    #[tokio::test]
    async fn final_record_io_failure_is_reported_without_output_acknowledgement(
        #[case] fault: FinalRecordFault,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let failed = Arc::new(AtomicBool::new(false));
        let writer = FailingFinalRecordWriter {
            fault,
            record: Vec::new(),
            final_seen: false,
            final_flushes: 0,
            failed: Arc::clone(&failed),
        };
        let control = RunControl::new();
        let observed = control.clone();
        let error = run_loop_with_control(config, writer, Vec::new(), control)
            .await
            .unwrap_err();

        let actual = failed.load(Ordering::Acquire);
        let actual_2 = error.contains("owned final record failure");
        let actual_3 = error.contains("report.finalization.failed");
        let actual_4 = !observed
            .finalization_events()
            .contains(&"output_acknowledged");
        let actual_5 = observed
            .managed_root_paths()
            .iter()
            .all(|root| !root.exists());

        assert!(actual);
        assert!(actual_2, "{error}");
        assert!(actual_3, "{error}");
        assert!(actual_4);
        assert!(actual_5);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cleanup_fresh_measurement_failure_preserves_the_completed_run_during_removal() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let mut control = RunControl::new();
        control.disk_sample_interval = Duration::from_secs(60);
        let measuring = control.clone();
        control.disk_meter = Some(TestDiskMeter(Arc::new(std::sync::Mutex::new(Box::new(
            PanicAtOwnedBoundaryMeter {
                // Baseline and analysis admission both sampled successfully. The next explicit
                // sample belongs to cleanup; no periodic sample can race it at this interval.
                reached: Box::new(move || measuring.dispatched_effect_count() >= 2),
                fault: ObservedDiskFault::Io,
            },
        )))));
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            run_loop_with_control(config, stdout.clone(), Vec::new(), control),
        )
        .await
        .unwrap();
        // Core deliberately ignores new stop requests in Cleaning after execution is complete.
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();

        let actual = result.unwrap();
        let actual_2 = observed.dispatched_effect_count();
        let actual_3 = observed.max_process_tasks();
        let actual_4 = observed.finalization_events().contains(&"disk_stop");
        let actual_5 = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        let actual_6 = observed
            .managed_root_paths()
            .iter()
            .all(|root| !root.exists());

        assert_eq!(actual, 0);
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(actual_2, 2);
        assert_eq!(actual_3, 1);
        assert!(actual_4);
        assert!(actual_5);
        assert!(actual_6);
    }
    #[rstest::rstest]
    #[case::final_record(b"run_finished", "final_report_deadline_observed", 0)]
    #[case::serial_record(b"baseline_finished", "serial_report_deadline_observed", 4)]
    #[tokio::test]
    async fn report_delivery_observes_total_deadline_and_finishes_within_grace(
        #[case] marker: &'static [u8],
        #[case] observed_event: &'static str,
        #[case] expected_exit: i32,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let timeout = config.limits.total_timeout.get();
        let mut control = RunControl::new();
        control.shutdown_grace = Duration::from_secs(5);
        let observed = control.clone();
        let releasing = control.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = PausedReportWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush: false,
            marker,
            record: Vec::new(),
            target_seen: false,
        };
        let delivery = async move {
            tokio::time::timeout(Duration::from_secs(5), entered_rx)
                .await
                .unwrap()
                .unwrap();
            tokio::time::pause();
            tokio::time::advance(timeout).await;
            tokio::time::resume();
            super::coverage_tests::wait_for_finalization_event(&releasing, observed_event).await;
            release_tx.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(
            run_loop_with_control(config, writer, Vec::new(), control),
            delivery,
        );

        let actual = result.unwrap();
        let actual_2 = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        let actual_3 = observed
            .managed_root_paths()
            .iter()
            .all(|root| !root.exists());

        assert_eq!(actual, expected_exit);
        assert!(actual_2);
        assert!(actual_3);
    }
    #[tokio::test]
    async fn ranked_saved_plan_rechecks_remaining_budget_after_baseline_restoration() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        std::fs::write(
            project.path().join("target.py"),
            "def compute():\n    return 1 + 2\n",
        )
        .unwrap();
        let plan = crate::plan::create(
            config,
            hoimin_core::DiscoveryConfig::new(Duration::from_secs(10)).unwrap(),
        )
        .await
        .unwrap();
        let artifact = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(artifact.path(), serde_json::to_vec(&plan.manifest).unwrap()).unwrap();
        let verified = crate::plan::prepare_verify_selection(
            artifact.path(),
            &crate::cli::VerifySelection::Top {
                count: std::num::NonZeroUsize::new(1).unwrap(),
                policy: crate::cli::TopSelectionPolicy::Strict,
                offset: 0,
            },
        )
        .await
        .unwrap();
        let stdout = CollectingWriter::default();
        let result = verified.execute(stdout.clone(), Vec::new()).await.unwrap();
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();

        let actual = report["mutants"].as_array().unwrap().len();

        assert_eq!(result, 0);
        assert_eq!(actual, 1);
        assert_eq!(report["summary"]["counts"]["survived"], 1);
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(report["summary"]["verification_selection"]["mode"], "top");
    }
    #[tokio::test]
    async fn managed_cleanup_retries_real_coordinator_contention_before_removal() {
        let parent = tempfile::tempdir().unwrap();
        let parent_path = Utf8Path::from_path(parent.path()).unwrap();
        let coordinator = ManagedRootCoordinator::open(parent_path).unwrap();
        let root =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let competing = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(parent_path.join("hoimin-workspaces-v2/.hoimin-coordinator"))
            .unwrap();
        competing.try_lock().unwrap();
        // Marker spawn, first cleanup spawn, then retry spawn. Release the real lock only
        // once the retry starts, so success proves contention did not end the attempt.
        let release = crate::io_faults::arm_action("cleanup.thread.spawn", 2, move || {
            drop(competing);
            Ok(())
        });
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );
        let outcome = cleanup_managed_root(Arc::clone(&root), &budget).await;

        let actual = release.triggered();
        let actual_2 = !root.path().exists();

        assert!(actual);
        assert_eq!(outcome.record.disposition, CleanupDisposition::Clean);
        assert_eq!(outcome.error, "");
        assert!(actual_2);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn baseline_dispatch_reports_an_unrepresentable_import_root_without_spawning() {
        let project = tempfile::tempdir().unwrap();
        let mut data = output_failure_test_config(&project).into_data();
        std::fs::create_dir(project.path().join("pkg:tools")).unwrap();
        std::fs::write(project.path().join("pkg:tools/module.py"), b"pass\n").unwrap();
        data.import_roots = vec![Utf8PathBuf::from("pkg:tools")];
        let config = RunConfig::try_from(data).unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let stdout = CollectingWriter::default();
        let stderr = CollectingWriter::default();
        let result = run_loop_with_control(config, stdout.clone(), stderr.clone(), control)
            .await
            .unwrap();
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();

        let actual = observed.max_process_tasks();
        let actual_2 = String::from_utf8(stderr.snapshot())
            .unwrap()
            .contains("cannot construct PYTHONPATH");
        let actual_3 = observed
            .managed_root_paths()
            .iter()
            .all(|root| !root.exists());

        assert_eq!(result, 2);
        assert_eq!(actual, 0);
        assert!(actual_2);
        assert_eq!(report["summary"]["complete"], false);
        assert!(actual_3);
    }
    async fn wait_for_guarded_dispatch_count(control: &RunControl, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::task::yield_now().await;
                if control.dispatched_effect_count() >= expected {
                    break;
                }
            }
        })
        .await
        .expect("owned dispatch must reach the requested boundary");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_under_the_baseline_spawn_gate_prevents_process_admission() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let control = RunControl::new();
        let observed = control.clone();
        let waiting = control.clone();
        let gate = control.start_gate();
        let (held_tx, held_rx) = tokio::sync::oneshot::channel();
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
        let controller = std::thread::spawn(move || {
            let held = gate.begin_spawn();
            held_tx.send(()).unwrap();
            cancel_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            gate.cancel_under_held_spawn_gate_for_tests();
            drop(held);
        });
        tokio::time::timeout(Duration::from_secs(5), held_rx)
            .await
            .unwrap()
            .unwrap();
        let cancellation = tokio::spawn(async move {
            wait_for_guarded_dispatch_count(&waiting, 1).await;
            cancel_tx.send(()).unwrap();
        });
        let result = run_loop_with_control(config, Vec::new(), Vec::new(), control).await;
        cancellation.await.unwrap();
        controller.join().unwrap();

        let actual = result.unwrap();
        let actual_2 = observed.max_process_tasks();
        let actual_3 = observed
            .managed_root_paths()
            .iter()
            .all(|root| !root.exists());

        assert_eq!(actual, 130);
        assert_eq!(actual_2, 0);
        assert!(actual_3);
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_under_the_mutant_spawn_gate_preserves_the_completed_baseline() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        std::fs::write(
            project.path().join("target.py"),
            "def compute():\n    return 1 + 2\n",
        )
        .unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let waiting = control.clone();
        let gate = control.start_gate();
        let (hold_tx, hold_rx) = std::sync::mpsc::channel();
        let (held_tx, held_rx) = tokio::sync::oneshot::channel();
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
        let controller = std::thread::spawn(move || {
            hold_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let held = gate.begin_spawn();
            held_tx.send(()).unwrap();
            cancel_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            gate.cancel_under_held_spawn_gate_for_tests();
            drop(held);
        });
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = PausedReportWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush: false,
            marker: b"baseline_finished",
            record: Vec::new(),
            target_seen: false,
        };
        let cancellation = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), entered_rx)
                .await
                .unwrap()
                .unwrap();
            hold_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), held_rx)
                .await
                .unwrap()
                .unwrap();
            release_tx.send(()).unwrap();
            wait_for_guarded_dispatch_count(&waiting, 3).await;
            cancel_tx.send(()).unwrap();
        });
        let result = run_loop_with_control(config, writer, Vec::new(), control).await;
        cancellation.await.unwrap();
        controller.join().unwrap();

        let actual = result.unwrap();
        let actual_2 = observed.max_process_tasks();
        let actual_3 = observed
            .finalization_events()
            .contains(&"output_acknowledged");
        let actual_4 = observed
            .managed_root_paths()
            .iter()
            .all(|root| !root.exists());

        assert_eq!(actual, 130);
        assert_eq!(actual_2, 1);
        assert!(actual_3);
        assert!(actual_4);
    }

    async fn run_with_failed_serial_interrupt(
        config: RunConfig,
        control: RunControl,
    ) -> Result<i32, String> {
        let releasing = control.clone();
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writer = PausedReportWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush: false,
            marker: b"baseline_finished",
            record: Vec::new(),
            target_seen: false,
        };
        let observation = async move {
            tokio::time::timeout(Duration::from_secs(5), entered_rx)
                .await
                .unwrap()
                .unwrap();
            signal_tx
                .send(Err("owned interrupt setup failure".into()))
                .unwrap();
            super::coverage_tests::wait_for_finalization_event(&releasing, "interrupt_observed")
                .await;
            release_tx.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(
            run_loop_prepared_timed(
                config,
                writer,
                Vec::new(),
                control,
                hoimin_core::RunSelection::all(),
                None,
                timing,
                interrupts,
            ),
            observation
        );
        result
    }

    #[rstest::rstest]
    #[case::execution(1, 0, 1)]
    #[case::delivery(3, 1, 0)]
    #[tokio::test]
    async fn outer_cleanup_spawn_failure_preserves_interrupt_error_and_the_failed_root(
        #[case] preceding_spawns: usize,
        #[case] retained_root: usize,
        #[case] removed_root: usize,
    ) {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let control = RunControl::new();
        let observed = control.clone();
        let fault = crate::io_faults::arm_after("cleanup.thread.spawn", preceding_spawns);
        let result = run_with_failed_serial_interrupt(config, control).await;
        let error = result.unwrap_err();
        let fault_triggered = fault.triggered();
        let primary_interrupt_preserved = error.contains("owned interrupt setup failure");
        let cleanup_spawn_failed = error.contains("cleanup thread could not start");
        let roots = observed.managed_root_paths();
        let failed_root_retained = roots[retained_root].exists();
        let other_root_removed = !roots[removed_root].exists();
        let output_unacknowledged = !observed
            .finalization_events()
            .contains(&"output_acknowledged");

        assert!(fault_triggered);
        assert!(primary_interrupt_preserved, "{error}");
        assert!(cleanup_spawn_failed, "{error}");
        assert!(failed_root_retained);
        assert!(other_root_removed);
        assert!(output_unacknowledged);
    }
    #[tokio::test]
    async fn public_run_loop_completes_and_delivers_a_report_with_default_control() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let stdout = CollectingWriter::default();
        let result = run_loop(config, stdout.clone(), Vec::new()).await.unwrap();
        let report: serde_json::Value = crate::test_report::from_slice(&stdout.snapshot()).unwrap();

        let actual = report["baseline"].is_object();

        assert_eq!(result, 0);
        assert_eq!(report["summary"]["complete"], true);
        assert_eq!(report["summary"]["exit_code"], result);
        assert!(actual);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn owned_blocking_cleanup_delivers_workspace_success_with_resource_close_diagnostic() {
        let (_project, parent, mut context, _, create) = context_with_worker().await;
        context.process = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests_with_close_failure()),
            Utf8PathBuf::from_path_buf(parent.path().join("blocking-close-output")).unwrap(),
        ));
        let task = prepare_blocking_effect(
            &mut context,
            RunEffect::Cleanup(Cleanup {
                id: EffectId(841),
                reservations: vec![create.reservation_id()],
            }),
        )
        .unwrap();
        let completion = task.execute();
        let event = accept_blocking_completion(&mut context, completion);

        let actual = matches!(event, RunEvent::CleanupFinished(value) if value.id == EffectId(841));
        let actual_2 = context.workspace().worker_count();
        let actual_3 = context.blocking_secondary_errors.len();
        let actual_4 = context.blocking_secondary_errors[0].starts_with("process.resource.close:");
        let actual_5 =
            context.blocking_secondary_errors[0].contains("injected portable close failure");

        assert!(actual);
        assert_eq!(actual_2, 0);
        assert_eq!(actual_3, 1);
        assert!(actual_4);
        assert!(actual_5);
    }
    struct ObservedPausedReportWriter {
        inner: PausedReportWriter,
        closed: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl Write for ObservedPausedReportWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.inner.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }

    impl Drop for ObservedPausedReportWriter {
        fn drop(&mut self) {
            let closed = self
                .closed
                .take()
                .expect("constructor installs one once-only drop observer");
            let _ = closed.send(());
        }
    }

    #[tokio::test]
    async fn serial_handler_failure_keeps_its_primary_when_report_delivery_outlives_grace() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut control = RunControl::new();
        control.shutdown_grace = Duration::from_millis(100);
        let observed = control.clone();
        let releasing = control.clone();
        let parent_owner = Arc::clone(&control.managed_parent);
        let timing = PreparationToken::new(config.limits.total_timeout.get(), control.clone());
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let interrupts = crate::interrupt::spawn_test_monitor(signal_rx, drop);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
        let writer = ObservedPausedReportWriter {
            inner: PausedReportWriter {
                entered: Some(entered_tx),
                release: release_rx,
                pause_flush: false,
                marker: b"baseline_finished",
                record: Vec::new(),
                target_seen: false,
            },
            closed: Some(closed_tx),
        };
        let observation = async move {
            tokio::time::timeout(Duration::from_secs(5), entered_rx)
                .await
                .unwrap()
                .unwrap();
            signal_tx
                .send(Err("owned interrupt setup failure".into()))
                .unwrap();
            super::coverage_tests::wait_for_finalization_event(&releasing, "interrupt_observed")
                .await;
        };
        let (result, ()) = tokio::join!(
            run_loop_prepared_timed(
                config,
                writer,
                Vec::new(),
                control,
                hoimin_core::RunSelection::all(),
                None,
                timing,
                interrupts,
            ),
            observation
        );
        let error = result.unwrap_err();
        let primary_interrupt_preserved = error.contains("owned interrupt setup failure");
        let shutdown_grace_expired = error.contains("shutdown grace expired");
        let delivery_root_before_drop = observed.managed_root_paths()[1].exists();
        let output_unacknowledged = !observed
            .finalization_events()
            .contains(&"output_acknowledged");
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), closed_rx)
            .await
            .unwrap()
            .unwrap();
        let parent_exists_after_drop = parent_owner.path().exists();
        let delivery_root_after_drop = observed.managed_root_paths()[1].exists();

        assert!(primary_interrupt_preserved, "{error}");
        assert!(shutdown_grace_expired, "{error}");
        assert!(delivery_root_before_drop);
        assert!(output_unacknowledged);
        assert!(parent_exists_after_drop);
        assert!(delivery_root_after_drop);
    }
    #[tokio::test]
    async fn historical_git_failure_survives_repeated_completed_drains() {
        let runner = crate::target::git::GitRunner::default()
            .with_historical_failure_for_tests("target.git.cleanup: owned historical reap failure");
        let first = runner
            .drain_for_shutdown(Duration::from_secs(5))
            .await
            .unwrap_err();
        let second = runner
            .drain_for_shutdown(Duration::from_secs(5))
            .await
            .unwrap_err();

        assert_eq!(first, "target.git.cleanup: owned historical reap failure");
        assert_eq!(second, first);
    }

    #[tokio::test]
    async fn inner_cleanup_preserves_a_historical_git_lifecycle_failure() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project);
        let mut control = RunControl::new();
        control.historical_git_failure = Some("target.git.cleanup: owned historical reap failure");
        let observed = control.clone();
        let error = run_loop_with_control(config, Vec::new(), Vec::new(), control)
            .await
            .unwrap_err();

        let actual = error.contains("owned historical reap failure");
        let actual_2 = observed.max_process_tasks();
        let actual_3 = observed
            .managed_root_paths()
            .iter()
            .all(|root| root.exists());
        let actual_4 = !observed
            .finalization_events()
            .contains(&"output_acknowledged");

        assert!(actual, "{error}");
        assert_eq!(actual_2, 1);
        assert!(actual_3);
        assert!(actual_4);
    }

    #[tokio::test]
    async fn outer_cleanup_preserves_primary_interrupt_and_historical_git_failure() {
        let project = tempfile::tempdir().unwrap();
        let config = output_failure_test_config(&project)
            .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
            .unwrap();
        let mut control = RunControl::new();
        control.historical_git_failure = Some("target.git.cleanup: owned historical reap failure");
        let observed = control.clone();
        let error = run_with_failed_serial_interrupt(config, control)
            .await
            .unwrap_err();

        let actual = error.contains("owned interrupt setup failure");
        let actual_2 = error.contains("owned historical reap failure");
        let actual_3 = observed.max_process_tasks();
        let actual_4 = observed
            .managed_root_paths()
            .iter()
            .all(|root| root.exists());
        let actual_5 = !observed
            .finalization_events()
            .contains(&"output_acknowledged");

        assert!(actual, "{error}");
        assert!(actual_2, "{error}");
        assert_eq!(actual_3, 1);
        assert!(actual_4);
        assert!(actual_5);
    }
}
