//! Project inputs shared by hierarchy analysis and fingerprint revalidation.
use super::rust::exception_hierarchy::{ExceptionIndex, MAX_SUMMARY_ENTRIES};
use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{MutationOperator, RunConfig, Selection};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct ExceptionProject {
    selection: Selection,
    roots: Vec<Utf8PathBuf>,
    prepared_inputs: Option<Arc<BTreeMap<Utf8PathBuf, String>>>,
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
                result.prepared_inputs = Some(Arc::new(
                    config
                        .fingerprint_inputs
                        .iter()
                        .map(|input| (input.path.clone(), input.hash.clone()))
                        .collect(),
                ));
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
            prepared_inputs: None,
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
            if self.prepared_inputs.as_ref().is_some_and(|inputs| {
                inputs
                    .get(&path)
                    .is_none_or(|expected| blake3::hash(&bytes).to_hex().as_str() != expected)
            }) {
                return Err(format!(
                    "{path}: exception hierarchy input changed after fingerprint preparation"
                ));
            }
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
        // Preserve prepared module origins even if a file temporarily disappears.
        // Missing or excluded inputs reserve their names without supplying classes.
        let inputs = self
            .prepared_inputs
            .iter()
            .flat_map(|inputs| inputs.keys())
            .filter(|path| path.extension() == Some("py"))
            .take(MAX_SUMMARY_ENTRIES + 1)
            .cloned()
            .collect::<Vec<_>>();
        if inputs.len() > MAX_SUMMARY_ENTRIES {
            return Err("exception hierarchy prepared input limit exceeded".into());
        }
        let index = Arc::new(
            ExceptionIndex::from_sources_with_inputs(&sources, &self.roots, &inputs, cancelled)
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
    #[test]
    fn hierarchy_project_rejects_inputs_changed_after_configuration_was_prepared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("errors.py");
        let original = "class Root(Exception): pass\nclass Child(Root): pass\n";
        std::fs::write(&path, original).unwrap();
        let config = crate::cli::parse_config_from([
            "hoimin",
            "run",
            "--root",
            dir.path().to_str().unwrap(),
            "--file",
            "errors.py",
            "--operators",
            "exception_hierarchy",
            "--",
            "unused-test-command",
        ])
        .unwrap();
        let config = crate::shell::prepare_run_config(config).unwrap();
        let project = ExceptionProject::from_config(&config).unwrap();
        std::fs::write(
            &path,
            "class Root(Exception): pass\nclass Child(ValueError): pass\n",
        )
        .unwrap();
        let result = project.load(&|| false);
        // Restoring A makes a later workspace recheck pass, but cannot make an
        // index built from B correspond to the prepared input fingerprint.
        std::fs::write(&path, original).unwrap();
        crate::fingerprint_inputs::recheck_config(&config, &config.root).unwrap();
        assert!(
            result.is_err(),
            "index accepted bytes outside its prepared fingerprint"
        );
        assert!(
            project.load(&|| false).is_ok(),
            "failed snapshot must not be cached"
        );
    }
    #[test]
    fn hierarchy_project_missing_snapshot_module_cannot_expose_lower_priority_module() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("lib")).unwrap();
        let path = dir.path().join("errors.py");
        let original = "class Root(Exception): pass\nclass Child(ValueError): pass\n";
        let service = "from errors import Root, Child\ndef f():\n    raise Child()\n";
        std::fs::write(&path, original).unwrap();
        std::fs::write(
            dir.path().join("lib/errors.py"),
            "class Root(Exception): pass\nclass Child(Root): pass\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("service.py"), service).unwrap();
        let config = crate::cli::parse_config_from([
            "hoimin",
            "run",
            "--root",
            dir.path().to_str().unwrap(),
            "--file",
            "service.py",
            "--import-root",
            "lib",
            "--operators",
            "exception_hierarchy",
            "--",
            "unused-test-command",
        ])
        .unwrap();
        let config = crate::shell::prepare_run_config(config).unwrap();
        let project = ExceptionProject::from_config(&config).unwrap();
        std::fs::remove_file(&path).unwrap();
        let result = project.load(&|| false);
        std::fs::write(&path, original).unwrap();
        crate::fingerprint_inputs::recheck_config(&config, &config.root).unwrap();
        if let Ok(index) = result {
            let parsed = ruff_python_parser::parse_module(service).unwrap();
            let mut replacements = Vec::new();
            index
                .collect(
                    Utf8Path::new("service.py"),
                    parsed.syntax(),
                    100,
                    &|| false,
                    |_, replacement| replacements.push(replacement),
                )
                .unwrap();
            assert!(
                replacements.is_empty(),
                "missing snapshot module exposed {replacements:?}"
            );
        }
    }
}

#[cfg(test)]
#[path = "exception_project_oracle_tests.rs"]
mod oracle_tests;
