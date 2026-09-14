use std::io::Write;

use hoimin_core::{BaselineFinished, Diagnostic, EffectId, EmitOutput, OutputEvent};
use tokio::io::AsyncReadExt;

use crate::process::ProcessHandler;
use crate::report::delivery::ReportDelivery;

const CHUNK_BYTES: usize = 16 * 1024;

enum ExportError {
    Read(String),
    Write,
}

pub(crate) async fn emit<Stdout: Write, Stderr: Write>(
    process: &ProcessHandler,
    report: &mut ReportDelivery<Stdout, Stderr>,
    baseline: &BaselineFinished,
) {
    if let Err(ExportError::Read(error)) = stream(process, report, baseline).await {
        let _ = diagnostic(report, baseline, "baseline.output.read", error).await;
    }
}

async fn stream<Stdout: Write, Stderr: Write>(
    process: &ProcessHandler,
    report: &mut ReportDelivery<Stdout, Stderr>,
    baseline: &BaselineFinished,
) -> Result<(), ExportError> {
    let path = process
        .spool_path(&baseline.output)
        .map_err(|error| ExportError::Read(error.to_string()))?;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| ExportError::Read(error.to_string()))?;
    diagnostic(report, baseline, "baseline.output", format!(
        "captured baseline stdout/stderr token={} retained={} observed={} truncated={} encoding=utf-8-lossy",
        baseline.output.token, baseline.output.retained, baseline.output.observed,
        baseline.output.observed > baseline.output.retained,
    )).await?;
    let mut remaining = baseline.output.retained;
    let mut offset = 0_u64;
    let mut pending = Vec::with_capacity(CHUNK_BYTES + 3);
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    while remaining > 0 {
        let limit = usize::try_from(remaining.min(CHUNK_BYTES as u64)).unwrap();
        let read = file
            .read(&mut buffer[..limit])
            .await
            .map_err(|error| ExportError::Read(error.to_string()))?;
        if read == 0 {
            return Err(ExportError::Read(format!(
                "baseline spool ended with {remaining} retained bytes missing"
            )));
        }
        remaining -= read as u64;
        pending.extend_from_slice(&buffer[..read]);
        let (text, consumed) = decode_prefix(&pending, remaining == 0);
        if consumed > 0 {
            diagnostic(
                report,
                baseline,
                "baseline.output",
                format!(
                    "baseline token={} offset={offset}:\n{}",
                    baseline.output.token,
                    terminal_text(&text),
                ),
            )
            .await?;
            offset += consumed as u64;
            pending.drain(..consumed);
        }
    }
    Ok(())
}

/// Decode complete prefixes, retaining a split UTF-8 suffix until the next read.
fn decode_prefix(bytes: &[u8], eof: bool) -> (String, usize) {
    let mut text = String::new();
    let mut consumed = 0;
    while consumed < bytes.len() {
        match std::str::from_utf8(&bytes[consumed..]) {
            Ok(valid) => {
                text.push_str(valid);
                consumed = bytes.len();
            }
            Err(error) => {
                let valid_end = consumed + error.valid_up_to();
                text.push_str(std::str::from_utf8(&bytes[consumed..valid_end]).unwrap());
                consumed = valid_end;
                match error.error_len() {
                    Some(invalid) => {
                        text.push('\u{fffd}');
                        consumed += invalid;
                    }
                    None if eof => {
                        text.push('\u{fffd}');
                        consumed = bytes.len();
                    }
                    None => break,
                }
            }
        }
    }
    (text, consumed)
}

fn terminal_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            escaped.extend(character.escape_default());
        } else {
            escaped.push(character);
        }
    }
    escaped
}

async fn diagnostic<Stdout: Write, Stderr: Write>(
    report: &mut ReportDelivery<Stdout, Stderr>,
    baseline: &BaselineFinished,
    code: &str,
    message: String,
) -> Result<(), ExportError> {
    report
        .handle(EmitOutput {
            id: EffectId(u64::MAX),
            event: OutputEvent::Diagnostic(Diagnostic::new(
                &baseline.run_id,
                u64::MAX,
                "warning",
                code,
                message,
            )),
        })
        .await
        .map(|_| ())
        .map_err(|_| ExportError::Write)
}
