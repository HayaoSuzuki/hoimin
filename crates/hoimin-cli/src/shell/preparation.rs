use super::{RunControl, ShutdownBudget, ShutdownCause, establish_shutdown_budget};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The run deadline is established once, before any measured preparation.
/// Blocking owners retain this capability through rollback, even after the waiter exits.
#[derive(Clone, Debug)]
pub(super) struct PreparationToken {
    pub(super) deadline: tokio::time::Instant,
    control: RunControl,
    shutdown: Arc<Mutex<Option<ShutdownBudget>>>,
    failures: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug)]
pub(super) enum PreparationError {
    Stopped(ShutdownCause),
    Failed(String),
}

impl From<String> for PreparationError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}

impl PreparationToken {
    pub(super) fn new(timeout: Duration, control: RunControl) -> Self {
        Self {
            deadline: tokio::time::Instant::now() + timeout,
            control,
            shutdown: Arc::new(Mutex::new(None)),
            failures: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn check(&self) -> Result<(), PreparationError> {
        let now = tokio::time::Instant::now();
        let cancelled_at = *self
            .control
            .cancelled_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let candidate = if let Some(at) = cancelled_at.filter(|at| *at < self.deadline) {
            Some(ShutdownBudget::after_observation_with_grace(
                ShutdownCause::Cancellation,
                at,
                self.control.shutdown_grace(),
            ))
        } else if now >= self.deadline {
            Some(ShutdownBudget::for_total_timeout_with_grace(
                self.deadline,
                self.control.shutdown_grace(),
            ))
        } else {
            None
        };
        if let Some(candidate) = candidate {
            let budget = self.establish(candidate);
            return Err(PreparationError::Stopped(budget.cause()));
        }
        if let Some(budget) = self.shutdown() {
            return Err(match budget.cause() {
                ShutdownCause::Failure => PreparationError::Failed("preparation failed".into()),
                cause => PreparationError::Stopped(cause),
            });
        }
        Ok(())
    }

    fn establish(&self, candidate: ShutdownBudget) -> ShutdownBudget {
        establish_shutdown_budget(
            &mut self
                .shutdown
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            candidate,
        )
    }

    pub(super) fn shutdown(&self) -> Option<ShutdownBudget> {
        *self
            .shutdown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn failure_budget(&self) -> ShutdownBudget {
        let _ = self.check();
        self.establish(ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            self.control.shutdown_grace(),
        ))
    }

    pub(super) fn rollback_remaining(&self) -> Duration {
        self.failure_budget()
            .deadline()
            .saturating_duration_since(tokio::time::Instant::now())
    }

    pub(super) fn record_failure(&self, error: String) {
        self.failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(error);
    }

    /// Only the asynchronous waiter stops waiting. The blocking closure still owns
    /// its operation and any result until completion; failed delivery drops there.
    pub(super) async fn run<T: Send + 'static>(
        &self,
        interrupts: &mut crate::interrupt::InterruptMonitor,
        operation: impl FnOnce() -> Result<T, PreparationError> + Send + 'static,
    ) -> Result<T, PreparationError> {
        self.check()?;
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let token = self.clone();
        tokio::task::spawn_blocking(move || {
            let result = token.check().and_then(|()| operation());
            let result = match token.check() {
                Err(stopped) => match result {
                    Err(PreparationError::Failed(error)) => Err(PreparationError::Failed(error)),
                    other => {
                        drop(other);
                        Err(stopped)
                    }
                },
                Ok(()) => result,
            };
            // A receiver that has stopped cannot cause the result's resources to
            // be dropped by an async task, or certify that this worker quiesced.
            let _ = sender.send(result);
        });
        let stopped = tokio::select! {
            biased;
            () = self.control.cancelled() => { self.check().err().unwrap_or(PreparationError::Stopped(ShutdownCause::Cancellation)) }
            () = tokio::time::sleep_until(self.deadline) => { self.check().expect_err("deadline check stops") }
            signal = interrupts.first() => {
                match signal {
                    Ok(()) => { self.control.cancel(); self.check().expect_err("cancelled preparation stops") }
                    Err(error) => { self.failure_budget(); PreparationError::Failed(error) }
                }
            }
            result = &mut receiver => {
                let result = result.map_err(|error| PreparationError::Failed(format!("preparation task failed: {error}")))?;
                return self.accept_result(result).await;
            }
        };
        let budget = self.failure_budget();
        match budget.wait(&mut receiver).await {
            Ok(Ok(result)) => {
                match result {
                    Ok(value) => self.dispose(value).await?,
                    Err(PreparationError::Failed(error)) => return Err(PreparationError::Failed(error)),
                    Err(PreparationError::Stopped(_)) => {}
                }
                Err(stopped)
            }
            Ok(Err(error)) => Err(PreparationError::Failed(format!("preparation task failed: {error}"))),
            Err(_) => Err(PreparationError::Failed("preparation shutdown grace expired; blocking operation retains its resources until completion".into())),
        }
    }

    async fn accept_result<T: Send + 'static>(
        &self,
        result: Result<T, PreparationError>,
    ) -> Result<T, PreparationError> {
        // A real operation failure survives cancellation racing receipt. Only
        // successfully returned owners need disposal after stop observation.
        if matches!(&result, Err(PreparationError::Failed(_))) {
            return result;
        }
        if let Err(stopped) = self.check() {
            if let Ok(owner) = result {
                self.dispose(owner).await?;
            }
            return Err(stopped);
        }
        result
    }

    pub(super) async fn admit<F: std::future::Future>(
        &self,
        interrupts: &mut crate::interrupt::InterruptMonitor,
        future: F,
    ) -> Result<F::Output, PreparationError> {
        self.check()?;
        let result = tokio::select! {
            biased;
            () = self.control.cancelled() => return Err(self.check().expect_err("cancelled preparation stops")),
            () = tokio::time::sleep_until(self.deadline) => return Err(self.check().expect_err("deadline check stops")),
            signal = interrupts.first() => match signal {
                Ok(()) => { self.control.cancel(); return Err(self.check().expect_err("interrupted preparation stops")); }
                Err(error) => { self.failure_budget(); return Err(PreparationError::Failed(error)); }
            },
            result = future => result,
        };
        self.check()?;
        Ok(result)
    }

    pub(super) async fn dispose<T: Send + 'static>(
        &self,
        owner: T,
    ) -> Result<(), PreparationError> {
        let budget = self.failure_budget();
        let disposal = tokio::task::spawn_blocking(move || drop(owner));
        match budget.wait(disposal).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(PreparationError::Failed(format!(
                "preparation rollback task failed: {error}"
            ))),
            Err(_) => Err(PreparationError::Failed(
                "preparation rollback exceeded shutdown grace".into(),
            )),
        }
    }

    pub(super) async fn report<Stderr: Write + Send + 'static>(
        &self,
        error: PreparationError,
        mut stderr: Stderr,
    ) -> Result<i32, String> {
        let budget = self.failure_budget();
        let failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let (result, diagnostic) = match (error, failures.is_empty()) {
            (
                PreparationError::Stopped(
                    cause @ (ShutdownCause::Cancellation | ShutdownCause::TotalTimeout),
                ),
                true,
            ) => {
                let code = if cause == ShutdownCause::Cancellation {
                    130
                } else {
                    4
                };
                (
                    Ok(code),
                    format!(
                        "{} during preparation; baseline was not started",
                        cause.label()
                    ),
                )
            }
            (error, _) => {
                let primary = match error {
                    PreparationError::Stopped(cause) => {
                        format!("{} during preparation", cause.label())
                    }
                    PreparationError::Failed(error) => format!("preparation failed: {error}"),
                };
                let message = if failures.is_empty() {
                    primary
                } else {
                    format!("{primary}; {}", failures.join("; "))
                };
                (Err(message.clone()), message)
            }
        };
        if tokio::time::Instant::now() < budget.deadline() {
            let diagnostic = tokio::task::spawn_blocking(move || {
                writeln!(stderr, "{diagnostic}").and_then(|()| stderr.flush())
            });
            let failure = match budget.wait(diagnostic).await {
                Ok(Ok(Ok(()))) => None,
                Ok(Ok(Err(error))) => {
                    Some(format!("preparation diagnostic output failed: {error}"))
                }
                Ok(Err(error)) => Some(format!("preparation diagnostic task failed: {error}")),
                Err(_) => Some("preparation diagnostic output exceeded shutdown grace".to_owned()),
            };
            if let Some(failure) = failure {
                return Err(match result {
                    Ok(_) => failure,
                    Err(primary) => format!("{primary}; {failure}"),
                });
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn preparation_uses_up_time_from_original_deadline() {
        // Arrange.
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        let deadline = timing.deadline;

        // Act.
        tokio::time::advance(Duration::from_secs(10)).await;

        // Assert.
        assert!(timing.check().is_ok());
        assert_eq!(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            Duration::from_secs(20)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_expiry_includes_deadline_equality() {
        // Arrange.
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());

        // Act.
        tokio::time::advance(Duration::from_secs(30)).await;
        let result = timing.check();

        // Assert.
        assert!(matches!(
            result,
            Err(PreparationError::Stopped(ShutdownCause::TotalTimeout))
        ));
        assert_eq!(
            timing
                .shutdown()
                .unwrap()
                .deadline()
                .saturating_duration_since(tokio::time::Instant::now()),
            Duration::from_secs(2)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_cancellation_grace_starts_at_request() {
        // Arrange.
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let requested = tokio::time::Instant::now();
        control.cancel();

        // Act: delayed observation must not award a fresh grace interval.
        tokio::time::advance(Duration::from_secs(1)).await;
        timing.check().unwrap_err();
        control.cancel();

        // Assert.
        assert_eq!(
            timing.shutdown().unwrap().deadline(),
            requested + Duration::from_secs(2)
        );
        assert_eq!(timing.rollback_remaining(), Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_timeout_with_orderly_finalization_returns_incomplete() {
        // Arrange.
        let timing = PreparationToken::new(Duration::from_secs(30), RunControl::new());
        tokio::time::advance(Duration::from_secs(30)).await;
        let error = timing.check().unwrap_err();

        // Act.
        let result = timing.report(error, Vec::new()).await;

        // Assert.
        assert_eq!(result, Ok(4));
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_timeout_does_not_extend_for_late_cancellation() {
        // Arrange.
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let deadline = timing.deadline;
        tokio::time::advance(Duration::from_secs(31)).await;

        // Act.
        control.cancel();
        let result = timing.check();

        // Assert.
        assert!(matches!(
            result,
            Err(PreparationError::Stopped(ShutdownCause::TotalTimeout))
        ));
        assert_eq!(
            timing.shutdown().unwrap().deadline(),
            deadline + Duration::from_secs(2)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_cancelled_worker_retains_owner_until_completion() {
        // Arrange: returning from the waiter cannot release the operation's owner.
        struct Owner(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let owner = Owner(Arc::clone(&dropped));
        let task = tokio::spawn(async move {
            let mut interrupts = crate::interrupt::InterruptMonitor::spawn();
            timing
                .run(&mut interrupts, move || {
                    let _owner = owner;
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                    let _ = finished_tx.send(());
                    Ok(())
                })
                .await
        });
        started_rx.await.unwrap();

        // Act.
        control.cancel();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        let result = task.await.unwrap();

        // Assert.
        assert!(matches!(result, Err(PreparationError::Failed(_))));
        assert!(!dropped.load(std::sync::atomic::Ordering::Acquire));
        release_tx.send(()).unwrap();
        finished_rx.await.unwrap();
    }

    #[tokio::test]
    async fn preparation_failure_survives_cancellation_racing_receipt() {
        // Arrange: operation finished, but cancellation wins before receipt.
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        let completed: Result<(), PreparationError> =
            Err(PreparationError::Failed("actual backend failure".into()));
        control.cancel();

        // Act.
        let result = timing.accept_result(completed).await;

        // Assert.
        assert!(
            matches!(result, Err(PreparationError::Failed(error)) if error == "actual backend failure")
        );
    }

    #[tokio::test]
    async fn preparation_rollback_panic_is_infrastructure_failure() {
        // Arrange.
        struct PanicOnDrop;
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("fixture rollback panic");
            }
        }
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();

        // Act.
        let result = timing.accept_result(Ok(PanicOnDrop)).await;

        // Assert.
        assert!(
            matches!(result, Err(PreparationError::Failed(error)) if error.contains("fixture rollback panic"))
        );
    }

    struct FailedFlushWriter;
    impl Write for FailedFlushWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("fixture flush failure"))
        }
    }

    #[tokio::test]
    async fn preparation_diagnostic_flush_failure_is_infrastructure_failure() {
        // Arrange.
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();

        // Act.
        let result = timing
            .report(timing.check().unwrap_err(), FailedFlushWriter)
            .await;

        // Assert.
        assert!(result.unwrap_err().contains("fixture flush failure"));
    }

    #[tokio::test(start_paused = true)]
    async fn preparation_diagnostic_wait_retains_blocked_writer() {
        // Arrange.
        struct BlockedWriter {
            entered: Option<tokio::sync::oneshot::Sender<()>>,
            release: std::sync::mpsc::Receiver<()>,
            dropped: Option<tokio::sync::oneshot::Sender<()>>,
        }
        impl Write for BlockedWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if let Some(entered) = self.entered.take() {
                    let _ = entered.send(());
                    self.release.recv_timeout(Duration::from_secs(10)).unwrap();
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl Drop for BlockedWriter {
            fn drop(&mut self) {
                if let Some(dropped) = self.dropped.take() {
                    let _ = dropped.send(());
                }
            }
        }
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (dropped_tx, mut dropped_rx) = tokio::sync::oneshot::channel();
        let writer = BlockedWriter {
            entered: Some(entered_tx),
            release: release_rx,
            dropped: Some(dropped_tx),
        };
        let task =
            tokio::spawn(async move { timing.report(timing.check().unwrap_err(), writer).await });
        entered_rx.await.unwrap();

        // Act.
        tokio::time::advance(Duration::from_secs(2)).await;
        let result = task.await.unwrap();

        // Assert.
        assert!(
            result
                .unwrap_err()
                .contains("diagnostic output exceeded shutdown grace")
        );
        assert!(matches!(
            dropped_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        release_tx.send(()).unwrap();
        dropped_rx.await.unwrap();
    }

    struct FailedWriter;
    impl Write for FailedWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("fixture diagnostic failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn preparation_diagnostic_write_failure_is_infrastructure_failure() {
        // Arrange.
        let control = RunControl::new();
        let timing = PreparationToken::new(Duration::from_secs(30), control.clone());
        control.cancel();

        // Act.
        let result = timing
            .report(timing.check().unwrap_err(), FailedWriter)
            .await;

        // Assert.
        assert!(result.unwrap_err().contains("fixture diagnostic failure"));
    }
}
