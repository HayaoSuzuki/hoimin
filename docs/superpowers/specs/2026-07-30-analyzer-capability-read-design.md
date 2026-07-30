# Analyzer Capability-Relative Read Design

## Scope

Fix #85 so both analyzer entry points read normalized source paths relative to
an already-open project-root capability. Parent symlinks, Windows reparse
points, and final links must fail as `analyzer.source.read`; outside-root bytes
must never contribute to candidate hashes, snippets, plans, or reports.

## Design

Promote the workspace module's hardened `WorkerRoot` reader behind a narrow
crate-private `RootRelativeReader`. Construction opens the root directory once.
Each `read(path)` walks parent components relative to that stable handle,
rejects links and reparse points, opens the final regular file without
following it, and returns bytes read from that same file handle. The existing
one-shot fingerprint helper delegates to this type, so Unix and Windows
security logic remains in one implementation.

`AnalyzerHandler` retains its configured root path without touching the
project during shell-context construction. Its first non-cancelled analysis
request opens and retains one `RootRelativeReader` for all subsequent source
reads. `discover_targets` constructs one reader before iterating its targets.
Both analyzer read sites remain synchronous internally: the source files are
bounded by the existing workspace-copy and analyzer limits, while removing
`tokio::fs::read` avoids reopening an authoritative source pathname outside
the capability boundary.

The analyzer still checks cancellation before reading. Candidate analysis,
ordering, hashing, spool behavior, and UTF-8 error mapping do not change.

## Error handling

`AnalyzerHandler::new` and `with_backend` preserve their no-project-I/O
construction behavior and fallible public signatures. A capability-open
failure on the first analysis request is mapped without exposing workspace
internals.

Any no-follow traversal, missing file, non-regular final entry, read failure,
or Windows reparse-point rejection maps to the existing typed
`analyzer.source.read` effect failure. Invalid UTF-8 remains
`analyzer.source.utf8`.

## Cross-platform behavior

Unix continues to use `cap-primitives` directory handles and no-follow opens.
Windows continues to use `NtCreateFile` relative to `RootDirectory`, with
`FILE_OPEN_REPARSE_POINT`; final handles whose metadata carries
`FILE_ATTRIBUTE_REPARSE_POINT` are rejected. No Unix-only canonicalization or
metadata preflight is added.

Tests use the existing platform-specific symlink helpers. A handler is created
before its selected parent is replaced, making the discovery-to-analysis
replacement deterministic. On Windows, a directory junction is the fallback
when symlink creation lacks privilege, so the reparse-point rejection
assertion is mandatory rather than skipped.

## Testing

- Add a runtime-handler regression that replaces a selected parent with a
  link after handler construction, expects `analyzer.source.read`, and proves
  the outside sentinel remains unchanged.
- Add the equivalent in-memory `discover_targets` regression for its separate
  read site.
- Retain ordinary candidate-equivalence and cancellation tests.
- Cross-compile-check the Windows target when its standard library is
  available, in addition to local formatting, Clippy, and workspace tests.

## Documentation

The change enforces the existing root-isolation contract and introduces no new
CLI option or user workflow. README changes are therefore unnecessary.
