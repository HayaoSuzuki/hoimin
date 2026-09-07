# Issue #354: manifest-backed run fingerprints

## Cause and scope

The shell built the workspace manifest, created and verified the disk snapshot,
and then reopened every selected target from the live project root to construct
the run fingerprint. A target changed in that interval could therefore give the
published run a fingerprint for bytes that were not in its snapshot. The second
read and BLAKE3 calculation also duplicated work already performed while
building the manifest.

Plan verification still reads current sources and compares them with the saved
plan before starting a run. Runtime preflight still validates fingerprint
inputs, copies from the initial manifest, and scans the original root again to
detect snapshot-time changes. This change does not weaken or replace those
checks.

## Implementation

Fingerprint construction now occurs in the validated-preflight callback and
uses each selected target's BLAKE3 value from the workspace manifest. The
existing target order, normalized target paths, fingerprint schema, resource
mode, and configuration input are unchanged. A selected target absent from the
manifest fails the same preflight effect with
`fingerprint.source.manifest_missing` and names the target path.

The completed event receives the prepared fingerprint only after workspace
preflight succeeds, so a snapshot or final root-verification failure cannot
publish it. The existing preflight integration test continues to require
`Some(expected_fingerprint)` on the real effect path.

## Regression evidence

The original focused preflight test passed before the change. Three regression
tests then built a manifest and exercised target modification, target removal,
and a selected target missing from the manifest.

For the behavioral RED run, the new helper signature and callback wiring were
kept while its body was temporarily restored to the former live-root read. All
three tests failed for the intended reasons: changed bytes produced a different
digest, a removed target produced `fingerprint.source.read`, and a
manifest-absent target incorrectly returned a fingerprint. Restoring manifest
lookup made all three tests pass.

## Verification

Commands ran on macOS with the isolated Cargo target directory
`/private/tmp/hoimin-issue-356-target`:

```sh
cargo test -p hoimin-cli manifest_fingerprint --lib
cargo test -p hoimin-cli shell::tests::preflight_filesystem_work_does_not_block_the_async_runtime --lib -- --exact
cargo test -p hoimin-cli fingerprint_recheck --lib
cargo test --workspace --all-features --quiet
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

The manifest regressions passed 3 tests, the real preflight fingerprint path
passed 1 test, and the fingerprint-input recheck slice passed 5 tests. The full
workspace suite exited 0; its main CLI library run passed 530 tests with 9
ignored, and every integration, core, and documentation test binary completed
without failure. Formatting, whitespace validation, and workspace Clippy across
all targets and features each exited 0 with warnings denied.

The full workspace suite used the main checkout's Python environment through a
temporary worktree `.venv` symlink. The symlink was removed immediately after
the test command. This cross-platform shell change was tested on macOS; Linux
and Windows execution were not run.
