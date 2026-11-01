# Issue 606 review and verification record

Design and implementation plan were committed as `a7b51c2` before changes to code or tests. Their three separate self-review passes are recorded in the corresponding artifacts. Execution follows the user's authorization to continue through implementation and commits; independent review and publication belong to the coordinating agent.

## Implementation self-review

1. Traced both dispatch paths through `VerifiedPlan`, common verified execution, `ShellContext`, blocking preflight, and destination inspection. Both paths supply the protected entries before baseline; ordinary run initializes an empty list. Confirmed no serialized config/schema change and no inode-based alias rejection was introduced.
2. Compared common verified execution with the previous borrowed and owned runners. Found that directly reusing owned execution's `unwrap_or(2)` would discard borrowed caller errors. Fixed common runner to preserve `Err` for borrowed writers while retaining the owned report path's existing handling. Checked selected candidate order, verification metadata, and fingerprint inputs remain forwarded unchanged.
3. Reviewed errors and maintainability with clippy. New read/protection logic pushed preparation over the function-size threshold; reduced dispatch no longer needed its lint expectation. Extracted `read_manifest`, preserving the `plan.manifest.invalid` mapping, and removed the obsolete expectation. All-target/all-feature clippy then passed. Destination authorization and uncertainty warnings continue through the existing inspector/finalizer.

## Test self-review

1. Negative oracle: confirmed tests invoke the real binary, set CWD explicitly, assert the exact collision diagnostic and exit 2, compare original manifest bytes, and assert absence of the external marker. Removing manifest protection caused both owned CLI and borrowed-writer tests to fail with exit 1 instead of 2. Tests exercise normal and deliberately failing baseline commands, with top and explicit candidate selectors.
2. Positive oracle: separate hardlink/symlink entries must actually become parseable, validated metrics files while manifest bytes remain unchanged. New-file and existing-file cases exercise normal and failed-baseline execution, requiring the marker and executed counts 1/0 respectively. These tests passed before the fix and guard against overbroad alias rejection.
3. Filesystem and dispatch coverage: reviewed absolute/relative, dot/dot-dot, parent symlink, supplied input symlink and canonical referent, and native case-spelling cases. Case cases run only when lookup actually aliases the saved name; symlink cases are Unix-gated. Both public CLI selectors and borrowed dispatch selectors are covered. No mocked filesystem comparisons or implementation-derived expected values are used. Windows symlink creation is not exercised locally; existing platform entry-inspector coverage remains unchanged.

## Verification evidence

- RED: `cargo test -p hoimin-cli --test plan verify_metrics_manifest -- --test-threads=1`: 1 passed, 2 failed, exit 101. Both failures were the expected accepted-collision result (1 instead of 2); positive replacements passed.
- GREEN: same targeted command: 3 passed, exit 0. Cases include both selectors and baseline outcomes.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0 after the review corrections.
- `cargo test --workspace -- --test-threads=1`: exit 0; 2290 passed, 0 failed, 22 ignored across 95 test/doc-test binaries. The final implementation and all three new regressions are included.
- `cargo fmt --all -- --check` and `git diff --check`: exit 0.

Cargo uses one build job, disabled dev/test debug information, disabled incremental builds, and the coordinating agent's assigned cache (relocated from issue-631/target to repository target/batch-verify after the first GREEN run). No Lean or Python production/test changes were needed.

## Independent review

The Issue 609 implementer independently reviewed this diff while the full workspace suite ran. The coordinating agent relayed no blocking findings: specified and canonical input entries, safe alias rename behavior, common runner's borrowed/owned error propagation, and both selector regression paths were assessed. Concurrent hostile path replacement is not claimed as covered.
