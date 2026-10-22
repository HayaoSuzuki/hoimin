# Issues #1081–#1083: implementation and evidence

Base: main `6ad22aaa87c6298f8c84e3a2c00cbd016cf0b7db` after PR #1080.
Implementation uses `audit/issues-1081-1082-1083`, the branch containing
[the audit](../../knowledge/audits/design-2026-10-22-post1080.md).
The audit and navigation are included with the implementation at the user's
request. Its source hashes and observations remain historical; this document
records subsequent execution evidence separately.

## Copy membership without full content manifests (#1081)

Prepare and preview previously built a full copy manifest only to test whether
fingerprint paths belonged to it. They now share a private membership operation
using the same selected-entry walker and capability-relative regular-file opens.
The membership result retains only requested matching paths. The shared walker
still transiently collects skipped-symlink diagnostics; no full content manifest
is built and no selected file bodies are read or hashed by this operation. Empty
fingerprint inputs return before opening a root or walking copy selection.

Actual copying/preflight still creates an independent content-hashed snapshot
and performs fingerprint freshness/ABA validation. Preparation is not a reusable
snapshot. Read failures or content growth/shrink formerly detected during the
unnecessary pass can instead be detected at actual preflight, as approved during
the audit. Dry-run does not perform copying and does not attest that every copied
asset's body was read successfully. Saved source/fingerprint/candidate validation
and selection order remain shared between prepare and preview.

The regression measures the actual manifest content read/hash points, excluding
fixture plan creation. Fixtures vary 0/1 fingerprint inputs and 1/4/16 unrelated
assets of 4/16/64 KiB; both prepare and preview are checked. Before the change,
the first zero-fingerprint fixture consumed and hashed 4,160 copy-manifest bytes,
then failed the zero-content-work assertion. Afterward all cases pass with zero
copy-manifest content bytes. These counters do **not** count all source I/O:
required source, candidate and fingerprint validation still performs reads.

Membership is compared with the old content-manifest intersection for ignore
rules, explicit include restoration, excludes, nested paths, directories,
missing paths, symlinks and Unix sockets. Literal file/tree exclusions and invalid
globs are checked at the shared helper. Native backslash rejection is compared
separately; raw non-UTF8 creation is tested on Linux because macOS APFS cannot
create that fixture. A Unix backslash asset demonstrates that empty fingerprints
skip the copy-selection walk. The native path policy is unchanged.

## Finite mutation reads (#1082)

Mutation reads compare the opened descriptor length with its manifest entry
before allocating or reading the body. The shared SnapshotReader accepts a
borrowed file while retaining the existing owned-file constructors and rewind
behavior. Its existing Read loop consumes at most the observed length plus a
one-byte growth probe and rejects truncation. A private typed length-change cause
maps only these failures to mutation hash mismatch; ordinary I/O errors remain
I/O errors. Reservation errors are fallible and also reported as I/O errors.

The same mutation descriptor remains owned through later writable-reopen checks.
Length rejection occurs before permission changes, truncation or writes. There
is no new fixed source cap, CLI option, duplicate read loop, whole mutated body,
extra normal-path hash or writer synchronization. Runtime contracts still keep
independent original bytes. The boundary does not detect every in-place rewrite
or guarantee arbitrary concurrent writer safety or bounded filesystem I/O time.

A deterministic append hook demonstrated 4,105 consumed bytes after a nine-byte
budget before the fix; the fixed path rejects after at most ten consumed bytes.
A truncation hook separately exercises short reads. A stable wrong-size worker
of 128 KiB required 131,154 peak requested Rust bytes before rejection in the
baseline and failed the <32 KiB bound. The fixed regression passes and verifies
unchanged worker/project bytes and read-only permissions. Exact successful peak
values, when printed, are allocator observations, not RSS or timing claims.
Existing 64/256/1024 KiB equal-length tests still require one source body in normal
builds, permitting independent original evidence in contracts builds.

## Borrowed identity checks (#1083)

The read-only plan identity loop and stored review-key validation now use the
existing CandidateIdentityRef. Its hash implementation, schema and normalization
are the same as the former owned projections. No public identity API or new type
is introduced. Owned stored keys, full descriptor equality and source validation
retain their separate responsibilities.

The regressions measure actual read_for_review and ReviewStore candidate reads,
not only an isolated identity helper. Fixtures and SQLite writes are outside the
allocation windows; 4/16/64 KiB payloads stay below current artifact/store/record
limits. Plan fields, public list output and corrupted-identity rejection are
checked outside the measurement. Existing JSON/human, path/hash normalization
and other identity boundary tests remain required.

| Measuring window / 64 KiB payload | Before peak requested Rust heap | After peak requested Rust heap |
| --- | ---: | ---: |
| Read-only plan, large original and replacement | 592,574 | 461,198 |
| Stored key, large replacement | 197,367 | 132,659 |

These are measuring-thread Rust allocations, excluding SQLite's C heap. Plan JSON
buffers, decoded candidates, database row strings, ID generation and normalization
still allocate. The change removes temporary projections rather than a whole
persistent history. There is no claimed RSS or end-to-end latency percentage.

## Verification and resources

Cargo commands are serialized with locked dependencies, two build jobs, disabled
development debug information and disabled incremental compilation, using this
worktree's target. Python tools use the existing shared environment without
synchronization. Generated build/site outputs are removed after verification;
audit, execution logs and the worktree remain.

Focused regressions: nine CLI library tests passed; both normal mutation heap
tests passed. The expected baseline failures were observed before implementation.
Initial fixture failures (Top versus explicit-ID metadata, invalid native paths,
macOS non-UTF8 creation and sandboxed socket creation) were corrected without
weakening product admission. Independent review found no production blockers.

Final local verification:

- Locked full workspace: 4,000 passed, 34 ignored, zero failures.
- Contracts-enabled CLI library plus mutation heap: 1,060 passed, 25 ignored,
  zero failures. Nested subprocess reports are not counted twice.
- Clippy: workspace, all targets/features, warnings denied, passed.
- Documentation tests: 21 passed; site build passed.
- Knowledge catalog: 56 pages, 24 historical audit hashes and six current
  contract hashes checked; zero missing references or unreachable pages.

Native CI results are reported separately. Formal sources/oracles are
unchanged; passing model and runtime tests are not a proof of all interleavings.
