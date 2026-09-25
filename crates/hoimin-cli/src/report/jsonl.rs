use std::io::{BufWriter, Write};

use hoimin_core::OutputEvent;

pub(super) fn write_event(
    writer: &mut impl Write,
    event: &OutputEvent,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(&mut *writer, event)?;
    writer.write_all(b"\n").map_err(serde_json::Error::io)?;
    writer.flush().map_err(serde_json::Error::io)
}

/// Coalesce diagnostic fragments without retaining a whole serialized event.
pub(super) fn write_buffered_event(
    writer: &mut impl Write,
    event: &OutputEvent,
) -> Result<(), serde_json::Error> {
    let mut buffered = BufWriter::with_capacity(8 * 1024, writer);
    let result = write_event(&mut buffered, event);
    // BufWriter's Drop would retry buffered bytes after a write error. The
    // explicit flush above owns delivery; discard any remainder without retry.
    let _ = buffered.into_parts();
    result
}
