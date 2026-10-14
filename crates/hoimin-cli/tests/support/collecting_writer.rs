use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// Owned test output with snapshots independent of the run's writer ownership.
#[derive(Clone, Debug, Default)]
pub struct CollectingWriter(Arc<Mutex<Vec<u8>>>);

#[allow(
    dead_code,
    reason = "shared test utility compiled separately by each integration target"
)]
impl CollectingWriter {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
    #[must_use]
    pub fn snapshot(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for CollectingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
