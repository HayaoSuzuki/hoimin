//! Instrument execution without choosing a subscriber for library callers.

use hoimin_core::{RunPhase, RunState};

pub(crate) fn stage(stage: &'static str) {
    tracing::info!(target: "hoimin_cli::progress", stage, "execution stage");
}

pub(crate) fn observe(state: &RunState) {
    let label = match state.phase() {
        RunPhase::Validate => "resolving targets",
        RunPhase::Preflight => "checking workspace",
        RunPhase::Copy | RunPhase::MaterializationVerification => "preparing workers",
        RunPhase::Baseline => "running baseline",
        RunPhase::BudgetCheck => "checking execution budget",
        RunPhase::Analyze => "analyzing sources",
        RunPhase::Mutants => "testing mutants",
        RunPhase::Finalize => "writing results",
        RunPhase::Cleaning | RunPhase::Finished => "cleaning up",
    };
    let completed = completed(state);
    tracing::info!(target: "hoimin_cli::progress", stage = label, completed, "execution progress");
}

pub(crate) fn completed(state: &RunState) -> u64 {
    let summary = state.summary();
    summary.killed
        + summary.survived
        + summary.timeout
        + summary.out_of_memory
        + summary.process_limit
        + summary.error
}
