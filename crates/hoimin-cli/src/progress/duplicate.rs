use std::collections::HashMap;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::input::InputDisposition;
use crate::artifacts::open_regular_file;
use crate::finite_file::SnapshotReader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DuplicateReason {
    SamePath,
    IdenticalBytes,
}

impl DuplicateReason {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::SamePath => "same path",
            Self::IdenticalBytes => "identical bytes",
        }
    }
}

#[derive(Debug)]
pub(super) struct DuplicateInput {
    pub(super) previous: usize,
    pub(super) reason: DuplicateReason,
}

#[derive(Default)]
pub(super) struct DuplicateInputs {
    paths: HashMap<PathBuf, usize>,
    fingerprints: HashMap<blake3::Hash, Vec<usize>>,
}

impl DuplicateInputs {
    pub(super) fn observe(
        &mut self,
        path: &Path,
        fingerprint: blake3::Hash,
        inputs: &[InputDisposition],
    ) -> Option<DuplicateInput> {
        let same_path = self.paths.get(path).copied();
        let candidates = self.fingerprints.entry(fingerprint).or_default();
        let duplicate = if let Some(previous) = same_path {
            Some(DuplicateInput {
                previous,
                reason: DuplicateReason::SamePath,
            })
        } else {
            candidates
                .iter()
                .copied()
                .find(|index| {
                    // Confirmation is optional. A file disappearing or becoming
                    // unreadable after validation must not change progress results.
                    identical_regular_files(&inputs[*index].source, path).unwrap_or(false)
                })
                .map(|previous| DuplicateInput {
                    previous,
                    reason: DuplicateReason::IdenticalBytes,
                })
        };
        self.paths.entry(path.to_path_buf()).or_insert(inputs.len());
        candidates.push(inputs.len());
        duplicate
    }
}

fn identical_regular_files(left: &Path, right: &Path) -> io::Result<bool> {
    let (Some(left), Some(right)) = (open_regular_file(left)?, open_regular_file(right)?) else {
        return Ok(false);
    };
    let mut left = BufReader::new(SnapshotReader::new(left)?);
    let mut right = BufReader::new(SnapshotReader::new(right)?);
    #[cfg(test)]
    tests::run_readers_prepared_hook();
    loop {
        let left_bytes = left.fill_buf()?;
        let right_bytes = right.fill_buf()?;
        if left_bytes.is_empty() || right_bytes.is_empty() {
            return Ok(left_bytes.is_empty() && right_bytes.is_empty());
        }
        let count = left_bytes.len().min(right_bytes.len());
        if left_bytes[..count] != right_bytes[..count] {
            return Ok(false);
        }
        left.consume(count);
        right.consume(count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static READERS_PREPARED: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
            std::cell::RefCell::new(None);
    }

    fn on_readers_prepared(action: impl FnOnce() + 'static) {
        READERS_PREPARED.with(|hook| {
            assert!(hook.borrow_mut().replace(Box::new(action)).is_none());
        });
    }

    pub(super) fn run_readers_prepared_hook() {
        let action = READERS_PREPARED.with(|hook| hook.borrow_mut().take());
        if let Some(action) = action {
            action();
        }
    }

    fn append_after_readers_prepared(paths: Vec<PathBuf>) {
        on_readers_prepared(move || {
            use std::io::Write;
            for path in paths {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(path)
                    .unwrap()
                    .write_all(&[b'x'; 128])
                    .unwrap();
            }
        });
    }

    #[test]
    fn confirmation_rejects_append_after_readers_are_prepared() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        std::fs::write(&first, b"same").unwrap();
        std::fs::write(&second, b"same").unwrap();
        append_after_readers_prepared(vec![first.clone(), second.clone()]);
        assert_eq!(
            identical_regular_files(&first, &second).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn append_confirmation_failure_omits_optional_annotation() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        std::fs::write(&first, b"same").unwrap();
        std::fs::write(&second, b"same").unwrap();
        let fingerprint = blake3::hash(b"same");
        let mut tracker = DuplicateInputs::default();
        assert!(tracker.observe(&first, fingerprint, &[]).is_none());
        let inputs = [input(first.clone())];
        append_after_readers_prepared(vec![first, second.clone()]);
        assert!(tracker.observe(&second, fingerprint, &inputs).is_none());
    }

    #[test]
    fn confirmation_rejects_truncate_after_readers_are_prepared() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        std::fs::write(&first, b"same").unwrap();
        std::fs::write(&second, b"same").unwrap();
        let paths = [first.clone(), second.clone()];
        on_readers_prepared(move || {
            for path in paths {
                std::fs::write(path, b"").unwrap();
            }
        });
        assert_eq!(
            identical_regular_files(&first, &second).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn hard_link_confirmation_rejects_append_after_readers_are_prepared() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        std::fs::write(&first, b"same").unwrap();
        std::fs::hard_link(&first, &second).unwrap();
        assert!(identical_regular_files(&first, &second).unwrap());
        append_after_readers_prepared(vec![first.clone()]);
        assert_eq!(
            identical_regular_files(&first, &second).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    fn input(path: PathBuf) -> InputDisposition {
        InputDisposition {
            source: path,
            reason: None,
            duplicate: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn confirmation_open_rejects_fifo_without_waiting_for_a_writer() {
        use std::os::unix::fs::OpenOptionsExt;
        use std::sync::mpsc;
        use std::time::Duration;

        let fixture = tempfile::tempdir().unwrap();
        let fifo = fixture.path().join("replaced-report");
        let c_path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: c_path is a valid NUL-terminated fixture path; mkfifo retains no pointer.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let (sender, receiver) = mpsc::channel();
        let worker_path = fifo.clone();
        let worker = std::thread::spawn(move || {
            let rejected = open_regular_file(&worker_path).unwrap().is_none();
            sender.send(rejected).unwrap();
        });
        let result = receiver.recv_timeout(Duration::from_secs(1));
        // Release a broken blocking opener before failing, leaving no stuck
        // thread or FIFO behind in the test process.
        let _rescue = result.is_err().then(|| {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
                .unwrap()
        });
        worker.join().unwrap();
        assert_eq!(
            result,
            Ok(true),
            "optional confirmation waited for a FIFO writer"
        );
    }

    #[test]
    fn digest_collision_requires_exact_bytes_and_keeps_distinct_candidates() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let different = fixture.path().join("different");
        let copy = fixture.path().join("copy");
        std::fs::write(&first, b"same prefix a").unwrap();
        std::fs::write(&different, b"same prefix b").unwrap();
        std::fs::write(&copy, b"same prefix b").unwrap();
        let forced_collision = blake3::hash(b"forced collision");
        let mut tracker = DuplicateInputs::default();
        let mut inputs = Vec::new();
        assert!(tracker.observe(&first, forced_collision, &inputs).is_none());
        inputs.push(input(first));
        assert!(
            tracker
                .observe(&different, forced_collision, &inputs)
                .is_none()
        );
        inputs.push(input(different));
        let duplicate = tracker.observe(&copy, forced_collision, &inputs).unwrap();
        assert_eq!(duplicate.previous, 1);
        assert_eq!(duplicate.reason, DuplicateReason::IdenticalBytes);
    }

    #[test]
    fn exact_comparison_checks_late_bytes_and_length() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let other = fixture.path().join("other");
        let bytes = vec![b'x'; 32 * 1024 + 1];
        std::fs::write(&first, &bytes).unwrap();
        std::fs::write(&other, &bytes).unwrap();
        assert!(identical_regular_files(&first, &other).unwrap());
        let mut different = bytes.clone();
        *different.last_mut().unwrap() = b'y';
        std::fs::write(&other, &different).unwrap();
        assert!(!identical_regular_files(&first, &other).unwrap());
        std::fs::write(&other, &bytes[..bytes.len() - 1]).unwrap();
        assert!(!identical_regular_files(&first, &other).unwrap());
        assert!(!identical_regular_files(&first, fixture.path()).unwrap_or(false));
    }

    #[test]
    fn unavailable_confirmation_does_not_add_input_failure() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        std::fs::write(&first, b"same").unwrap();
        std::fs::write(&second, b"same").unwrap();
        let fingerprint = blake3::hash(b"same");
        let mut tracker = DuplicateInputs::default();
        assert!(tracker.observe(&first, fingerprint, &[]).is_none());
        std::fs::remove_file(&first).unwrap();
        assert!(
            tracker
                .observe(&second, fingerprint, &[input(first)])
                .is_none()
        );
    }
}
