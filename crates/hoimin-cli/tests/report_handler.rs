use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use hoimin_cli::report::ReportHandler;
use hoimin_core::{
    BaselineFinished, ByteSpan, Diagnostic, EffectFailure, EffectId, EmitOutput, MutantFinished,
    MutantStarted, MutationCandidate, MutationStatus, MutationSummary, OutputEvent, OutputFormat,
    OutputSpoolRef, ProcessTermination, REPORT_SCHEMA_VERSION, ResourceMode, RunStarted,
    RunSummary,
};

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<WriterState>>);

#[derive(Default)]
struct WriterState {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}

impl SharedWriter {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().bytes.clone()).unwrap()
    }

    fn flushes(&self) -> usize {
        self.0.lock().unwrap().flushes
    }
}

#[test]
fn jsonl_flushes_each_event_and_keeps_diagnostics_on_stderr() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Jsonl,
        stdout.clone(),
        stderr.clone(),
        std::env::temp_dir(),
    )
    .unwrap();

    let emitted = handler
        .handle(EmitOutput {
            id: EffectId(41),
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
        })
        .unwrap();
    assert_eq!(emitted.id, EffectId(41));

    handler
        .handle(EmitOutput {
            id: EffectId(42),
            event: OutputEvent::Diagnostic(Diagnostic::new(
                "run-1",
                2,
                "warning",
                "resource.best_effort",
                "hard memory control unavailable",
            )),
        })
        .unwrap();

    assert_eq!(stdout.flushes(), 1);
    assert_eq!(stderr.flushes(), 1);
    let line: serde_json::Value = serde_json::from_str(stdout.text().trim()).unwrap();
    assert_eq!(line["schema_version"], REPORT_SCHEMA_VERSION);
    assert_eq!(line["kind"], "run_started");
    assert_eq!(line["sequence"], 1);
    assert!(!stdout.text().contains("diagnostic"));
    assert!(stderr.text().contains("\"kind\":\"diagnostic\""));
    assert_eq!(
        stdout.text(),
        format!(
            "{}\n",
            serde_json::to_string(&OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)))
                .unwrap()
        )
    );
}

#[test]
fn jsonl_uses_all_exact_event_kinds_and_flushes_each_record() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let mut handler = ReportHandler::new(
        OutputFormat::Jsonl,
        stdout.clone(),
        stderr.clone(),
        std::env::temp_dir(),
    )
    .unwrap();

    for (offset, event) in events().into_iter().enumerate() {
        handler
            .handle(EmitOutput {
                id: EffectId(100 + offset as u64),
                event,
            })
            .unwrap();
    }

    let stdout_kinds = stdout
        .text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["kind"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        stdout_kinds,
        [
            "run_started",
            "baseline_finished",
            "mutant_started",
            "mutant_finished",
            "run_finished",
        ]
    );
    assert_eq!(stdout.flushes(), 5);
    assert_eq!(stderr.flushes(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(stderr.text().trim()).unwrap()["kind"],
        "diagnostic"
    );
}

#[test]
fn json_streams_mutants_from_disk_into_one_document() {
    let stdout = SharedWriter::default();
    let stderr = SharedWriter::default();
    let spool = tempfile::tempdir().unwrap();
    let mut handler = ReportHandler::new(
        OutputFormat::Json,
        stdout.clone(),
        stderr.clone(),
        spool.path(),
    )
    .unwrap();
    let event_records = events();
    for event in event_records.iter().cloned() {
        handler
            .handle(EmitOutput {
                id: EffectId(event.sequence()),
                event,
            })
            .unwrap();
    }

    let document: serde_json::Value = serde_json::from_str(stdout.text().trim()).unwrap();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["run"]["kind"], "run_started");
    assert_eq!(document["baseline"]["kind"], "baseline_finished");
    assert_eq!(document["mutants"].as_array().unwrap().len(), 1);
    assert_eq!(document["mutants"][0]["kind"], "mutant_finished");
    assert_eq!(document["summary"]["kind"], "run_finished");
    assert!(!stdout.text().contains("diagnostic"));
    assert!(stderr.text().contains("\"kind\":\"diagnostic\""));
    assert_eq!(stdout.flushes(), 1);
    assert_eq!(
        stdout.text(),
        format!(
            "{{\"schema_version\":1,\"run\":{},\"baseline\":{},\"mutants\":[{}],\"summary\":{}}}\n",
            serde_json::to_string(&event_records[0]).unwrap(),
            serde_json::to_string(&event_records[1]).unwrap(),
            serde_json::to_string(&event_records[3]).unwrap(),
            serde_json::to_string(&event_records[5]).unwrap(),
        )
    );
}

#[test]
fn json_resident_buffers_do_not_grow_with_mutant_count() {
    let spool = tempfile::tempdir().unwrap();
    let mut handler =
        ReportHandler::new(OutputFormat::Json, io::sink(), io::sink(), spool.path()).unwrap();
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
    );
    let initial = handler.resident_buffer_bytes();
    for index in 0..10_000 {
        emit(&mut handler, index + 2, mutant_finished(index + 2, index));
    }
    assert!(handler.resident_buffer_bytes() <= initial + 1024);
}

#[test]
fn report_errors_are_typed_and_echo_the_effect_id() {
    let mut handler = ReportHandler::new(
        OutputFormat::Jsonl,
        FailingWriter,
        io::sink(),
        std::env::temp_dir(),
    )
    .unwrap();
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(99),
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(99));
    assert!(matches!(failed.failure, EffectFailure::ReportIo { .. }));

    let spool = tempfile::tempdir().unwrap();
    let mut handler =
        ReportHandler::new(OutputFormat::Json, io::sink(), io::sink(), spool.path()).unwrap();
    emit(
        &mut handler,
        1,
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
    );
    let failed = handler
        .handle(EmitOutput {
            id: EffectId(100),
            event: OutputEvent::RunStarted(RunStarted::minimal("run-1", 2)),
        })
        .unwrap_err();
    assert_eq!(failed.id, EffectId(100));
    assert!(matches!(failed.failure, EffectFailure::ReportState { .. }));
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("fixture write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("fixture flush failure"))
    }
}

fn emit(handler: &mut ReportHandler<impl Write, impl Write>, id: u64, event: OutputEvent) {
    handler
        .handle(EmitOutput {
            id: EffectId(id),
            event,
        })
        .unwrap();
}

fn events() -> Vec<OutputEvent> {
    vec![
        OutputEvent::RunStarted(RunStarted::minimal("run-1", 1)),
        OutputEvent::BaselineFinished(BaselineFinished {
            schema_version: 1,
            sequence: 2,
            run_id: "run-1".to_owned(),
            termination: ProcessTermination::Exit(0),
            elapsed_ms: 10,
            resource_mode: ResourceMode::Hard,
            output: output_ref(),
        }),
        OutputEvent::MutantStarted(MutantStarted::new("run-1", 3, "m1", 0)),
        mutant_finished(4, 0),
        OutputEvent::Diagnostic(Diagnostic::new(
            "run-1",
            5,
            "warning",
            "x",
            "diagnostic text",
        )),
        OutputEvent::RunFinished(RunSummary {
            schema_version: 1,
            sequence: 6,
            run_id: "run-1".to_owned(),
            counts: MutationSummary {
                killed: 1,
                score: Some(1.0),
                ..MutationSummary::default()
            },
            complete: true,
            exit_code: 0,
        }),
    ]
}

fn mutant_finished(event_sequence: u64, mutant_sequence: u64) -> OutputEvent {
    OutputEvent::MutantFinished(MutantFinished {
        schema_version: 1,
        sequence: event_sequence,
        run_id: "run-1".to_owned(),
        candidate: MutationCandidate {
            id: format!("m{mutant_sequence}"),
            sequence: mutant_sequence,
            path: "src/example.py".into(),
            span: ByteSpan {
                start: mutant_sequence,
                length: 1,
            },
            original: "+".to_owned(),
            replacement: "-".to_owned(),
            operator: "binary".to_owned(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: "hash".to_owned(),
        },
        status: MutationStatus::Killed,
        termination: Some(ProcessTermination::Exit(1)),
        elapsed_ms: 2,
        resource_mode: ResourceMode::Hard,
        output: Some(output_ref()),
    })
}

fn output_ref() -> OutputSpoolRef {
    OutputSpoolRef {
        token: "output".to_owned(),
        retained: 4,
        observed: 9,
    }
}
