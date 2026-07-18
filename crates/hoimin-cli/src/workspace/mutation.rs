use std::fs;

use hoimin_core::MutationCandidate;

use super::{WorkerWorkspace, WorkspaceError, make_writable, resolve_worker_path};

impl WorkerWorkspace {
    pub fn apply_mutation(&mut self, candidate: &MutationCandidate) -> Result<(), WorkspaceError> {
        self.verify_originals()?;
        let path = resolve_worker_path(&self.root, &candidate.path)?;
        let expected = self.manifest.entry(&candidate.path).ok_or_else(|| {
            WorkspaceError::MutationTargetMissing {
                path: candidate.path.clone(),
            }
        })?;
        let bytes = fs::read(&path)
            .map_err(|error| WorkspaceError::io("read mutation target", &candidate.path, error))?;
        let actual_hash = blake3::hash(&bytes);
        if candidate.file_hash != expected.blake3.to_hex().as_str()
            || actual_hash != expected.blake3
        {
            return Err(WorkspaceError::MutationHashMismatch {
                path: candidate.path.clone(),
            });
        }

        let start = usize::try_from(candidate.span.start).map_err(|_| {
            WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            }
        })?;
        let length = usize::try_from(candidate.span.length).map_err(|_| {
            WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            }
        })?;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            })?;
        if bytes.get(start..end) != Some(candidate.original.as_bytes()) {
            return Err(WorkspaceError::MutationOriginalMismatch {
                path: candidate.path.clone(),
            });
        }

        let mut mutated = Vec::with_capacity(bytes.len() - length + candidate.replacement.len());
        mutated.extend_from_slice(&bytes[..start]);
        mutated.extend_from_slice(candidate.replacement.as_bytes());
        mutated.extend_from_slice(&bytes[end..]);
        make_writable(&path)?;
        fs::write(&path, &mutated)
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;

        hoimin_core::contract_ensure!(
            "workspace.mutation.post",
            mutated[..start] == bytes[..start]
                && mutated[start + candidate.replacement.len()..] == bytes[end..],
            &candidate.path,
        );
        Ok(())
    }
}
