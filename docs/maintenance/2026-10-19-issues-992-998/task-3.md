# Task 3: native disk-meter names (#993)

Disk measurement now derives bounded lossy diagnostic paths separately from
native entry names. Unix stat/open operations still use the enumerated CStr;
Windows open operations still use the enumerated OsString and its original
UTF-16 code units. No entry is skipped because its name is not Unicode.
The existing diagnostic-detail truncator bounds display paths to 4 KiB on a
character boundary. The display string is never an I/O argument in production.

The existing no-follow flags, reparse-point rejection, mount/device and
opened-entry identity checks, hard-link accounting, depth/entry/time bounds,
and directory-handle limits are unchanged.

## Requirements self-review (five passes)

1. Read #993 and the accepted task brief: native file names must be counted,
   rather than bypassed, with no public API or CLI setting added.
2. Distinguished Linux regular-file, nested-directory and symlink acceptance;
   each has a separate test and a literal independent byte/entry expectation.
3. Traced the public CLI acceptance condition: successful baseline completion
   must proceed through restoration to an actual mutant, preserve originals,
   and remove owned directories, rather than merely avoid an error string.
4. Checked Windows scope: original OsString code units remain authoritative;
   the native surrogate test runs only if fixture creation succeeds. Recognized
   filesystem/name rejection is reported explicitly; unexpected errors fail.
5. Checked platform evidence: APFS rejects Linux invalid-byte fixtures before
   the meter. Host display tests are evidence for conversion, not Linux native
   filesystem acceptance. Actual Linux and Windows runs remain CI work.

## Design self-review (five passes)

1. Kept the existing meter API and walker structure. A single private helper
   renders names for both walkers and hides the conversion/truncation policy.
2. Followed every subsequent filesystem operation after conversion: all use
   native names and the enumerating directory capability, never display text.
3. Reused the existing diagnostic-detail byte bound and UTF-8-safe truncator;
   no new diagnostic knob, unbounded retained display path, or dependency.
4. Reviewed lossy-display collisions: they cannot merge files, directories or
   file identities because accounting and traversal never use rendered paths.
5. Kept test-only display hooks intact for existing race/deadline tests. Native
   tests live in a focused disk submodule; the public CLI test shares lifecycle
   setup with a host-compatible positive control.

## Implementation self-review (five passes)

1. Read the Unix walker diff against bf24cfb5: only display construction changes;
   statat still uses SYMLINK_NOFOLLOW and original CStr, openat2 still enforces
   BENEATH/NO_SYMLINKS/NO_XDEV, and non-Linux openat retains NOFOLLOW.
2. Read the Windows walker diff: original names still reach regular/entry/
   directory open functions, while file-ID and reparse-point checks are intact.
3. Traced nested traversal, identity deduplication, checked byte accumulation,
   MAX_TREE_DEPTH, MAX_TREE_ENTRIES, deadline checks and handle observations;
   all retain their existing ordering and authority.
4. Checked truncation at a multibyte boundary and parent-path accumulation.
   Display truncation affects only diagnostics; native child enumeration stays
   relative to opened capabilities and is unaffected by truncated parents.
5. Inspected final diff scope, formatting and dependencies. No unrelated source
   changes, new runtime dependencies or additional meter settings were added.

## Tests self-review (five passes)

1. Verified RED before the fix: extracted the existing strict conversion into
   the helper, then observed both invalid-UTF-8 rejection and oversized display
   failures. No filesystem acceptance assumption was needed on this host.
2. Checked AAA and one scenario per function. Linux file, directory and symlink
   cases are independent, and all filesystem fixtures use actual native names.
3. Checked independent oracles: 5 bytes/1 entry for the file, 7 bytes/3 entries
   for nesting, and 0 bytes/1 entry for the external-directory symlink. Following
   the symlink would count the external 1 KiB payload and fail the test.
4. Checked the CLI oracle: before each command it asserts the generated path
   is absent, writes exactly one byte, records baseline/mutant phase externally,
   and runs the boolean assertion. Rust checks one killed mutant, complete
   report, unchanged original, no generated original entries, and removed roots
   plus wrappers. The host-compatible control executes this same real CLI path.
5. Checked platform gates and existing regressions. Linux native cases cannot
   be accidentally reported as host execution. Windows has both a no-filesystem
   surrogate display test and an accepted-name real meter test. Existing disk
   races, deadline/depth/overflow/accounting and workspace-reset tests pass here.

## TDD and targeted verification

Every Cargo command used these environment values:

```sh
CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target
CARGO_BUILD_JOBS=2
CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0
RUST_TEST_THREADS=2
```

- RED: `cargo test --offline -p hoimin-cli --lib workspace::disk::native_name_tests -- --nocapture`
  failed as intended, 0 passed / 2 failed. Invalid UTF-8 returned
  `non-UTF-8 workspace entry`; the deep display exceeded 4096 bytes.
  Log: `/tmp/hoimin-issues-992-998/task-3-red.log`.
- GREEN: `cargo test --offline -p hoimin-cli --lib workspace::disk -- --nocapture`
  passed 26 tests, 0 failed, 1 existing ignored subprocess fixture
  (`directory_handle_bound_child`). Its parent test invokes the fixture.
  Log: `/tmp/hoimin-issues-992-998/task-3-disk-green.log`.
- `cargo test --offline -p hoimin-cli --test run_e2e baseline_generated -- --nocapture`
  passed 1 host-compatible public CLI control, 0 failed. The Linux counterpart
  is compiled only on Linux. Log: `/tmp/hoimin-issues-992-998/task-3-cli.log`.
- `cargo test --offline -p hoimin-cli --test workspace_handler`
  passed 36 tests, 0 failed. Log: `/tmp/hoimin-issues-992-998/task-3-workspace.log`.
- `cargo clippy --offline -p hoimin-cli --lib --test run_e2e --test workspace_handler -- -D warnings`
  passed without warnings. Log: `/tmp/hoimin-issues-992-998/task-3-clippy.log`.
- `cargo fmt --all -- --check` and `git diff --check` passed.

Full workspace validation is reserved for Task 8 by the accepted resource
policy. Linux CI must execute the three native meter tests and native public
CLI test. The manually dispatched non-Linux CI must execute the Windows tests;
if Windows rejects the name, the native test prints a SKIP reason and provides
no accepted-surrogate I/O evidence. No Linux or Windows native execution is
claimed from this macOS/APFS host.

Files changed: `crates/hoimin-cli/src/workspace/disk.rs`, its new
`disk/native_name_tests.rs`, `crates/hoimin-cli/tests/run_e2e.rs`, and this
maintenance record. The existing large walker and E2E files were kept within
scope; no unrelated restructuring was attempted.
