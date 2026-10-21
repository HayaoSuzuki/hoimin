use std::io::Read;

use super::{PLAN_SCHEMA_VERSION, PlanError, PlanManifest};

/// Decode the current closed shape exactly once, including end-of-input.
/// Unsupported shapes need no special diagnostic precedence or routing pass.
pub(super) fn decode_manifest(bytes: &[u8]) -> Result<PlanManifest, PlanError> {
    decode_reader(bytes)
}

pub(super) fn decode_reader(reader: impl Read) -> Result<PlanManifest, PlanError> {
    let manifest: PlanManifest = serde_json::from_reader(reader)
        .map_err(|error| PlanError::ManifestInvalid(error.to_string()))?;
    if manifest.schema_version != PLAN_SCHEMA_VERSION {
        return Err(PlanError::ManifestInvalid(format!(
            "unsupported schema version {}; regenerate the plan with this hoimin version",
            manifest.schema_version
        )));
    }
    Ok(manifest)
}
