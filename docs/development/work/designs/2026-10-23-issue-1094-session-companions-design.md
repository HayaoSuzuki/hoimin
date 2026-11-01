# Issue #1094: session companion hardlink admission

The user requested implementation of #1094 on the branch retaining the audit,
including that audit in the PR. Use the issue's proposed pre-connection rejection
policy. The same branch/worktree is renamed audit/issues-1094 / audit-1094.

Before any file-backed SQLite connection, inspect existing journal/WAL/SHM names
from SqliteCompanions using a regular-file descriptor. Missing entries are fine;
nonregular or inaccessible entries fail inspection. Reuse native file_link_count
and the typed SharedFile error to reject nlink > 1. Keep this policy in session
schema admission. Do not create/open a writable SQLite connection to inspect it.

Preserve source/database/companion bytes and permissions, avoid new sidecars/run
locks and baseline execution on rejection. Keep normal new/empty/current sessions,
single-link companions and resume. No deletion, repair, journal-mode conversion,
schema/CLI/dependency change, global inode index or compatibility reader. This
preflight is not synchronization against concurrent pathname replacement.

Use native unit fixtures for DB-state/suffix/error-priority coverage, public CLI
fixtures for actual source protection inside/outside the project, and normal
session/resume controls. Private TMPDIR/TMP/TEMP prevent unrelated janitor work.
Cargo uses two jobs, no incremental/dev debug info, cached locked dependencies.
