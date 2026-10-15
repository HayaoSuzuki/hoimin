use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use camino::Utf8Path;
use hoimin_core::{
    CandidateDescriptor, CandidateValidationContext, MutationCandidate,
    validate_candidate_with_context,
};

use super::ReviewError;
use super::model::{ArtifactReference, ReportReference, ReviewKey, SourceState};
use crate::artifacts::run_report::ReviewReport;
use crate::artifacts::{READ_ONLY_ARTIFACT_LIMIT, bounded_read};
use crate::plan::ReadOnlyPlan;

pub(crate) fn load_plan(path: &Path) -> Result<ReadOnlyPlan, ReviewError> {
    crate::plan::read_for_review(path).map_err(|error| ReviewError::Invalid(error.to_string()))
}

pub(crate) type PlanCandidates<'a> = HashMap<&'a str, &'a MutationCandidate>;

/// Borrows the checked plan for report matching and stale membership.
/// `load_plan` rejects duplicate IDs before this index is built.
pub(super) fn index_plan(plan: &ReadOnlyPlan) -> PlanCandidates<'_> {
    #[cfg(test)]
    super::JOIN_WORK.with_borrow_mut(|work| work.plan_builds += 1);
    plan.manifest
        .candidates
        .iter()
        .map(|ranked| {
            #[cfg(test)]
            super::JOIN_WORK.with_borrow_mut(|work| work.plan_entries += 1);
            (ranked.id.as_str(), &ranked.candidate)
        })
        .collect()
}

pub(crate) fn load_report(
    path: &Path,
    plan: &ReadOnlyPlan,
    planned: &PlanCandidates<'_>,
) -> Result<(ReviewReport, ReportReference), ReviewError> {
    let bytes = bounded_read(path, READ_ONLY_ARTIFACT_LIMIT)?;
    let report = crate::artifacts::run_report::read_review_json(path, &bytes)
        .map_err(|error| ReviewError::Invalid(error.to_string()))?;
    if report.project != plan.project {
        return Err(ReviewError::Invalid(
            "report belongs to another project".into(),
        ));
    }
    for mutant in &report.mutants {
        let descriptor = CandidateDescriptor::from(&mutant.candidate);
        let id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(&descriptor));
        if id.as_str() != mutant.candidate.id {
            return Err(ReviewError::Invalid(
                "report candidate identity conflict".into(),
            ));
        }
        #[cfg(test)]
        super::JOIN_WORK.with_borrow_mut(|work| work.plan_probes += 1);
        if let Some(planned) = planned.get(mutant.candidate.id.as_str())
            && CandidateDescriptor::from(*planned) != descriptor
        {
            return Err(ReviewError::Invalid(
                "report candidate descriptor differs from plan".into(),
            ));
        }
    }
    let reference = ReportReference {
        artifact: ArtifactReference {
            path: std::fs::canonicalize(path)?,
            content_hash: blake3::hash(&bytes).to_hex().to_string(),
        },
        run_id: report.run_id.clone(),
    };
    Ok((report, reference))
}

pub(crate) fn review_key(root: &Utf8Path, candidate: &MutationCandidate) -> ReviewKey {
    ReviewKey {
        project: root.to_owned(),
        candidate_id: candidate.id.clone(),
        descriptor: CandidateDescriptor::from(candidate),
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static SOURCE_WORK: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

/// Validates each path's source once, retaining results in candidate order.
/// Errors remain individual results so callers preserve their other validation precedence.
/// Only one file's bytes and borrowed context are alive at a time.
pub(crate) fn source_states(
    root: &Utf8Path,
    candidates: &[&MutationCandidate],
) -> Vec<Result<SourceState, ReviewError>> {
    let mut groups = std::collections::BTreeMap::<&Utf8Path, Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        groups
            .entry(candidate.path.as_path())
            .or_default()
            .push(index);
    }
    let mut results: Vec<_> = candidates.iter().map(|_| None).collect();
    // Visit files in first-occurrence order, including interleaved candidate paths.
    for candidate in candidates {
        let Some(indices) = groups.remove(candidate.path.as_path()) else {
            continue;
        };
        let group: Vec<_> = indices.iter().map(|&index| candidates[index]).collect();
        for (index, result) in indices
            .into_iter()
            .zip(source_states_for_file(root, &group))
        {
            results[index] = Some(result);
        }
    }
    results
        .into_iter()
        .map(|result| result.expect("every candidate belongs to a file group"))
        .collect()
}

/// Reads a new observation, independently of a display batch or previous command.
pub(crate) fn source_state(
    root: &Utf8Path,
    candidate: &MutationCandidate,
) -> Result<SourceState, ReviewError> {
    source_states_for_file(root, &[candidate])
        .pop()
        .expect("one candidate has one source result")
}

fn source_states_for_file(
    root: &Utf8Path,
    candidates: &[&MutationCandidate],
) -> Vec<Result<SourceState, ReviewError>> {
    let source_path = &candidates[0].path;
    let invalid = |message: String| {
        candidates
            .iter()
            .map(|_| Err(ReviewError::Invalid(message.clone())))
            .collect()
    };
    if !hoimin_core::normalized_relative_path(source_path.as_str()) {
        return invalid("candidate path must be root-relative".into());
    }
    let Ok(path) = std::fs::canonicalize(root.join(source_path)) else {
        return candidates
            .iter()
            .map(|_| Ok(SourceState::Unavailable))
            .collect();
    };
    if !path.starts_with(root.as_std_path()) {
        return invalid("candidate source escapes project root".into());
    }
    // Bound allocation before constructing the shared source validator.
    #[cfg(test)]
    SOURCE_WORK.with(|count| {
        let (reads, contexts) = count.get();
        count.set((reads + 1, contexts));
    });
    let Ok(bytes) = bounded_read(&path, READ_ONLY_ARTIFACT_LIMIT) else {
        return candidates
            .iter()
            .map(|_| Ok(SourceState::Unavailable))
            .collect();
    };
    #[cfg(test)]
    SOURCE_WORK.with(|count| {
        let (reads, contexts) = count.get();
        count.set((reads, contexts + 1));
    });
    let context = match CandidateValidationContext::new(&bytes) {
        Ok(context) => context,
        Err(error) => return invalid(error.to_string()),
    };
    candidates
        .iter()
        .map(|candidate| validate_source_state(&context, candidate))
        .collect()
}

fn validate_source_state(
    context: &CandidateValidationContext<'_>,
    candidate: &MutationCandidate,
) -> Result<SourceState, ReviewError> {
    if context.file_hash() != candidate.file_hash {
        return Ok(SourceState::Stale);
    }
    let descriptor = CandidateDescriptor::from(candidate);
    validate_candidate_with_context(context, &descriptor)
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
    let companions = crate::artifacts::SqliteCompanions::new(store);
    for path in [
        store,
        companions.journal.as_path(),
        companions.wal.as_path(),
        companions.shm.as_path(),
    ] {
        validate_destination_entry(path, plan, report)?;
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
