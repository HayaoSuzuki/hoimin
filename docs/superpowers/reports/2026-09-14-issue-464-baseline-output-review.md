# Issue 464 baseline output review

Base 8b33167, reviewer Codex, worktree .worktrees/issue-464.

## Design self-review

1. Traced spool ownership: ProcessHandler exposes a UUID-checked spool path and execution cleanup removes it. Export must precede cleanup; the BaselineFinished output effect is queued before finalization effects.
2. Checked stdout schema and diagnostic routing: ReportDelivery sends diagnostics to stderr without adding entries to document/JSONL stdout. Existing metrics warnings establish out-of-band warning delivery. Reuse this path rather than add report fields or file destinations.
3. Checked resource/encoding limits: full-buffer read would scale memory with max-output; choose fixed16KiB streaming and UTF8 carry, retaining every retained byte. Distinguish baseline timeout from cancellation before an accepted completion. Read/write failures cannot manufacture a different baseline verdict.

## Plan self-review

1. Matched all issue conditions to real CLI markers, max-output and nonUTF8 cases; added verify path because it shares shell output handling.
2. Checked scheduler wrappers: serial effect execution observes total deadline/cancellation and stopping uses remaining grace. Export stays inside the wrapper and uses the owned reporter's admission.
3. Checked decoding edge cases: chunk boundaries can split valid UTF8, and retained tails can begin with invalid continuation bytes. Tests must distinguish carry from lossy decoding and prove bounded progress on invalid input.

## Implementation self-review

1. Reviewed spool ownership and effect ordering: export consumes the existing UUID-checked retained spool before cleanup, opens no new persistent artifact, and limits all reads to retained bytes. Successful baseline bypasses export. Diagnostic read/write failure cannot replace the accepted baseline termination.
2. Reviewed decoding and memory: fixed16KiB reads retain at most3 incomplete UTF8 bytes. Clippy found a17KiB async future from a stack array; moved the same fixed-size buffer to a Vec, then Clippy passed. This is bounded streaming, not a retained-output-sized allocation.
3. Reviewed delivery and deadlines: reused stderr-only diagnostic handling without changing stdout schema/sequence; export remains inside effect cancellation/timeout wrapper. Added a full-stderr-pipe regression to prove total timeout plus grace still bounds real CLI termination.

## Test self-review

1. Original real CLI regression failed because baseline markers were missing. Four new tests now cover human/JSON/JSONL failure, verify, baseline timeout, successful-baseline silence, retained-tail truncation, split UTF8, invalid byte and incomplete final scalar. Each spawned CLI has timeout/kill-on-drop.
2. Checked assertions against actual JSON schema: cleanup is summary.disk.cleanup (array). Tests assert actual clean cleanup, existing exit/termination, and JSON/JSONL stdout parseability, not only marker presence. Initial fixture escape/schema mistakes were corrected before green; they are not implementation defects.
3. Complete existing run_e2e passed74 tests including blocked stderr. After moving the async buffer to heap and extending incomplete-scalar coverage, reran all4 new tests and the blocked-consumer test. Clippy all-targets/all-features passed. Raw byte offset and count metadata distinguish truncation from decoding replacement. Spool read errors are handled but do not have a dedicated injected failure test.

## OKF self-review

1. Compared the session/report addition with actual emitter and public README: stderr diagnostics preserve report schema; no durable spool path is promised after cleanup. Saveability comes from ordinary stderr redirection.
2. Checked provenance: design/report source entries use actual SHA256 and untracked working-tree state at source capture. Existing unrelated catalog revision metadata is preserved.
3. Validate all19 OKF pages, local links, source IDs/footnotes and complete design/audit index coverage. The concept documents cancellation and output-read limitations rather than claiming unconditional export.

## PR self-review

1. Scope audit: one issue worktree; only emitter hook, bounded exporter, tests, README and issue documents/OKF are staged. The local virtualenv symlink and other issue worktrees are excluded.
2. Requirements audit: failed-baseline output is visible and saveable for all formats before cleanup, truncation/encoding is specified, and exits/resource policy remain unchanged. User-facing instructions use existing shell redirection and no new unsupported flag.
3. Evidence audit: report initial red and Clippy failure separately from successful checks; disclose absence of a read-error injection and interrupted-export limitation. Publication uses base main and closes464; remote CI is tracked separately from local checks.

## Verification evidence

- New real CLI baseline_output suite:4 passed (final run, including incomplete UTF8 EOF).
- Complete run_e2e:74 passed; blocked-stderr exact regression rerun after heap buffer change.
- cargo clippy --workspace --all-targets --all-features -- -D warnings:passed after fixed-size heap buffer change.
- cargo fmt --all -- --check and OKF validator:passed;19pages/757local links, changed source hashes and complete source indexes.


## Independent review

The issue-476 agent reviewed chunk/UTF8 carry bounds, terminal escaping, retained-tail metadata and raw offsets, spool lifecycle, read/write failure handling, original BaselineFinished reporting and owned delivery deadlines. No blocking findings. It independently reran all4 baseline_output tests and the blocked stderr regression (1passed,73filtered;3.49s). The review confirmed the stated absence of an injected read-failure test, borrowed-writer progress assumption and interrupted-export limitation.
