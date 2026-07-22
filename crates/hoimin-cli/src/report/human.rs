use std::io::{self, Write};

use hoimin_core::{MutationStatus, OutputEvent};

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
            }
            None => writeln!(writer, "run started: {}", value.run_id)?,
        },
        OutputEvent::BaselineFinished(value) => {
            writeln!(writer, "baseline finished: {:?}", value.termination)?;
        }
        OutputEvent::MutantStarted(value) => {
            writeln!(writer, "mutant started: {}", value.mutant_id)?;
        }
        OutputEvent::MutantFinished(value) => writeln!(
            writer,
            "mutant finished: {} {}",
            value.candidate.id,
            status_name(value.status)
        )?,
        OutputEvent::Diagnostic(value) => {
            writeln!(writer, "{} {}: {}", value.level, value.code, value.message)?;
        }
        OutputEvent::RunFinished(value) => {
            writeln!(writer, "run finished: exit {}", value.exit_code)?;
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
