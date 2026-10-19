use std::io::{self, Write};

use hoimin_core::{
    OutputEvent, ProcessTermination, RunSummary, SessionDiagnostic, VerificationSelectionMode,
};

use crate::human_text::HumanText;

pub(super) fn write_mutant_diagnostics(
    writer: &mut impl Write,
    diagnostics: &[SessionDiagnostic],
) -> io::Result<()> {
    for diagnostic in diagnostics {
        writeln!(
            writer,
            "{} {}: {}",
            HumanText(&diagnostic.level),
            HumanText(&diagnostic.code),
            HumanText(&diagnostic.message)
        )?;
    }
    writer.flush()
}

pub(super) fn write_event(writer: &mut impl Write, event: &OutputEvent) -> io::Result<()> {
    match event {
        OutputEvent::RunStarted(value) => {
            match value.normalized_config.as_ref() {
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
                        HumanText(&value.run_id),
                        config.profile.as_str(),
                    )?;
                    if !config.fingerprint_includes.is_empty() {
                        writeln!(writer, "fingerprint includes: [{}]", HumanText(&patterns))?;
                    }
                    if !config.fingerprint_files.is_empty() {
                        writeln!(
                            writer,
                            "fingerprint files: [{}]",
                            HumanText(&config.fingerprint_files.join(", "))
                        )?;
                    }
                    if !config.fingerprint_inputs.is_empty() {
                        writeln!(writer, "fingerprint inputs: [{}]", HumanText(&inputs))?;
                    }
                    if let Some(selection) = &value.verification_selection {
                        write_selection(writer, selection)?;
                    }
                }
                None => writeln!(writer, "run started: {}", HumanText(&value.run_id))?,
            }
            if let Some(outcome) = value.resume {
                match outcome {
                    hoimin_core::ResumeOutcome::Resumed => {
                        writeln!(writer, "resume: continuing saved run")?;
                    }
                    hoimin_core::ResumeOutcome::Fresh { reason } => writeln!(
                        writer,
                        "resume: starting new run [{}]: {}",
                        reason.code(),
                        reason.explanation()
                    )?,
                }
            }
        }
        OutputEvent::BaselineFinished(value) => {
            writeln!(
                writer,
                "baseline finished: {}",
                termination_name(value.termination)
            )?;
        }
        OutputEvent::MutantStarted(value) => {
            writeln!(writer, "mutant started: {}", HumanText(&value.mutant_id))?;
        }
        OutputEvent::MutantFinished(value) => writeln!(
            writer,
            "mutant finished: {}:{}:{} {} {:?} -> {:?} {}",
            HumanText(value.candidate.path.as_str()),
            value.candidate.line,
            value.candidate.column,
            HumanText(&value.candidate.operator),
            HumanText(&value.candidate.original),
            HumanText(&value.candidate.replacement),
            value.outcome.status().as_str()
        )?,
        OutputEvent::Diagnostic(value) => {
            write!(
                writer,
                "{} {}: ",
                HumanText(&value.level),
                HumanText(&value.code),
            )?;
            if value.code == "baseline.output" {
                // Child transcripts have an explicit multiline policy; ordinary
                // diagnostics remain single-line even if their text looks similar.
                writeln!(writer, "{}", HumanText(&value.message).baseline_output())?;
            } else {
                writeln!(writer, "{}", HumanText(&value.message))?;
            }
        }
        OutputEvent::RunFinished(value) => {
            write_summary(writer, value)?;
        }
    }
    writer.flush()
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

fn write_selection(
    writer: &mut impl Write,
    selection: &hoimin_core::VerificationSelection,
) -> io::Result<()> {
    let mode = selection.mode().as_str();
    let scope = selection.scope().as_str();
    let policy = selection.policy().as_str();
    writeln!(
        writer,
        "verification selection: mode={mode} policy={policy} requested={} selected={} scope={scope} plan_truncated={}",
        selection.requested(),
        selection.selected(),
        selection.plan_truncated()
    )?;
    if let Some(sampling) = selection.sampling() {
        writeln!(
            writer,
            "sample: population={} seed={}",
            sampling.population(),
            sampling.seed()
        )?;
    }
    Ok(())
}

fn write_summary(writer: &mut impl Write, summary: &RunSummary) -> io::Result<()> {
    let counts = &summary.counts;
    writeln!(writer, "run summary:")?;
    if summary
        .verification_selection
        .as_ref()
        .is_some_and(|s| s.mode() == VerificationSelectionMode::Sample)
    {
        writeln!(
            writer,
            "  scope: sampled candidates only; score and completion do not describe the population"
        )?;
    }
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
    writeln!(writer, "  exit: {}", summary.exit_code)?;
    writeln!(
        writer,
        "  disk max owned bytes: {}",
        summary.disk.configured_max_owned_bytes
    )?;
    writeln!(
        writer,
        "  disk minimum free bytes: {}",
        summary.disk.configured_min_free_bytes
    )
}
