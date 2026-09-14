# Issue 467 JSONL progress implementation and review

Issue: https://github.com/tokyogas-tech/hoimin/issues/467.
Base: `8b33167`; environment: macOS, controlled repository Python interpreter.

## Behavior and evidence scope

Progress now reads current-schema JSONL incrementally and projects finished
results into the existing document validation path. Existing v2/v3 JSON stays
supported. Start and diagnostic events are validated before their payloads are
discarded. Current JSONL requires a complete lifecycle and terminal summary;
legacy v2 JSONL remains explicitly unsupported. Candidate results and the
largest physical event still determine memory use.

All reviews below were performed by the implementing agent as separate source,
contract and evidence checks. They are not independent human approvals.

## Three review passes at each stage

| Stage | Pass 1 | Pass 2 | Pass 3 |
| --- | --- | --- | --- |
| OKF | Read overview, progress-input audit, development and workflow; preserve existing #460/#483 result/coherence claims. | The new reader belongs to the existing progress-input concept; added source records for the JSONL module, core validator, spec and this report instead of a duplicate concept. | Source hashes are captured after final edits; old audit counts remain historical. Format/link checks are separate from runtime correctness and OS scope. |
| Design | Compared whole-file parsing, independent parser validation and streamed projection; selected one-line retention with shared validators. | Found that RunStarted's RunConfig would reject future config objects already accepted by progress. Seed sequence state from validated header identity while preserving opaque config validation. | ReportSequence does not enforce baseline uniqueness/order and contracts builds assert producer violations. Add baseline checks and a pure validate method before observe. |
| Plan | Split tests, reader, real producer/heap evidence and documentation; specify schema v2 JSON versus current JSONL coverage. | Synthetic JSON documents omit mutant_started, so the JSONL fixture must explicitly insert lifecycle starts and renumber events. | Dedicated no-debug target and two build jobs prevent shared-cache contamination. No production Python changes means Python mutation testing is inapplicable. |
| Implementation | Confirmed common validation retains result/count/exit/incomplete and legacy-null rules after projection. | Rejected duplicate/mixed runs, unmatched finishes, active terminal state and metadata errors on discarded diagnostics through core sequence validation. | Tightened blank-line skipping from ASCII whitespace to JSON whitespace; vertical-tab lines must not be accepted. Extracted pure core validation while retaining producer observe assertions. |
| Verification | Baseline v2 test passed; new JSONL parity test failed on the original reader with trailing characters at line 2, then passed with the stream path. | A real-producer assertion used latest.common instead of comparisons[0].common. Checked the established output schema, corrected the test, and strengthened synthetic parity to compare all comparison fields. | Contracts build reproduced a producer invariant panic on invalid JSONL before pure validation; all JSONL contracts tests passed after the fix. Added separate diagnostic-history peak heap and real failed-baseline checks. |
| PR preparation | Checked acceptance coverage against mixed-format parity, lifecycle corruption, true run producers and streaming memory evidence. | The body must state current-schema JSONL only, one-largest-event memory, and native macOS scope; existing Lean corpus replay is not a new proof or a Windows execution. | Check changed-file scope, final source hashes, clean formatting and actual check outputs before committing; use the repository Change/Validation/OKF template with no invented CI results. |

## Resource and compatibility limits

Build settings: `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, worktree-local target directory.
The `.venv` link is local test setup and is not part of the change.

The heap lane compares 32, 2,000 and 8,000 diagnostic events with 1 KiB
messages while keeping candidates fixed at zero, and permits 128 KiB of peak
allocation variation. This measures allocator peak, not RSS. Existing report
history heap checks remain separate. No constant-memory claim is made for
candidate data, a very large single event, or legacy JSON document parsing.

No Python production mutation run, full Rust workspace suite, new Lean proof
build, Windows execution or release RSS benchmark is claimed. Existing Lean
adapters replay checked-in expectations against changed code.

## Parent review follow-up

The parent agent reviewed the reader and shared-validator separation and raised
whether a not_run finish could bypass the baseline order guard. Inspection of
ReportSequence and machine synthetic-start production showed that every finish,
including not_run, requires a start. The parent accepted this evidence. Added
`jsonl_not_run_requires_start_and_preceding_baseline` to fix that assumption in
an explicit regression; no production rule was changed for this suggestion.

## Executed checks

All commands used the build settings above unless they do not invoke Cargo.

- Baseline oldest v2 normalized config: 1 passed before implementation.
- Red JSONL comparison regression: failed with trailing characters at line 2.
- Red contracts corruption regression: failed with report.sequence.invariant
  panic before side-effect-free validation was introduced.
- `cargo test -p hoimin-cli --test progress --test progress_heap
  --test progress_jsonl_heap --test lean_progress_input_oracle -- --test-threads=1`:
  71 progress, 1 report-history heap, 1 JSONL-history heap and 2 Lean-input tests passed.
- After the parent-review regression, `cargo test -p hoimin-cli --features
  contracts --test progress jsonl_`: all 8 JSONL tests passed.
- `cargo test -p hoimin-core --test report_policy --test lean_report_sequence_oracle`:
  40 policy and 5 Lean sequence adapter tests passed.
- `cargo clippy -p hoimin-cli --test progress --test progress_jsonl_heap -- -D warnings`:
  passed after correcting a test's missing statement semicolon.

- `cargo test -p hoimin-core --features contracts --test report_policy`:
  26 passed, including producer should-panic contracts.
- Final clippy (including the parent-review regression), `cargo fmt --all --
  --check` and `git diff --check`: passed.

OKF validation: PyYAML 6.0.3 parsed 19 pages; 760 local links, source
ID/footnote correspondence, changed issue-467/input hashes and complete
spec/report source-index coverage passed. External links were not probed.
