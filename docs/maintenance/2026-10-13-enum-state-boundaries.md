# Enum state boundaries

## Scope and design

Review baseline: `b2759feb9f03c2c307f335d44e6b3790885046ee`.
Use enums for mutually exclusive states whose associated data must move with
the state. Preserve CLI, JSON, persisted formats, error precedence, and cleanup
ordering. This is an internal refactor, not a performance claim.

Selected boundaries:

- Metrics: running/finished stages and idle/queued/running worker processes.
- Report sequence: not started, running, and finished, with the run identity.
- Report encoding: human, JSON Lines, JSON with its spool, and released JSON.
- Owned report delivery: idle with a handler, running with a join handle, and
  unavailable after a task panic. Cancellation retains the running join handle.
- Python boundary runner: a closed set of result statuses using `Enum`,
  retaining the existing JSON strings and raw observation input validation.

Independent observations remain independent: outcome and exit-policy flags can
coexist; managed-root cleanup readiness and cleanup attempts can occur in either
order; worker drop policy differs from cleanup completion; telemetry fields can
arrive together; optional configuration and diagnostic fields are not phases.
Core worker execution already has a phase enum; moving every optional payload
into its many variants would require a separate, higher-risk protocol rewrite.
Python registry modes already use a discriminated `Literal` union. These are
deliberate exclusions, not a requirement to eliminate every bool or string.

## Design self-reviews

1. **Exclusivity:** selected states have a single owner; independent flags remain.
2. **Failure behavior:** retain unavailable writers and released JSON explicitly;
   do not silently turn them into an idle or streaming output state.
3. **Cancellation:** keep the join handle in the driver across cancelled awaits;
   do not move it into a temporary future that could lose ownership.
4. **Compatibility:** retain status strings, error messages and precedence;
   exported output/configuration enums and schemas are unchanged.
5. **Complexity:** prefer private enums with payloads over parallel Options;
   add no generic state-machine framework, dependency or public configuration.

## Implementation plan and self-reviews

1. **Baseline:** run existing Rust and Python behavior checks before edits;
   use them to verify the refactor rather than add tests mirroring variants.
2. **Order:** change metrics, report sequence, encoding, delivery, then Python;
   inspect each diff before the next boundary.
3. **Focused risks:** check rejected metric transitions, report error precedence,
   released spools, writer panic, cancelled delivery and JSON status round trips.
4. **Resources:** use a dedicated target, two build/test workers, no incremental
   artifacts or debug symbols; clean that exact target after final verification.
5. **Completion:** run workspace tests, contracts checks, strict lint and docs
   checks; obtain a fresh independent read-only review, then commit and create
   one cohesive PR. No runtime or public-feature dependency requires a stack.

## Implementation self-reviews

1. **Metrics:** rejected queue/start/cancel/finish operations do not consume an
   accepted timestamp. Keep saturating arithmetic and the existing error order.
2. **Sequence:** lifecycle errors still precede mutant validation and monotonic
   sequence errors. Finish moves the original identity without allocating a copy.
3. **Encoding:** releasing a JSON spool still happens after both flush attempts,
   including writer failures. Diagnostics remain JSON Lines on stderr afterward.
4. **Delivery:** a cancelled await retains `Running`; a join panic becomes
   `Unavailable`, rejects later writes and does not claim quiescence. Owners and
   stderr locking retain their original drop order and selection policy.
5. **Python and scope:** `BoundaryStatus` members use enum identity; string
   comparison and JSON serialization explicitly use `.value`. Membership and
   count generation share the same closed
   set; parsed observation records remain strings until existing validation.
   Independent flags and public schemas are unchanged.

## Verification

The existing Python boundary suite passes before and after the change: 40 passed,
1 skipped on Windows. The full Python suite passes before and after the change:
972 passed, 29 skipped (`python -m pytest -q`). A temporary differential probe
compared 324 combinations of exit codes, test output, and valid or malformed
observations against the baseline implementation: classifications, errors, and
JSON values matched. The probe is investigative evidence, not a permanent test
that duplicates implementation details.
The first sandboxed focused attempt had temporary-directory setup errors; the
same command passed with the required filesystem permissions before code edits.

The Rust baseline binaries were built before production Rust edits. The baseline
workspace run passed: 2,801 passed, 0 failed, 27 ignored, summed across 151 harness
summaries (including child harnesses). The new property and spool-release checks
characterize preserved behavior, not a new feature or a claimed test-first fix.

The changed CLI library passes its focused run: 858 passed, 0 failed, 16 ignored
in its main harness. Strict workspace Clippy passes with all targets and features
(`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`).
The first lint attempts exposed a stale test fixture and an empty-array assertion;
both were corrected without exclusions.

The changed workspace passes `cargo test --locked --workspace --no-fail-fast`:
2,803 passed, 0 failed, 27 ignored across the same 151 harness summaries. The
increase of two is the new property test and released-JSON behavior test.
Selected contracts-feature checks also pass:

- `cargo test --locked -p hoimin-core --features contracts --lib --test report_policy --test machine --no-fail-fast`:
  138 passed, 0 failed, 2 ignored across three harnesses.
- `cargo test --locked -p hoimin-cli --features contracts --lib --test report_handler --test metrics_destinations --no-fail-fast`:
  912 passed, 0 failed, 16 ignored across five summaries including child harnesses
  (858 main-library tests, 19 metrics-destination tests, 33 report-handler tests,
  and two child-process tests).

Ruff checks and formatting, ty, `cargo fmt --all --check`, and
`git diff --check` pass. The supported publication command,
`python -m tools.docs_site --output site`, passes its strict build and link
validation. An initial direct `mkdocs build --strict` invocation bypassed the
allowlist/link resolver and failed on historical documentation links; the
supported helper was then used successfully.

## Verification self-reviews

1. **Behavior coverage:** compare baseline and changed suites; exercise rejected
   metric transitions, released-spool diagnostics, writer panic and cancelled
   delivery through behavior rather than enum-layout assertions.
2. **Error precedence:** re-read the original and changed report lifecycle and
   worker transitions; invalid operations retain accepted data and original
   identities, including errors after finish and panic.
3. **Wire compatibility:** inspect exported schemas and serialization boundaries;
   compare Python classifications and JSON values with the baseline. There are
   no CLI, schema, default, dependency or package changes. The compatibility
   floor remains 0.4.0; no migration is required.
4. **Quality checks:** run strict Rust/Python lint, formatting, typing and docs
   checks. Consult runtime-lifecycle and session-report design records plus
   `docs/knowledge/test-structure.md` and
   `docs/knowledge/typed-results-and-contracts.md`; their protocol claims remain
   unchanged. Add a maintainer-guide link rather than refresh historical audits.
5. **Limits and resources:** verify on Windows with Python 3.14; Linux/macOS are
   not run locally. Contracts checks cover the selected changed targets, not the
   entire feature-enabled end-to-end suite. No new formal proof is claimed for
   this representation refactor. Restrict Cargo concurrency and clean only the
   dedicated verified target after verification; no memory high-water mark was
   measured.

An independent read-only reviewer checked cancellation ownership, panic state,
spool release, metric timing, error precedence and Python wire compatibility,
and found no actionable issues, including a second review of the verification
record and test-fixture updates. After all test processes finished, the dedicated
target path was resolved, checked against the exact workspace path and checked
for redirection, then cleaned with `cargo clean --target-dir`: 4,749 files and
4.8 GiB removed. Drive C had 181,815,681,024 bytes free afterward. No shared Cargo
target or cache was cleaned.

## Follow-up: ordinary Python Enum

Use `Enum`, not `StrEnum` or a `str` mixin, as requested. Classification returns
an enum member; raw observation and report records retain strings. Convert with
`.value` at those boundaries, including JSON count keys. This replaces the
initial implicit string compatibility without changing the wire format.

Five design reviews checked type separation, serialization, captured-input
validation, error precedence and the absence of new dependencies. Five plan
reviews checked the existing PR branch, a failing type assertion before edits,
all status-use sites, focused regression coverage and retained Rust results.
Five implementation reviews checked observation comparisons, all execution exit
paths, missing-result rows, captured-result membership and count/exit-policy
comparisons. Five verification reviews checked the expected three red cases,
40 passing boundary tests with one platform skip, strict lint/typing, baseline
parity with explicit enum-value serialization, and unchanged Rust sources (no
Cargo rebuild or additional disk artifacts needed).

The focused suite initially failed three type assertions with `StrEnum`, then
passed after conversion. The 324-case differential probe also passes after
explicit `.value` conversion. Ruff check/format and ty pass. The preceding full
Rust/Python suite results apply to the initial change; this follow-up reruns the
affected Python suite rather than claiming a fresh full run.
The supported strict documentation build and link checks pass. A fresh
independent read-only review of every status-use site found no issues.
