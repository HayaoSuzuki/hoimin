# Issue #1089: borrowed descriptor comparisons and validation

Base: main `5064140df8e8f24df42045664d72dbf887ef8372` after PR #1088.
Work continued on the branch containing
[the audit](../../knowledge/audits/design-2026-10-22-post1088.md), renamed from
`audit/post-1088` to `audit/issues-1089`; the same worktree was renamed to
`.worktrees/audit-1089`. Audit and navigation are included at the user's request.
Historical audit hashes/observations are preserved; implementation evidence is
recorded here separately.

## Ownership and comparison semantics

CandidateDescriptorRef borrows all ten descriptor fields. Both stored owned
descriptors and current MutationCandidate values project into this single view.
Its derived equality preserves schema, Utf8Path comparison, span, original,
replacement, operator, line, column, optional symbol and file hash. Assigned ID
and discovery sequence stay outside descriptor equality; full MutationCandidate
Eq or identity-only equality would change that contract.

The source validator accepts the borrowed view through Into from either current
representation. There is one validation implementation and no legacy reader,
compatibility wrapper, schema change, dependency, cache or new CLI setting.
Verify rediscovery comparison/source checks, worker mutation validation and
review report/execution/source checks use borrowed inputs. Review source-state
validation reuses its returned stable ID instead of hashing replacement again.

Stored ReviewKey/descriptor and reconstructed stale candidates retain ownership.
The optional display-side expected ReviewKey still uses the shared owned review
model; removing that materialization would require changing the model boundary,
so this issue does not claim to remove every transient CLI copy. Independent
review accepted this scope. Source encoding buffers and independent source
snapshots also retain their separate jobs.

## Allocation regression: observed RED then GREEN

Fixtures use 16/64 distinct candidates valid for core source validation, with
4/16/64 KiB original and
replacement strings. Fixture candidates, IDs, source contexts, report records
and indexes are built before measuring. Source validation first warms its source
location index. No public CLI is invoked by these fixtures, so there is no
startup janitor or external temporary-root operation.

One test calls the actual review execution-matching function for every candidate;
the other calls the actual source-state validator on the warm context. The
existing CLI test allocator measures Rust allocations on that thread. They do
not substitute an isolated equality helper for the production call.

Both tests completed all six shapes and failed their <4 KiB bounds before the
product change. They pass afterward. Measurements are identical for counts 16
and 64 because each transient comparison/validation releases its own temporary
objects; the table reports the peak, not cumulative allocation over all rows.

| Window | Bytes each for original and replacement | Before peak | After peak |
| --- | ---: | ---: | ---: |
| Execution comparison | 4,096 | 16,588 | 0 |
| Execution comparison | 16,384 | 65,740 | 0 |
| Execution comparison | 65,536 | 262,348 | 0 |
| Warm source-state validation | 4,096 | 8,371 | 77 |
| Warm source-state validation | 16,384 | 32,947 | 77 |
| Warm source-state validation | 65,536 | 131,251 | 77 |

[All observations](allocations.json) include both candidate counts. These are
requested Rust heap bytes, excluding SQLite C allocations, allocator metadata,
source decoding/index construction and fixture data. Full JSON decode or source
rediscovery may still dominate a command's peak. No RSS, end-to-end timing or
improvement percentage is asserted. The additional duplicate hash removal is
verified by the changed control flow and identity/error regressions; no independent
CPU-time or hash-byte measurement is claimed.

## Actual worker application window

An additional integration regression measures apply_mutation with 4/16/64 KiB
sources whose original/replacement cover the large literal. Workspace creation,
copy grants and identity generation occur before measuring. After each application
it checks both the mutated worker bytes and the unchanged original project.
This integration helper tracks global requested Rust allocations and serializes
its windows with the other mutation-heap tests, unlike the review thread-local
tracker above.

To validate the guard, the caller's original owned descriptor projection was
temporarily restored while retaining the fixed validator. The 64 KiB case failed
with a 196,855-byte peak. The exact fixed production source was then restored;
all five focused tests passed. This controlled regression isolates the worker
caller rather than claiming a complete rollback of all #1089 changes.

| Source bytes | Restored owned caller, normal | Fixed normal | Fixed contracts |
| ---: | ---: | ---: | ---: |
| 4,096 | 12,535 | 4,252 | 8,259 |
| 16,384 | 49,399 | 16,540 | 32,835 |
| 65,536 | 196,855 | 65,692 | 131,139 |

The worker retains one source body in normal builds and a second independent
original-evidence body for runtime contracts. The bound permits those bodies
plus 32 KiB slack; the 64 KiB shape detects a restored descriptor copy. The
smaller shapes provide coverage but the slack can tolerate copies at those sizes.
This window includes required source work and is distinct from review's <4 KiB
comparison/validation bounds. No before-contracts measurement is claimed.

## Semantic checks

The core regression compares every changed field against the owned descriptor's
existing equality, checks path spellings and hash case, and confirms both current
input representations project identically. ID and sequence remain ignored.
Explicit validation cases check source/hash/span/location error precedence and
stable IDs. Existing encoding, candidate-policy and source-index tests remain
required, including non-UTF8 and Latin-1 behavior.

The real review execution test accepts differing discovery sequences while
rejecting differing symbol metadata. Source-state tests check a valid descriptor
with a wrong ID, original mismatch taking priority over identity conflict, and
stale source taking priority over subsequent validation. Existing public output,
report mismatch, stale/unavailable, persistence, revision-conflict and mutation
application tests are covered by the full suite.

Initial test-fixture decoding included the outer report event kind in a plain
MutantFinished. That fixture error was corrected before the expected execution
allocation RED was observed. Clippy findings in test-only unit patterns, names
and tuple types were corrected without suppressions. Final independent static
review reported no Critical, Important or Minor findings; it did not run Cargo.

## Verification and resources

Cargo is serialized, offline/locked, with two build jobs, development debug
information disabled and incremental compilation disabled. This worktree owns
its target. Python tools use the existing shared environment without sync or
installation. Build/site outputs are removed after verification, leaving the
worktree, audit and local execution logs.

Final local verification:

- Full locked workspace: 4,013 passed, 34 ignored, zero failures across 191 Cargo
  targets. This run included both new core tests and four CLI regressions.
- The additional worker regression was added afterward and checked separately:
  the final focused run passed four CLI regressions plus that worker test. The
  product bytes match the implementation used in the full workspace run.
- Contracts-enabled CLI library and all three mutation-source heap tests:
  1,070 passed, 25 ignored, zero failures.
- Contracts-enabled core borrowed-descriptor tests: two passed, zero failures.
- Clippy: workspace/all targets/all features, warnings denied, passed after the
  additional worker test was added.
- Documentation tests: 21 passed; site build passed.
- Knowledge catalog: 58 pages, 55 YAML documents plus three plain indexes;
  32 historical audit hashes and five current contract hashes matched. Missing
  references and unreachable pages: zero.

Counts take the final result in each Cargo target section so nested subprocess
reports are not counted twice. Native CI is started separately and reported
without frequent polling. All local commands and failure/measurement logs remain
in this worktree's ignored .superpowers/implementation-1089 directory.

The audit's separate session-companion hardlink finding remains a recorded,
unresolved design question. It is not implemented or closed by #1089. Historical
formal evidence remains unchanged; passing runtime/oracle tests does not extend
its proof scope.
