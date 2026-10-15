# Issue 915 review and verification

Baseline: `54093222abe08c79e8e36e4656ee04b9c6d8dda0`.
Host: macOS; tests use the pinned repository Rust toolchain.

## Characterization scope

The pure conversion checks enumerate all 1,365 byte strings of length 0–5 over
`x`, CR, LF, and a non-UTF-8 byte, comparing every valid Git interval with the
independent core line-start index. Separate count and conversion tests keep each
assertion group about one behavior. The materialized-text property oracle handles
mixed Unicode/newline strings, disjoint intervals, physical counts, and invalid
range starts/ends; its generation and shrinking bounds remain unchanged.

New Git integration examples assert literal physical rows for tracked edits with
zero/one Git context line, an explicit base, unborn index entries, and untracked
files. Existing integration examples retain normalization, path batching, binary
and deletion exclusion, rename directions, and repository/base diagnostics.

This is a behavior-preserving refactor. Characterization must pass on the original
implementation; sensitivity probes, rather than a fabricated feature failure,
will establish that these assertions detect unit confusion. Tests do not assert
private type names or source spelling.

## Execution records

All heavy commands use `python3 /tmp/hoimin-task-guard.py <tree> <label>` with
this issue's absolute worktree, private target directory, two Cargo jobs, and two
Rust test threads. The guard caps process-group RSS at 8 GiB, target size at
12 GiB, requires 20 GiB free space, and caps each command at 2,400 seconds.

| Stage | Guard label / command | Result |
| --- | --- | --- |
| Before production edits | `issue915-baseline-lines`: `cargo test -p hoimin-cli --lib target::git -- --test-threads=2` | 30 passed; 40.6 s; peak group RSS 1,479,507,968 bytes; no guard stop |
| Before production edits | `issue915-baseline-scope`: `cargo test -p hoimin-cli --test changed_git_scope -- --test-threads=2` | 11 passed; 8.1 s; peak group RSS 776,568,832 bytes; no guard stop |
| After refactor | `issue915-green-lines`: `cargo test -p hoimin-cli --lib target::git -- --test-threads=2` | 30 passed; 16.2 s; peak group RSS 1,510,719,488 bytes; no guard stop |
| After refactor | `issue915-green-scope`: `cargo test -p hoimin-cli --test changed_git_scope -- --test-threads=2` | 11 passed; 8.1 s; peak group RSS 838,451,200 bytes; no guard stop |
| Temporary missing conversion | `issue915-mutant-no-translation`: `cargo test -p hoimin-cli --test changed_git_scope tracked_mixed_newlines_select_physical_rows_with_git_context -- --exact --test-threads=2` | Expected exit 101: actual rows 2–2 versus expected 2–3; 6.1 s; peak 833,355,776 bytes |
| Temporary unborn reconversion | `issue915-mutant-unborn-translation`: `cargo test -p hoimin-cli --test changed_git_scope unborn_index_ranges_already_count_python_physical_rows -- --exact --test-threads=2` | Expected exit 101: source-range mismatch; 8.1 s; peak 778,977,280 bytes |
| Full restored workspace | `issue915-workspace`: `cargo test --workspace --all-features -- --test-threads=2` | Exit 0; 3,024 passed, 29 ignored, 0 failed across 161 summaries including doctests; 346.7 s; peak 1,712,439,296 bytes; no warnings or guard stop |
| First quality run | `issue915-prek`: `prek run --all-files` | Exit 1: workspace Clippy rejected `assert!(actual.is_empty())` in the split empty-range test; both format checks and vendored Clippy passed; 18.4 s; peak 1,107,197,952 bytes |
| Final assertion check | `issue915-final-lines`: `cargo test -p hoimin-cli --all-features --lib target::git -- --test-threads=2` | 30 passed; 16.3 s; peak 1,564,573,696 bytes; no guard stop |
| Final quality gates | `issue915-prek-final`: `prek run --all-files` | All four hooks passed; 16.3 s; peak 1,094,582,272 bytes; no guard stop |

Logs and guard summaries use `/tmp/hoimin-<label>.log` and `.summary`.

## Implementation review passes

1. Producer/consumer pass: checked both tracked branches return `GitChangedRanges`
   and consume it once; only physical maps join unborn and untracked results.
2. Algorithm pass: compared the old converter body with the consuming method
   after removing indentation; they match. The production byte cursor is unchanged.
3. Ordering pass: inspected the diff for exclusion-before-normalization and final
   normalization. Moving the former into `tracked::collect` preserves ordering
   and leaves empty tracked results distinct from absent HEAD.
4. Compatibility pass: read unchanged public structs, `LineRange` uses, Git flags,
   and `DiffPlan::Legacy`; no CLI/schema/default/rename policy changed.
5. Test-quality pass: reviewed all added cases and split checks for AAA, one
   behavior per function, literal integration expectations, bounded generation,
   and independent oracles. Moved tracked-case selection construction into Arrange.
   New tests target missing tracked conversion or repeated conversion of physical
   ranges; no assertion relies on the new type.

## Test review passes

1. Exhaustive-domain pass: counted 1,365 generated byte inputs and checked the
   alphabet includes non-UTF-8 data, empty input, lone CR, LF, and CRLF. Expected
   physical rows come from a separate line-start index, not the cursor.
2. Property-oracle pass: traced CRLF normalization followed by bare-CR replacement
   and inclusive splitting. Git row boundaries cannot divide CRLF; generated
   selected intervals remain sorted/disjoint. Count/start/end checks have separate
   actions and retain the `0..256` character-length strategy and 2,048-shrink-iteration
   limit.
3. Literal-fixture pass: hand-counted tracked Git row 2 as Python rows 2–3, context
   as rows 1–4, and the explicit-base first Git row as rows 1–2. Unborn/untracked
   cases have two physical rows across each selected newline/EOF category.
4. Test-structure pass: reread new and changed tests for AAA and independent
   state per parameter case. Both empty ranges and row conversion bounds have
   separate functions; no new scenario-selecting conditional assertion exists.
5. Sensitivity pass: inspected the two actual failure logs. The missing-conversion
   probe fails on literal rows 2–2 versus 2–3; unborn reconversion fails on the
   valid fixture's range extending past its one Git row. Both compile and reach
   their intended assertions. Restored the original production bytes before the
   workspace run; these are sensitivity failures, not baseline defects.

## Verification review passes

1. Freshness pass: matched the pre-production characterization logs to the test
   edits, then checked the post-refactor focused logs (30 and 11 passed). The
   baseline demonstrates existing behavior, not a repaired defect.
2. Sensitivity/restoration pass: read each expected exit-101 failure, confirmed
   both reached the intended behavior, and compared the restored production bytes
   with the saved pre-probe source before starting the full suite.
3. Workspace pass: read the completed all-feature log and aggregated all 161
   result summaries: 3,024 passed, 29 ignored, zero failed. Cargo exited 0, emitted
   no warning, and the guard recorded no resource-limit stop.
4. Quality pass: inspected the first Clippy failure and its one-assertion fix;
   final all-feature Git tests passed (30), and all four configured prek hooks
   passed. No production change followed the full workspace run.
5. Delivery pass: checked the final diff scope and absence of temporary mutations,
   all 49 OKF page structures, the four new source IDs/hashes/footnotes, local
   links, and index reachability. Preserved unrelated source history and recorded
   the ignored-test and platform limits below.

The first quality run exposed a test-only `clippy::assert_is_empty` violation.
The diagnostic requests an equality assertion so failure displays the returned
ranges. Replaced that one assertion with `assert_eq!(actual, Vec::<LineRange>::new())`;
production code and test inputs/expectations remain unchanged. The full workspace
result above precedes this assertion-only diagnostic improvement; focused
all-feature tests and the configured quality gates recheck the final source.

## Compatibility, review and limits

No CLI, schema, default, package, platform-support, or version-floor change is
required; existing plans and sessions need no migration. The original byte
conversion algorithm and `DiffPlan::Legacy` path remain in use.

A separate reader completed five read-only review passes and reported no
production/test findings. Design, plan, implementation, test and verification
self-reviews each have five recorded passes. The philosophy assessment remains
8.75/10: seven observable diagnostics pass; design-time allocation is unmeasured.

The workspace execution occurred on macOS with Rust 1.99.0. The 29 ignored tests
were not run, and this work does not claim Windows/Linux execution or new Lean
proofs. Existing independent Rust/text oracles provide the line-unit evidence.
The logs under `/tmp` are local execution artifacts; the commands, counts,
failures, corrections and resource measurements above are retained in this PR.
