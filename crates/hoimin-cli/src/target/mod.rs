pub mod fs;
pub mod git;

use hoimin_core::{
    EffectFailed, ResolveTargets, Selection, TargetError, TargetSlice, TargetsResolved,
    intersect_changed, normalize_logical_path, resolve_explicit,
};

use self::git::{GitError, GitRunner, resolve_changed_scoped};
use crate::process::ProcessCancellation;

pub struct TargetHandler;

impl TargetHandler {
    /// Resolves configured selection to concrete mutation targets.
    ///
    /// # Errors
    ///
    /// Returns an error when filesystem or Git target discovery fails.
    #[tracing::instrument(name = "resolve_targets", level = "debug", skip_all)]
    pub async fn resolve(selection: &Selection) -> Result<Vec<TargetSlice>, TargetError> {
        Self::resolve_with_git(
            selection,
            &GitRunner::default(),
            &ProcessCancellation::new(),
        )
        .await
        .map_err(GitError::into_target)
    }

    async fn resolve_with_git(
        selection: &Selection,
        runner: &GitRunner,
        cancellation: &ProcessCancellation,
    ) -> Result<Vec<TargetSlice>, GitError> {
        crate::live_progress::stage("resolving targets");
        validate_sources(selection)?;
        for path in selection
            .files
            .iter()
            .chain(selection.lines.iter().map(|line| &line.path))
        {
            let relative = normalize_logical_path(&selection.root, path)?;
            if crate::copy_policy::excluded_path(relative.as_std_path()) {
                return Err(TargetError::DiscoveryFailed(format!(
                    "target {relative} is excluded by the built-in workspace policy; choose source outside the excluded directory (--include cannot override this policy)"
                )).into());
            }
        }
        let discovered = fs::discover_explicit(selection)
            .map_err(|error| TargetError::DiscoveryFailed(error.to_string()))?;
        let explicit = resolve_explicit(selection, &discovered)?;
        tracing::debug!(targets = explicit.len(), "resolved explicit targets");
        validate_symbols(selection, &explicit)?;
        let Some(changed_selection) = &selection.changed else {
            return Ok(explicit);
        };

        let changed = resolve_changed_scoped(
            &selection.root,
            changed_selection.diff_base(),
            Some(&explicit),
            changed_selection.context(),
            runner,
            cancellation,
        )
        .await?;
        // Successful explicit file/line/symbol selectors retain at least one
        // target; only a source selector can resolve successfully to an empty set.
        if explicit.is_empty() && !selection.sources.is_empty() {
            return Ok(Vec::new());
        }
        Ok(intersect_changed(&explicit, &changed))
    }

    /// Handles a target-resolution effect.
    ///
    /// # Errors
    ///
    /// Returns an effect failure when target resolution fails.
    pub async fn handle(request: ResolveTargets) -> Result<TargetsResolved, EffectFailed> {
        Self::handle_with_cancellation(request, ProcessCancellation::new(), &GitRunner::default())
            .await
    }

    /// Resolves targets with the shell-owned Git lifecycle and stop token.
    /// Cancellation is reported separately only after child reap and pipe closure.
    ///
    /// # Errors
    ///
    /// Returns target discovery failures, Git failures, or proven Git cancellation.
    pub async fn handle_with_cancellation(
        request: ResolveTargets,
        cancellation: ProcessCancellation,
        runner: &GitRunner,
    ) -> Result<TargetsResolved, EffectFailed> {
        let id = request.id;
        Self::resolve_with_git(&request.selection, runner, &cancellation)
            .await
            .map(|targets| TargetsResolved { id, targets })
            .map_err(|error| match error {
                GitError::Cancelled => EffectFailed::other(
                    id,
                    "target.cancelled",
                    "target Git command cancelled after reap and pipe closure",
                ),
                GitError::Failed(error) => {
                    EffectFailed::other(id, "target.resolve", error.to_string())
                }
            })
    }
}

fn validate_sources(selection: &Selection) -> Result<(), TargetError> {
    for source in &selection.sources {
        let relative = normalize_logical_path(&selection.root, source)?;
        if let Err(error) = std::fs::metadata(selection.root.join(relative)) {
            let message = if error.kind() == std::io::ErrorKind::NotFound {
                format!("source {source:?} does not exist")
            } else {
                format!("cannot inspect source {source:?}: {error}")
            };
            return Err(TargetError::DiscoveryFailed(message));
        }
    }
    Ok(())
}

fn validate_symbols(selection: &Selection, targets: &[TargetSlice]) -> Result<(), TargetError> {
    if selection.symbols.is_empty() {
        return Ok(());
    }
    let root = crate::source_file::PortableFileReader::open(&selection.root)
        .map_err(|error| TargetError::DiscoveryFailed(error.to_string()))?;
    let mut selectors = std::collections::BTreeMap::<&str, Vec<String>>::new();
    for symbol in &selection.symbols {
        selectors
            .entry(&symbol.qualname)
            .or_default()
            .push(format!("{}:{}", symbol.module, symbol.qualname));
    }
    for target in targets.iter().filter(|target| !target.symbols.is_empty()) {
        let failure =
            |error: String| TargetError::DiscoveryFailed(format!("{}: {error}", target.path));
        let bytes = root
            .read(&target.path)
            .map_err(|error| failure(error.to_string()))?;
        let source = hoimin_core::decode_python_source(&bytes)
            .map_err(|error| failure(error.to_string()))?;
        let definitions = hoimin_python::definition_names(source.text()).map_err(failure)?;
        for qualname in &target.symbols {
            if !definitions.contains(qualname) {
                let requested = selectors[qualname.as_str()].join(", ");
                return Err(failure(format!(
                    "symbol definition not found: {qualname} (requested --symbol {requested})"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod rejection_coverage {
    use super::*;
    use camino::Utf8PathBuf;
    use hoimin_core::SymbolSelection;

    fn symbol_selection(root: Utf8PathBuf) -> Selection {
        Selection {
            root,
            symbols: vec![SymbolSelection {
                module: "app".to_owned(),
                qualname: "selected".to_owned(),
            }],
            ..Selection::default()
        }
    }

    fn selected_target() -> TargetSlice {
        TargetSlice {
            path: "app.py".into(),
            lines: Vec::new(),
            symbols: vec!["selected".to_owned()],
        }
    }

    #[tokio::test]
    async fn explicit_path_outside_root_preserves_the_normalization_error() {
        let temp = tempfile::tempdir().unwrap();
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
            files: vec!["../outside.py".into()],
            ..Selection::default()
        };
        let error = TargetHandler::resolve(&selection).await.unwrap_err();
        assert!(matches!(error, TargetError::PathOutsideRoot(_)));
    }

    #[test]
    fn source_beneath_a_regular_file_preserves_the_metadata_error() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("parent"), b"regular file").unwrap();
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
            sources: vec!["parent/child".into()],
            ..Selection::default()
        };
        let error = validate_sources(&selection).unwrap_err();
        assert!(
            matches!(error, TargetError::DiscoveryFailed(message) if message.starts_with("cannot inspect source \"parent/child\": "))
        );
    }

    #[test]
    fn missing_symbol_root_preserves_the_reader_error() {
        let temp = tempfile::tempdir().unwrap();
        let selection =
            symbol_selection(Utf8PathBuf::from_path_buf(temp.path().join("missing")).unwrap());
        let error = validate_symbols(&selection, &[selected_target()]).unwrap_err();
        assert!(
            matches!(error, TargetError::DiscoveryFailed(message) if message.contains("missing"))
        );
    }

    #[test]
    fn removed_symbol_source_prefixes_the_read_error_with_its_path() {
        let temp = tempfile::tempdir().unwrap();
        let selection =
            symbol_selection(Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap());
        let error = validate_symbols(&selection, &[selected_target()]).unwrap_err();
        assert!(
            matches!(error, TargetError::DiscoveryFailed(message) if message.starts_with("app.py: "))
        );
    }

    #[test]
    fn invalid_symbol_source_encoding_prefixes_the_decode_error_with_its_path() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("app.py"), b"# coding: utf-8\n\xff").unwrap();
        let selection =
            symbol_selection(Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap());
        let error = validate_symbols(&selection, &[selected_target()]).unwrap_err();
        assert!(
            matches!(error, TargetError::DiscoveryFailed(message) if message.starts_with("app.py: ") && message.contains("UTF-8"))
        );
    }
}
