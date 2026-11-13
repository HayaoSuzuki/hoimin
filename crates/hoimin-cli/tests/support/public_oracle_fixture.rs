//! Filesystem boundary shared by public candidate correspondence fixtures.
use std::path::PathBuf;

pub fn create(
    directory: std::io::Result<tempfile::TempDir>,
    id: &str,
    source: &str,
) -> Result<(tempfile::TempDir, PathBuf), String> {
    let directory = directory
        .map_err(|error| format!("infrastructure-error: {id} fixture tempdir: {error}"))?;
    let root = directory.path().join("project");
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("infrastructure-error: {id} fixture root: {error}"))?;
    std::fs::write(root.join("target.py"), source)
        .map_err(|error| format!("infrastructure-error: {id} fixture source: {error}"))?;
    Ok((directory, root))
}
