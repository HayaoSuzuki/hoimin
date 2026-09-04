# Issue 352 POSIX Backslash Path Handling Design

## Goal

Prevent hoimin from rewriting a literal backslash in a Unix filename into a path separator. Reject paths that the portable plan and candidate formats cannot represent, before a map insertion, file read, or Git-path lookup can lose or redirect an entry.

## Scope

This change covers each unconditional backslash-to-slash conversion in `hoimin-cli`:

- CLI `--line` path parsing;
- exact and glob fingerprint inputs;
- fingerprint walk results;
- explicit filesystem target discovery;
- Git binary-numstat paths, patch paths, and current-worktree paths;
- workspace manifest paths.

The change keeps Windows native path input working. Windows code converts `\` separators to `/` logical separators. Unix code preserves native text, then rejects a literal `\` where hoimin requires a portable logical path.

The change does not alter `hoimin-core`'s plan schema, candidate identity, `normalized_relative_path`, or `canonical_identity_path`. Those contracts already reject a candidate path containing `\`; changing them would require a new cross-platform path representation and a schema migration.

## Confirmed behavior on main

Given a Unix project containing only `foo\bar.py`, the public command

```console
hoimin plan --root PROJECT --source . --allow-best-effort-memory -- python3 -c 'raise SystemExit(0)'
```

exits 2 and reports:

```text
plan.source.read: foo/bar.py: No such file or directory (os error 2)
```

Given both `foo\bar.py` and `foo/bar.py`, `plan` exits 0 but emits only `foo/bar.py`. The `BTreeMap` key created from the rewritten literal name collides with the real nested path, so one source disappears without a diagnostic.

## Root cause

The affected callers use `.replace('\\', "/")` as if `\` were a separator on every host. Windows uses `\` as a native separator. Unix filesystems treat it as an ordinary filename byte. Git already emits `/` as its path separator on every supported host, and Git C-quoting decodes `\\` into a literal backslash.

The conversion therefore has two different meanings:

- on Windows native paths, it creates the portable `/` form that hoimin stores;
- on Unix native paths or decoded Git paths, it changes the filename.

`hoimin_core::normalized_relative_path` and `WorkerRoot` reject `\`, so preserving the Unix filename past discovery would only move the failure to a later layer. The current conversion hides that contract violation and can make two distinct native paths share one logical key.

## Contract decision

Hoimin will continue to use `/`-separated portable logical paths in plans, candidates, fingerprints, and worker operations. On Unix, hoimin will reject an encountered filename containing `\` with the original spelling in the error. On Windows, hoimin will translate native `\` separators to `/` as before.

Git output receives a stricter rule. Git uses `/` separators on Windows and Unix, so a backslash after Git C-quote decoding represents a literal filename character. The Git adapters will reject it on every host instead of treating it as a separator.

This decision changes a silent corruption or misleading late I/O error into an early typed error. It does not make any accepted path invalid on Windows and does not change serialized data for accepted paths.

## Approaches considered

### 1. Reject unrepresentable Unix names at ingress

Add a small internal path helper with two operations:

- convert native separators only on Windows;
- reject a logical or Git path containing a backslash.

Each boundary maps the helper result into its existing domain error. Filesystem and manifest walkers validate before inserting a logical path into a map. This design preserves the current schema and blocks both confirmed failure modes.

### 2. Preserve Unix backslashes and rely on later validation

Conditional conversion alone would preserve the correct native spelling. The analyzer protocol, candidate validator, and `WorkerRoot` would reject it later. Users would receive different errors depending on the command and selected files, and manifest construction could still retain paths that worker operations cannot address. This design does not provide one enforceable ingress contract.

### 3. Support literal Unix backslashes end to end

Full support needs a logical path type that distinguishes separators from component contents across hosts. Plans created on Unix must remain unambiguous when read on Windows, and stable candidate IDs must preserve that distinction. That work requires a schema version change and migration rules. Issue 352 does not justify that compatibility cost.

Approach 1 is selected.

## Components

### Internal path helper

Create `crates/hoimin-cli/src/portable_path.rs` and register it as a private module in `lib.rs`.

The module will expose focused crate-private functions:

```rust
pub(crate) fn from_native(value: &str) -> Result<Cow<'_, str>, PortablePathError>;
pub(crate) fn from_git(value: &str) -> Result<&str, PortablePathError>;
```

`from_native` returns an owned slash-normalized value on Windows. On Unix it returns a borrowed value without a backslash and rejects a value containing one. `from_git` rejects a backslash on every host because Git uses `/` as its separator. `PortablePathError` retains the rejected spelling for diagnostics. Callers may normalize `.` components or validate roots after these operations under their existing contracts.

Git adapters call `from_git` on decoded Git text. Filesystem and CLI adapters call `from_native`.

### CLI and fingerprint inputs

`parse_line_selection` will use `from_native`. It will preserve Windows input support and reject a Unix backslash as `CliError::InvalidValue` before target resolution. `--file` and `--source` continue through filesystem discovery, which enforces the same portable-path contract on encountered files.

`normalize_exact_path` and `validate_pattern` will convert separators on Windows and reject a remaining backslash on Unix. `resolve_one` will validate each walked relative name before it creates a fingerprint record.

The input parsers retain their existing public error families:

- `CliError::InvalidValue` for malformed `--line` syntax;
- `fingerprint.file.invalid_path` for exact inputs;
- `fingerprint.include.invalid_glob` or `fingerprint.include.unsupported_file` for glob configuration and matched filesystem entries.

### Filesystem target discovery

Add a `FsTargetError` variant that names an unrepresentable path. `collect` will create the host-correct logical spelling, validate it, and only then insert it into the `BTreeMap`. A root containing both `foo\bar.py` and `foo/bar.py` will return an error instead of dropping one entry.

`TargetHandler` will continue wrapping this error in `TargetError::DiscoveryFailed`, preserving the existing error boundary and exit code 2.

### Git target discovery

`insert_binary_numstat_path`, `parse_patch_path`, and `collect_current_worktree_paths` will stop rewriting decoded backslashes. Each function will reject a remaining backslash as malformed for hoimin's portable target contract. The error remains `TargetError::GitFailed`, with the original path spelling in the message.

This check occurs after C-quote decoding. A quoted `"a/weird\\\\name.py"` therefore produces the intended literal `a/weird\name.py` witness and is rejected rather than changed to `a/weird/name.py`.

### Workspace manifest

`relative_utf8` will apply Windows-only separator conversion and reject a remaining backslash with `WorkspaceError::InvalidPath`. `collect` calls this function before it inserts entries, so both a single unsupported filename and a colliding pair fail before snapshot creation.

The workspace path error keeps the stable `workspace.path.invalid` code.

### Documentation

README path-selection documentation will state that hoimin stores portable `/`-separated paths and rejects Unix filenames containing a literal backslash. The note will cover mutation targets, fingerprint inputs, and copied workspace files.

## Error and data flow

For `plan --source`, the rejection path is:

```text
native walk entry
  -> Windows-only separator conversion
  -> portable-path validation
  -> FsTargetError
  -> TargetError::DiscoveryFailed
  -> plan exits 2 without JSON output
```

For workspace preflight through the library API, manifest construction performs the same validation and returns `WorkspaceError::InvalidPath`. It does not read a rewritten path and does not create a snapshot.

For `--changed`, explicit discovery runs first and rejects an unsupported selected tree. The Git adapters also enforce the contract when callers use the Git handler directly or when future call ordering changes.

## Testing

Tests will use real filesystem and Git fixtures on Unix.

- A target-discovery test creates only `foo\bar.py` and asserts an error containing that exact spelling.
- A collision test creates both `foo\bar.py` and `foo/bar.py` and asserts rejection before either can replace the other in the map.
- A public `plan` test asserts exit 2, no plan JSON, and no misleading `foo/bar.py: No such file` diagnostic.
- A workspace preflight test asserts `WorkspaceError::InvalidPath` for a literal backslash filename.
- Fingerprint exact and glob tests assert stable domain error prefixes and the original spelling.
- Git unit and integration tests assert that raw NUL paths and decoded C-quoted paths keep their literal backslash long enough to trigger `GitFailed`.
- Windows-only helper tests assert that native `pkg\file.py` still becomes `pkg/file.py`.
- A source scan asserts that unconditional `.replace('\\', "/")` no longer remains in `hoimin-cli`; Windows-only conversions and the documented core identity canonicalizer remain allowed.

The implementation will follow red-green-refactor for each behavior. The full workspace suite, formatting, Clippy, and repository Python tests will run before delivery.

## Lean and mutation testing decision

Lean would model a reduced string transformation, while the main risk lies in host path parsing, Git quoting, and real filesystem behavior. Native Rust integration tests exercise the same premises and expose the complete public observation. This change will not add a Lean model.

The fix replaces a deterministic conversion and adds boundary checks. Focused unit, integration, and property tests can kill the plausible mutations without launching mutation execution. A cargo-mutants run would duplicate that evidence and consume extra disk, so the plan omits mutation testing unless review finds an uncovered predicate branch.

## Compatibility

- Windows native input retains its current behavior.
- Accepted Unix paths retain their spelling and serialized form.
- Unix roots containing a copied or selected filename with `\` now fail early instead of producing an incomplete or redirected plan/workspace.
- Plan and report schema versions do not change.
- Error families and process exit codes do not change.

## Out of scope

- Supporting a literal Unix backslash in plan or candidate paths;
- changing colon handling for NTFS alternate data streams in issue 348;
- changing `--line` range validation from issue 360;
- altering stable candidate ID canonicalization for invalid candidate inputs;
- changing include/exclude glob syntax beyond rejecting an unportable Unix backslash.

## Self-review record

### Round 1: contract and compatibility

The review traced accepted paths from CLI input through plan serialization and worker access. It found that conditional preservation alone conflicts with `normalized_relative_path` and `WorkerRoot`. The design now selects early rejection and keeps the plan schema unchanged. The review also separated Windows native separators from Git output, since Git already emits `/` on Windows.

### Round 2: failure ordering and testability

The review traced the two reproduced failures through target discovery and the workspace manifest. It found that validation must happen before `BTreeMap::insert`; validation after collection would still allow a collision to erase evidence. The test design now covers a single literal filename, a colliding pair, direct Git adapters, workspace preflight, and the public `plan` exit.

### Round 3: scope and implementation consistency

The review searched all `hoimin-cli` backslash-to-slash conversions. It added the CLI line parser and both fingerprint input normalizers, which the issue evidence did not enumerate. It excluded Windows-only root comparison and `hoimin-core::canonical_identity_path`, where conversion either runs only on Windows or handles identities that production validation rejects. The review also rejected a Lean artifact and cargo-mutants run because native tests provide stronger same-premise evidence for this change.
