# Owned report delivery and shutdown deadlines

Issue: #335

## Failure

`ReportHandler::handle` and its final flush call `Write` while the dispatch
future is being polled. A full output transport prevents that future from
observing cancellation or its deadline. The real-binary regression fills a
Unix stream before handing its producer to the CLI. With a one-second total
timeout, JSONL reporting still has not exited after six seconds.

## Design

Keep the public borrowed-writer APIs compatible. Arbitrary borrowed,
non-`Send` writers cannot safely move into a detached thread, so their
synchronous semantics remain explicit. The CLI's owned standard streams use
an owned report delivery driver instead. Both routes share the run state
machine and report serialization.

The owned driver moves the entire report handler into one blocking operation
at a time. The driver retains the join handle when an awaiting future is
dropped. Output is acknowledged only after the write actually succeeds.
The blocking operation retains managed-root ownership until it returns.

Every CLI report operation, including final output, flush, and warning
delivery, is awaited asynchronously. Normal execution observes cancellation
and the total deadline; shutdown work shares the established absolute grace
deadline. A timed-out report operation prevents delivery-root cleanup.
Execution-root cleanup remains governed by process and workspace quiescence.
No second write or flush is started while an earlier report operation owns
the handler. Late completion cannot acknowledge an already abandoned run.

Final error diagnostics must also avoid an unbounded write to a stalled
stderr. They cannot re-enter a standard-stream lock held by a retained report
operation on the dispatcher thread.

Once the absolute deadline is exhausted, the CLI returns exit code 2 without
requiring a final stderr diagnostic. In particular, the message that shutdown
grace expired cannot be guaranteed: its condition only becomes true when no
delivery time remains. Reports already written remain unchanged; stdout can
be partial if its consumer stalls. Startup errors, before a run owns a
shutdown budget, use a separately bounded diagnostic write.

Execution cleanup uses its own process/workspace safety checks and the
remaining shared budget. Report quiescence additionally gates delivery-root
cleanup; it does not prevent independently safe execution cleanup.

## Verification

- Real CLI with unread stdout: JSON, JSONL, and human formats.
- First-interrupt shutdown while stdout is blocked.
- Blocked stderr and flush, without a second unbounded diagnostic write.
- Controlled owned writer proves dispatch responsiveness, physical-write
  acknowledgement, retained root ownership, and safe late completion.
- Existing borrowed/non-`Send` API and report-failure tests remain valid.
- Existing shutdown and disk-lifecycle oracle contracts remain unchanged.
- macOS Rust tests, formatting, Clippy, independent review, then CI on the
  exact final pushed commit.
