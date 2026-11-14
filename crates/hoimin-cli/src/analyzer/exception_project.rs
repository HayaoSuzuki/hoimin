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

// A single cancellation adapter instantiation lets parameterized boundary fixtures
// exercise the same generic loading implementation across all callback behaviors.
#[cfg(test)]
fn test_load(
    project: &ExceptionProject,
    cancelled: &dyn Fn() -> bool,
) -> Result<Arc<ExceptionIndex>, String> {
    project.load(&|| cancelled())
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

        let acquisition_error = test_load(&project, &|| false).err().unwrap();
        std::fs::write(root.join("z.py"), "").unwrap();
        let analysis_error = test_load(&project, &|| false).err().unwrap();
        std::fs::write(root.join("a.py"), "class Root(Exception): pass\n").unwrap();
        let first = test_load(&project, &|| false).unwrap();
        let second = test_load(&project, &|| false).unwrap();

        assert!(
            acquisition_error.starts_with("z.py:"),
            "{acquisition_error}"
        );
        assert!(
            analysis_error.contains("analysis depth"),
            "{analysis_error}"
        );
        assert!(Arc::ptr_eq(&first, &second));
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

        let acquisition_error = test_load(&project, &|| false).err().unwrap();
        std::fs::write(root.join("source.py"), "").unwrap();
        Arc::make_mut(project.prepared_inputs.as_mut().unwrap())
            .insert("source.py".into(), blake3::hash(b"").to_hex().to_string());
        let limit_error = test_load(&project, &|| false).err().unwrap();

        assert!(
            acquisition_error.starts_with("source.py:"),
            "{acquisition_error}"
        );
        assert!(
            limit_error.contains("prepared input limit"),
            "{limit_error}"
        );
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

        let oversized = test_load(&project, &|| false);
        std::fs::write(path, "class Root(Exception): pass\n").unwrap();
        let cancelled = test_load(&project, &|| true);
        let first = test_load(&project, &|| false).unwrap();
        let second = test_load(&project, &|| false).unwrap();

        assert!(oversized.err().unwrap().contains("byte limit"));
        assert!(cancelled.is_err());
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
        let result = test_load(&project, &|| false);
        // Restoring A makes a later workspace recheck pass, but cannot make an
        // index built from B correspond to the prepared input fingerprint.
        std::fs::write(&path, original).unwrap();
        crate::fingerprint_inputs::recheck_config(&config, &config.selection.root).unwrap();

        let actual_1 = test_load(&project, &|| false);

        assert!(
            result.is_err(),
            "index accepted bytes outside its prepared fingerprint"
        );
        assert!(actual_1.is_ok(), "failed snapshot must not be cached");
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
        let result = test_load(&project, &|| false);
        std::fs::write(&path, original).unwrap();
        crate::fingerprint_inputs::recheck_config(&config, &config.selection.root).unwrap();
        let candidates = result
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
            })
            .unwrap_or_default();

        let actual_0 = candidates.is_empty();

        assert!(
            actual_0,
            "missing snapshot module exposed {} candidates",
            candidates.len()
        );
    }
}

#[cfg(test)]
#[path = "exception_project_oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
mod coverage_regressions {
    use super::*;
    use std::cell::Cell;

    fn poison_cache(project: &ExceptionProject) {
        let result = std::panic::catch_unwind(|| {
            let _guard = project.index.lock().unwrap();
            panic!("poison test-owned exception index");
        });
        assert!(result.is_err());
    }

    fn poison_after_initial_check(project: &ExceptionProject, calls: &Cell<usize>) -> bool {
        let call = calls.replace(calls.get() + 1);
        if call == 1 {
            poison_cache(project);
        }
        false
    }

    #[test]
    fn poisoned_cache_is_reported_before_project_acquisition() {
        let directory = tempfile::tempdir().unwrap();
        let project = ExceptionProject::new(Utf8Path::from_path(directory.path()).unwrap());
        poison_cache(&project);

        let actual_0 = test_load(&project, &|| false);

        assert!(actual_0.err().unwrap().contains("poison"));
    }

    #[test]
    fn cache_poisoned_after_discovery_is_reported_when_storing_the_index() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("source.py"),
            "class Root(Exception): pass\n",
        )
        .unwrap();
        let project = ExceptionProject::new(Utf8Path::from_path(directory.path()).unwrap());
        let calls = Cell::new(0);
        let error = test_load(&project, &|| poison_after_initial_check(&project, &calls))
            .err()
            .unwrap();

        let actual_1 = calls.get() > 1;

        assert!(error.contains("poison"));
        assert!(actual_1);
    }

    #[test]
    fn missing_project_root_reports_acquisition_failure() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(directory.path().join("absent")).unwrap();
        let project = ExceptionProject::new(&root);

        let actual_0 = test_load(&project, &|| false);

        assert!(actual_0.is_err());
    }

    #[test]
    fn decoded_text_must_fit_the_per_file_bound() {
        let directory = tempfile::tempdir().unwrap();
        let mut source = b"# coding: latin-1\n# ".to_vec();
        source.resize(MAX_FILE_BYTES / 2 + 128, 255);
        std::fs::write(directory.path().join("source.py"), source).unwrap();
        let project = ExceptionProject::new(Utf8Path::from_path(directory.path()).unwrap());

        let actual_0 = test_load(&project, &|| false);

        assert!(
            actual_0
                .err()
                .unwrap()
                .contains("decoded source byte limit")
        );
    }

    fn count_poll(calls: &Cell<usize>) -> bool {
        calls.set(calls.get() + 1);
        false
    }

    fn cancel_after(calls: &Cell<usize>, allowed: usize) -> bool {
        calls.replace(calls.get() + 1) >= allowed
    }

    #[test]
    fn cancellation_during_file_discovery_propagates_without_caching() {
        let directory = tempfile::tempdir().unwrap();
        let project = ExceptionProject::new(Utf8Path::from_path(directory.path()).unwrap());
        let calls = Cell::new(0);
        let error = test_load(&project, &|| cancel_after(&calls, 1))
            .err()
            .unwrap();

        let actual_1 = project.index.lock();

        assert!(error.contains("cancel"));
        assert!(actual_1.unwrap().is_none());
    }

    #[test]
    fn cancellation_after_file_discovery_prevents_source_acquisition() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("source.py"),
            "class Root(Exception): pass\n",
        )
        .unwrap();
        let project = ExceptionProject::new(Utf8Path::from_path(directory.path()).unwrap());
        let discovery_polls = Cell::new(0);
        project.files(&|| count_poll(&discovery_polls)).unwrap();
        let calls = Cell::new(0);
        let error = test_load(&project, &|| {
            cancel_after(&calls, discovery_polls.get() + 1)
        })
        .err()
        .unwrap();

        let actual_1 = project.index.lock();

        assert_eq!(error, "analyzer.cancelled");
        assert!(actual_1.unwrap().is_none());
    }

    #[test]
    fn aggregate_decoded_bytes_must_fit_the_project_bound() {
        let directory = tempfile::tempdir().unwrap();
        let mut source = b"# coding: latin-1\n# ".to_vec();
        source.resize(MAX_FILE_BYTES / 2 - 1024, 255);
        std::fs::write(directory.path().join("a.py"), &source).unwrap();
        std::fs::write(directory.path().join("b.py"), &source).unwrap();
        std::fs::write(directory.path().join("c.py"), &source).unwrap();
        std::fs::write(directory.path().join("d.py"), &source).unwrap();
        source.truncate(6144);
        std::fs::write(directory.path().join("z.py"), source).unwrap();
        let project = ExceptionProject::new(Utf8Path::from_path(directory.path()).unwrap());

        let actual_0 = test_load(&project, &|| false);
        let actual_1 = project.index.lock();

        assert!(
            actual_0
                .err()
                .unwrap()
                .contains("total decoded source byte limit")
        );
        assert!(actual_1.unwrap().is_none());
    }

    #[test]
    fn configuration_deduplicates_the_root_across_import_and_source_paths() {
        let directory = tempfile::tempdir().unwrap();
        let config = crate::cli::parse_config_from([
            "hoimin",
            "run",
            "--root",
            directory.path().to_str().unwrap(),
            "--source",
            ".",
            "--import-root",
            ".",
            "--operators",
            "exception_hierarchy",
            "--",
            "unused-test-command",
        ])
        .unwrap();
        let config = crate::shell::prepare_run_config(config).unwrap();

        let actual_0 = ExceptionProject::from_config(&config);

        assert_eq!(actual_0.unwrap().roots, vec![Utf8PathBuf::new()]);
    }
}
