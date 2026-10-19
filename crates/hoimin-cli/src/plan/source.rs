use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};

use crate::workspace::{PortableFileReadError, PortableFileReader};

/// Acquires plan sources from one capability root without blocking the executor.
/// Root acquisition is lazy so descriptor errors keep their requested-ID priority.
/// A cancelled caller does not stop an already started filesystem operation.
pub(super) struct SourceReader {
    root: Utf8PathBuf,
    reader: Option<Arc<PortableFileReader>>,
}

impl SourceReader {
    pub(super) fn new(root: &Utf8Path) -> Self {
        Self {
            root: root.to_owned(),
            reader: None,
        }
    }

    pub(super) async fn read(&mut self, path: &Utf8Path) -> Result<Vec<u8>, String> {
        self.with_file(path, PortableFileReader::read).await
    }

    pub(super) async fn hash(&mut self, path: &Utf8Path) -> Result<blake3::Hash, String> {
        self.with_file(path, PortableFileReader::hash).await
    }

    async fn with_file<T: Send + 'static>(
        &mut self,
        path: &Utf8Path,
        operation: impl FnOnce(&PortableFileReader, &Utf8Path) -> Result<T, PortableFileReadError>
        + Send
        + 'static,
    ) -> Result<T, String> {
        let result = async {
            if self.reader.is_none() {
                let root = self.root.clone();
                self.reader = Some(Arc::new(
                    blocking(move || {
                        PortableFileReader::open(root).map_err(PortableFileReadError::from)
                    })
                    .await?,
                ));
            }
            let reader = Arc::clone(self.reader.as_ref().expect("reader was opened"));
            let path = path.to_owned();
            blocking(move || operation(&reader, &path)).await
        }
        .await;
        result.map_err(|error| format!("{path}: {error}"))
    }
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, PortableFileReadError> + Send + 'static,
) -> Result<T, String> {
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(operation))
        .await
        .map_err(|error| format!("source I/O task failed: {error}"))?
        .map_err(|error| error.to_string())
}
