#[path = "support/collecting_writer.rs"]
mod collecting_writer;
use collecting_writer::CollectingWriter;

use std::io::{self, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

struct PausedWriter {
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    release: mpsc::Receiver<()>,
    pause_flush: bool,
    dropped: Arc<AtomicBool>,
}

impl PausedWriter {
    fn pause(&mut self) -> io::Result<()> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            self.release
                .recv_timeout(Duration::from_secs(10))
                .map_err(io::Error::other)?;
        }
        Ok(())
    }
}
impl Write for PausedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.pause_flush {
            self.pause()?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.pause_flush {
            self.pause()?;
        }
        Ok(())
    }
}
impl Drop for PausedWriter {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Release);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn owned_public_entry_bounds_stalled_write_and_flush_without_dropping_writer() {
    for pause_flush in [false, true] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("target.py"), "pass\n").unwrap();
        let python = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
            "../../.venv/Scripts/python.exe"
        } else {
            "../../.venv/bin/python"
        });
        assert!(
            python.is_file(),
            "missing controlled Python: {}",
            python.display()
        );
        let args = vec![
            "hoimin".into(),
            "run".into(),
            "--root".into(),
            project.path().as_os_str().to_owned(),
            "--file".into(),
            "target.py".into(),
            "--format".into(),
            "json".into(),
            "--allow-best-effort-memory".into(),
            "--min-free-space".into(),
            "1B".into(),
            "--total-timeout".into(),
            "1s".into(),
            "--".into(),
            python.into_os_string(),
            "-c".into(),
            "pass".into(),
        ];
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel();
        let dropped = Arc::new(AtomicBool::new(false));
        let writer = PausedWriter {
            entered: Some(entered_tx),
            release: release_rx,
            pause_flush,
            dropped: Arc::clone(&dropped),
        };
        let mut run = Box::pin(hoimin_cli::run_with_writers(args, writer, io::sink()));
        tokio::select! {
            result = &mut run => panic!("run returned before writer entered: {result}; pause_flush={pause_flush}"),
            entered = entered_rx => entered.expect("writer pause was entered"),
        }
        let result = tokio::time::timeout(Duration::from_secs(4), &mut run).await;
        let retained = !dropped.load(Ordering::Acquire);
        release_tx.send(()).unwrap();
        assert_eq!(
            result.expect("stalled writer exceeded total timeout plus grace"),
            2
        );
        assert!(retained, "in-flight writer ownership was dropped");
        tokio::time::timeout(Duration::from_secs(2), async {
            while !dropped.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("released writer was not eventually dropped");
    }
}

#[tokio::test]
async fn owned_entry_reports_preparation_failure_once_on_stderr() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), "pass\n").unwrap();
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let exit = hoimin_cli::run_with_writers(
        [
            "hoimin",
            "run",
            "--root",
            project.path().to_str().unwrap(),
            "--file",
            "target.py",
            "--fingerprint-include",
            "missing.file",
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--",
            "unused-command",
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;
    assert_eq!(exit, 2);
    assert!(stdout.is_empty());
    let diagnostic = String::from_utf8(stderr.snapshot()).unwrap();
    assert_eq!(diagnostic.lines().count(), 1, "{diagnostic}");
    assert!(diagnostic.contains("missing.file"), "{diagnostic}");
}

#[tokio::test]
async fn owned_entry_reports_loop_failure_once_without_dispatcher_reprint() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("target.py"), "pass\n").unwrap();
    let missing_command = project.path().join("command-that-does-not-exist.exe");
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let exit = hoimin_cli::run_with_writers(
        [
            "hoimin",
            "run",
            "--root",
            project.path().to_str().unwrap(),
            "--file",
            "target.py",
            "--format",
            "jsonl",
            "--allow-best-effort-memory",
            "--min-free-space",
            "1B",
            "--",
            missing_command.to_str().unwrap(),
            "-c",
            "pass",
        ],
        stdout.clone(),
        stderr.clone(),
    )
    .await;
    assert_eq!(exit, 2);
    let diagnostic = String::from_utf8(stderr.snapshot()).unwrap();
    let events = diagnostic
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1, "{diagnostic}");
    assert_eq!(events[0]["code"], "process.spawn", "{diagnostic}");
}
