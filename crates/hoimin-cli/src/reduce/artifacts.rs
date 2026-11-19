//! Artifact names stay relative to an owned directory even if its public path is
//! renamed or replaced. The runner owns disk admission before these writes.
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions};
use std::{io::Write, path::Path, sync::Arc};

#[derive(Clone)]
pub(super) struct Artifacts {
    directory: Arc<Dir>,
}
impl Artifacts {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let parent = Dir::open_ambient_dir(
            path.parent()
                .ok_or_else(|| std::io::Error::other("missing output parent"))?,
            cap_std::ambient_authority(),
        )?;
        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("missing output name"))?;
        parent.create_dir(name)?;
        Ok(Self {
            directory: Arc::new(parent.open_dir_nofollow(name)?),
        })
    }
    pub fn capability(
        &self,
        path: camino::Utf8PathBuf,
    ) -> std::io::Result<crate::workspace::RootCapability> {
        crate::workspace::RootCapability::from_dir(&self.directory, path)
    }
    fn with_new_file(
        &self,
        name: &str,
        write: impl FnOnce(&mut cap_std::fs::File) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let mut file = self
            .directory
            .open_with(name, OpenOptions::new().write(true).create_new(true))?;
        let result = write(&mut file).and_then(|()| file.sync_all());
        drop(file);
        if result.is_err() {
            let _ = self.directory.remove_file(name);
        }
        result
    }
    pub fn write(&self, name: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.with_new_file(name, |file| file.write_all(bytes))
    }
    pub fn copy(&self, name: &str, source: &Path, expected: u64) -> std::io::Result<()> {
        let source = crate::artifacts::open_regular_file(source)?
            .ok_or_else(|| std::io::Error::other("execution output is not a regular file"))?;
        if source.metadata()?.len() != expected {
            return Err(std::io::Error::other("execution output length changed"));
        }
        let mut source = crate::finite_file::SnapshotReader::new(source)?;
        self.with_new_file(name, |target| {
            std::io::copy(&mut source, target).map(|_| ())
        })
    }
    pub fn remove(&self, name: &str) -> std::io::Result<()> {
        self.directory.remove_file(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn output_directory_replacement_cannot_redirect_publication() {
        // Arrange
        let parent = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = parent.path().join("output");
        let artifacts = Artifacts::create(&path).unwrap();
        let retained = parent.path().join("retained");
        std::fs::rename(&path, &retained).unwrap();
        std::os::unix::fs::symlink(outside.path(), &path).unwrap();
        // Act
        artifacts.write("report.json", b"evidence").unwrap();
        // Assert
        assert_eq!(
            (
                std::fs::read(retained.join("report.json")).unwrap(),
                outside.path().join("report.json").exists()
            ),
            (b"evidence".to_vec(), false)
        );
    }
    #[test]
    fn failed_write_removes_partial_artifact() {
        // Arrange
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("output");
        let artifacts = Artifacts::create(&path).unwrap();
        // Act
        let result = artifacts.with_new_file("proposal.diff", |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write failure"))
        });
        // Assert
        assert!(result.is_err());
        assert!(!path.join("proposal.diff").exists());
    }

    #[test]
    fn artifact_creation_does_not_overwrite_existing_evidence() {
        // Arrange
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("output");
        let artifacts = Artifacts::create(&path).unwrap();
        artifacts.write("report.json", b"first").unwrap();
        // Act
        let result = artifacts.write("report.json", b"second");
        // Assert
        assert!(result.is_err());
    }
}
