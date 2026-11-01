# Process lifecycle API contract (#858)

Base: `6c638da243865273c55886739e9bbf77803f8fcd`.
Branch: `docs/process-lifecycle-contract-858`.
Remote: `https://github.com/HayaoSuzuki/hoimin.git`; PR base: `main`.

Document the existing process boundary in English Rustdoc. The process module
owns child lifecycles, resource supervision and output collection; shell owns
dispatch admission, completion acceptance and workspace cleanup admission.
Keep one local API contract, a development-guide link and an update to the
existing runtime-lifecycle knowledge concept. No lifecycle code or tests change.

## Design self-reviews

1. Followed `run` through the owned Tokio task and the dropped-waiter test;
   identified token retention and runtime lifetime as caller obligations.
2. Compared cancellation's spawn gate with termination, root wait and supervisor
   finish; a request cannot promise those operations have completed.
3. Followed both counters and sticky failure flags through drain; successful
   observation is not an admission gate and does not include pre-spawn setup.
4. Compared output quiescence with spool I/O success and portable descendant
   limitations; neither an event nor usable output is implied by both flags.
5. Compared synchronous close with deadline-bound async observation and the
   shell's absolute deadline; retained links rather than copying shell policy.

## Planning self-reviews

1. Limited edits to module/API comments and their documentation entry points;
   no new API, state machine or configuration option is needed.
2. Mapped ownership claims to the existing dropped-caller, spawn-boundary and
   cancellation tests before editing comments.
3. Mapped independent evidence and secondary errors to drain/report tests;
   included output-error and unproven-descendant cases in the inspection.
4. Selected Rustdoc with denied warnings, strict docs build and targeted process
   plus existing Lean-oracle adapters; no comment-text assertions are needed.
5. Reserved sequential Cargo runs with two build/test workers, a task-owned
   target and sampled resource guards; planned cleanup after commit hooks.

## Implementation self-reviews

1. Checked `run`/`handle` comments distinguish a never-polled future from a
   detached task, and describe how callers retain cancellation control.
2. Checked token docs distinguish repeated requests from termination/reap waits;
   corrected the existing error summary to allow a cancelled result after spawn.
3. Checked drain docs disclose caller-controlled dispatch and in-flight setup,
   a shared deadline and the absence of hard wall-clock preemption.
4. Checked report fields preserve sticky evidence, independent close errors and
   the both-false/no-close path when a post-deadline poll observes active work.
5. Compared Rust source with base after excluding Rustdoc and blank lines:
   executable source is identical. Checked cross-links and added one OKF source
   record without rewriting historical source revisions or evidence.

## Design assessment

For the process API documentation boundary, the initial score was 6/8 diagnostic
rows (7.5/10). Interface comments did not describe ownership and shutdown
semantics, and a newcomer had to read implementation to discover those
boundaries. Local module/API contracts address both failed rows.

| Diagnostic | Final assessment |
| --- | --- |
| One-sentence module purpose | Process owns lifecycles, supervision and output collection |
| Interface simpler than implementation | Existing request/run/drain API hides platform and collector mechanics |
| Implementation can change without changing callers | Contract describes observable ownership and evidence, not internal counter layout |
| Interface comments describe abstraction | Request, wait, evidence and release semantics are explicit |
| Design reviewed for complexity | Existing ownership is documented without additional abstractions |
| Module hides an important decision | Process retains backend and output coordination |
| Newcomer can understand boundaries | Caller obligations and shell/guide links are at the API |
| Design improvement receives attention | Dedicated review addresses previously implicit caller obligations |

The final local assessment is 8/8 (10/10); no failed row remains in this bounded
documentation assessment. It does not score the whole application, validate
all platform behavior or claim the shutdown API needs no future redesign.

## Verification self-reviews

1. Baseline process unit tests passed: 42 passed, one ignored with contracts
   enabled. The ignored fixture is launched explicitly by its parent test.
2. Checked named drain tests distinguish failed evidence, close errors and close
   timeout. The timeout test blocks the pool before close starts; it does not
   directly test a running backend close outliving the waiter.
3. Checked process-handler tests cover pre-spawn cancellation, cancelled results,
   descendant termination and naturally exited portable roots with live children.
   Native platform guarantees must not be inferred from a macOS run.
4. Checked runtime-lifecycle provenance and historical audit scope stay separate
   from this change; fresh validation results belong below.
5. Reviewed whether new tests or proofs would add evidence: only documentation
   changes, existing behavioral tests cover the claims, and source comparison
   protects against incidental code edits. No new Lean proof, proptest generator
   or mutation run is warranted; existing oracle adapters remain useful checks.

## Compatibility and knowledge maintenance

No CLI option, schema, default, public signature, package or platform behavior
changes. The compatibility floor remains 0.5.0; no migration is needed.
Consulted the development guide, OKF workflow, knowledge overview and existing
runtime-lifecycle concept. Updated that concept with a process source and API
entry point; the existing index already links it. Historical source hashes and
audit claims retain their original scope.

## Execution evidence

Baseline command: `cargo test --locked -p hoimin-cli --features contracts --lib
process::`. Guard: exit 0, 42.6 seconds, peak process-group RSS 1,531,740,160 bytes.
Local artifacts: `/tmp/hoimin-858-*.log` and `.summary`; these are not fixtures.

Final contracts-enabled process unit tests passed 42 tests with one ignored
subprocess fixture (executed by its parent). `process_handler` passed 27 tests;
`lean_shutdown_oracle` and `lean_disk_shutdown_oracle` passed eight tests combined.
The process/integration guards recorded exit 0, 16.3/18.3 seconds and peak
process-group RSS 1,536,475,136/810,041,344 bytes respectively, without a stop.

`cargo doc --locked -p hoimin-cli --all-features --no-deps` with
`RUSTDOCFLAGS=-D warnings` passed. The strict documentation-site build, workspace
formatting and whitespace checks passed. A focused OKF check parsed 45 concept
frontmatters, checked the new process source hash/revision and footnote, and
resolved local links in the two affected guide/concept files; index pages have
no frontmatter. This is not a new full validation of historical provenance.

Independent read-only review identified a deadline wording issue: the drain
loop checks counters before time, so work settling after the deadline but before
the next poll can still yield observed evidence and a close attempt. Reworded
the both-false path around what a post-deadline poll observes, and refreshed the
OKF source hash. This corrects the description without changing scheduling.

All local execution was on macOS. Tests do not extend Linux/Windows native
coverage or prove arbitrary OS interleavings. Cargo runs used two build workers,
two test threads, no incremental/debug artifacts and a task-owned target. The
sampled guard limits were 8 GiB process-group RSS, 12 GiB target usage, 20 GiB
minimum free space and 40 minutes per command, not OS-enforced quotas.
