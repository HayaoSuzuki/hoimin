use std::fs::File;
use std::path::Path;

use camino::Utf8Path;
use hoimin_core::{CandidateValidationContext, MutationCandidate, validate_candidate_with_context};

use super::ReviewError;
use super::model::{ArtifactReference, ReviewKey, SourceState};
use crate::artifacts::run_report::ReviewReport;
use crate::artifacts::{READ_ONLY_ARTIFACT_LIMIT, bounded_read};
use crate::plan::ReadOnlyPlan;

pub(crate) fn load_plan(path: &Path) -> Result<ReadOnlyPlan, ReviewError> {
    crate::plan::read_for_review(path).map_err(|error| ReviewError::Invalid(error.to_string()))
}

pub(crate) fn load_report(
    path: &Path,
    plan: &ReadOnlyPlan,
) -> Result<(ReviewReport, ArtifactReference), ReviewError> {
    let bytes = bounded_read(path, READ_ONLY_ARTIFACT_LIMIT)?;
    let report = crate::artifacts::run_report::read_review_json(path, &bytes)
        .map_err(|error| ReviewError::Invalid(error.to_string()))?;
    if report.project != plan.project {
        return Err(ReviewError::Invalid(
            "report belongs to another project".into(),
        ));
    }
    for mutant in &report.mutants {
        let descriptor = crate::plan::candidate_descriptor(&mutant.candidate);
        let id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&descriptor));
        if id.as_str() != mutant.candidate.id {
            return Err(ReviewError::Invalid(
                "report candidate identity conflict".into(),
            ));
        }
        if let Some(planned) = plan
            .manifest
            .candidates
            .iter()
            .find(|item| item.id == mutant.candidate.id)
            && crate::plan::candidate_descriptor(&planned.candidate) != descriptor
        {
            return Err(ReviewError::Invalid(
                "report candidate descriptor differs from plan".into(),
            ));
        }
    }
    let reference = ArtifactReference {
        path: std::fs::canonicalize(path)?,
        content_hash: blake3::hash(&bytes).to_hex().to_string(),
        run_id: Some(report.run_id.clone()),
    };
    Ok((report, reference))
}

pub(crate) fn review_key(root: &Utf8Path, candidate: &MutationCandidate) -> ReviewKey {
    ReviewKey {
        project: root.to_owned(),
        candidate_id: candidate.id.clone(),
        descriptor: crate::plan::candidate_descriptor(candidate),
    }
}

pub(crate) fn source_state(
    root: &Utf8Path,
    candidate: &MutationCandidate,
) -> Result<SourceState, ReviewError> {
    if !hoimin_core::normalized_relative_path(candidate.path.as_str()) {
        return Err(ReviewError::Invalid(
            "candidate path must be root-relative".into(),
        ));
    }
    let Ok(path) = std::fs::canonicalize(root.join(&candidate.path)) else {
        return Ok(SourceState::Unavailable);
    };
    if !path.starts_with(root.as_std_path()) {
        return Err(ReviewError::Invalid(
            "candidate source escapes project root".into(),
        ));
    }
    // Bound allocation before constructing the shared source validator.
    let Ok(bytes) = bounded_read(&path, READ_ONLY_ARTIFACT_LIMIT) else {
        return Ok(SourceState::Unavailable);
    };
    let context = CandidateValidationContext::new(&bytes)
        .map_err(|error| ReviewError::Invalid(error.to_string()))?;
    if context.file_hash() != candidate.file_hash {
        return Ok(SourceState::Stale);
    }
    let descriptor = crate::plan::candidate_descriptor(candidate);
    validate_candidate_with_context(&context, &descriptor)
        .map_err(|error| ReviewError::Invalid(error.to_string()))?;
    if hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&descriptor)).as_str()
        != candidate.id
    {
        return Err(ReviewError::Invalid("candidate identity conflict".into()));
    }
    Ok(SourceState::Fresh)
}

pub(crate) fn validate_store_destination(
    store: &Path,
    plan: &ReadOnlyPlan,
    report: Option<&Path>,
) -> Result<(), ReviewError> {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let mut name = store.as_os_str().to_os_string();
        name.push(suffix);
        validate_destination_entry(Path::new(&name), plan, report)?;
    }
    Ok(())
}

fn validate_destination_entry(
    store: &Path,
    plan: &ReadOnlyPlan,
    report: Option<&Path>,
) -> Result<(), ReviewError> {
    let metadata = match std::fs::symlink_metadata(store) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ReviewError::Invalid(
            "store must be a regular file, not a symlink".into(),
        ));
    }
    let store_id = file_identity(&File::open(store)?)?;
    let mut protected = vec![plan.canonical_path.clone()];
    if let Some(path) = report {
        protected.push(path.to_path_buf());
    }
    protected.extend(
        plan.manifest
            .sources
            .iter()
            .map(|source| plan.project.join(&source.path).into_std_path_buf()),
    );
    protected.extend(
        plan.manifest
            .fingerprint_inputs
            .iter()
            .map(|source| plan.project.join(&source.path).into_std_path_buf()),
    );
    for path in protected {
        match File::open(path) {
            Ok(file) if file_identity(&file)? == store_id => {
                return Err(ReviewError::Invalid(
                    "store aliases a protected input".into(),
                ));
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: file owns a live handle and info has the required writable size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the successful API call initialized all fields.
    let info = unsafe { info.assume_init() };
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}
