use std::io::{self, Write};

use hoimin_core::{
    MutationStatus, OutputEvent, ProcessTermination, RunSummary, SessionDiagnostic,
    VerificationSelectionMode, VerificationSelectionPolicy, VerificationSelectionScope,
};

pub(super) fn write_mutant_diagnostics(
    writer: &mut impl Write,
    diagnostics: &[SessionDiagnostic],
) -> io::Result<()> {
    for diagnostic in diagnostics {
        writeln!(
            writer,
            "{} {}: {}",
            diagnostic.level, diagnostic.code, diagnostic.message
        )?;
    }
    writer.flush()
}

pub(super) fn write_event(writer: &mut impl Write, event: &OutputEvent) -> io::Result<()> {
    match event {
        OutputEvent::RunStarted(value) => match value.normalized_config.as_ref() {
            Some(config) => {
                let patterns = config.fingerprint_includes.join(", ");
                let inputs = config
                    .fingerprint_inputs
                    .iter()
                    .map(|input| format!("{}={}", input.path, input.hash))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    writer,
                    "run started: {} (profile: {})",
                    value.run_id,
                    config.profile.as_str(),
                )?;
                if !config.fingerprint_includes.is_empty() {
                    writeln!(writer, "fingerprint includes: [{patterns}]")?;
                }
                if !config.fingerprint_files.is_empty() {
                    writeln!(
                        writer,
                        "fingerprint files: [{}]",
                        config.fingerprint_files.join(", ")
                    )?;
                }
                if !config.fingerprint_inputs.is_empty() {
                    writeln!(writer, "fingerprint inputs: [{inputs}]")?;
                }
                if let Some(selection) = &value.verification_selection {
                    let mode = match selection.mode {
                        VerificationSelectionMode::CandidateIds => "candidate_ids",
                        VerificationSelectionMode::Top => "top",
                    };
                    let scope = match selection.scope {
                        VerificationSelectionScope::ExplicitCandidates => "explicit_candidates",
                        VerificationSelectionScope::RetainedCandidates => "retained_candidates",
                    };
                    let policy = match selection.policy {
                        VerificationSelectionPolicy::ExplicitCandidates => "explicit_candidates",
                        VerificationSelectionPolicy::Strict => "strict",
                        VerificationSelectionPolicy::FileRoundRobinV1 => "file_round_robin_v1",
                    };
                    writeln!(
                        writer,
                        "verification selection: mode={mode} policy={policy} requested={} selected={} scope={scope} plan_truncated={}",
                        selection.requested, selection.selected, selection.plan_truncated
                    )?;
                }
            }
            None => writeln!(writer, "run started: {}", value.run_id)?,
        },
        OutputEvent::BaselineFinished(value) => {
            writeln!(
                writer,
                "baseline finished: {}",
                termination_name(value.termination)
            )?;
        }
        OutputEvent::MutantStarted(value) => {
            writeln!(writer, "mutant started: {}", value.mutant_id)?;
        }
        OutputEvent::MutantFinished(value) => writeln!(
            writer,
            "mutant finished: {}:{}:{} {} {:?} -> {:?} {}",
            value.candidate.path,
            value.candidate.line,
            value.candidate.column,
            value.candidate.operator,
            value.candidate.original,
            value.candidate.replacement,
            status_name(value.status)
        )?,
        OutputEvent::Diagnostic(value) => {
            writeln!(writer, "{} {}: {}", value.level, value.code, value.message)?;
        }
        OutputEvent::RunFinished(value) => {
            write_summary(writer, value)?;
        }
    }
    writer.flush()
}

fn status_name(status: MutationStatus) -> &'static str {
    match status {
        MutationStatus::Killed => "killed",
        MutationStatus::Survived => "survived",
        MutationStatus::Timeout => "timeout",
        MutationStatus::OutOfMemory => "out_of_memory",
        MutationStatus::ProcessLimit => "process_limit",
        MutationStatus::Error => "error",
        MutationStatus::NotRun => "not_run",
    }
}

fn termination_name(termination: ProcessTermination) -> String {
    match termination {
        ProcessTermination::Exit(code) => format!("exit ({code})"),
        ProcessTermination::Timeout => "timeout".to_owned(),
        ProcessTermination::OutOfMemory => "out_of_memory".to_owned(),
        ProcessTermination::ProcessLimit => "process_limit".to_owned(),
        ProcessTermination::Cancelled => "cancelled".to_owned(),
    }
}

fn write_summary(writer: &mut impl Write, summary: &RunSummary) -> io::Result<()> {
    let counts = &summary.counts;
    writeln!(writer, "run summary:")?;
    writeln!(writer, "  killed: {}", counts.killed)?;
    writeln!(writer, "  survived: {}", counts.survived)?;
    writeln!(writer, "  timeout: {}", counts.timeout)?;
    writeln!(writer, "  out_of_memory: {}", counts.out_of_memory)?;
    writeln!(writer, "  process_limit: {}", counts.process_limit)?;
    writeln!(writer, "  error: {}", counts.error)?;
    writeln!(writer, "  not_run: {}", counts.not_run)?;
    match counts.score {
        Some(score) => writeln!(writer, "  score: {score:.2}")?,
        None => writeln!(writer, "  score: none")?,
    }
    writeln!(writer, "  complete: {}", summary.complete)?;
    writeln!(writer, "  exit: {}", summary.exit_code)
}
