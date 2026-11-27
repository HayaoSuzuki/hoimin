use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hoimin_core::{EffectFailed, EffectFailure, EffectId, EmitOutput, OutputEmitted};
use tokio::task::JoinHandle;

use super::ReportHandler;

type OwnedHandler = ReportHandler<Box<dyn Write + Send>, Box<dyn Write + Send>>;
type OperationResult = Result<Option<OutputEmitted>, EffectFailed>;

/// Owns every report operation and retains its delivery lease across cancelled waits.
pub(crate) struct ReportDelivery {
    state: OwnedDeliveryState,
    owner: Option<Arc<dyn Send + Sync>>,
    admission: Arc<ReportAdmission>,
}

impl ReportDelivery {
    pub(crate) fn new<Stdout, Stderr>(
        handler: ReportHandler<Stdout, Stderr>,
        owner: Arc<dyn Send + Sync>,
    ) -> Self
    where
        Stdout: Write + Send + 'static,
        Stderr: Write + Send + 'static,
    {
        Self {
            state: OwnedDeliveryState::Idle(ReportHandler {
                stdout: Box::new(handler.stdout),
                stderr: Box::new(handler.stderr),
                lock_stdio_stderr: std::any::TypeId::of::<Stderr>()
                    == std::any::TypeId::of::<std::io::Stderr>(),
            }),
            owner: Some(owner),
            admission: Arc::new(ReportAdmission::default()),
        }
    }

    pub(crate) async fn handle(
        &mut self,
        request: EmitOutput,
    ) -> Result<OutputEmitted, EffectFailed> {
        let id = request.id;
        self.execute(ReportOperation::Emit(Box::new(request)))
            .await?
            .ok_or_else(|| delivery_failed(id, "output acknowledgement was missing"))
    }

    pub(crate) async fn flush(&mut self) -> io::Result<()> {
        if matches!(self.state, OwnedDeliveryState::Running(_)) {
            let _ = self.join(EffectId(u64::MAX)).await;
        }
        let result = self.execute(ReportOperation::Flush).await;
        if self.is_idle() {
            self.owner = None;
        }
        result
            .map(|_| ())
            .map_err(|error| io::Error::other(format!("{error:?}")))
    }

    pub(crate) fn is_quiescent(&self) -> bool {
        self.is_idle()
    }
    pub(crate) fn admission(&self) -> Arc<ReportAdmission> {
        Arc::clone(&self.admission)
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

/// The handler is either available locally or owned by the blocking task.
/// Unavailable retains the old post-panic behavior; it is never quiescent.
enum OwnedDeliveryState {
    Idle(OwnedHandler),
    Running(JoinHandle<(OwnedHandler, OperationResult)>),
    Unavailable,
}

impl Drop for ReportDelivery {
    fn drop(&mut self) {
        self.admission.abandoned.store(true, Ordering::Release);
    }
}

enum ReportOperation {
    Emit(Box<EmitOutput>),
    Flush,
}

impl ReportDelivery {
    fn is_idle(&self) -> bool {
        matches!(self.state, OwnedDeliveryState::Idle(_))
    }

    async fn execute(&mut self, operation: ReportOperation) -> OperationResult {
        let id = match &operation {
            ReportOperation::Emit(request) => request.id,
            ReportOperation::Flush => EffectId(u64::MAX),
        };
        if matches!(self.state, OwnedDeliveryState::Running(_)) {
            return Err(delivery_failed(
                id,
                "an earlier report operation is still pending",
            ));
        }
        let OwnedDeliveryState::Idle(mut handler) =
            std::mem::replace(&mut self.state, OwnedDeliveryState::Unavailable)
        else {
            return Err(delivery_failed(
                id,
                "report handler ownership is unavailable",
            ));
        };
        let owner = self.owner.clone();
        let admission = Arc::clone(&self.admission);
        self.state = OwnedDeliveryState::Running(tokio::task::spawn_blocking(move || {
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
                    .flush()
                    .map(|()| None)
                    .map_err(|error| delivery_failed(id, error.to_string())),
            };
            (handler, result)
        }));
        self.join(id).await
    }

    async fn join(&mut self, id: EffectId) -> OperationResult {
        // Borrow the retained handle across await: cancelling this wait must
        // leave the running task and its handler in the driver for finalization.
        let OwnedDeliveryState::Running(task) = &mut self.state else {
            unreachable!("report task ownership is retained until joined");
        };
        let result = task.await;
        self.state = OwnedDeliveryState::Unavailable;
        match result {
            Ok((handler, result)) => {
                self.state = OwnedDeliveryState::Idle(handler);
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
    use std::sync::mpsc;
    use std::time::Duration;

    fn idle_handler(delivery: &mut ReportDelivery) -> &mut OwnedHandler {
        match &mut delivery.state {
            OwnedDeliveryState::Idle(handler) => handler,
            _ => panic!("expected idle owned delivery"),
        }
    }

    struct ProbeWriter {
        begin: Option<mpsc::SyncSender<()>>,
        acquired: mpsc::Receiver<()>,
        interleaved: Arc<AtomicBool>,
        bytes: Arc<Mutex<Vec<u8>>>,
    }
    impl Write for ProbeWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(begin) = self.begin.take() {
                begin.send(()).unwrap();
                let acquired = self
                    .acquired
                    .recv_timeout(Duration::from_millis(100))
                    .is_ok();
                self.interleaved.store(acquired, Ordering::Release);
            }
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn owned_stdio_diagnostic_holds_stderr_lock_across_buffer_flushes() {
        let (begin_tx, begin_rx) = mpsc::sync_channel(1);
        let (acquired_tx, acquired_rx) = mpsc::sync_channel(1);
        let contender = std::thread::spawn(move || {
            begin_rx.recv().unwrap();
            let _lock = io::stderr().lock();
            let _ = acquired_tx.send(());
        });
        let handler = ReportHandler::new(io::sink(), io::stderr());
        let mut delivery = ReportDelivery::new(handler, Arc::new(()));
        let handler = idle_handler(&mut delivery);
        let interleaved = Arc::new(AtomicBool::new(false));
        let captured = Arc::new(Mutex::new(Vec::new()));
        // Preserve production stdio selection, but observe each serialized fragment
        // without writing a large diagnostic into the test harness's stderr.
        handler.stderr = Box::new(ProbeWriter {
            begin: Some(begin_tx),
            acquired: acquired_rx,
            interleaved: Arc::clone(&interleaved),
            bytes: Arc::clone(&captured),
        });
        handler
            .handle(EmitOutput {
                id: EffectId(1),
                event: hoimin_core::OutputEvent::Diagnostic(hoimin_core::Diagnostic::new(
                    "run",
                    1,
                    "warning",
                    "baseline.output",
                    "x".repeat(32 * 1024),
                )),
            })
            .unwrap();
        contender.join().unwrap();
        serde_json::from_slice::<serde_json::Value>(&captured.lock().unwrap()).unwrap();

        assert!(
            !interleaved.load(Ordering::Acquire),
            "another stderr writer entered mid-record"
        );
    }

    use std::sync::Mutex;

    use hoimin_core::{OutputEvent, RunStarted};

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
            event: OutputEvent::RunStarted(RunStarted::minimal(
                "owned-report",
                id,
                hoimin_core::ResourceControl {
                    mode: hoimin_core::ResourceMode::Hard,
                    mechanism: "test_supplied_hard".into(),
                },
            )),
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
        let handler = ReportHandler::new(PanickingWriter, io::sink());
        let mut delivery = ReportDelivery::new(handler, Arc::new(()));
        let error = delivery.handle(event(17)).await.unwrap_err();
        let later_error = delivery.handle(event(18)).await.unwrap_err();

        assert_eq!(error.id, EffectId(17));
        assert!(matches!(error.failure, EffectFailure::ReportState { .. }));
        assert_eq!(later_error.id, EffectId(18));
        assert!(matches!(
            later_error.failure,
            EffectFailure::ReportState { .. }
        ));
        assert!(!delivery.is_quiescent());
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum ReportTask {
        Queued,
        Active,
        Refused,
        Returned,
    }

    #[derive(serde::Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum AdmissionEvent {
        Restrict { deadline: u64 },
        Advance { duration: u64 },
        Abandon,
        Start,
        Finish,
    }

    #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct AdmissionObservation {
        may_start: bool,
        task: ReportTask,
        owner_held: bool,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct AdmissionCase {
        schema: u8,
        mode: String,
        id: String,
        gate_flush: bool,
        events: Vec<AdmissionEvent>,
        expected: Vec<AdmissionObservation>,
    }

    #[test]
    fn report_admission_and_owner_lifetime_match_lean() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .max_blocking_threads(1)
            .build()
            .unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for line in
            include_str!("../../../../formal/HoiminOracle/corpus/report-admission.jsonl").lines()
        {
            let case: AdmissionCase = serde_json::from_str(line).unwrap();
            assert_eq!(case.schema, 1, "{}", case.id);
            assert_eq!(case.mode, "internal-fixture", "{}", case.id);
            assert!(ids.insert(case.id.clone()));
            runtime.block_on(replay_report_admission(case));
        }
        assert_eq!(ids.len(), 18);
    }

    async fn replay_report_admission(case: AdmissionCase) {
        const OUTPUT_ID: u64 = 7;
        let start = tokio::time::Instant::now();
        let (pool_entered, pool_ready) = tokio::sync::oneshot::channel();
        let (pool_release, hold_pool) = mpsc::channel();
        let pool_blocker = tokio::task::spawn_blocking(move || {
            pool_entered.send(()).unwrap();
            hold_pool.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        pool_ready.await.unwrap();

        let (writer_entered, mut writer_ready) = tokio::sync::oneshot::channel();
        let (writer_release, hold_writer) = mpsc::channel();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let handler = ReportHandler::new(
            GatedWriter {
                entered: Some(writer_entered),
                release: hold_writer,
                bytes: Arc::clone(&bytes),
                gate_flush: case.gate_flush,
            },
            io::sink(),
        );
        let owner = Arc::new(());
        let lease = Arc::downgrade(&owner);
        let mut delivery = ReportDelivery::new(handler, owner);
        let admission = delivery.admission();
        let mut write = Box::pin(delivery.handle(event(OUTPUT_ID)));
        tokio::select! {
            biased;
            result = &mut write => panic!("{}: queued operation completed: {result:?}", case.id),
            () = tokio::task::yield_now() => {}
        }
        drop(write);
        // Keep the real blocking task observable after dropping its driver.
        let OwnedDeliveryState::Running(worker) =
            std::mem::replace(&mut delivery.state, OwnedDeliveryState::Unavailable)
        else {
            panic!("{}: expected a queued report task", case.id);
        };

        let mut status = ReportTask::Queued;
        let mut worker = Some(worker);
        let mut driver = Some(delivery);
        assert_eq!(case.events.len(), case.expected.len(), "{}", case.id);
        for (index, (event, expected)) in case.events.into_iter().zip(case.expected).enumerate() {
            match event {
                AdmissionEvent::Restrict { deadline } => {
                    admission.restrict_to(start + Duration::from_secs(deadline));
                }
                AdmissionEvent::Advance { duration } => {
                    tokio::time::advance(Duration::from_secs(duration)).await;
                }
                AdmissionEvent::Abandon => drop(driver.take()),
                AdmissionEvent::Start => {
                    pool_release.send(()).unwrap();
                    let task = worker.as_mut().unwrap();
                    status = tokio::select! {
                        result = task => {
                            let (_, result) = result.unwrap();
                            let error = result.expect_err("disallowed queued operation");
                            assert_eq!(error.id, EffectId(OUTPUT_ID));
                            let EffectFailure::ReportState { message } = error.failure else {
                                panic!("{}: wrong admission error: {error:?}", case.id);
                            };
                            assert!(message.contains("abandoned or its deadline expired"), "{message}");
                            worker = None;
                            ReportTask::Refused
                        }
                        ready = &mut writer_ready => {
                            ready.unwrap();
                            ReportTask::Active
                        }
                    };
                }
                AdmissionEvent::Finish => {
                    writer_release.send(()).unwrap();
                    let (_, result) = worker.take().unwrap().await.unwrap();
                    assert_eq!(result.unwrap().unwrap().id, EffectId(OUTPUT_ID));
                    status = ReportTask::Returned;
                }
            }
            assert_eq!(
                AdmissionObservation {
                    may_start: admission.permits_start(),
                    task: status,
                    owner_held: lease.upgrade().is_some(),
                },
                expected,
                "{} step {index}",
                case.id
            );
        }
        assert!(worker.is_none(), "{}", case.id);
        pool_blocker.await.unwrap();
        let bytes = bytes.lock().unwrap();
        if status == ReportTask::Returned {
            let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(record["sequence"], OUTPUT_ID, "{}", case.id);
        } else {
            assert!(bytes.is_empty(), "{}", case.id);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_wait_retains_writer_and_owner_without_acknowledgement_or_retry() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let owner = Arc::new(());
        let lease = Arc::downgrade(&owner);
        let handler = ReportHandler::new(
            GatedWriter {
                entered: Some(entered_tx),
                release: release_rx,
                bytes: Arc::clone(&bytes),
                gate_flush: false,
            },
            io::sink(),
        );
        let mut delivery = ReportDelivery::new(handler, owner);
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
        let handler = ReportHandler::new(
            GatedWriter {
                entered: Some(entered_tx),
                release: release_rx,
                bytes: Arc::new(Mutex::new(Vec::new())),
                gate_flush: true,
            },
            io::sink(),
        );
        let mut delivery = ReportDelivery::new(handler, owner);
        let mut flush = Box::pin(delivery.flush());
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
