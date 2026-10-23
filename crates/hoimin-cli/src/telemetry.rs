//! Binary-owned diagnostics. Sink I/O never runs on the executor thread.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

const FLUSH_BUDGET: Duration = Duration::from_millis(250);
const MAX_RECORD_BYTES: usize = 64 * 1024;
const QUEUE_CAPACITY: usize = 128;

pub(super) struct Guard {
    stop: Arc<AtomicBool>,
    sender: mpsc::SyncSender<Vec<u8>>,
    done: mpsc::Receiver<()>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake an idle receiver; a full queue already guarantees it can make progress.
        let _ = self.sender.try_send(Vec::new());
        // A blocked stderr must not extend execution/shutdown deadlines indefinitely.
        let _ = self.done.recv_timeout(FLUSH_BUDGET);
    }
}

pub(super) fn init() -> Option<Guard> {
    let (guard, writer) = output_worker(io::stderr()).ok()?;
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::OFF.into())
        .with_regex(false)
        .try_from_env()
        .unwrap_or_else(|_| EnvFilter::new("off"));
    let logs = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .with_writer(move || RecordWriter::new(writer.clone()))
        .with_filter(filter);
    Registry::default().with(logs).try_init().ok()?;
    Some(guard)
}

pub(super) fn finish(exit_code: i32) {
    tracing::info!(target: "hoimin_cli::progress", finished = true, exit_code, "command finished");
    tracing::debug!(target: "hoimin_cli", exit_code, "command completed");
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
) -> io::Result<(Guard, mpsc::SyncSender<Vec<u8>>)> {
    let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(QUEUE_CAPACITY);
    let (done_tx, done) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("hoimin-diagnostics".to_owned())
        .spawn(move || {
            let _ = write_output(&mut sink, &receiver, &worker_stop);
            let _ = done_tx.send(());
        })?;
    Ok((
        Guard {
            stop,
            sender: sender.clone(),
            done,
        },
        sender,
    ))
}

fn write_output(
    sink: &mut impl Write,
    receiver: &mpsc::Receiver<Vec<u8>>,
    stop: &AtomicBool,
) -> io::Result<()> {
    while !stop.load(Ordering::Acquire) {
        match receiver.recv() {
            Ok(bytes) => sink.write_all(&bytes)?,
            Err(_) => break,
        }
    }
    // A finite snapshot avoids waiting for producers during shutdown.
    for bytes in receiver.try_iter().take(QUEUE_CAPACITY) {
        sink.write_all(&bytes)?;
    }
    sink.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

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

    struct FlushNotice {
        written: mpsc::Sender<()>,
        flushed: mpsc::Sender<()>,
    }

    impl Write for FlushNotice {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.written.send(()).unwrap();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed.send(()).unwrap();
            Ok(())
        }
    }

    #[test]
    fn idle_worker_flushes_on_drop_with_a_live_publisher() {
        let (written_tx, written_rx) = mpsc::channel();
        let (flushed_tx, flushed_rx) = mpsc::channel();
        let (guard, sender) = output_worker(FlushNotice {
            written: written_tx,
            flushed: flushed_tx,
        })
        .unwrap();
        sender.send(b"record\n".to_vec()).unwrap();
        written_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(guard);
        flushed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Keeping this publisher alive prevents channel disconnect from waking the worker.
        drop(sender);
    }

    #[test]
    fn shutdown_drains_queued_records_despite_a_live_publisher() {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        sender.send(b"first\n".to_vec()).unwrap();
        sender.send(b"second\n".to_vec()).unwrap();
        let mut output = Vec::new();
        write_output(&mut output, &receiver, &AtomicBool::new(true)).unwrap();
        assert_eq!(output, b"first\nsecond\n");
        drop(sender);
    }

    struct ReplenishingSink {
        sender: mpsc::SyncSender<Vec<u8>>,
        output: Vec<u8>,
        flushed: bool,
        remaining: usize,
    }

    impl Write for ReplenishingSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.output.extend_from_slice(bytes);
            if self.remaining > 0 {
                self.sender.try_send(b"record\n".to_vec()).unwrap();
                self.remaining -= 1;
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed = true;
            Ok(())
        }
    }

    #[test]
    fn shutdown_drain_is_finite_when_a_publisher_keeps_queuing() {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        sender.send(b"record\n".to_vec()).unwrap();
        let mut sink = ReplenishingSink {
            sender,
            output: Vec::new(),
            flushed: false,
            remaining: QUEUE_CAPACITY + 1,
        };
        write_output(&mut sink, &receiver, &AtomicBool::new(true)).unwrap();
        assert_eq!(sink.output, b"record\n".repeat(QUEUE_CAPACITY));
        assert!(sink.flushed);
        assert_eq!(receiver.try_recv().unwrap(), b"record\n");
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
        let (guard, sender) = output_worker(sink).unwrap();
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
