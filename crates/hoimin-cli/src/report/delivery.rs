use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hoimin_core::{EffectFailed, EffectFailure, EffectId, EmitOutput, OutputEmitted};
use tokio::task::JoinHandle;

use super::ReportHandler;

type OwnedHandler = ReportHandler<Box<dyn Write + Send>, Box<dyn Write + Send>>;
type OperationResult = Result<Option<OutputEmitted>, EffectFailed>;

pub(crate) enum ReportDelivery<Stdout, Stderr> {
    Inline(ReportHandler<Stdout, Stderr>),
    Owned(OwnedDelivery),
}

impl<Stdout: Write, Stderr: Write> ReportDelivery<Stdout, Stderr> {
    pub(crate) async fn handle(
        &mut self,
        request: EmitOutput,
    ) -> Result<OutputEmitted, EffectFailed> {
        match self {
            Self::Inline(handler) => handler.handle(request),
            Self::Owned(driver) => {
                let id = request.id;
                driver
                    .execute(ReportOperation::Emit(Box::new(request)))
                    .await?
                    .ok_or_else(|| delivery_failed(id, "output acknowledgement was missing"))
            }
        }
    }

    pub(crate) async fn flush_and_release_spool(&mut self) -> io::Result<()> {
        match self {
            Self::Inline(handler) => handler.flush_and_release_spool(),
            Self::Owned(driver) => {
                if driver.task.is_some() {
                    let _ = driver.join(EffectId(u64::MAX)).await;
                }
                let result = driver.execute(ReportOperation::Flush).await;
                if driver.is_idle() {
                    driver.owner = None;
                }
                result
                    .map(|_| ())
                    .map_err(|error| io::Error::other(format!("{error:?}")))
            }
        }
    }

    pub(crate) fn is_quiescent(&self) -> bool {
        match self {
            Self::Inline(_) => true,
            Self::Owned(driver) => driver.is_idle(),
        }
    }

    pub(crate) fn admission(&self) -> Option<Arc<ReportAdmission>> {
        match self {
            Self::Inline(_) => None,
            Self::Owned(driver) => Some(Arc::clone(&driver.admission)),
        }
    }
}

#[derive(Default)]
pub(crate) struct ReportAdmission {
    deadline: Mutex<Option<tokio::time::Instant>>,
    abandoned: AtomicBool,
}

impl ReportAdmission {
    pub(crate) fn restrict_to(&self, limit: tokio::time::Instant) {
        let mut deadline = self
            .deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *deadline = Some(deadline.map_or(limit, |previous| previous.min(limit)));
    }

    fn permits_start(&self) -> bool {
        !self.abandoned.load(Ordering::Acquire)
            && self
                .deadline
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none_or(|deadline| tokio::time::Instant::now() < deadline)
    }
}

impl<Stdout: Write + Send + 'static, Stderr: Write + Send + 'static>
    ReportDelivery<Stdout, Stderr>
{
    pub(crate) fn into_owned(self, owner: Arc<dyn Send + Sync>) -> Self {
        let Self::Inline(handler) = self else {
            return self;
        };
        Self::Owned(OwnedDelivery {
            handler: Some(ReportHandler {
                format: handler.format,
                stdout: Box::new(handler.stdout),
                stderr: Box::new(handler.stderr),
                json: handler.json,
            }),
            task: None,
            owner: Some(owner),
            admission: Arc::new(ReportAdmission::default()),
        })
    }
}

pub(crate) struct OwnedDelivery {
    handler: Option<OwnedHandler>,
    task: Option<JoinHandle<(OwnedHandler, OperationResult)>>,
    owner: Option<Arc<dyn Send + Sync>>,
    admission: Arc<ReportAdmission>,
}

impl Drop for OwnedDelivery {
    fn drop(&mut self) {
        self.admission.abandoned.store(true, Ordering::Release);
    }
}

enum ReportOperation {
    Emit(Box<EmitOutput>),
    Flush,
}

impl OwnedDelivery {
    fn is_idle(&self) -> bool {
        self.task.is_none() && self.handler.is_some()
    }

    async fn execute(&mut self, operation: ReportOperation) -> OperationResult {
        let id = match &operation {
            ReportOperation::Emit(request) => request.id,
            ReportOperation::Flush => EffectId(u64::MAX),
        };
        if self.task.is_some() {
            return Err(delivery_failed(
                id,
                "an earlier report operation is still pending",
            ));
        }
        let mut handler = self
            .handler
            .take()
            .ok_or_else(|| delivery_failed(id, "report handler ownership is unavailable"))?;
        let owner = self.owner.clone();
        let admission = Arc::clone(&self.admission);
        self.task = Some(tokio::task::spawn_blocking(move || {
            let _owner = owner;
            if !admission.permits_start() {
                return (
                    handler,
                    Err(delivery_failed(
                        id,
                        "report delivery was abandoned or its deadline expired",
                    )),
                );
            }
            let result = match operation {
                ReportOperation::Emit(request) => handler.handle(*request).map(Some),
                ReportOperation::Flush => handler
                    .flush_and_release_spool()
                    .map(|()| None)
                    .map_err(|error| delivery_failed(id, error.to_string())),
            };
            (handler, result)
        }));
        self.join(id).await
    }

    async fn join(&mut self, id: EffectId) -> OperationResult {
        let result = self
            .task
            .as_mut()
            .expect("report task ownership is retained until joined")
            .await;
        self.task = None;
        match result {
            Ok((handler, result)) => {
                self.handler = Some(handler);
                result
            }
            Err(error) => Err(delivery_failed(id, error.to_string())),
        }
    }
}

fn delivery_failed(id: EffectId, message: impl Into<String>) -> EffectFailed {
    EffectFailed {
        id,
        failure: EffectFailure::ReportState {
            message: message.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    use hoimin_core::{OutputEvent, OutputFormat, RunStarted};

    use super::*;

    struct GatedWriter {
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        release: mpsc::Receiver<()>,
        bytes: Arc<Mutex<Vec<u8>>>,
        gate_flush: bool,
    }

    impl GatedWriter {
        fn wait_for_release(&mut self) -> io::Result<()> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
                self.release
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(io::Error::other)?;
            }
            Ok(())
        }
    }

    impl Write for GatedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.gate_flush {
                self.wait_for_release()?;
            }
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.gate_flush {
                self.wait_for_release()?;
            }
            Ok(())
        }
    }

    fn event(id: u64) -> EmitOutput {
        EmitOutput {
            id: EffectId(id),
            event: OutputEvent::RunStarted(RunStarted::minimal("owned-report", id)),
        }
    }

    #[tokio::test]
    async fn writer_panic_fails_the_original_output_effect() {
        struct PanickingWriter;
        impl Write for PanickingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                panic!("injected report writer panic")
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let handler = ReportHandler::with_mutant_spool(
            OutputFormat::Jsonl,
            PanickingWriter,
            io::sink(),
            io::Cursor::new(Vec::new()),
        );
        let mut delivery = ReportDelivery::Inline(handler).into_owned(Arc::new(()));
        let error = delivery.handle(event(17)).await.unwrap_err();
        assert_eq!(error.id, EffectId(17));
        assert!(matches!(error.failure, EffectFailure::ReportState { .. }));
    }

    #[test]
    fn abandoned_report_does_not_begin_a_queued_write() {
        assert_queued_write_is_rejected(true);
    }

    #[test]
    fn expired_report_does_not_begin_a_queued_write_while_driver_is_alive() {
        assert_queued_write_is_rejected(false);
    }

    fn assert_queued_write_is_rejected(abandon: bool) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            });
            started_rx.await.unwrap();
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let handler = ReportHandler::with_mutant_spool(
                OutputFormat::Jsonl,
                GatedWriter {
                    entered: None,
                    release: mpsc::channel().1,
                    bytes: Arc::clone(&bytes),
                    gate_flush: false,
                },
                io::sink(),
                io::Cursor::new(Vec::new()),
            );
            let mut delivery = ReportDelivery::Inline(handler).into_owned(Arc::new(()));
            let mut write = Box::pin(delivery.handle(event(1)));
            tokio::select! {
                biased;
                result = &mut write => panic!("queued write completed: {result:?}"),
                () = tokio::task::yield_now() => {}
            }
            drop(write);
            let ReportDelivery::Owned(driver) = &mut delivery else {
                unreachable!()
            };
            let queued = driver.task.take().unwrap();
            if !abandon {
                delivery
                    .admission()
                    .unwrap()
                    .restrict_to(tokio::time::Instant::now());
            }
            let retained = if abandon {
                drop(delivery);
                None
            } else {
                Some(delivery)
            };
            release_tx.send(()).unwrap();
            blocker.await.unwrap();
            let (_, result) = queued.await.unwrap();
            assert!(
                result.is_err(),
                "an abandoned report acknowledged late output"
            );
            assert!(bytes.lock().unwrap().is_empty());
            drop(retained);
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_wait_retains_writer_and_owner_without_acknowledgement_or_retry() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let owner = Arc::new(());
        let lease = Arc::downgrade(&owner);
        let handler = ReportHandler::with_mutant_spool(
            OutputFormat::Jsonl,
            GatedWriter {
                entered: Some(entered_tx),
                release: release_rx,
                bytes: Arc::clone(&bytes),
                gate_flush: false,
            },
            io::sink(),
            io::Cursor::new(Vec::new()),
        );
        let mut delivery = ReportDelivery::Inline(handler).into_owned(owner);
        let mut write = Box::pin(delivery.handle(event(7)));
        tokio::select! {
            result = &mut write => panic!("acknowledged a blocked write: {result:?}"),
            entered = entered_rx => entered.unwrap(),
        }
        drop(write);
        assert!(!delivery.is_quiescent());
        assert!(bytes.lock().unwrap().is_empty());
        assert!(delivery.handle(event(8)).await.is_err());
        drop(delivery);
        assert!(lease.upgrade().is_some(), "pending write lost its owner");
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while lease.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let bytes = bytes.lock().unwrap();
        let event: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(event["sequence"], 7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn flush_keeps_its_owner_until_the_writer_has_finished() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let owner = Arc::new(());
        let lease = Arc::downgrade(&owner);
        let handler = ReportHandler::with_mutant_spool(
            OutputFormat::Jsonl,
            GatedWriter {
                entered: Some(entered_tx),
                release: release_rx,
                bytes: Arc::new(Mutex::new(Vec::new())),
                gate_flush: true,
            },
            io::sink(),
            io::Cursor::new(Vec::new()),
        );
        let mut delivery = ReportDelivery::Inline(handler).into_owned(owner);
        let mut flush = Box::pin(delivery.flush_and_release_spool());
        tokio::select! {
            result = &mut flush => panic!("acknowledged a blocked flush: {result:?}"),
            entered = entered_rx => entered.unwrap(),
        }
        assert!(lease.upgrade().is_some());
        release_tx.send(()).unwrap();
        flush.await.unwrap();
        assert!(delivery.is_quiescent());
        assert!(lease.upgrade().is_none());
    }
}
