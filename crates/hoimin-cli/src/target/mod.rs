pub mod fs;
pub mod git;

use hoimin_core::{
    EffectFailed, ResolveTargets, Selection, TargetError, TargetSlice, TargetsResolved,
    intersect_changed, normalize_logical_path, resolve_explicit,
};

use self::git::resolve_changed_scoped;

pub struct TargetHandler;

impl TargetHandler {
    /// Resolves configured selection to concrete mutation targets.
    ///
    /// # Errors
    ///
    /// Returns an error when filesystem or Git target discovery fails.
    pub async fn resolve(selection: &Selection) -> Result<Vec<TargetSlice>, TargetError> {
        for path in selection
            .files
            .iter()
            .chain(selection.lines.iter().map(|line| &line.path))
        {
            let relative = normalize_logical_path(&selection.root, path)?;
            if crate::copy_policy::excluded_path(relative.as_std_path()) {
                return Err(TargetError::DiscoveryFailed(format!(
                    "target {relative} is excluded by the built-in workspace policy; choose source outside the excluded directory (--include cannot override this policy)"
                )));
            }
        }
        let discovered = fs::discover_explicit(selection)
            .map_err(|error| TargetError::DiscoveryFailed(error.to_string()))?;
        let explicit = resolve_explicit(selection, &discovered)?;
        if !selection.changed {
            return Ok(explicit);
        }

        let changed = resolve_changed_scoped(
            &selection.root,
            selection.diff_base.as_deref(),
            Some(&explicit),
        )
        .await?;
        if explicit.is_empty()
            && (!selection.sources.is_empty()
                || !selection.files.is_empty()
                || !selection.lines.is_empty()
                || !selection.symbols.is_empty())
        {
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
        let id = request.id;
        Self::resolve(&request.selection)
            .await
            .map(|targets| TargetsResolved { id, targets })
            .map_err(|error| EffectFailed::other(id, "target.resolve", error.to_string()))
    }
}
