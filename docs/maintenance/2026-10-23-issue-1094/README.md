# Issue #1094: reject hardlinked session companion files before SQLite

Base: main `e8f0e7b7f86a849d1305ef1ccaddd80c1df80f0b` after PR #1093.
The work continues on the branch retaining
[the audit](../../knowledge/audits/design-2026-10-23-post1093.md), renamed to
`audit/issues-1094` / `.worktrees/audit-1094`. Audit, navigation and implementation
are included together at the user's request. Historical audit hashes remain pinned
to the audited base. At issue creation, main `ee6d6ac9` only added the UV action
update; the relevant session sources were unchanged.

## Admission policy and ownership

SessionArtifacts resolves the database path without creating artifacts. Then
schema::open inspects the journal, WAL and SHM names from SqliteCompanions before
any file-backed SQLite connection, including read-only schema admission. Native
symlink_metadata rejects nonregular entries and symlinks, including dangling
symlinks; missing entries are allowed. The regular-file reader checks the opened
descriptor and avoids waiting for Unix FIFO writers. The existing native
file_link_count rejects link counts greater than one with SchemaError::SharedFile.
Inspection errors propagate as typed Inspect errors. The SharedFile message now
says session file because it covers both the database and its companions.

A hardlinked companion takes priority over foreign/unsupported schema diagnostics:
SQLite must not touch that companion just to decide which database error to report.
The existing body hardlink and schema/identity checks are retained. No writable
connection, deletion, unlink, automatic repair, journal-mode conversion, global
inode index, legacy reader, CLI/schema/dependency or package-floor change is added.
The guard does not synchronize with concurrent pathname replacement or link creation.
It does not inspect SQLite file contents or impose a new file-size cap.

Symlinks already failed SQLite's own admission in the original reproductions;
rejecting them before connection also prevents premature creation of the database.
The lexical and descriptor checks have separate jobs: reject an unsupported entry
name first, then check the actual opened regular file and its native link count.

## Observed RED then GREEN

Before the production edit, the native regression completed all 15 shapes:
three suffixes × missing/zero-byte/current/foreign/unsupported database states.
Every shape failed the expected SharedFile admission. Some also changed disk
contents: missing/empty DB with a journal changed the shared source from 0 to
512 bytes; a current DB with SHM changed it from 0 to 32,768 bytes. These are new
native test observations, separate from the earlier audit's 13 public CLI cases.

The public CLI regression also failed before the edit, observing a changed source.
A nonregular-companion regression failed because SQLite created the database
before refusing the entry. A further symlink regression failed before the lexical
metadata check was added. The ordinary resume test initially used an incorrect
assumption about an outcome.kind JSON field; it was corrected to decode the actual
MutationOutcome enum and check Reused. This was a test assertion error, not a
production serializer change.

After the fix, focused validation passed five native tests and three new public
CLI tests. The matrices cover:

- 15 native hardlink shapes: SharedFile and unchanged directory entries, bytes
  and permissions, including existing DB/other companion preservation.
- 9 normal single-link companion shapes: missing/zero-byte/current databases
  initialize and reopen with schema 5.
- 12 public CLI alias shapes: new/current DB × inside/outside project × three
  suffixes. Source and all known session artifact bytes/permissions remain;
  no extra companions, ownership lock or baseline marker appears; exit 2 contains
  the hardlink diagnostic. Original calc.py remains unchanged.
- Real incomplete-run resume: first run uses one of two mutations (exit 4), then
  the larger-budget resume finishes (exit 1) with the same run ID, a typed Reused
  outcome, a successful baseline and unchanged source.
- Native directory refusal on every platform; Unix existing/dangling symlinks,
  unreadable companion (non-root only), and public CLI FIFO/directory refusal
  within five seconds without database creation or baseline execution.

The two original #1085 public DB-body hardlink tests reuse the strengthened
artifact-preservation helper and remain. All public CLI fixtures use the freshly
built CARGO_BIN_EXE binary and isolate TMPDIR/TMP/TEMP; no installed CLI or unrelated
startup janitor root is used. No large benchmark is needed or claimed.

## Review and verification

A fresh read-only reviewer found no blocking implementation issue and confirmed
the native suffixes, canonical database path, pre-connection ordering, opened-file
link count and ordinary resume assertions. It recommended a populated single-link
WAL/SHM positive fixture; that follow-up is recorded with the final checks below.

The first Clippy run rejected decimal Unix permission mode 0 in a test; it was
changed to the equivalent octal 0o000. Workspace Clippy then passed all targets and
all features with warnings denied. Cargo uses two jobs, incremental compilation
disabled, no development debug info and cached offline/locked dependencies.

Final local verification:

- `cargo test --offline --locked --workspace --all-features`: 4,020 passed,
  34 ignored, no failures. The populated-WAL control below was added after this
  suite's CLI library binary was compiled; production code did not change.
- Follow-up with contracts enabled, filtered to issue_1094: 6 native and 3 public
  CLI tests passed, including that additional populated-WAL control. An idle
  SQLite connection retains real nonempty WAL/SHM files with nlink=1; opening a
  second session handler and reopening after close both succeed.
- Documentation tests: 21 passed; site generation and local-reference inventory
  succeeded. Historical audit hashes are checked against the recorded revision;
  the current contract's source hashes are checked against working files.
  The inventory has 60 pages, 57 YAML headers and 3 plain indexes; 37 historical
  audit hashes and 3 current source/design hashes match, with no missing or
  unreachable local references.
- Normal commit hooks rerun formatting and Clippy for the workspace and vendored
  parser; their actual result is recorded in the local commit log and PR.

No Linux/Windows success or finished GitHub CI result is claimed here. The native checks here run on macOS; Linux/Windows
coverage requires CI, and no success for those platforms is inferred from local
file APIs. The audit's historical evidence remains distinct from these tests.
