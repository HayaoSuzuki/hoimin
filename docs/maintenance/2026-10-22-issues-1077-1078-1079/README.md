# Issues #1077–#1079 implementation and evidence

Implementation continues from main `12b34294ba2f61cd6b26425695b9ff08ed289875`
on `audit/issues-1077-1078-1079`, the worktree containing the post-1076 audit.
The audit page and its two navigation edits were initially kept uncommitted.
At the user's subsequent request, [the audit](../../knowledge/audits/design-2026-10-22-post1076.md)
and navigation are included in the same PR #1080. Its historical source hashes
are not rewritten; a separate follow-up distinguishes implementation evidence
from the original audit observations.

## Reject review WAL stores before SQLite access (#1077)

Valid review stores use DELETE mode. Existing-file admission opens a regular
file through the shared nonblocking helper, rechecks descriptor size, and reads
at most 20 bytes. A SQLite header with journal read/write formats other than
`[1, 1]` is rejected before either ordinary read-only or read-write SQLite open.
The inspection descriptor closes before SQLite opens, including on Windows.
Empty/corrupt/non-SQLite cases still receive the existing SQLite/schema rejection.
There is no URI handling, WAL support, migration, repair or sidecar deletion.

The new regression demonstrated the actual failure in bundled Rust and public
`review list/show`: a closed foreign WAL DB gained companion files before being
rejected. Two tests failed before the fix; a write-side control passed. Unlike
the earlier Python equivalent probe, these exercise the real product path.
Baseline snapshots read the filesystem only; logical observations use a disposable
copy so the observer cannot create companions beside the protected fixture.
The fixed paths preserve complete directory names/content as well as DB rows,
version and journal mode. Tests also exercise current DELETE history, read/write
reopen, Unicode/space/#/% paths, nonregular inputs and nonblocking FIFO rejection.
Unix non-UTF8 paths follow ordinary SQLite capability; an unsupported native
filename is rejected without new files rather than requiring new VFS support.

The check retains current size, schema, identity, project, integrity and revision
validation, private first publication and protected-input rules. It is not an
atomic snapshot or a guarantee under path replacement, active foreign writers,
hot-journal recovery or power loss. No other's files are removed on rejection.

## Borrow review history for rendering (#1078)

The snapshot remains the owner of review events until synchronous rendering ends.
Private `ReviewRow<'a>` fields borrow history and its latest event; the document
carries the same lifetime. Empty histories use `&[]`; stale candidates retain
their existing candidate ownership and borrow only history from the snapshot.
There is no cache, new shared mutable state or public/wire representation change.
The latest event still appears in both `review` and `history` in JSON.

Whole-command allocator regressions vary the selected history, complementing the
existing unrelated-history boundary. Fixtures use 1/16/64 events and reason/
evidence strings of 1–4 KiB, below 1 MiB per fixture. A counting writer avoids
holding the complete serialized output in the allocation window. Separate JSON
and human assertions check every history revision, text, latest event and stale
applicability. Record verifies saved state through public show afterward and an
existing record test now also checks emitted history/latest fields. SavedOutput,
conflict and other error semantics retain their existing regression coverage.

| Public command / selected history | Before peak requested heap | After peak requested heap |
| --- | ---: | ---: |
| show, 64 events, 1 KiB reason + 1 KiB evidence | 312,472 | 160,485 |
| record, 64 events, 4 KiB reason + 2 KiB evidence | 866,022 | 447,349 |

Before the change, both selected-history and record allocation regressions fail.
After the change, they pass. This counts requested Rust heap on the measuring
thread; SQLite's C heap, RSS and end-to-end CLI latency are not measured. The
snapshot must still retain the selected history once; this removes the second
owned rendering copy rather than discarding useful persistent history.

## Reuse mutation source work (#1079)

Normal mutation application builds `CandidateValidationContext` once and reuses
its raw hash. A private helper preserves hash-mismatch precedence when context
construction fails on raw/decoded size bounds: only that exceptional path needs
a separate raw hash. Its private constructor factory allows a bounded failure
injection instead of a multi-gigabyte fixture; no unchecked-hash API is exposed.
Obvious candidate/manifest hash mismatch still fails before span/location checks.

All validation and replacement encoding complete before dropping the borrowed
context and splicing the owned raw buffer. The filesystem capability and writable
reopen checks are unchanged and occur before truncation/write. Normal builds do
not allocate a separate whole mutated body. Contract builds intentionally retain
independent original bytes for the prefix/suffix postcondition; it never compares
the buffer to itself. Growth may require reallocating the one mutable buffer.

An isolated allocator test applies real worker mutations to 64/256/1024-KiB
comment-padded ASCII sources with the replacement near the start, middle and end.
Source/candidate/worker setup is outside each window. All nine cases check exact
prefix/replacement/suffix, unchanged original project and reset. The fixture
source is at most 1 MiB at a time; normal owned workspace copies remain necessary.

| Source bytes | Before peak requested heap | After peak requested heap |
| --- | ---: | ---: |
| 65,536 | 131,219 | 65,775 |
| 262,144 | 524,435 | 262,383 |
| 1,048,576 | 2,097,299 | 1,048,815 |

All three replacement positions gave these values. These are apply-window
requested-heap observations, not RSS, total memory halving, or latency claims.
The meaningful before test failed its one-source-body allowance; the after test
passes. Contracts permit the separate original evidence body in that bound.

A temporary print at each real raw-hash call site, bracketed by the actual apply
window, observed two raw hashes before and one afterward in all nine cases.
The temporary changes were restored to exact original source bytes after each
run; no observation API or hash-logging remains in production. The
[measurement script](measure-mutation-hashes.py) reproduces both observations in
an idle checkout and restores both production source files
without changing dependencies. It uses the existing standalone regression test;
restore must finish before formatting, editing or another Cargo command.

Direct functional tests cover growth/shrink, Unicode byte-length changes, BOM,
Latin-1 replacement, exact bytes/reset, unrepresentable Latin-1 replacement with
unchanged readonly permissions, and compound hash/span/location errors. Synthetic
constructor failure checks hash-mismatch precedence without huge allocations.

## Verification and resource limits

Local Cargo commands are serialized, use locked dependencies, two build jobs,
disabled development debug information and disabled incremental compilation in
this worktree's own target directory. Shared Python dependencies are not updated.
Independent review found no implementation blockers; its requested direct splice/
encoding/error-order coverage was added and passed.

- Review unit/whole-command tests: 114 passed.
- Mutation unit tests: 16 passed, 1 ignored.
- Isolated mutation heap regression: passed after a demonstrated baseline failure.
- Actual raw-hash-site observations: 2 to 1 in nine apply windows.

- Full locked workspace suite: 3,990 passed, 34 ignored, zero failures.
- Contracts-enabled CLI library and mutation heap suite: 1,050 passed,
  25 ignored, zero failures (nested subprocess reports are not counted twice).
- Workspace Clippy, all targets/features with warnings denied: passed.
- Ruff format/check and ty: passed without dependency synchronization.
- Documentation tests: 21 passed; generated site build passed.
- Knowledge references: zero missing links/resources or unreachable pages;
  55 pages include the audit added to this PR.

The first workspace run lacked the worktree's `.venv` reference and failed
Python-launch fixtures. A symlink to the existing shared environment fixed this
setup error; the complete workspace suite was then rerun successfully. No Python
dependencies were installed or updated.

Native CI status is reported separately rather than inferred from
local or model results. Existing proof sources and abstract expectations are not
changed by these fixes.
