//! Binary-owned diagnostics. Sink I/O never runs on the executor thread.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::{EnvFilter, LevelFilter, filter_fn};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

const PROGRESS_TARGET: &str = "hoimin_cli::progress";
const REFRESH: Duration = Duration::from_secs(1);
const FLUSH_BUDGET: Duration = Duration::from_millis(250);
const MAX_RECORD_BYTES: usize = 64 * 1024;
const QUEUE_CAPACITY: usize = 128;

#[derive(Clone)]
struct Snapshot {
    stage: String,
    completed: u64,
    started: Instant,
}

type Progress = Arc<Mutex<Option<Snapshot>>>;

pub(super) struct Guard {
    stop: Arc<AtomicBool>,
    sender: mpsc::SyncSender<Vec<u8>>,
    done: mpsc::Receiver<()>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.sender.try_send(Vec::new());
        // A blocked stderr must not extend execution/shutdown deadlines indefinitely.
        let _ = self.done.recv_timeout(FLUSH_BUDGET);
    }
}

pub(super) fn init() -> Option<Guard> {
    let terminal = io::stderr().is_terminal();
    let (guard, writer, progress) = output_worker(io::stderr(), terminal).ok()?;
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::OFF.into())
        .with_regex(false)
        .try_from_env()
        .unwrap_or_else(|_| EnvFilter::new("off"));
    let logs = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(move || RecordWriter::new(writer.clone()));
    let logs = if terminal {
        logs.with_filter(filter).boxed()
    } else {
        logs.json().with_filter(filter).boxed()
    };
    let progress = terminal.then(|| {
        ProgressLayer(progress).with_filter(filter_fn(|metadata| {
            metadata.target() == PROGRESS_TARGET && metadata.is_event()
        }))
    });
    Registry::default()
        .with(logs)
        .with(progress)
        .try_init()
        .ok()?;
    Some(guard)
}

pub(super) fn finish(exit_code: i32) {
    tracing::info!(target: "hoimin_cli::progress", finished = true, exit_code, "command finished");
    tracing::debug!(target: "hoimin_cli", exit_code, "command completed");
}

struct ProgressLayer(Progress);

#[derive(Default)]
struct Update {
    stage: Option<String>,
    completed: Option<u64>,
    finished: bool,
}

impl Visit for Update {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "stage" {
            self.stage = Some(value.to_owned());
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "completed" {
            self.completed = Some(value);
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == "finished" {
            self.finished = value;
        }
    }

    fn record_debug(&mut self, _: &Field, _: &dyn fmt::Debug) {}
}

impl<S: Subscriber> Layer<S> for ProgressLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut update = Update::default();
        event.record(&mut update);
        let mut progress = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(stage) = update.stage {
            let snapshot = progress.get_or_insert_with(|| Snapshot {
                stage: stage.clone(),
                completed: 0,
                started: Instant::now(),
            });
            snapshot.stage = stage;
        }
        if let Some(snapshot) = progress.as_mut() {
            if let Some(completed) = update.completed {
                snapshot.completed = completed;
            }
            if update.finished {
                "finished".clone_into(&mut snapshot.stage);
            }
        }
    }
}

struct RecordWriter {
    sender: mpsc::SyncSender<Vec<u8>>,
    bytes: Vec<u8>,
    oversized: bool,
}

impl RecordWriter {
    const fn new(sender: mpsc::SyncSender<Vec<u8>>) -> Self {
        Self {
            sender,
            bytes: Vec::new(),
            oversized: false,
        }
    }
}

impl Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > MAX_RECORD_BYTES {
            self.oversized = true;
        } else if !self.oversized {
            self.bytes.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RecordWriter {
    fn drop(&mut self) {
        if !self.oversized && !self.bytes.is_empty() {
            // Drop whole records under pressure; never emit a truncated JSON record.
            let _ = self.sender.try_send(std::mem::take(&mut self.bytes));
        }
    }
}

fn output_worker(
    mut sink: impl Write + Send + 'static,
    terminal: bool,
) -> io::Result<(Guard, mpsc::SyncSender<Vec<u8>>, Progress)> {
    let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(QUEUE_CAPACITY);
    let (done_tx, done) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let progress: Progress = Arc::default();
    let worker_progress = Arc::clone(&progress);
    let worker_stop = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("hoimin-diagnostics".to_owned())
        .spawn(move || {
            let _ = write_output(
                &mut sink,
                &receiver,
                &worker_stop,
                &worker_progress,
                terminal,
            );
            let _ = done_tx.send(());
        })?;
    Ok((
        Guard {
            stop,
            sender: sender.clone(),
            done,
        },
        sender,
        progress,
    ))
}

fn write_output(
    sink: &mut impl Write,
    receiver: &mpsc::Receiver<Vec<u8>>,
    stop: &AtomicBool,
    progress: &Progress,
    terminal: bool,
) -> io::Result<()> {
    let mut next_refresh = Instant::now() + REFRESH;
    while !stop.load(Ordering::Acquire) {
        match receiver.recv_timeout(next_refresh.saturating_duration_since(Instant::now())) {
            Ok(bytes) => sink.write_all(&bytes)?,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() >= next_refresh {
            if terminal {
                render_progress(sink, progress)?;
            }
            next_refresh = Instant::now() + REFRESH;
        }
    }
    // A finite snapshot avoids waiting for producers during shutdown.
    for bytes in receiver.try_iter().take(QUEUE_CAPACITY) {
        sink.write_all(&bytes)?;
    }
    if terminal {
        render_progress(sink, progress)?;
    }
    sink.flush()
}

fn render_progress(sink: &mut impl Write, progress: &Progress) -> io::Result<()> {
    let snapshot = progress
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(snapshot) = snapshot {
        writeln!(
            sink,
            "hoimin: {} | {} completed | elapsed {}s",
            snapshot.stage,
            snapshot.completed,
            snapshot.started.elapsed().as_secs()
        )?;
        sink.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_queue_and_oversized_records_drop_whole_records() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut record = RecordWriter::new(sender.clone());
        record.write_all(b"{\"message\":").unwrap();
        record.write_all(b"\"first\"}\n").unwrap();
        drop(record);
        let mut dropped = RecordWriter::new(sender.clone());
        dropped.write_all(b"{\"message\":\"second\"}\n").unwrap();
        drop(dropped);
        assert_eq!(receiver.try_recv().unwrap(), b"{\"message\":\"first\"}\n");
        assert!(receiver.try_recv().is_err());
        let mut oversized = RecordWriter::new(sender);
        oversized.write_all(&vec![b'x'; MAX_RECORD_BYTES]).unwrap();
        oversized.write_all(b"x").unwrap();
        drop(oversized);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn progress_counts_and_timer_survive_stage_changes() {
        let progress: Progress = Arc::default();
        let subscriber = Registry::default().with(ProgressLayer(Arc::clone(&progress)));
        tracing::subscriber::with_default(subscriber, || {
            // Help/completions finish without ever starting progress.
            finish(0);
            let mut output = Vec::new();
            render_progress(&mut output, &progress).unwrap();
            assert!(output.is_empty());
            tracing::info!(stage = "testing mutants", completed = 2_u64);
            progress.lock().unwrap().as_mut().unwrap().started =
                Instant::now().checked_sub(Duration::from_secs(9)).unwrap();
            tracing::info!(stage = "cleaning up");
            finish(1);
            render_progress(&mut output, &progress).unwrap();
            let output = String::from_utf8(output).unwrap();
            assert!(
                output.contains("finished | 2 completed | elapsed "),
                "{output}"
            );
            let seconds: u64 = output
                .split("elapsed ")
                .nth(1)
                .unwrap()
                .trim()
                .trim_end_matches('s')
                .parse()
                .unwrap();
            assert!(seconds >= 9, "{output}");
        });
    }

    struct BlockedSink {
        entered: mpsc::SyncSender<()>,
        release: Option<mpsc::Receiver<()>>,
    }

    impl Write for BlockedSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(release) = self.release.take() {
                self.entered.send(()).unwrap();
                release.recv().unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn blocked_sink_cannot_block_publishers_or_shutdown() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let sink = BlockedSink {
            entered: entered_tx,
            release: Some(release_rx),
        };
        let (guard, sender, _) = output_worker(sink, false).unwrap();
        sender.send(b"first\n".to_vec()).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        for _ in 0..QUEUE_CAPACITY * 2 {
            let mut record = RecordWriter::new(sender.clone());
            record.write_all(b"message\n").unwrap();
        }
        drop(guard);
        let elapsed = start.elapsed();
        release_tx.send(()).unwrap();
        assert!(
            elapsed < Duration::from_secs(1),
            "publish/flush blocked for {elapsed:?}"
        );
    }
}
