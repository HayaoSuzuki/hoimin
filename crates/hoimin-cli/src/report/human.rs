use std::io::{self, Write};

use hoimin_core::{MutationStatus, OutputEvent};

pub(super) fn write_event(writer: &mut impl Write, event: &OutputEvent) -> io::Result<()> {
    match event {
        OutputEvent::RunStarted(value) => writeln!(writer, "run started: {}", value.run_id)?,
        OutputEvent::BaselineFinished(value) => {
            writeln!(writer, "baseline finished: {:?}", value.termination)?
        }
        OutputEvent::MutantStarted(value) => {
            writeln!(writer, "mutant started: {}", value.mutant_id)?
        }
        OutputEvent::MutantFinished(value) => writeln!(
            writer,
            "mutant finished: {} {}",
            value.candidate.id,
            status_name(value.status)
        )?,
        OutputEvent::Diagnostic(value) => {
            writeln!(writer, "{} {}: {}", value.level, value.code, value.message)?
        }
        OutputEvent::RunFinished(value) => {
            writeln!(writer, "run finished: exit {}", value.exit_code)?
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
