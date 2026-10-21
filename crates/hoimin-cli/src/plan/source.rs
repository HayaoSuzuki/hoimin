use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};

use crate::workspace::{PortableFileReadError, PortableFileReader};

/// Acquires plan sources from one capability root without blocking the executor.
/// Root acquisition is lazy so descriptor errors keep their requested-ID priority.
/// Read/hash reject growth or truncation against the opened descriptor length.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{WorkspaceRaceHook, install_workspace_race_hook};
    use std::io::Write;

    struct ChangeAfterBudget {
        path: Utf8PathBuf,
        shrink: bool,
    }

    impl WorkspaceRaceHook for ChangeAfterBudget {
        fn parent_opened(&self, operation: &'static str, _: &Utf8Path) {
            if operation == "read-budget" {
                if self.shrink {
                    std::fs::File::create(&self.path).unwrap();
                } else {
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(&self.path)
                        .unwrap()
                        .write_all(&[b' '; 4096])
                        .unwrap();
                }
            }
        }
    }

    #[test]
    fn fingerprint_rejects_changes_after_read_budget() {
        for shrink in [false, true] {
            for exact in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let root = Utf8Path::from_path(dir.path()).unwrap();
                let path = root.join("input.py");
                std::fs::write(&path, b"x = 1\n").unwrap();
                let _hook =
                    install_workspace_race_hook(Arc::new(ChangeAfterBudget { path, shrink }));
                let (patterns, files) = if exact {
                    (vec![], vec!["input.py".to_owned()])
                } else {
                    (vec!["*.py".to_owned()], vec![])
                };
                let error =
                    crate::fingerprint_inputs::resolve(root, &patterns, &files).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains(if shrink { "file shrank" } else { "file grew" }),
                    "{error}"
                );
            }
        }
    }

    #[tokio::test]
    async fn stable_sources_preserve_bytes_and_hash_without_a_fixed_cap() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let mut reader = SourceReader::new(root);
        for length in [0, 1, 128 * 1024] {
            let bytes: Vec<_> = (0..length)
                .map(|i| u8::try_from(i % 251).unwrap())
                .collect();
            std::fs::write(root.join("input.py"), &bytes).unwrap();
            assert_eq!(reader.read(Utf8Path::new("input.py")).await.unwrap(), bytes);
            assert_eq!(
                reader.hash(Utf8Path::new("input.py")).await.unwrap(),
                blake3::hash(&bytes)
            );
        }
    }

    // Both plan source operations must reject a changed file. Skipping the
    // finite reader would accept the appended hash/bytes or an empty prefix.
    #[test]
    fn plan_source_rejects_changes_after_read_budget() {
        for shrink in [false, true] {
            for hash in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let root = Utf8Path::from_path(dir.path()).unwrap();
                let path = root.join("input.py");
                std::fs::write(&path, b"x = 1\n").unwrap();
                let hook = Arc::new(ChangeAfterBudget { path, shrink });
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .max_blocking_threads(1)
                    .on_thread_start(move || {
                        // The runtime owns the blocking thread and its thread-local
                        // hook; both end when this test's runtime is dropped.
                        std::mem::forget(install_workspace_race_hook(hook.clone()));
                    })
                    .build()
                    .unwrap();
                let result = runtime.block_on(async {
                    let mut reader = SourceReader::new(root);
                    if hash {
                        reader.hash(Utf8Path::new("input.py")).await.map(|_| ())
                    } else {
                        reader.read(Utf8Path::new("input.py")).await.map(|_| ())
                    }
                });
                let error = result.unwrap_err();
                assert!(
                    error.contains(if shrink { "file shrank" } else { "file grew" }),
                    "hash={hash}: {error}"
                );
            }
        }
    }
}
