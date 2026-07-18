use std::io::Write;

use hoimin_core::OutputEvent;

pub(super) fn write_event(
    writer: &mut impl Write,
    event: &OutputEvent,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(&mut *writer, event)?;
    writer.write_all(b"\n").map_err(serde_json::Error::io)?;
    writer.flush().map_err(serde_json::Error::io)
}
