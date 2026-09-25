# Issue 607 review and verification record

Design and implementation plan were committed as `47e9ac1` before code or test changes. Three separate self-review passes for each are recorded in those artifacts. The implementation follows the user's authorization through publication; the coordinator owns CI polling and merging.

## Implementation self-review

1. Compared the old and new record equality paths. Both still use `record_map`, including its existing duplicate-key behavior, and equal maps still return success. The saved/current classification is correct for removed, added and changed hashes, with unchanged keys excluded.
2. Traced all shared preparation callers and earlier error returns. Neither source/fingerprint acquisition nor candidate validation moves; source comparison remains first. The formatter consumes already available maps and adds no reads, hashing, parsing or test execution. Error variants and prefixes are unchanged.
3. Reviewed ordering, allocation and output safety. The extra change map borrows keys, globally sorts all change kinds and formats only ten paths. Its full length supplies the exact omitted count. String Debug quoting escapes controls without changing the root-relative path. Detail allocation scales with changed path count; the requirement bounds displayed entries, not comparison memory. README states the same ten-path contract. CI clippy and both format checks passed without exemptions.

## Test self-review

1. Checked literal classification/escape expectations independently of the formatter. The mixed fixture interleaves modified, removed and added paths and contains an unchanged path; reversing saved records must not alter the message. Newline, carriage return, tab, quote, backslash and ESC are escaped and the diagnostic stays one line. Empty and reordered-equal maps succeed.
2. Checked limits at one, exactly ten, eleven and thirteen differences, with reversed input order and both empty-side orientations. Assertions require the exact first ten sorted paths, no omission suffix at ten, and accurate one/three omitted counts above the limit. These tests failed against the old generic diagnostic.
3. Checked subprocess reachability and rejection evidence. Six isolated source/fingerprint change fixtures retain another glob match so removal reaches record comparison. Each compares normal and dry-run stderr, requires status 2 and empty stdout, and checks an external test marker remains absent. Missing exact input, non-regular input (a directory), and missing symbol retain earlier resolution errors; simultaneous source/fingerprint changes preserve source priority. This is not an OS permission-denial test. Both public regressions failed on the old missing-details behavior and passed with the implementation.

## Verification evidence

- RED: `cargo test -p hoimin-cli stale_ -- --test-threads=1`: 4 existing tests passed and 2 new comparison tests failed with the old generic message; exit 101.
- RED: `cargo test -p hoimin-cli --test plan verify_stale_ -- --test-threads=1`: 2 failed with the expected missing path suffix; exit 101. Earlier resolution cases passed before the source-priority assertion reached the expected failure.
- GREEN: `cargo test -p hoimin-cli --lib stale_record_details -- --test-threads=1`: 2 passed; exit 0.
- GREEN: `cargo test -p hoimin-cli --test plan verify_stale_ -- --test-threads=1`: 2 passed; exit 0, covering all six stale cases and four precedence/error cases in both modes.
- `cargo test --workspace -- --test-threads=1` on base `efe35318`: exit 0; 2302 passed, 0 failed, 22 ignored across 98 test/doc-test binaries.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo clippy --locked -p littrs-ruff-python-parser --lib --no-deps -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`, `cargo fmt --manifest-path vendor/ruff_python_parser/Cargo.toml -- --check` and `git diff --check`: exit 0.

Cargo uses the assigned `target/batch-verify` cache, one build job, disabled dev/test debug information and disabled incremental builds. Core and CLI artifacts were cleaned on entering this worktree. No Lean or Python code/test changes were needed.

## Independent review

The coordinating agent independently reviewed the production diff and all four new tests and reported no blockers. Review covered exact map equality, change labels, borrowed keys, sorting/cap/count, escaping, preserved error prefixes, six public cases in both modes, early resolution errors and source precedence. Avoiding collection of all changed keys was noted as an optional future optimization, not an acceptance requirement.
