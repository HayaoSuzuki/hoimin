//! Project inputs shared by hierarchy analysis and fingerprint revalidation.
use super::rust::exception_hierarchy::ExceptionIndex;
use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{MutationOperator, RunConfig, Selection};
use std::sync::{Arc, Mutex};

const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct ExceptionProject {
    selection: Selection,
    roots: Vec<Utf8PathBuf>,
    index: Arc<Mutex<Option<Arc<ExceptionIndex>>>>,
}

impl ExceptionProject {
    pub(crate) fn from_config(config: &RunConfig) -> Option<Self> {
        config
            .operators
            .contains(MutationOperator::ExceptionHierarchy)
            .then(|| {
                let mut roots = vec![Utf8PathBuf::new()];
                for root in config.import_roots.iter().chain(&config.selection.sources) {
                    if let Ok(path) = hoimin_core::normalize_logical_path(&config.root, root)
                        && !roots.contains(&path)
                    {
                        roots.push(path);
                    }
                }
                let mut result = Self::new(&config.root);
                result
                    .selection
                    .includes
                    .clone_from(&config.selection.includes);
                result
                    .selection
                    .excludes
                    .clone_from(&config.selection.excludes);
                result.roots = roots;
                result
            })
    }
    pub(crate) fn new(root: &Utf8Path) -> Self {
        Self {
            selection: Selection {
                root: root.to_owned(),
                ..Selection::default()
            },
            roots: vec![Utf8PathBuf::new()],
            index: Arc::default(),
        }
    }
    pub(crate) fn files(&self, cancelled: &impl Fn() -> bool) -> Result<Vec<Utf8PathBuf>, String> {
        crate::target::fs::discover_python_bounded(&self.selection, MAX_FILES, cancelled)
            .map(|files| files.into_iter().map(|f| f.path).collect())
            .map_err(|e| format!("analyzer.exception_hierarchy: {e}"))
    }
    pub(crate) fn load(
        &self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<ExceptionIndex>, String> {
        if cancelled() {
            return Err("analyzer.cancelled".into());
        }
        if let Some(index) = self.index.lock().map_err(|e| e.to_string())?.as_ref() {
            return Ok(Arc::clone(index));
        }
        let reader = crate::workspace::PortableFileReader::open(self.selection.root.clone())
            .map_err(|e| e.to_string())?;
        let mut sources = Vec::new();
        let mut remaining = MAX_TOTAL_BYTES;
        for path in self.files(cancelled)? {
            if cancelled() {
                return Err("analyzer.cancelled".into());
            }
            let bytes = reader
                .read_bounded(&path, MAX_FILE_BYTES.min(remaining))
                .map_err(|e| format!("{path}: {e}"))?;
            let decoded =
                hoimin_core::decode_python_source(&bytes).map_err(|e| format!("{path}: {e}"))?;
            if decoded.text().len() > MAX_FILE_BYTES {
                return Err(format!("{path}: decoded source byte limit exceeded"));
            }
            remaining = remaining
                .checked_sub(decoded.text().len())
                .ok_or("exception hierarchy total decoded source byte limit exceeded")?;
            sources.push((path, decoded.text().to_owned()));
        }
        let index = Arc::new(
            ExceptionIndex::from_sources(&sources, &self.roots, cancelled)
                .map_err(|e| e.to_string())?,
        );
        *self.index.lock().map_err(|e| e.to_string())? = Some(Arc::clone(&index));
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hierarchy_project_bounds_reads_and_does_not_cache_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("errors.py");
        std::fs::File::create(&path)
            .unwrap()
            .set_len((MAX_FILE_BYTES + 1) as u64)
            .unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().into()).unwrap();
        let project = ExceptionProject::new(&root);
        assert!(
            project
                .load(&|| false)
                .err()
                .unwrap()
                .contains("byte limit")
        );
        std::fs::write(path, "class Root(Exception): pass\n").unwrap();
        assert!(project.load(&|| true).is_err());
        let first = project.load(&|| false).unwrap();
        let second = project.load(&|| false).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
    }
    #[test]
    fn hierarchy_project_bounds_file_discovery_and_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().into()).unwrap();
        let project = ExceptionProject::new(&root);
        assert!(project.files(&|| true).is_err());
        for i in 0..=MAX_FILES {
            std::fs::write(dir.path().join(format!("{i}.py")), "").unwrap();
        }
        assert!(project.files(&|| false).unwrap_err().contains("file limit"));
    }
}
