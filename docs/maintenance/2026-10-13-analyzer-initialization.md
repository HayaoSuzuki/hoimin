# Analyzer initialization API (#849)

Work starts from main `e40621a` on `refactor/849-analyzer-init`.
The only remote is `origin`, with fetch and push URL
`https://github.com/HayaoSuzuki/hoimin.git`. The branch starts without an upstream;
it must never use `origin/main` as its push destination.

## Design and compatibility

Use `AnalyzerHandler::new(root) -> Self`. Remove `with_backend`, its ignored
resource arguments, and construction-only error handling. Keep lazy project
reads and candidate-store creation in their existing operations. Move the
resource backend into `ProcessHandler`; preserve the setup boundary order.

Under [the compatibility policy](../compatibility.md), this changes an internal
Rust source API. In-tree callers migrate with this change. External Rust callers
may exist: replace `new(root)?` / `new(root).unwrap()` with `new(root)`, or
`with_backend(root, backend, memory, processes)?` with `new(root)`. Resource
controls belong to process execution. Direct Rust callers can construct or mutate
an invalid `RunConfig`: a process count above `u32::MAX` no longer fails at the
removed shell argument conversion. Validate the configuration before calling the
shell; the machine still rejects overflow when constructing process limits.
CLI configuration and verified plans already validate this bound. The absence
of an independent Rust API stability guarantee does not establish the absence of external users.

The version floor stays at 0.4.0. CLI arguments/defaults/exit codes, reports,
plan/session formats, candidate identity, process limits, supported platforms and
package layout do not change. No saved-data or CLI migration is required.

### Design self-review passes

1. Read both constructors: all state fields start identically, no I/O occurs,
   and the only returned variant is `Ok`.
2. Trace backend ownership: shell clones the backend only to pass the original
   into the ignored analyzer argument. Keep the original in `ProcessHandler`.
3. Trace errors: source capability opening, decoding, analysis and candidate
   store operations remain fallible at their current execution points.
4. Trace validation: `RunLimits::try_from(&RawRunLimits)` and `validate_limits` reject
   process counts above `u32::MAX`. Remove the constructor argument conversion;
   retain core validation and process enforcement.
5. Compare alternatives: retaining a deprecated forwarding constructor would
   preserve the misleading parameters; introducing a factory adds an interface
   with no work to hide. Direct construction satisfies the issue's chosen API.

### Plan self-review passes

1. Search all Rust targets for construction calls, including unit tests and the
   Lean bounded-candidate oracle adapter. Compile all targets after migration.
2. Use the existing analyzer-handler and CLI library tests as the before/after
   behavioral baseline; preserve their assertions. Do not add syntax tests that
   duplicate the new signature.
3. Keep shell creation boundaries in order: roots, disk policy, workspace,
   backend, process, analyzer, report. Include rollback and setup tests.
4. Reconcile the current OKF analyzer contract and annotate the earlier
   capability-read design/plan as historical signatures. Record migration here.
5. Use one task-owned target, two build jobs/test threads, no incremental build
   or debug information; monitor process-group RSS (8 GiB), target size (12 GiB),
   free disk (20 GiB minimum), and elapsed time (40 minutes per command).
   Run workspace tests, all-target/all-feature compilation and strict Clippy,
   then clean only this target. Request an independent final source review.

## Verification scope

No new Lean model or proptest was added: this change removes constructor
wrappers without changing a state transition, algorithm or arithmetic rule.
Existing proptests run with the workspace tests. The existing bounded-candidate
Lean corpus adapter exercises the migrated constructor, but corpus replay does
not constitute a fresh Lean proof or proof of Rust implementation equivalence.

## Implementation self-review passes

1. Compare constructor fields against the base: only the `Ok` wrapper and lint
   exemptions disappear; all initial values remain identical. Add an interface
   comment stating when I/O and allocation occur.
2. Search production and test Rust sources: migrate all seven construction
   sites, including the oracle adapter; remove the unused resource import.
3. Compare shell setup statements: retain each boundary callback and the
   workspace/backend/process/analyzer/report order; eliminate the backend clone.
4. Inspect the unchanged `handle_with_cancellation`, `read_source`, and
   `analyze_and_store` bodies: cancellation, store creation/error mapping,
   capability caching, cleanup and request execution are untouched.
5. Review the entire diff against compatibility and design intent: no new
   abstraction, dependency, configuration, lint exemption or data-format change.
   Historical signature promises now point to this migration; OKF metadata and
   earlier source provenance remain intact.

## Scoped philosophy assessment

For the constructor boundary, seven of eight diagnostics pass (8.75/10).
Responsibility, interface depth, caller independence from resource details,
interface comments, design review, ownership of initialization decisions, and
investment in simplification pass. The remaining diagnostic is understanding
module boundaries without reading implementations: the wider analyzer/shell
coordination still requires implementation reading. A module-level ownership and
lifecycle contract could address that in a separate change. Adding a factory or
more wrappers here would increase interface cost without hiding useful work.

## Independent review

A separate reviewer inspected the production/test diff, constructor state,
backend clone/drop behavior, setup boundaries, core validation and documentation
against `e40621a`. The reviewer found an incorrect validation function name in
this record; it was checked against `config.rs` and corrected. The reviewer
rechecked that correction and the historical plan formatting and reported no
remaining actionable findings. The invalid internal configuration caveat above
came from that review. The reviewer did not execute builds or tests.

## Executed checks

Environment: macOS aarch64, pinned Rust 1.99.0. Commands used
`CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and the absolute
worktree-local `target` as `CARGO_TARGET_DIR`.

- Before the change, `cargo test --locked -p hoimin-cli --lib --test
  analyzer_handler --test lean_bounded_candidate_discovery_oracle --no-fail-fast`
  passed 828 library tests (18 ignored), 41 handler tests and four oracle tests.
- After the change, `cargo test --locked --workspace --no-fail-fast` passed
  2,837 tests, with zero failures and 29 ignored tests across 149 top-level
  result groups. Two subprocess runs each repeat one library test and are
  excluded from that count. The guarded command took 329.8 seconds and sampled
  a maximum process-group RSS of 1,870,856,192 bytes.
- Workspace execution includes the existing candidate-store Unicode/cursor
  proptest, arbitrary-input analyzer properties, source-read failures, in-flight
  cancellation, handler reuse after depth failure,
  `run_e2e::shell_context_construction_performs_no_project_io`, and rollback at
  each setup boundary.
- Rust workspace/vendor formatting and `git diff --check` pass. OKF YAML/type
  and reserved-file checks pass for 44 concepts; the added source/footnote and
  affected local links resolve. This checks structure and the changed claims,
  not historical evidence throughout the catalog.
- Strict `tools/docs_site.py` build and `tests/test_docs_site.py` pass (16 tests).
  The shared Python environment lacked Markdown; a separate `/tmp` environment
  installed the frozen `docs` dependency group. The builder requires the
  worktree's `site` directory, which was used for the successful run.

Initial resource-monitor attempts failed because sandboxed `ps` was unavailable
and because `du` observed a compiler temporary file disappearing. The first
unmonitored process group was stopped; a later test invocation finished before
the monitor could report its exit. After correcting monitoring, the complete
baseline was rerun with an observed exit code of zero. These early attempts do
not establish a monitored memory bound. Subsequent checks use the working
monitor; RSS and disk sampling every two seconds are guard observations, not
OS-enforced peak allocation limits.

The following guarded checks also exited zero:

- `cargo check --locked --workspace --all-targets --all-features`
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`
- `cargo test --locked --workspace --all-features --lib --no-fail-fast`:
  828 CLI and 35 core tests passed; 20 tests were ignored. This covers library
  tests with contracts, not every integration target with contracts enabled.

Their sampled process-group RSS maxima were 878,460,928; 1,045,757,952;
2,195,456; and 1,509,965,824 bytes respectively. Short commands may allocate and
exit between samples. The target occupied about 4.2 GiB before cleanup.
Native Linux/Windows execution and a fresh standalone Lean proof were not run.

### Verification self-review passes

1. Read command exit codes and result groups, exclude repeated subprocess test
   results, and report ignored cases separately. Baseline and final behavioral
   checks succeed; no red test is omitted from the result.
2. Match preserved behaviors to successful existing tests: no-project-I/O
   construction, source-read failure, spool replay/properties, cancellation,
   reuse after failure, and setup rollback. Store creation and error mapping
   remain outside construction and unchanged in the diff; no new failure
   injection or exhaustive store-failure proof is claimed.
3. Check the complete Rust call inventory and compile every target/feature.
   Strict Clippy and formatting pass without adding lint exclusions.
4. Recheck compatibility against the diff: core resource validation, process
   controls, manifests and persistence schemas have no changes. Document the
   delayed rejection of manually invalid internal configurations and distinguish
   source compatibility from the absence of external consumers.
5. Recheck OKF source attribution, historical design annotations, strict site
   rendering, and the reviewer correction. Verify the dedicated branch has no
   upstream to main; restrict cleanup to its own target. Sampling limits and
   unrun platforms/proofs remain explicit.

Cleanup: after confirming that the absolute task-owned target was not a symlink,
`cargo clean --target-dir <worktree>/target` removed 5,157 files (4.2 GiB).
The ordinary checkout's target and unrelated caches were not cleaned.

The existing commit hook reran both formatting checks and both Clippy checks;
all four passed. A second cleanup removed the hook-generated target. The final
independent evidence review confirmed baseline, workspace and contracts test
counts and successful compiler/lint logs. Monitor measurements above come from
this session's command output; the reviewer checked the guard configuration,
but did not independently verify the measured RSS/elapsed values.
