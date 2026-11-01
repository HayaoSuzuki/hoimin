# Task 1 — shared pure Python analysis (#996)

`hoimin-python` owns the existing pure analyzer, candidate payload leases,
exception index, visitor and private semantic/oracle tests. CLI source reading,
project acquisition/fingerprints, conversion and spools remain in `hoimin-cli`.
CLI, dedicated heap/corpus tests and fuzz now use Cargo imports. Private AST
collection is not exported; CLI snapshot tests use normal analysis requests.

The normal graph does not enable `test-support`; CLI dev-dependencies enable
only logical payload observations needed by existing orchestration tests/probes.
No always-on counters, uncharged result adapter, compatibility namespace or
additional budget authority was introduced. Existing 16 MiB accounting,
cancellation and candidate ordering are unchanged. Pure payload cases moved;
all 33 named existing payload tests remain registered in their owning crate.

Both Cargo roots resolve the same patched parser. Fuzz retains its workspace-root
patch. CLI/fuzz no longer declare the pure implementation's numeric/AST libraries;
CLI retains dev parser/AST dependencies for independent reparsing and
small-stack parser assertions. The
normal Python crate graph has no CLI, Tokio, SQLite or tempfile dependency.
The parser's existing `stacker -> libc` dependency remains. Release version edits
include the new workspace package; the license policy scopes its ELv2 exception.

## Evidence

All Cargo commands and commit hooks use the task-owned target, jobs=2,
incremental=0, dev/test debug=0 and RUST_TEST_THREADS=2. Logs reside under
`/tmp/hoimin-issues-992-998/task-1-*`.

- API RED: `cargo test -p hoimin-cli --test python_api` failed because
  `hoimin_python` did not exist. The test observes the retained candidate's lease
  through subsequent reservation admission, without test counters.
- Pure unit GREEN: `cargo test --offline -p hoimin-python`: 363 passed, 3 ignored.
- CLI analyzer filter: 43 passed, 10 existing release fixture probes ignored.
- Dedicated consumers: public API 1, heap/analyzer 22, valid Python corpus 7,
  exception hierarchy integration 2, handler integration 30, parser depth 10 passed.
- Before/after `--list`: exactly the same 342 private `rust::*` test names,
  1026 registrations before and 342 after. All 19 moved performance gate names
  resolve exactly once. The 3 visitor tests similarly changed from 9 to 3
  registrations. Payload test name multiset is preserved.
- Release RED: existing stable/dev version cases failed because the new package
  stayed at 0.13.0. GREEN: all 53 release tests passed with task-owned UV cache.
- Performance/boundary tooling tests: 157 passed; registry check: 29 shapes valid.
- Focused Rust clippy with all features passed; formatting passed.
- Seven fuzz binaries built with pinned nightly-2026-10-08, ASan and `--dev`
  into a separate task-owned fuzz target. All 11 existing Python seeds replayed
  individually; a seed-based 200-run bounded fuzz execution passed its existing
  cancellation, ordering, candidate validation and prefix assertions.
- cargo-deny 0.20.2: both root and fuzz licenses/sources passed with warnings denied.
- Maturin sdist succeeded; archive includes the new crate and stdlib-name data.

## Self-review passes

Requirements 1: traced the three source-import consumers; all now import the
same production types and functions, with no reconstructed private namespace.
Requirements 2: followed leases through candidate return, conversion scratch,
spool overlap and detached worker paths; existing CLI ownership tests remain.
Requirements 3: measured duplicate registrations before moving code; compared
exact names after, preserving all 342 private cases and dedicated heap/corpus tests.
Requirements 4: bounded scope to Python analysis; no language/plugin interface,
CLI knob, filesystem migration or exception lookup algorithm was introduced.
Requirements 5: audited packaging consumers; parent review found release.py's
package-name enumeration, now covered by stable/dev failing-then-passing cases.

Design 1: chose a private rust/types/visitor module layout with root exports;
kept the AST visitor and exception collect operation inaccessible to consumers.
Design 2: retained Charged and non-Clone PayloadLease rather than replacing
results with naked candidates; output field ownership remains unchanged.
Design 3: split pure payload tests from filesystem/orchestration tests; this
avoids exporting CandidatePrefix and preserves existing failure observations.
Design 4: scoped payload counters to cfg(test) or explicit test-support;
normal CLI feature-tree inspection confirms the feature is absent.
Design 5: retained shared test-only oracle fixture inclusion without importing
production source; historical revision/hash-pinned knowledge evidence stays
unchanged, while development commands and performance selectors are current.

Implementation 1: inspected moved analyzer diffs: import paths, visibility,
documentation and test configuration only; candidate algorithms are unchanged.
Implementation 2: compiler found remaining private collect callers in project
snapshot tests; migrated both to AnalyzeRequest instead of widening the API.
Implementation 3: removed CLI type/function reexports after checking typed
callers; plan, target resolution and handler tests import hoimin-python directly.
Implementation 4: checked both metadata graphs resolve one identical local parser
patch; removed runtime AST/numeric dependencies without changing versions. The
workspace clippy hook found analysis_depth still requires AST types; retained
that dev-dependency and verified its dedicated suite.
Implementation 5: reviewed public functions with clippy; documented error/poison
contracts, kept private output statistics private and added must_use where required.

Tests 1: new lease test uses a hand-counted 13-byte candidate, a 32-byte budget,
and an admission attempt that fails while retained and succeeds after drop.
Tests 2: pure unit suite preserves independent semantic/oracle expectations and
three existing ignored benchmarks; no relocated private test runs in consumers.
Tests 3: retained allocator child-process and valid-Python reparsing/corpus checks;
all dedicated suites passed against Cargo API imports.
Tests 4: retained CLI conversion, cross-file, spool and detached-worker payload
observations using dev-only instrumentation; 33 payload test names are preserved.
Tests 5: verified runnable performance selectors, assertion-bearing fuzz seed
replays, license policy, sdist inclusion and stable/dev release lockfile changes.

## Limits

This task runs focused suites; the root integration task owns workspace-wide
and cross-platform verification. Existing release-only fixture probes were not
run because their pinned /tmp audit fixtures are unavailable. No compile-time or
RSS improvement is claimed. Fuzz used bounded development builds, not a long
campaign; its 200-run log includes an unavailable external-symbolizer warning,
with no sanitizer failure. Initial online Cargo resolution failed under sandbox
DNS, then offline resolution succeeded. Initial full release tests had two UV
cache permission errors; the same suite passed using a task-owned cache.
