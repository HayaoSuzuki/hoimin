# Issue 453 scoped discovery report

Base: `165a2d284a1af92eb02ffd214ba8c0070c2f3808`.

## Three-pass review record

### OKF

1. Provenance review replaced untracked markers with the exact design SHA-256 in both consuming OKF pages.
2. Contract review confirms the Japanese text names the exact-only activation rule, full-walk fallback, and malformed-path boundary.
3. Catalog review checked counts, table rows, source footnotes, local links, YAML, and reserved-file rules with the repository validator.

### Design

1. Scope review limited pruning to file/line-only selections because sources and symbols require enumeration.
2. Ignore review retained a root-origin walk and applies the same scope to normal and restoration passes.
3. Diagnostic review made the intentional unselected-malformed-path boundary explicit rather than claiming complete diagnostic identity.

### Plan

1. TDD review chose visit and retained-record counters instead of elapsed-time assertions.
2. Coverage review includes two requested paths and shared ancestors, not only a root-level file.
3. Evidence review separates deterministic operation counts from release timing.

### Implementation

1. Complexity review replaced linear selector scans and linear ancestor deduplication with platform equality keys in `BTreeSet`; construction is O(S·D log(S·D)) and each visited-entry membership check is O(log(S·D)).
2. Hot-path review found initial test counters imposed atomic increments in production; counter fields and increments are now compiled only for tests, leaving `DiscoveryStats` zero-sized in production.
3. Semantics review added native requested-path indexes so an unrepresentable selected path still reaches `collect` and keeps its portable-path diagnostic; both walks share the scope, while source/symbol selections deliberately retain the original full walk.

### Tests and measurement

1. The pre-change operation regression visited and collected the 1,000 unrelated files; the green implementation collects one record and invokes the filter only for the selected file and root-level unrelated directory boundary.
2. A 256-selector review added a counter assertion for one equality-key lookup per visited entry and exactly 256 retained records; paired Unix tests confirm selected malformed paths still fail while unrelated malformed paths do not.
3. Release five-sample medians were 115 µs for 2,000, 117.041 µs for 4,000, and 210.041 µs for 8,000 unrelated files; output remained the one requested file and timing is nonblocking.

### PR

1. Scope review confirms the diff is limited to issue #453 discovery/core-key code, tests, artifacts, and issue-specific OKF/index entries.
2. Claim review avoids constant-time language because root-level siblings are still enumerated and release timings are environment evidence only.
3. Publication review verified PR #533 is open from `perf/issue-453-scoped-discovery` into `main`, and its rendered body retains the diagnostic caveat, deterministic and release evidence, validation list, and `Closes #453`.

## Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test -p hoimin-cli --test target_handler`: 44 passed.
- `cargo test --workspace`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- Focused discovery tests: 3 passed, 1 ignored measurement.
- Python integration suite: 66 passed under Python 3.14.7.
- Release scoped-discovery measurement: passed.
- OKF validator: 16 pages passed.
