# Issue 464: Failed baseline captured output

## Contract

When an accepted baseline result exits nonzero or times out, emit its retained combined stdout/stderr as `baseline.output` diagnostics on stderr before execution-spool cleanup. Human format renders readable diagnostic text; JSON/JSONL retain machine-readable stdout and emit JSON diagnostic records on stderr. The run_id, spool token, raw-byte offset, retained/observed bytes, truncation and UTF-8-lossy decoding are identified in diagnostic messages. Redirect stderr to save these records.

Stream the existing spool in 16 KiB chunks, carrying at most three trailing UTF-8 bytes between reads. Invalid bytes become U+FFFD; valid multibyte characters split between reads remain intact. Escape terminal control characters except newline and tab in diagnostic text. --max-output remains the sole retained-byte bound: this feature neither creates another permanent spool nor silently crops retained output. Observed bytes beyond retained are described as truncated, not recovered.

## Integration and failures

The shell intercepts the BaselineFinished output effect before delivering it, reads through ProcessHandler::spool_path and emits through the existing ReportDelivery. The effect executes before finalization/cleanup and under the existing scheduler deadline/cancellation wrapper. Owned output remains deadline-aware; borrowed synchronous writers retain the documented progress assumption.

The diagnostic operation is observational: spool read/diagnostic failures do not change baseline verdict or exit policy. A read failure emits baseline.output.read when the writer still works; failed diagnostic writes stop the export. No changes to core states, report schemas, output sequence allocation or disk-budget accounting. Out-of-band diagnostics follow the same stderr delivery convention as metrics warnings. A successful baseline is quiet. Total-run cancellation before a baseline completion is accepted cannot promise a baseline log; this contract covers accepted failed/timed-out baselines.

Alternatives: a new export destination needs another protected-path and persistence contract; embedding log bytes in result schemas changes all readers and risks full-buffer copies. Streaming existing stderr diagnostics provides the requested viewing/saving method without a new output file owner.

## Verification

Real CLI run and verify must expose both reproduction marker lines for failure and timeout, preserve exit3 and clean execution cleanup, and keep JSON/JSONL parseable. Test max-output truncation, invalid UTF-8, multibyte boundaries and success silence. Existing blocked-output deadline tests and process/output capture tests cover shared delivery/retention rules; run their relevant lanes. No native OS enforcement claims follow from macOS tests.
