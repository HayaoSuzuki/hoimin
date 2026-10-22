# Issues #1085–#1087: implementation and evidence

Base: main `11eb28d3ff43c441c3aabc78cd7fa6d7d0c72493` after PR #1084.
Implementation uses `audit/issues-1085-1086-1087`, the existing branch containing
[the audit](../../knowledge/audits/design-2026-10-22-post1084.md).
The audit and navigation are included at the user's request. Historical source
hashes and audit observations are preserved; subsequent evidence is recorded here.

## Session hardlinks (#1085)

Existing session files with native descriptor link count greater than one are
rejected before file-backed SQLite reads/writes, schema setup, WAL or run lock
creation. A small private helper shares Unix nlink / Windows file-information
queries with worker mutation guards and Windows cleanup. No project-wide inode
cache, schema change, repair, unlinking or compatibility reader is added.

Before implementation, the empty-session hardlink test changed directory
artifacts, and a valid current-session hardlink was admitted. Public CLI aliases
inside and outside the project both changed an empty __init__.py into a SQLite
database before reporting workspace.original.changed. Afterward regressions
require exit 2 and the hard-link diagnostic, unchanged source/alias bytes and
permissions, no baseline execution and no companions or ownership lock. Public
fixtures isolate TMPDIR/TMP/TEMP under their own directory. The single-link
zero-byte initialization/reopen control passes; existing normal session and
resume tests remain required. The check does not synchronize arbitrary concurrent
path replacement or link creation.

## Borrowed parser tokens (#1086)

Discovery uses the parser's borrowed contiguous token slice. Previous/next token
searches remove the obsolete reference-copy adapters. The parser remains owner;
no new API, dependency, cache or candidate behavior is introduced.

Temporary instrumentation brackets the actual token-view acquisition with a
tracking allocator. It runs alone in hoimin-python's library test process and
always restores the source byte-for-byte; instrumentation is not product code.
The fixture includes unary signs, identity/membership negation, Unicode f/t-string
content, candidate truncation and cancellation. Full descriptors, stable IDs,
diagnostics and truncation match the baseline exactly.

| Source bytes | Tokens | Before view peak requested Rust bytes | After |
| ---: | ---: | ---: | ---: |
| 972 | 360 | 2,880 | 0 |
| 4,050 | 1,500 | 12,000 | 0 |
| 16,362 | 6,060 | 48,480 | 0 |

These observations equal token count times the 8-byte native reference size
before the change. They measure only that phase, not the full AST, allocator
metadata, process RSS or total latency. The before test fails the zero-buffer
assertion; the after test passes. Reproduction script and observations are stored
alongside this record. Run the following from the repository root with no other
Cargo process or edit of rust.rs in progress; it temporarily instruments that
file and restores its exact bytes in a finally block:

```sh
python docs/maintenance/2026-10-22-issues-1085-1086-1087/measure-token-view.py before
python docs/maintenance/2026-10-22-issues-1085-1086-1087/measure-token-view.py after
```

The script treats the expected baseline test failure as a successful observation
and checks the changed implementation against it. Logs/JSON are written beside
the script; rerunning replaces the recorded observations for the local platform.

## Borrowed current review candidates (#1087)

Private ReviewRow uses Cow: current rows borrow plan candidates, while stale rows
own descriptors reconstructed from stored keys. Plan/snapshot lifetimes cover
synchronous rendering. Persisted keys, history, source states, JSON/human fields,
ordering and sequence-zero stale metadata remain unchanged.

The actual public review command writes JSON into a noncollecting writer. The
first real write observes live allocations on the measuring thread after decode
buffers/source contexts have been released. Fixtures use 16/64 candidates with
4/16/64 KiB replacements. A single-population growth bound plus 128 KiB slack
rejects another retained full copy. The baseline fails: 16 candidates grow
393,216 bytes between 4 and 16 KiB, versus a single-population 196,608 bytes.
Complete JSON/human output is checked outside the window, and a separate stale
regression checks every descriptor/history field and sequence zero.

The contracts-enabled successful run observed these first-write live bytes:

| Candidates | Replacement bytes each | Live requested Rust bytes |
| ---: | ---: | ---: |
| 16 | 4,096 | 78,324 |
| 16 | 16,384 | 274,932 |
| 16 | 65,536 | 1,061,364 |
| 64 | 4,096 | 309,444 |
| 64 | 16,384 | 1,095,876 |
| 64 | 65,536 | 4,241,604 |

The 16-candidate 4→16 KiB increment is exactly 196,608 bytes, one replacement
population. Both normal and contracts-enabled regression bounds pass. The quoted
baseline failure was a normal build; this table is explicitly a contracts build
observation rather than a claim about total process memory.

This measures Rust live allocations, excluding SQLite's C heap. It does not
claim that all transient decode allocations disappear or predict RSS/latency.

## Verification and resources

Cargo commands are serialized with locked cached dependencies, two build jobs,
disabled development debug information and disabled incremental compilation.
The worktree owns its build directory; Python tools use the existing shared
environment without dependency synchronization. Generated build/site output is
removed after verification, preserving the worktree and execution evidence.

Focused regression run: 14 CLI library tests and two public hardlink tests passed.
Independent static reviews of all three changes found no production blockers. Native Windows
execution is left to CI; local development is on macOS. Formal sources/models
are unchanged; these tests do not prove arbitrary filesystem interleavings.

Final local verification:

- Locked full workspace: 4,007 passed, 34 ignored, zero failures.
- Contracts-enabled CLI library, mutation-source heap and public hardlink tests:
  1,067 passed, 25 ignored, zero failures. Nested subprocess reports are counted
  only once per Cargo target.
- Clippy: workspace, all targets/features, warnings denied, passed.
- Documentation tests: 21 passed; site build passed.
- Knowledge catalog: 57 pages (54 YAML frontmatter documents plus three plain
  indexes), 38 historical audit hashes and five new current contract hashes
  checked; zero missing references or unreachable pages.

Native CI results are reported separately. The only initial full-window RED
unrelated to these changes was a sandbox-denied existing Unix-socket fixture;
it passes under normal OS permissions. The contracts command's initial target
spelling was corrected from mutation_heap to mutation_source_heap before tests
ran. Clippy's two format-push-string test-helper findings and the subsequent
import-placement finding were corrected; the final warnings-denied check passes.
