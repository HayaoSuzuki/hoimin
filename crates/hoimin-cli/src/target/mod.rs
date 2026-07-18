pub mod fs;
pub mod git;

use hoimin_core::{
    EffectFailed, ResolveTargets, Selection, TargetError, TargetSlice, TargetsResolved,
    intersect_changed, resolve_explicit,
};

use self::git::resolve_changed;

pub struct TargetHandler;

impl TargetHandler {
    pub async fn resolve(selection: &Selection) -> Result<Vec<TargetSlice>, TargetError> {
        let discovered = fs::discover_explicit(selection)
            .map_err(|error| TargetError::DiscoveryFailed(error.to_string()))?;
        let explicit = resolve_explicit(selection, &discovered)?;
        if !selection.changed {
            return Ok(explicit);
        }

        let changed = resolve_changed(&selection.root, selection.diff_base.as_deref()).await?;
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

    pub async fn handle(request: ResolveTargets) -> Result<TargetsResolved, EffectFailed> {
        let id = request.id;
        Self::resolve(&request.selection)
            .await
            .map(|targets| TargetsResolved { id, targets })
            .map_err(|error| EffectFailed::other(id, "target.resolve", error.to_string()))
    }
}
