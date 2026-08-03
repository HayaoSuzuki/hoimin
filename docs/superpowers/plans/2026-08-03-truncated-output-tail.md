# Retain the tail of truncated process output

Issue: #120

## Problem

The process collector currently retains only the first `max_output_bytes`.
Tracebacks and test summaries are normally emitted last, so verbose output can
discard the most useful diagnostic evidence.

## Design

Treat the spool as a disk-backed ring with capacity `max_output_bytes`. While
draining, count every byte and overwrite only the oldest retained bytes. If the
stream fits, leave its exact bytes unchanged. If it exceeds the cap, rebuild
the spool as an explicit truncation marker followed by the newest bytes that
fit in the remaining budget. Very small non-zero caps that cannot fit the full
marker retain only their newest bytes.

The ring avoids retaining an output-sized memory buffer and continues draining
stdout and stderr after reaching the cap. Final spool size never exceeds the
configured limit.

## Verification

- Unit tests cover exact untruncated output, marked tail retention, a tiny cap
  that still keeps the final bytes, and a zero-byte cap.
- A process-handler test confirms the behavior through a real child process.
- Existing non-UTF-8, combined-stream, drain, cancellation, and timeout tests
  remain unchanged.
