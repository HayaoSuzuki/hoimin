pub(crate) mod input;
pub mod model;
mod render;
mod store;

use std::collections::HashMap;

use hoimin_core::CandidateDescriptor;

use crate::artifacts::run_report::ReviewReport;
use crate::cli::ReviewArgs;
use model::{
    Applicability, ArtifactReference, ReportReference, ReviewDraft, ReviewState, SourceState,
};
use render::{ReviewDocument, ReviewExecution, ReviewRow};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct JoinWork {
    plan_builds: usize,
    plan_entries: usize,
    plan_probes: usize,
    report_builds: usize,
    report_entries: usize,
    report_probes: usize,
    snapshot_builds: usize,
    snapshot_entries: usize,
    snapshot_probes: usize,
}

#[cfg(test)]
thread_local! {
    static JOIN_WORK: std::cell::RefCell<JoinWork> = std::cell::RefCell::default();
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("invalid review input: {0}")]
    Invalid(String),
    #[error(transparent)]
    Update(#[from] model::UpdateRejection),
    #[error("review I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("review JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("review database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(
        "review saved at revision {revision}, but output failed: {message}; read it with review show before retrying"
    )]
    SavedOutput { revision: u64, message: String },
}

impl From<crate::artifacts::ArtifactReadError> for ReviewError {
    fn from(error: crate::artifacts::ArtifactReadError) -> Self {
        match error {
            crate::artifacts::ArtifactReadError::Io(error) => Self::Io(error),
            crate::artifacts::ArtifactReadError::Invalid(message) => Self::Invalid(message),
        }
    }
}

/// Records or displays bounded local human assessments.
///
/// # Errors
/// Rejects invalid identities, stale writes, revision conflicts, and I/O failures.
pub fn run(args: &ReviewArgs, stdout: &mut dyn std::io::Write) -> Result<i32, ReviewError> {
    let (common, candidate_id, include_stale, command) = match &args {
        ReviewArgs::List(args) => (&args.common, None, args.stale, "list"),
        ReviewArgs::Show(args) => (&args.common, Some(args.candidate.as_str()), false, "show"),
        ReviewArgs::Record(args) => (&args.common, Some(args.candidate.as_str()), false, "record"),
    };
    let plan = input::load_plan(&common.plan)?;
    input::validate_store_destination(&common.store, &plan, common.report.as_deref())?;
    let planned = input::index_plan(&plan);
    let report = common
        .report
        .as_deref()
        .map(|path| input::load_report(path, &plan, &planned))
        .transpose()?;
    let selected: Vec<_> = plan
        .manifest
        .candidates
        .iter()
        .filter(|candidate| candidate_id.is_none_or(|id| candidate.id == id))
        .collect();
    if candidate_id.is_some() && selected.is_empty() {
        return Err(ReviewError::Invalid(
            "candidate ID is not in the plan".into(),
        ));
    }
    let (snapshot, saved_revision) = if let ReviewArgs::Record(args) = args {
        let (key, state) = record_store(args, &plan, report.as_ref(), &selected[0].candidate)?;
        let revision = state.history.last().map_or(0, |event| event.revision);
        (vec![(key, state)], Some(revision))
    } else {
        let snapshot = match store::ReviewStore::open_read(&common.store, &plan.project)? {
            Some(mut store) => match candidate_id {
                Some(id) => store.candidate(id)?.into_iter().collect(),
                None => store.snapshot()?,
            },
            None => Vec::new(),
        };
        (snapshot, None)
    };
    saved_output(saved_revision, || {
        let reported = report.as_ref().map(|(report, _)| index_report(report));
        let reviewed = index_snapshot(&snapshot);
        let mut candidates = Vec::new();
        let sources = input::source_states(
            &plan.project,
            &selected
                .iter()
                .map(|ranked| &ranked.candidate)
                .collect::<Vec<_>>(),
        );
        for (ranked, source) in selected.into_iter().zip(sources) {
            let candidate = &ranked.candidate;
            let key = input::review_key(&plan.project, candidate);
            #[cfg(test)]
            JOIN_WORK.with_borrow_mut(|work| work.snapshot_probes += 1);
            let stored = reviewed.get(key.candidate_id.as_str()).copied();
            let stored_key = stored.map_or(&key, |(key, _)| key);
            let history = stored.map_or(&[][..], |(_, state)| state.history.as_slice());
            let applicable = model::applicability(&key, stored_key, source?);
            if applicable == Applicability::IdentityConflict {
                return Err(ReviewError::Invalid(
                    "stored candidate identity conflict".into(),
                ));
            }
            let execution = execution(candidate, reported.as_ref())?;
            candidates.push(ReviewRow {
                candidate: candidate.clone(),
                execution,
                review: (applicable == Applicability::Current)
                    .then(|| history.last())
                    .flatten(),
                applicability: applicable,
                history,
            });
        }
        let stale_candidates = if include_stale {
            stale_rows(&planned, &snapshot, reported.as_ref())?
        } else {
            Vec::new()
        };
        let document = ReviewDocument {
            schema_version: 2,
            command,
            project: plan.project.clone(),
            operators: render::counts(&candidates),
            candidates,
            stale_candidates,
        };
        render::write(&document, common.format, stdout)?;
        Ok(0)
    })
}

fn saved_output<T>(
    revision: Option<u64>,
    operation: impl FnOnce() -> Result<T, ReviewError>,
) -> Result<T, ReviewError> {
    operation().map_err(|error| match revision {
        Some(revision) => ReviewError::SavedOutput {
            revision,
            message: error.to_string(),
        },
        None => error,
    })
}

type ReportCandidates<'a> = HashMap<&'a str, &'a hoimin_core::MutantFinished>;
type SnapshotCandidates<'a> = HashMap<&'a str, &'a (model::ReviewKey, ReviewState)>;

fn index_report(report: &ReviewReport) -> ReportCandidates<'_> {
    #[cfg(test)]
    JOIN_WORK.with_borrow_mut(|work| work.report_builds += 1);
    // Report structure validation rejects duplicate stable IDs before indexing.
    report
        .mutants
        .iter()
        .map(|mutant| {
            #[cfg(test)]
            JOIN_WORK.with_borrow_mut(|work| work.report_entries += 1);
            (mutant.candidate.id.as_str(), mutant)
        })
        .collect()
}

fn index_snapshot(snapshot: &[(model::ReviewKey, ReviewState)]) -> SnapshotCandidates<'_> {
    #[cfg(test)]
    JOIN_WORK.with_borrow_mut(|work| work.snapshot_builds += 1);
    // Store snapshots validate key/ID agreement and have unique SQLite primary keys.
    snapshot
        .iter()
        .map(|entry| {
            #[cfg(test)]
            JOIN_WORK.with_borrow_mut(|work| work.snapshot_entries += 1);
            (entry.0.candidate_id.as_str(), entry)
        })
        .collect()
}

fn execution(
    candidate: &hoimin_core::MutationCandidate,
    report: Option<&ReportCandidates<'_>>,
) -> Result<ReviewExecution, ReviewError> {
    let Some(report) = report else {
        return Ok(ReviewExecution::NotLoaded);
    };
    #[cfg(test)]
    JOIN_WORK.with_borrow_mut(|work| work.report_probes += 1);
    let Some(mutant) = report.get(candidate.id.as_str()) else {
        return Ok(ReviewExecution::NotInReport);
    };
    if CandidateDescriptor::from(candidate) != CandidateDescriptor::from(&mutant.candidate) {
        return Err(ReviewError::Invalid(
            "report candidate descriptor differs from stored candidate".into(),
        ));
    }
    Ok(ReviewExecution::Result(mutant.outcome.status()))
}

#[cfg(test)]
mod store_scaling_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tests;

fn record_store(
    args: &crate::cli::ReviewRecordArgs,
    plan: &crate::plan::ReadOnlyPlan,
    report: Option<&(ReviewReport, ReportReference)>,
    candidate: &hoimin_core::MutationCandidate,
) -> Result<(model::ReviewKey, ReviewState), ReviewError> {
    if report.is_some_and(|(report, _)| {
        !report
            .mutants
            .iter()
            .any(|mutant| mutant.candidate.id == candidate.id)
    }) {
        return Err(ReviewError::Invalid(
            "record candidate is not in the report".into(),
        ));
    }
    let source = input::source_state(&plan.project, candidate)?;
    if source != SourceState::Fresh {
        return Err(model::UpdateRejection::NotCurrent.into());
    }
    let timestamp_ms = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| ReviewError::Invalid(error.to_string()))?
            .as_millis(),
    )
    .map_err(|_| ReviewError::Invalid("timestamp exceeds u64 milliseconds".into()))?;
    let draft = ReviewDraft {
        assessment: args.assessment,
        reason: args.reason.trim().to_owned(),
        evidence: args.evidence.clone(),
        timestamp_ms,
        plan: ArtifactReference {
            path: plan.canonical_path.clone(),
            content_hash: plan.content_hash.clone(),
        },
        report: report.map(|(_, reference)| reference.clone()),
    };
    model::validate_draft(&draft)?;
    let mut store = store::ReviewStore::open_write(&args.common.store, &plan.project)?;
    let key = input::review_key(&plan.project, candidate);
    let state = store.append(&key, source, args.expected_revision, draft)?;
    Ok((key, state))
}

fn stale_rows<'a>(
    plan: &input::PlanCandidates<'_>,
    snapshot: &'a [(model::ReviewKey, ReviewState)],
    report: Option<&ReportCandidates<'_>>,
) -> Result<Vec<ReviewRow<'a>>, ReviewError> {
    let mut stale_candidates = Vec::new();
    for (key, state) in snapshot {
        #[cfg(test)]
        JOIN_WORK.with_borrow_mut(|work| work.plan_probes += 1);
        if plan.contains_key(key.candidate_id.as_str()) {
            continue;
        }
        let descriptor = &key.descriptor;
        let candidate = hoimin_core::MutationCandidate {
            id: key.candidate_id.clone(),
            sequence: 0,
            path: descriptor.path.clone(),
            span: descriptor.span,
            original: descriptor.original.clone(),
            replacement: descriptor.replacement.clone(),
            operator: descriptor.operator.clone(),
            line: descriptor.line,
            column: descriptor.column,
            symbol: descriptor.symbol.clone(),
            file_hash: descriptor.file_hash.clone(),
        };
        stale_candidates.push(ReviewRow {
            execution: execution(&candidate, report)?,
            candidate,
            review: None,
            applicability: Applicability::Stale,
            history: &state.history,
        });
    }
    Ok(stale_candidates)
}
