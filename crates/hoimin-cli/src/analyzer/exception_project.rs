//! Project inputs shared by hierarchy analysis and fingerprint revalidation.
use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{MutationOperator, RunConfig, Selection};
use hoimin_python::{ExceptionIndex, ExceptionIndexBuilder, MAX_SUMMARY_ENTRIES};
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
                    if let Ok(path) =
                        hoimin_core::normalize_logical_path(&config.selection.root, root)
                        && !roots.contains(&path)
                    {
                        roots.push(path);
                    }
                }
                let mut result = Self::new(&config.selection.root);
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
        let reader = crate::source_file::PortableFileReader::open(&self.selection.root)
            .map_err(|e| e.to_string())?;
        let paths = self.files(cancelled)?;
        // Reserved origins need paths, never all source bodies. Keep construction
        // errors deferred until every bounded acquisition has succeeded.
        let inputs = self
            .prepared_inputs
            .iter()
            .flat_map(|inputs| inputs.keys())
            .filter(|path| path.extension() == Some("py"))
            .take(MAX_SUMMARY_ENTRIES + 1)
            .cloned()
            .collect::<Vec<_>>();
        let mut builder = if inputs.len() > MAX_SUMMARY_ENTRIES {
            Err("exception hierarchy prepared input limit exceeded".to_owned())
        } else {
            Ok(ExceptionIndexBuilder::new(
                &paths,
                &self.roots,
                &inputs,
                cancelled,
            ))
        };
        let mut remaining = MAX_TOTAL_BYTES;
        for path in paths {
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
            if let Ok(builder) = &mut builder {
                builder.add_source(&path, decoded.text(), cancelled);
            }
        }
        let index = Arc::new(
            builder?
                .finish(cancelled)
                .map_err(|error| error.to_string())?,
        );
        *self.index.lock().map_err(|e| e.to_string())? = Some(Arc::clone(&index));
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_acquisition_failure_precedes_an_earlier_analysis_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let deep = format!("x = {}1\n", "1 + ".repeat(160));
        std::fs::write(root.join("a.py"), deep).unwrap();
        std::fs::write(root.join("z.py"), b"# coding: ascii\n# \xff\n").unwrap();
        let project = ExceptionProject::new(root);
        let error = project.load(&|| false).err().unwrap();
        assert!(error.starts_with("z.py:"), "{error}");
        std::fs::write(root.join("z.py"), "").unwrap();
        let error = project.load(&|| false).err().unwrap();
        assert!(error.contains("analysis depth"), "{error}");
        std::fs::write(root.join("a.py"), "class Root(Exception): pass\n").unwrap();
        let first = project.load(&|| false).unwrap();
        assert!(Arc::ptr_eq(&first, &project.load(&|| false).unwrap()));
    }

    #[test]
    fn hierarchy_acquisition_failure_precedes_prepared_origin_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let invalid = b"# coding: ascii\n# \xff\n";
        std::fs::write(root.join("source.py"), invalid).unwrap();
        let mut project = ExceptionProject::new(root);
        let mut inputs = (0..MAX_SUMMARY_ENTRIES)
            .map(|i| (format!("missing{i}.py").into(), String::new()))
            .collect::<BTreeMap<Utf8PathBuf, String>>();
        inputs.insert(
            "source.py".into(),
            blake3::hash(invalid).to_hex().to_string(),
        );
        project.prepared_inputs = Some(Arc::new(inputs));
        let error = project.load(&|| false).err().unwrap();
        assert!(error.starts_with("source.py:"), "{error}");
        std::fs::write(root.join("source.py"), "").unwrap();
        Arc::make_mut(project.prepared_inputs.as_mut().unwrap())
            .insert("source.py".into(), blake3::hash(b"").to_hex().to_string());
        let error = project.load(&|| false).err().unwrap();
        assert!(error.contains("prepared input limit"), "{error}");
    }
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
        crate::fingerprint_inputs::recheck_config(&config, &config.selection.root).unwrap();
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
        crate::fingerprint_inputs::recheck_config(&config, &config.selection.root).unwrap();
        let replacements = result
            .map(|index| {
                let operators = serde_json::from_str("[\"exception_hierarchy\"]").unwrap();
                hoimin_python::analyze_source_with_exceptions(
                    &hoimin_python::AnalyzeRequest {
                        payload: hoimin_python::payload::PayloadBudget::default(),
                        path: Utf8Path::new("service.py"),
                        lines: &[],
                        symbols: &[],
                        operators: &operators,
                        profile: hoimin_core::MutationProfile::Full,
                        max_candidates: 100,
                    },
                    service,
                    &|| false,
                    Some(&index),
                )
                .unwrap()
                .candidates
                .into_iter()
                .map(|candidate| candidate.value.replacement)
                .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        assert!(
            replacements.is_empty(),
            "missing snapshot module exposed {replacements:?}"
        );
    }
}

#[cfg(test)]
#[path = "exception_project_oracle_tests.rs"]
mod oracle_tests;
