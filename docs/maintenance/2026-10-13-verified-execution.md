# Verified execution entry points (#850)

This change starts from main `63f9d62` on `refactor/850-verified-execution`.
The fetch and push remote is `https://github.com/HayaoSuzuki/hoimin.git`;
the branch starts without tracking main.

## Design

Call `prepare_verify` or `prepare_verify_selection`, inspect the resulting
`VerifiedPlan` through shared references, optionally call `with_metrics`, and
consume `execute` for borrowed writers. CLI dispatch keeps the existing owned
writer path through `run_owned_verified`. Both paths pass the checked plan to
`run_verified`, which unpacks configuration, selection, provenance and protected
inputs together. No caller needs to reconstruct those execution arguments.

Remove these wrappers:

| API | Visibility before removal | In-tree users |
| --- | --- | --- |
| `run_selected_loop` | public | Two missing-candidate integration tests via one helper |
| `run_selected_loop_with_fingerprint_inputs` | public, doc-hidden | `run_selected_loop` |
| `run_ordered_selected_loop` | public | None |
| `run_ordered_selected_loop_with_fingerprint_inputs` | crate-private | `run_ordered_selected_loop` |

Keep `run_loop` and `run_loop_with_control` for ordinary runs, and keep the
private execution functions. Move the two runtime missing-candidate tests into
`shell::tests`, where they can inject invalid selection into `run_loop_prepared`
without publishing an unchecked API. They still execute real analysis, candidate
spooling, baseline execution and scheduler/report handling. A platform-native
successful command replaces the Python assertion because candidate rejection is
the behavior under test. Public plan tests continue to reject unknown IDs during
preparation; those tests alone cannot exercise runtime spool exhaustion.

### Design self-review passes

1. Trace the existing production path: both owned CLI dispatch and borrowed
   `VerifiedPlan::execute` already require the complete checked plan.
2. Search each selected/ordered wrapper: the only external in-tree call is a
   helper for two negative runtime tests; ordered wrappers have no independent
   production consumer.
3. Compare alternatives: retaining deprecated wrappers preserves unchecked
   argument combinations; changing their signatures duplicates `execute`.
   Removing them narrows the API without a new scheduler trait or options type.
4. Trace all handoff fields: explicit/ranked IDs, provenance, manifest paths,
   fingerprint-copy inputs and metrics destination remain in `VerifiedPlan`.
5. Distinguish preparation rejection from runtime missing-candidate rejection;
   preserve both layers and the owned/borrowed writer distinction.

### Plan self-review passes

1. Run existing plan, metrics-destination and run E2E tests before editing;
   include compile-fail contracts and all-target compilation in final checks.
2. Move runtime negative tests into the shell test module before removing the
   public wrappers. Keep real filesystem, analyzer and scheduler execution.
3. Check test sensitivity by temporarily disabling only the missing-selection
   rejection, then restore the original source before final verification.
4. Update the current plan/verify OKF contract and development guide, and link
   the previous ownership record to this follow-up. Record API migration here.
5. Use a task-owned target, two build jobs/test threads, disabled incremental
   compilation/debug information, and sampled resource guards: 8 GiB group RSS,
   12 GiB target, 20 GiB minimum free disk, 40 minutes per command. Run checks
   sequentially, request independent review, commit, then clean this target.

## Compatibility and verification scope

Under [the compatibility policy](../compatibility.md), these internal Rust APIs
have no independent stability guarantee. Repository search does not establish
that external Rust users are absent. Such callers should prepare a saved plan
through the existing preparation APIs and consume `VerifiedPlan::execute`.
There is no replacement public API for unchecked selected execution.

The version floor stays 0.4.0. CLI behavior, explicit membership, ranked order,
selection provenance, metrics-only overrides, protected inputs, and
plan/session/report formats remain unchanged. No saved-data migration is needed.
The existing `VerifiedPlan` field/mutable-reference compile-fail contracts remain.

No new Lean model or proptest is needed for removing wrappers: scheduler state
transitions, selection algorithms and ownership payloads do not change. Existing
properties and Lean corpus adapters run with the workspace tests. Corpus replay
is not a fresh Lean proof or a proof of Rust/OS equivalence.

## Implementation self-review passes

1. Compare the deletion boundary: remove exactly four wrappers; ordinary run,
   checked-plan handoff, scheduler execution and output delivery bodies remain.
2. Recheck call sites after deletion: no selected/ordered wrapper references
   remain in Rust sources; the runtime test helper now calls a private function
   from `shell::tests`.
3. Compare negative-test assertions: both still require the missing ID in the
   returned error and reject a complete report. Nonempty and empty source
   fixtures remain distinct. The fixtures use the existing native successful
   baseline command and isolated managed roots.
4. Disable the core missing-ID guard temporarily: both moved tests fail because
   execution returns `Ok(0)`. Restore the original file byte-for-byte before
   workspace testing. No core state-machine change is retained.
5. Check API comments, migration and scope: the shell module names the checked
   execution entry points; no configuration type, scheduler trait, dependency,
   lint exception, saved-format change or mutable plan accessor was introduced.

## Scoped philosophy assessment

The selected-execution boundary passes all eight diagnostics (10/10): each
retained entry point has one purpose; preparation and execution hide validation
and scheduling; callers depend on a checked value rather than its internal
fields; comments name the handoff and writer contracts; design is part of the
review; the plan owns its execution facts; module documentation explains the
boundary; and this change invests in removing redundant interfaces. No diagnostic
row remains unmet for this boundary. This is not a score for the entire shell or
repository, nor a claim that unrelated scheduler complexity has been removed.

## Verification evidence

Before source changes, plan (99 passed, one ignored), metrics destinations
(26 passed) and run E2E (74 passed) tests succeeded. The first command used the
nonexistent singular target name `metrics_destination` and exited before running
tests; the corrected command used `metrics_destinations`.

The two relocated tests passed before wrapper removal. A temporary removal of
`RunState::ensure_selected_candidates_discovered`'s missing-ID rejection made
both fail with `unwrap_err` receiving `Ok(0)`; the original core source was
restored byte-for-byte. This sensitivity probe is an intentional failing run,
not a final test failure or a new formal proof.

A byte-for-byte comparison against the base confirms that `VerifiedPlan`, the
core machine, and shell's owned/borrowed checked-plan handoff, execution loop and
shutdown bodies are unchanged. Old selected API names remain in the July audit
as descriptions of that audit's revision; this change does not rewrite its
historical findings.

Workspace/vendor formatting, whitespace checks, a strict documentation-site
build and 16 documentation tests passed. YAML/type/reserved-file checks passed
for 44 OKF concepts; the new source reference and footnote resolve. The content
review covers the updated execution contract, not every historical source in the
catalog.

## Independent review

A separate reviewer read the complete diff against `63f9d62`, checked the
removed API inventory and retained handoff, and traced the relocated negative
tests through empty/nonempty spool rejection. The reviewer found no actionable
issue and confirmed migration/compatibility wording. This was a read-only source
review, not an independent rerun of builds or the sensitivity probe.

`cargo test --locked --workspace --no-fail-fast` passed 2,837 tests with zero
failures and 29 ignored tests across 149 top-level result groups. Two subprocess
runs each repeated one library test and are excluded from that count. The moved
runtime rejection tests both pass, and both `VerifiedPlan` compile-fail contracts
pass. Library test count increased by two and run E2E decreased by two.

The workspace run includes explicit candidate execution, strict/diverse ranked
execution, unknown-ID preparation rejection, manifest destination protection for
owned and borrowed writers, late fingerprint-input deletion, and metrics-only
state preservation. Existing proptests and Lean corpus adapters also run here.

Environment: macOS aarch64, pinned Rust 1.99.0, `CARGO_BUILD_JOBS=2`,
`RUST_TEST_THREADS=2`, `CARGO_INCREMENTAL=0`, and dev/test debug information off.
`CARGO_TARGET_DIR` is the absolute worktree-local target. The workspace command
completed in 334.0 seconds; sampled maximum process-group RSS was 1,800,749,056
bytes. The monitor samples RSS and disk every two seconds; these are observations
and termination thresholds, not OS-enforced peak-allocation limits. Native Linux/
Windows execution and a new standalone Lean proof were not run.

These guarded checks also exited zero:

- `cargo check --locked --workspace --all-targets --all-features`
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`
- `cargo test --locked --workspace --all-features --lib --no-fail-fast`:
  830 CLI and 35 core tests passed, with 20 ignored. This runs contracts-enabled
  libraries, not the entire contracts-enabled integration suite.

The target occupied about 4.2 GiB after validation. Monitor summaries and Cargo
logs were saved separately under `/tmp/issue850-<check>.summary` and `.log` for
this session; these local artifacts are not repository fixtures.

### Verification self-review passes

1. Check baseline, moved tests and final workspace results separately. Count
   top-level tests without duplicate subprocess runs; record the intentional
   sensitivity failures and the original mistyped target instead of treating
   either as a passing check.
2. Match explicit membership, strict/diverse ranked order, provenance, protected
   inputs and metrics override to successful existing tests. Check both owned
   and borrowed manifest-collision tests, and late fingerprint deletion.
3. Confirm both compile-fail contracts still reject field and mutable-reference
   writes. Compile all targets/features and run strict Clippy without adding a
   lint exemption. Recheck that the core sensitivity edit was fully restored.
4. Reconcile documentation with implementation: list each removed API and caller,
   keep external-user uncertainty explicit, and preserve the version floor and
   CLI/plan/session/report contracts. Check OKF metadata and new source links.
5. Confirm independent source review has no findings, test counts account for the
   two relocated cases, and resource/OS/proof limits are stated. Verify the
   branch/remote configuration and confine final cleanup to this task's target.

The commit hook reran workspace/vendor formatting and both Clippy checks; all
four passed. After the hook completed, the absolute task-owned target was
checked against the expected path and confirmed not to be a symlink.
`cargo clean --target-dir <worktree>/target` removed 5,157 files (4.2 GiB).
No ordinary-checkout target or unrelated cache was cleaned.

The final independent evidence review confirmed the saved workspace/contracts
counts, the two intentional sensitivity failures followed by passing final tests,
successful compiler/lint summaries, and the reported workspace resource figures.
It found no actionable issue. Formatting, documentation/OKF checks and final
cleanup remain observations from the implementing session. A final source-name
check corrected this record's guard owner to `RunState`.
